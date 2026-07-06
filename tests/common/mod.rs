//! Shared test helpers: a live pool + a fake idempotent-per-timesheet billing port (for golden/integrity),
//! the REAL backbone-billing adapter (for the billing seam), and an event-capturing sink. Fresh random
//! ids per test.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use backbone_project::application::service::project_events::{ProjectEvent, ProjectEventSink};
pub use backbone_project::application::service::project_events::LoggingSink;
use backbone_project::application::service::project_ports::{
    BillingPort, InvoiceAck, InvoiceFromTimesheet, ProjectRejected,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

pub fn dburl() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/backbone_project".into())
}
pub async fn pool() -> PgPool {
    PgPool::connect(&dburl()).await.expect("connect")
}
pub fn dec(s: &str) -> Decimal {
    s.parse().unwrap()
}

/// A sink that records every published project event, so a test can assert on what the write path emitted.
#[derive(Clone, Default)]
pub struct CapturingSink {
    pub events: Arc<Mutex<Vec<ProjectEvent>>>,
}
impl CapturingSink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn events(&self) -> Vec<ProjectEvent> {
        self.events.lock().unwrap().clone()
    }
}
impl ProjectEventSink for CapturingSink {
    fn publish(&self, event: &ProjectEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

/// A fake billing seam that creates a service invoice idempotently per timesheet.
#[derive(Clone, Default)]
pub struct FakeBilling {
    pub invoiced: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    pub calls: Arc<Mutex<u32>>,
}
impl FakeBilling {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn invoice_count(&self) -> u32 {
        *self.calls.lock().unwrap()
    }
}
#[async_trait::async_trait]
impl BillingPort for FakeBilling {
    async fn create_service_invoice(
        &self,
        req: &InvoiceFromTimesheet,
    ) -> Result<InvoiceAck, ProjectRejected> {
        *self.calls.lock().unwrap() += 1;
        let mut m = self.invoiced.lock().unwrap();
        let id = *m.entry(req.timesheet_id).or_insert_with(Uuid::new_v4);
        Ok(InvoiceAck { invoice_id: id })
    }
}

/// ACL over the REAL backbone-billing: build a service Sales Invoice from a billable timesheet
/// (idempotent per timesheet via a stable invoice_number = TS-<timesheet_id>; on a duplicate number,
/// look up the existing invoice).
pub struct RealBilling {
    pub svc: backbone_billing::application::service::billing_write_service::BillingWriteService,
    pub pool: PgPool,
}
#[async_trait::async_trait]
impl BillingPort for RealBilling {
    async fn create_service_invoice(
        &self,
        req: &InvoiceFromTimesheet,
    ) -> Result<InvoiceAck, ProjectRejected> {
        use backbone_billing::application::service::billing_write_service::{
            NewInvoiceLine, NewSalesInvoice,
        };
        let number = format!("TS-{}", req.timesheet_id);
        let res = self
            .svc
            .create_sales_invoice(NewSalesInvoice {
                invoice_number: number.clone(),
                company_id: req.company_id,
                branch_id: None,
                customer_id: req.customer_id,
                source_so_id: None,
                posting_date: chrono::Utc::now().date_naive(),
                due_date: None,
                currency: Some(req.currency.clone()),
                receivable_account_id: Uuid::new_v4(),
                lines: req
                    .lines
                    .iter()
                    .map(|l| NewInvoiceLine {
                        item_id: l.item_id,
                        account_id: Uuid::new_v4(),
                        description: l.description.clone(),
                        quantity: l.hours,
                        unit_price: l.rate,
                    })
                    .collect(),
                tax_lines: vec![],
            })
            .await;
        match res {
            Ok(id) => Ok(InvoiceAck { invoice_id: id }),
            Err(_) => {
                let id: Uuid = sqlx::query_scalar(
                    "SELECT id FROM billing.sales_invoices WHERE invoice_number=$1")
                    .bind(&number)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| ProjectRejected {
                        code: "invoice_lookup_failed".into(),
                        message: e.to_string(),
                    })?;
                Ok(InvoiceAck { invoice_id: id })
            }
        }
    }
}
