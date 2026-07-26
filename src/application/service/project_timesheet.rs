//! Timesheet writes (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk over the vocabulary in [`super::project_write_service`]: log
//! effort against a project as a SUBMITTED timesheet (each line snapshots its rates, computes amounts,
//! and rolls the project's cost/billable totals up in the SAME transaction), and cancel a mis-entry
//! timesheet (the reverse gear — reverses the billable + costing off the roll-up, status-gated).
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on `TimesheetRepository`,
//! `TimesheetDetailRepository`, `ActivityTypeRepository`, and `ProjectRepository`; the insert + roll-up
//! repo methods take THIS service's transaction so a timesheet + its roll-up commit as one unit.

use backbone_orm::company_scope;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::infrastructure::persistence::{NewTimesheetDetailRow, NewTimesheetRow};

use super::project_events::{ProjectEvent, ProjectEventSink, TimeLogged, TimesheetCancelled};
use super::project_write_service::{money, NewTimesheet, ProjectError, ProjectWriteService};

impl ProjectWriteService {
    /// Log effort against a project as a SUBMITTED timesheet. Each line snapshots its billing/costing
    /// rates (explicit, else from the activity type), computes `billable = hours·billing_rate` (billable
    /// lines only) + `costing = hours·costing_rate` (all lines), and rolls the project's cost/billable
    /// totals up in the SAME transaction. Emits `TimeLogged`.
    pub async fn log_time(
        &self,
        project_id: Uuid,
        ts: NewTimesheet,
        sink: &dyn ProjectEventSink,
    ) -> Result<Uuid, ProjectError> {
        if ts.lines.is_empty() {
            return Err(ProjectError::Invalid("a timesheet needs at least one line".into()));
        }
        // RLS scope (ADR-0008), ID-only pattern: identified by the project id alone — the read rides the
        // request-dedicated connection, and the company it returns is bound onto the tx below.
        let proj = self.projects.find_for_time_log(&self.pool, project_id).await?
            .ok_or(ProjectError::NotFound("project"))?;
        if proj.status != "open" {
            return Err(ProjectError::InvalidState("cannot log time to a closed project"));
        }
        let company_id = proj.company_id;
        let currency = ts.currency.unwrap_or(proj.currency);

        // Resolve each line's rates (explicit, else the activity type's defaults) and compute amounts.
        struct Line {
            activity_type_id: Option<Uuid>,
            task_id: Option<Uuid>,
            description: Option<String>,
            hours: Decimal,
            billing_rate: Decimal,
            costing_rate: Decimal,
            is_billable: bool,
            billable_amount: Decimal,
            costing_amount: Decimal,
        }
        let mut lines = Vec::with_capacity(ts.lines.len());
        let (mut total_hours, mut total_billable, mut total_costing) =
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        for l in &ts.lines {
            if l.hours <= Decimal::ZERO {
                return Err(ProjectError::Invalid("a timesheet line needs positive hours".into()));
            }
            let (mut billing_rate, mut costing_rate) =
                (l.billing_rate.unwrap_or(Decimal::ZERO), l.costing_rate.unwrap_or(Decimal::ZERO));
            if l.billing_rate.is_none() || l.costing_rate.is_none() {
                if let Some(at) = l.activity_type_id {
                    let rates = self.activity_types.find_rates(&self.pool, at).await?;
                    if let Some(r) = rates {
                        if l.billing_rate.is_none() { billing_rate = r.billing_rate; }
                        if l.costing_rate.is_none() { costing_rate = r.costing_rate; }
                    }
                }
            }
            let billable_amount = if l.is_billable { money(l.hours * billing_rate) } else { Decimal::ZERO };
            let costing_amount = money(l.hours * costing_rate);
            total_hours += l.hours;
            total_billable += billable_amount;
            total_costing += costing_amount;
            lines.push(Line {
                activity_type_id: l.activity_type_id, task_id: l.task_id, description: l.description.clone(),
                hours: l.hours, billing_rate, costing_rate, is_billable: l.is_billable,
                billable_amount, costing_amount,
            });
        }

        let ts_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        // RLS scope (ADR-0008): bind the company read off the project row onto this tx, so the timesheet
        // insert + roll-up satisfy the fence on every statement in it.
        company_scope::bind_company_on(&mut tx, company_id).await?;
        self.timesheets.insert_timesheet(&mut tx, &NewTimesheetRow {
            id: ts_id,
            company_id,
            project_id,
            employee_id: ts.employee_id,
            currency: &currency,
            total_hours,
            total_billable_amount: total_billable,
            total_costing_amount: total_costing,
        }).await?;
        for l in &lines {
            self.timesheet_details.insert_detail(&mut tx, &NewTimesheetDetailRow {
                id: Uuid::new_v4(),
                company_id,
                timesheet_id: ts_id,
                activity_type_id: l.activity_type_id,
                task_id: l.task_id,
                description: l.description.as_deref(),
                hours: l.hours,
                billing_rate: l.billing_rate,
                costing_rate: l.costing_rate,
                is_billable: l.is_billable,
                billable_amount: l.billable_amount,
                costing_amount: l.costing_amount,
            }).await?;
        }
        // Roll the project's cost/billable visibility up (billed happens later, at bill_timesheet).
        // Runs on THIS tx, so its open-status guard settles the race against a concurrent
        // `complete_project` and rolls the whole timesheet insert back if it lost — see
        // `ProjectRepository::roll_up_open` for why that guard is load-bearing.
        let rolled = self.projects
            .roll_up_open(&mut tx, project_id, total_billable, total_costing)
            .await?;
        if rolled != 1 {
            tx.rollback().await?;
            return Err(ProjectError::InvalidState("cannot log time to a closed project"));
        }
        tx.commit().await?;
        sink.publish(&ProjectEvent::TimeLogged(TimeLogged {
            timesheet_id: ts_id, project_id, company_id,
            billable_amount: total_billable, costing_amount: total_costing,
        }));
        Ok(ts_id)
    }

