//! The billing seam (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk: the ONE outbound seam, now over the converged analytic row.
//! The billing unit is the APPROVED (project, employee, year, month) slice — the natural key of the
//! timesheet approval cycle. `bill_timesheet_period` gates on that cycle
//! (`timesheet.timesheet_approvals.status='approved'` is THE billability gate — the rows carry no
//! state of their own), hands the billable rows to backbone-billing as a service Sales Invoice, then
//! on ONE tx stamps the echoed `invoice_id` onto the row set and rolls the project's
//! `total_billed_amount` up. Bills **at most once** per slice: the stamp matches only rows that do
//! not carry an invoice yet, so a raced double call claims nothing and reports `already: true`.
//!
//! `unbill_invoice` is the reversal half: on a credit note it clears the `invoice_id` link off the
//! rows carrying it and rolls the credited amount back off the project's billed total, one tx — the
//! rows re-open for editing and re-billing (the invoiced-row write guard keys on
//! `invoice_id IS NOT NULL`, so a cleared row is editable again by construction).
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on
//! `ConvergedTimesheetRepository` (cross-schema, over the timesheet module's tables) and
//! `ProjectRepository`.

use backbone_orm::company_scope;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::project_events::{ProjectEvent, ProjectEventSink, TimesheetBilled, TimesheetBillingReversed};
use super::project_ports::{BillingPort, InvoiceFromTimesheetPeriod, InvoiceLineFromTimesheet};
use super::project_write_service::{BillPeriodOutcome, ProjectError, ProjectWriteService};

impl ProjectWriteService {
    /// Bill the approved (project, employee, year, month) slice of converged rows — the ONE
    /// outbound seam. Gate order: (1) the covering approval cycle is `approved`; (2) the slice has
    /// live billable rows not yet invoiced; (3) the project has a customer. Then hand the rows to
    /// billing, and on ONE tx stamp `invoice_id` onto the row set + roll the project's billed total
    /// up. Bills **at most once**; a repeat of a billed slice reports the prior invoice
    /// (`already: true`) without driving billing again.
    ///
    /// `company_id` scopes the whole path up front, resolving the chicken-and-egg where the first
    /// reads otherwise run on the pool BEFORE the company is knowable. The HTTP handler or event
    /// subscriber passes it; an event/job caller can no longer forget to scope these reads.
    pub async fn bill_timesheet_period(
        &self,
        project_id: Uuid,
        employee_id: Uuid,
        year: i32,
        month: i32,
        company_id: Uuid,
        billing: &dyn BillingPort,
        sink: &dyn ProjectEventSink,
    ) -> Result<BillPeriodOutcome, ProjectError> {
        // RLS scope (ADR-0008): company on the parameter — scope the reads + writes so they run with
        // `app.company_id` set from the first read. The stamp tx binds it explicitly below as
        // defense-in-depth (the scope wrapper stays here, in the service).
        company_scope::with_company_scope(Some(company_id), async move {
            // Gate (1) — the billability gate: the covering per-(company, employee, year, month)
            // approval cycle must be approved. The row carries NO state and NO approval of its own.
            let approval = self
                .rows
                .approval_status(&self.pool, company_id, employee_id, year, month)
                .await?;
            if approval.as_deref() != Some("approved") {
                return Err(ProjectError::Guarded(
                    "the timesheet period is not approved — approve the cycle before billing",
                ));
            }

            // Already-billed pre-check: a slice whose billable rows all carry an invoice reports the
            // prior invoice instead of cutting a second one.
            if let Some((invoice_id, amount)) = self
                .rows
                .find_invoice_for_period(&self.pool, company_id, project_id, employee_id, year, month)
                .await?
            {
                let still_open = self
                    .rows
                    .list_billable_period_lines(&self.pool, company_id, project_id, employee_id, year, month)
                    .await?;
                if still_open.is_empty() {
                    return Ok(BillPeriodOutcome { invoice_id, amount, already: true });
                }
            }

            // Gate (2) — the billable set: live, billable, not-yet-invoiced rows of the slice.
            let lines = self
                .rows
                .list_billable_period_lines(&self.pool, company_id, project_id, employee_id, year, month)
                .await?;
            if lines.is_empty() {
                return Err(ProjectError::Invalid("nothing billable in this timesheet period".into()));
            }

            // Gate (3) — somebody to bill: the project's customer (an internal project refuses).
            let customer_id: Uuid = self.projects.find_customer_id(&self.pool, project_id).await?
                .ok_or(ProjectError::Invalid("project has no customer to bill".into()))?;

            // The project's currency is the money context of its rate snapshots.
            let currency = self.projects.find_currency(&self.pool, project_id).await?;

            // Hand off to billing (idempotent per period slice — see the adapter-duty note on the
            // port module: number TS-{project}-{employee}-{YYYYMM}, -R{n} after a reversal).
            let req = InvoiceFromTimesheetPeriod {
                company_id,
                project_id,
                employee_id,
                year,
                month,
                customer_id,
                currency,
                lines: lines
                    .iter()
                    .map(|l| InvoiceLineFromTimesheet {
                        row_id: l.row_id,
                        item_id: l.activity_type_id.unwrap_or_else(Uuid::nil),
                        description: l.description.clone(),
                        hours: l.hours,
                        rate: l.billing_rate,
                    })
                    .collect(),
            };
            let ack = billing
                .create_service_invoice(&req)
                .await
                .map_err(|r| ProjectError::BillingRejected(r.code))?;

            // Claim the billing exactly once, AND roll the project's billed total up in the same tx.
            let mut tx = self.pool.begin().await?;
            // RLS scope (ADR-0008): bind the company onto this tx so the stamp + the project
            // roll-up both pass the fence (defense-in-depth alongside the surrounding scope).
            company_scope::bind_company_on(&mut tx, company_id).await?;
            let stamp = self
                .rows
                .stamp_invoice_on_period(&mut tx, company_id, project_id, employee_id, year, month, ack.invoice_id)
                .await?;
            if stamp.rows_stamped == 0 {
                // Raced: another caller billed this slice. Report the invoice the rows actually
                // carry — not the (unused) one our ack echoed.
                tx.rollback().await?;
                if let Some((invoice_id, amount)) = self
                    .rows
                    .find_invoice_for_period(&self.pool, company_id, project_id, employee_id, year, month)
                    .await?
                {
                    return Ok(BillPeriodOutcome { invoice_id, amount, already: true });
                }
                return Ok(BillPeriodOutcome {
                    invoice_id: ack.invoice_id,
                    amount: stamp.amount,
                    already: true,
                });
            }
            self.projects.add_billed(&mut tx, project_id, stamp.amount).await?;
            tx.commit().await?;
            sink.publish(&ProjectEvent::TimesheetBilled(TimesheetBilled {
                project_id,
                employee_id,
                company_id,
                year,
                month,
                invoice_id: ack.invoice_id,
                billed_amount: stamp.amount,
            }));
            Ok(BillPeriodOutcome { invoice_id: ack.invoice_id, amount: stamp.amount, already: false })
        })
        .await
    }

