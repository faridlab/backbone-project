//! The billing seam (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk over the vocabulary in [`super::project_write_service`]: bill a
//! submitted timesheet — the ONE outbound seam. Hands the billable lines to backbone-billing as a
//! service Sales Invoice (idempotent per timesheet), then transition-gates `submitted → billed` with the
//! echoed `invoice_id` and rolls the project's `total_billed_amount` up. Bills **at most once**.
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on `TimesheetRepository`,
//! `TimesheetDetailRepository`, and `ProjectRepository`; the billed-flip + project roll-up run on one tx
//! so a crash between them can't double-bill.

use backbone_orm::company_scope;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::project_events::{ProjectEvent, ProjectEventSink, TimesheetBilled};
use super::project_ports::{BillingPort, InvoiceFromTimesheet, InvoiceLineFromTimesheet};
use super::project_write_service::{BillOutcome, ProjectError, ProjectWriteService};

impl ProjectWriteService {
    /// Bill a submitted timesheet — the ONE outbound seam. Hands the billable lines to billing as a
    /// service Sales Invoice (idempotent per timesheet), then transition-gates `submitted → billed` with
    /// the echoed `invoice_id` and rolls the project's `total_billed_amount` up. Bills **at most once**.
    ///
    /// `company_id` scopes the whole path up front, resolving the chicken-and-egg where the first read
    /// (`find_for_billing`) otherwise runs on the pool BEFORE the company is knowable. The HTTP handler
    /// or event subscriber passes it; an event/job caller can no longer forget to scope these reads.
    pub async fn bill_timesheet(
        &self,
        timesheet_id: Uuid,
        company_id: Uuid,
        billing: &dyn BillingPort,
        sink: &dyn ProjectEventSink,
    ) -> Result<BillOutcome, ProjectError> {
        // RLS scope (ADR-0008): company on the parameter — scope the read + writes so they run with
        // `app.company_id` set from the first read. The billed-flip tx binds it explicitly below as
        // defense-in-depth (the scope wrapper stays here, in the service).
        company_scope::with_company_scope(Some(company_id), async move {
            let ts = self.timesheets.find_for_billing(&self.pool, timesheet_id).await?
                .ok_or(ProjectError::NotFound("timesheet"))?;
            let status = ts.status.as_str();
            let amount = ts.total_billable_amount;
            if status == "billed" {
                let inv = ts.invoice_id
                    .ok_or(ProjectError::InvalidState("billed without an invoice"))?;
                return Ok(BillOutcome { invoice_id: inv, amount, already: true });
            }
            if status != "submitted" {
                return Err(ProjectError::InvalidState("only a submitted timesheet can be billed"));
            }
            if amount <= Decimal::ZERO {
                return Err(ProjectError::Invalid("nothing billable on this timesheet".into()));
            }
            let project_id = ts.project_id;
            let currency = ts.currency;

            let customer_id: Uuid = self.projects.find_customer_id(&self.pool, project_id).await?
                .ok_or(ProjectError::Invalid("project has no customer to bill".into()))?;

            let line_rows = self.timesheet_details.list_billable_lines(&self.pool, timesheet_id).await?;
            let lines: Vec<InvoiceLineFromTimesheet> = line_rows
                .into_iter()
                .map(|r| InvoiceLineFromTimesheet {
                    item_id: r.activity_type_id.unwrap_or_else(Uuid::nil),
                    description: r.description,
                    hours: r.hours,
                    rate: r.billing_rate,
                })
                .collect();
            if lines.is_empty() {
                return Err(ProjectError::Invalid("no billable lines to invoice".into()));
            }

            // Hand off to billing (idempotent per timesheet_id).
            let ack = billing
                .create_service_invoice(&InvoiceFromTimesheet {
                    company_id, project_id, timesheet_id, customer_id, currency, lines,
                })
                .await
                .map_err(|r| ProjectError::BillingRejected(r.code))?;

            // Gate: claim the billing exactly once, AND roll the project's billed total up in the same tx.
            let mut tx = self.pool.begin().await?;
            // RLS scope (ADR-0008): bind the company onto this tx so the billed-flip + the project
            // roll-up both pass the fence (defense-in-depth alongside the surrounding scope).
            company_scope::bind_company_on(&mut tx, company_id).await?;
            let moved = self.timesheets.mark_billed(&mut tx, timesheet_id, ack.invoice_id).await?;
            if moved != 1 {
                tx.rollback().await?;
                let inv = self.timesheets.fetch_invoice_id(&self.pool, timesheet_id).await?;
                return Ok(BillOutcome { invoice_id: inv, amount, already: true });
            }
            self.projects.add_billed(&mut tx, project_id, amount).await?;
            tx.commit().await?;
            sink.publish(&ProjectEvent::TimesheetBilled(TimesheetBilled {
                timesheet_id, project_id, company_id, invoice_id: ack.invoice_id, billed_amount: amount,
            }));
            Ok(BillOutcome { invoice_id: ack.invoice_id, amount, already: false })
        }).await
    }
}
