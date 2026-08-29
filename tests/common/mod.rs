//! Shared test helpers: a live pool + a fake idempotent-per-period billing port (for golden/integrity),
//! the REAL backbone-billing adapter with the retry-after-reversal numbering duty (for the billing
//! seam), an event-capturing sink, and seeders for the CONVERGED analytic row —
//! `timesheet.timesheets`, owned by the timesheet module — plus its approval cycle
//! (`timesheet.timesheet_approvals`, the billability gate). Fresh random ids per test.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use backbone_project::application::service::project_events::{ProjectEvent, ProjectEventSink};
pub use backbone_project::application::service::project_events::LoggingSink;
use backbone_project::application::service::project_ports::{
    BillingPort, InvoiceAck, InvoiceFromTimesheetPeriod, ProjectRejected,
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

/// The billing unit as a map key: (company, project, employee, year, month).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeriodKey {
    pub company_id: Uuid,
    pub project_id: Uuid,
    pub employee_id: Uuid,
    pub year: i32,
    pub month: i32,
}
impl PeriodKey {
    pub fn of(company_id: Uuid, project_id: Uuid, employee_id: Uuid, year: i32, month: i32) -> Self {
        Self { company_id, project_id, employee_id, year, month }
    }
    /// The stable invoice number the billing ADAPTER derives for a period bill
    /// (the module's DTOs deliberately carry no number — numbering is adapter duty).
    pub fn number(&self) -> String {
        format!("TS-{}-{}-{:04}{:02}", self.project_id, self.employee_id, self.year, self.month)
    }
}

/// A fake billing seam that creates a service invoice idempotently per PERIOD slice, and models the
/// adapter's reversal duty: [`Self::reverse`] forgets the period so a re-bill mints a fresh invoice
/// (the -R{n} renumbering a real adapter performs after a credit note).
#[derive(Clone, Default)]
pub struct FakeBilling {
    pub invoiced: Arc<Mutex<HashMap<PeriodKey, Uuid>>>,
    pub calls: Arc<Mutex<u32>>,
}
impl FakeBilling {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn invoice_count(&self) -> u32 {
        *self.calls.lock().unwrap()
    }
    /// Simulate the credit-note half: the period's invoice is gone, so the next bill of this
    /// slice must mint a NEW invoice (a real adapter numbers it `{base}-R{n}`).
    pub fn reverse(&self, k: &PeriodKey) {
        self.invoiced.lock().unwrap().remove(k);
    }
}
#[async_trait::async_trait]
impl BillingPort for FakeBilling {
    async fn create_service_invoice(
        &self,
        req: &InvoiceFromTimesheetPeriod,
    ) -> Result<InvoiceAck, ProjectRejected> {
        *self.calls.lock().unwrap() += 1;
        let k = PeriodKey::of(
            req.company_id, req.project_id, req.employee_id, req.year, req.month,
        );
        let mut m = self.invoiced.lock().unwrap();
        let id = *m.entry(k).or_insert_with(Uuid::new_v4);
        Ok(InvoiceAck { invoice_id: id })
    }
}