    /// Reverse a timesheet-based Sales Invoice (a credit note posted against it): clear the
    /// `invoice_id` link off every live row carrying it and roll the credited amount back off the
    /// project's billed total — one tx. The rows re-open for editing and re-billing; a re-bill of
    /// the slice mints a fresh invoice (`-R{n}` numbering is the billing adapter's duty).
    ///
    /// Idempotent: an invoice no row carries anymore (a repeated reversal) is a no-op. The
    /// `credited_amount` comes from the credit-note event — a composing host supplies it.
    pub async fn unbill_invoice(
        &self,
        invoice_id: Uuid,
        company_id: Uuid,
        credited_amount: Decimal,
        sink: &dyn ProjectEventSink,
    ) -> Result<(), ProjectError> {
        company_scope::with_company_scope(Some(company_id), async move {
            let mut tx = self.pool.begin().await?;
            company_scope::bind_company_on(&mut tx, company_id).await?;
            let cleared = self.rows.clear_invoice(&mut tx, company_id, invoice_id).await?;
            if cleared.rows_cleared == 0 {
                tx.rollback().await?;
                return Ok(()); // idempotent — nothing carries the link anymore
            }
            self.projects.subtract_billed(&mut tx, cleared.project_id, credited_amount).await?;
            tx.commit().await?;
            sink.publish(&ProjectEvent::TimesheetBillingReversed(TimesheetBillingReversed {
                project_id: cleared.project_id,
                company_id,
                invoice_id,
                credited_amount,
                rows_reopened: cleared.rows_cleared,
            }));
            Ok(())
        })
        .await
    }
}