    /// Cancel a submitted timesheet (a mis-entry — wrong hours/project/billable flag) and REVERSE its
    /// billable + costing off the project roll-up, in one status-gated tx. This is the reverse gear the
    /// roll-up needs: without it a single typo poisons the project's margin number forever (completeness
    /// council 2026-07-06). Idempotent (a re-cancel is a no-op); a BILLED timesheet cannot be cancelled
    /// (reverse via billing instead); the reversal is gated on an OPEN project so a completed project's
    /// "final" totals stay final. Emits `TimesheetCancelled`.
    pub async fn cancel_timesheet(
        &self,
        timesheet_id: Uuid,
        sink: &dyn ProjectEventSink,
    ) -> Result<(), ProjectError> {
        // RLS scope (ADR-0008), ID-only pattern: the read rides the request-dedicated connection; the
        // company it returns is bound onto the reversal tx below.
        let ts = self.timesheets.find_for_cancel(&self.pool, timesheet_id).await?
            .ok_or(ProjectError::NotFound("timesheet"))?;
        let status = ts.status.as_str();
        if status == "cancelled" {
            return Ok(()); // idempotent
        }
        if status == "billed" {
            return Err(ProjectError::InvalidState("a billed timesheet cannot be cancelled — reverse via billing"));
        }
        if status != "submitted" {
            return Err(ProjectError::InvalidState("only a submitted timesheet can be cancelled"));
        }
        let project_id = ts.project_id;
        let company_id = ts.company_id;
        let billable = ts.total_billable_amount;
        let costing = ts.total_costing_amount;

        let mut tx = self.pool.begin().await?;
        // RLS scope (ADR-0008): bind the company read off the timesheet row onto this tx.
        company_scope::bind_company_on(&mut tx, company_id).await?;
        // Gate: claim the cancellation exactly once (submitted → cancelled).
        let moved = self.timesheets.mark_cancelled(&mut tx, timesheet_id).await?;
        if moved != 1 {
            tx.rollback().await?;
            return Ok(()); // raced — the winner already cancelled + reversed
        }
        // Reverse the roll-up, gated on an OPEN project (a completed project's totals are final).
        let reversed = self.projects
            .reverse_roll_up_open(&mut tx, project_id, billable, costing)
            .await?;
        if reversed != 1 {
            tx.rollback().await?;
            return Err(ProjectError::InvalidState("cannot cancel time on a closed project"));
        }
        tx.commit().await?;
        sink.publish(&ProjectEvent::TimesheetCancelled(TimesheetCancelled {
            timesheet_id, project_id, company_id,
            reversed_billable: billable, reversed_costing: costing,
        }));
        Ok(())
    }
}