/// ACL over the REAL backbone-billing: build a service Sales Invoice from a billable period slice,
/// idempotent per slice via the stable number `TS-{project}-{employee}-{YYYYMM}`. On a duplicate
/// number, look up the existing invoice — and if NO live converged analytic row carries it anymore
/// (the link was cleared by a reversal), the number is STALE: retry with `-R{n}` so a re-bill after
/// a credit note mints a fresh invoice instead of double-linking the dead one.
pub struct RealBilling {
    pub svc: backbone_billing::application::service::billing_write_service::BillingWriteService,
    pub pool: PgPool,
}
impl RealBilling {
    fn try_number(k: &PeriodKey, attempt: usize) -> String {
        if attempt == 0 {
            k.number()
        } else {
            format!("{}-R{}", k.number(), attempt)
        }
    }
}
#[async_trait::async_trait]
impl BillingPort for RealBilling {
    async fn create_service_invoice(
        &self,
        req: &InvoiceFromTimesheetPeriod,
    ) -> Result<InvoiceAck, ProjectRejected> {
        use backbone_billing::application::service::billing_write_service::{
            NewInvoiceLine, NewSalesInvoice,
        };
        let k = PeriodKey::of(
            req.company_id, req.project_id, req.employee_id, req.year, req.month,
        );
        for attempt in 0..9usize {
            let number = Self::try_number(&k, attempt);
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
                    payment_term_id: None,
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
                            tax_template_id: None,
                        })
                        .collect(),
                    tax_lines: vec![],
                })
                .await;
            match res {
                Ok(id) => return Ok(InvoiceAck { invoice_id: id }),
                Err(_) => {
                    // Duplicate number (or a real failure): resolve which.
                    let existing: Option<Uuid> = sqlx::query_scalar(
                        "SELECT id FROM billing.sales_invoices WHERE invoice_number=$1")
                        .bind(&number)
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(|e| ProjectRejected {
                            code: "invoice_lookup_failed".into(),
                            message: e.to_string(),
                        })?;
                    let Some(id) = existing else {
                        return Err(ProjectRejected {
                            code: "invoice_create_failed".into(),
                            message: format!("billing refused number {}", number),
                        });
                    };
                    // Stale-link check: does any LIVE converged row still carry this invoice?
                    let linked: i64 = sqlx::query_scalar(
                        r#"SELECT count(*) FROM timesheet.timesheets
                           WHERE invoice_id=$1 AND (metadata->>'deleted_at') IS NULL"#)
                        .bind(id)
                        .fetch_one(&self.pool)
                        .await
                        .map_err(|e| ProjectRejected {
                            code: "link_probe_failed".into(),
                            message: e.to_string(),
                        })?;
                    if linked > 0 {
                        return Ok(InvoiceAck { invoice_id: id }); // the live bill of this slice
                    }
                    // Stale number (its slice was reversed): fall through to -R{n+1}.
                }
            }
        }
        Err(ProjectRejected {
            code: "invoice_retry_exhausted".into(),
            message: "could not derive a free invoice number after 9 retries".into(),
        })
    }
}

/// Seed one converged analytic row into `timesheet.timesheets` (the timesheet module's table —
/// tests write it directly because the converged write path belongs to that module's guarded
/// surface). Amounts are stored POSITIVE: billable only when `is_billable`, cost always.
/// Returns the row id.
pub async fn seed_row(
    pool: &PgPool,
    company: Uuid,
    employee: Uuid,
    project: Uuid,
    task: Option<Uuid>,
    year: i32,
    month: i32,
    day: u32,
    hours: Decimal,
    billing_rate: Decimal,
    costing_rate: Decimal,
    is_billable: bool,
    activity: Option<Uuid>,
) -> Uuid {
    let billable_amount = if is_billable {
        (hours * billing_rate).round_dp(2)
    } else {
        Decimal::ZERO
    };
    let costing_amount = (hours * costing_rate).round_dp(2);
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO timesheet.timesheets
             (company_id, employee_id, project_id, task_id, year, month, date, remark,
              entry_type, unit_amount, currency, activity_type_id, billing_rate, costing_rate,
              is_billable, billable_amount, costing_amount)
           VALUES ($1,$2,$3,$4,$5,$6,make_date($5,$6,$7),'seeded','work',$8,'IDR',$9,$10,$11,$12,$13,$14)
           RETURNING id"#,
    )
    .bind(company)
    .bind(employee)
    .bind(project)
    .bind(task)
    .bind(year)
    .bind(month)
    .bind(day as i32)
    .bind(hours)
    .bind(activity)
    .bind(billing_rate)
    .bind(costing_rate)
    .bind(is_billable)
    .bind(billable_amount)
    .bind(costing_amount)
    .fetch_one(pool)
    .await
    .expect("seed converged row");
    id
}

/// Seed the covering approval cycle for (company, employee, year, month) — THE billability gate.
/// One live cycle per key is enforced by the timesheet module's partial unique index.
pub async fn seed_approval(
    pool: &PgPool,
    company: Uuid,
    employee: Uuid,
    year: i32,
    month: i32,
    status: &str,
) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO timesheet.timesheet_approvals
             (company_id, employee_id, year, month, status)
           VALUES ($1,$2,$3,$4,$5::timesheet_approval_status)
           RETURNING id"#,
    )
    .bind(company)
    .bind(employee)
    .bind(year)
    .bind(month)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("seed approval cycle");
    id
}
