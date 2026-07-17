//! The hand-authored project write path (user-owned; survives regen).
//!
//! Project delivery + time tracking + the ONE outbound seam. Projects posts NO GL: a submitted,
//! billable Timesheet is handed to backbone-billing through `BillingPort` (zero normal Cargo edge),
//! which builds the Sales Invoice and posts (Dr A/R · Cr Service Revenue · Cr PPN). Projects only records
//! the echoed `invoice_id` and marks the timesheet BILLED. The hand-off is idempotent + transition-gated
//! (a timesheet bills once). Money is IDR, 2dp, half-away-from-zero.

use backbone_orm::company_scope;
use rust_decimal::{Decimal, RoundingStrategy};
use sqlx::PgPool;
use uuid::Uuid;

use crate::infrastructure::persistence::{
    ActivityTypeRepository, NewProjectRow, NewTaskRow, NewTimesheetDetailRow, NewTimesheetRow,
    ProjectRepository, ProjectTemplateRepository, ProjectTemplateTaskRepository, TaskRepository,
    TimesheetDetailRepository, TimesheetRepository,
};

use super::project_events::*;
use super::project_ports::*;

fn money(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("invalid state: {0}")]
    InvalidState(&'static str),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("billing rejected: {0}")]
    BillingRejected(String),
}

pub struct NewProject {
    pub company_id: Uuid,
    pub project_name: String,
    pub project_type: String, // project_type variant: external | internal
    pub customer_id: Option<Uuid>,
    pub source_so_id: Option<Uuid>,
    pub currency: Option<String>,
}

pub struct NewTask {
    pub project_id: Uuid,
    pub parent_task_id: Option<Uuid>,
    pub subject: String,
    pub task_type: Option<String>,
    pub expected_time: Decimal,
}

pub struct NewTimeLine {
    pub activity_type_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub description: Option<String>,
    pub hours: Decimal,
    /// Explicit rate; when None and an activity type is given, the activity's default is snapshotted.
    pub billing_rate: Option<Decimal>,
    pub costing_rate: Option<Decimal>,
    pub is_billable: bool,
}

pub struct NewTimesheet {
    pub employee_id: Option<Uuid>,
    pub currency: Option<String>,
    pub lines: Vec<NewTimeLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BillOutcome {
    pub invoice_id: Uuid,
    pub amount: Decimal,
    pub already: bool,
}

pub struct ProjectWriteService {
    pool: PgPool,
    projects: ProjectRepository,
    tasks: TaskRepository,
    templates: ProjectTemplateRepository,
    template_tasks: ProjectTemplateTaskRepository,
    timesheets: TimesheetRepository,
    timesheet_details: TimesheetDetailRepository,
    activity_types: ActivityTypeRepository,
}

impl ProjectWriteService {
    pub fn new(pool: PgPool) -> Self {
        let projects = ProjectRepository::new(pool.clone());
        let tasks = TaskRepository::new(pool.clone());
        let templates = ProjectTemplateRepository::new(pool.clone());
        let template_tasks = ProjectTemplateTaskRepository::new(pool.clone());
        let timesheets = TimesheetRepository::new(pool.clone());
        let timesheet_details = TimesheetDetailRepository::new(pool.clone());
        let activity_types = ActivityTypeRepository::new(pool.clone());
        Self {
            pool, projects, tasks, templates, template_tasks, timesheets, timesheet_details,
            activity_types,
        }
    }

    /// Open a project.
    pub async fn create_project(&self, p: NewProject) -> Result<Uuid, ProjectError> {
        if p.project_name.trim().is_empty() {
            return Err(ProjectError::Invalid("project needs a name".into()));
        }
        if p.project_type == "external" && p.customer_id.is_none() {
            return Err(ProjectError::Invalid("an external (billable) project needs a customer".into()));
        }
        let id = Uuid::new_v4();
        let currency = p.currency.unwrap_or_else(|| "IDR".into());
        // RLS scope (ADR-0008): company on the DTO — bind it so the INSERT satisfies the WITH CHECK
        // fence on `app.company_id` (a non-request caller has no ambient scope otherwise).
        let company = p.company_id;
        company_scope::with_company_scope(
            Some(company),
            self.projects.insert_project(&self.pool, &NewProjectRow {
                id,
                company_id: p.company_id,
                project_name: &p.project_name,
                project_type: &p.project_type,
                customer_id: p.customer_id,
                source_so_id: p.source_so_id,
                currency: &currency,
            }),
        )
        .await?;
        Ok(id)
    }

    /// Add a task to a project (adjacency-list tree). A parent, if given, must be in the same project.
    pub async fn add_task(&self, t: NewTask) -> Result<Uuid, ProjectError> {
        if t.subject.trim().is_empty() {
            return Err(ProjectError::Invalid("task needs a subject".into()));
        }
        // RLS scope (ADR-0008), ID-only pattern: identified by the project id alone, so the lookups ride
        // the request-dedicated connection (which carries the caller's `app.company_id`) — another
        // company's project is simply not found. The INSERT then binds the company read off the row.
        let proj = self.projects.find_scope_by_id(&self.pool, t.project_id).await?
            .ok_or(ProjectError::NotFound("project"))?;
        if proj.status != "open" {
            return Err(ProjectError::InvalidState("project is not open"));
        }
        let company_id = proj.company_id;
        if let Some(parent) = t.parent_task_id {
            let ok = self.tasks.find_id_in_project(&self.pool, parent, t.project_id).await?;
            if ok.is_none() {
                return Err(ProjectError::Invalid("parent task is not in this project".into()));
            }
        }
        let id = Uuid::new_v4();
        company_scope::with_company_scope(
            Some(company_id),
            self.tasks.insert_task(&self.pool, &NewTaskRow {
                id,
                company_id,
                project_id: t.project_id,
                parent_task_id: t.parent_task_id,
                subject: &t.subject,
                task_type: t.task_type.as_deref(),
                expected_time: t.expected_time,
            }),
        )
        .await?;
        Ok(id)
    }

    /// Move a task's status / progress.
    pub async fn advance_task(
        &self,
        task_id: Uuid,
        status: &str,
        progress: Decimal,
    ) -> Result<(), ProjectError> {
        if progress < Decimal::ZERO || progress > Decimal::from(100) {
            return Err(ProjectError::Invalid("progress must be 0..100".into()));
        }
        // RLS scope (ADR-0008), ID-only pattern: no company argument — the UPDATE runs on the
        // request-dedicated connection, so RLS fences it to the caller's tenant (0 rows otherwise,
        // which the guard below already reports as not-advanceable).
        let moved = self.tasks.advance(&self.pool, task_id, status, progress).await?;
        if moved != 1 {
            return Err(ProjectError::InvalidState("task is not advanceable (open/working only)"));
        }
        Ok(())
    }

    /// Instantiate a project from a template, materializing its template tasks as real tasks
    /// (preserving `sequence` order). Returns the new project id.
    pub async fn instantiate_template(
        &self,
        template_id: Uuid,
        company_id: Uuid,
        project_name: String,
        customer_id: Option<Uuid>,
    ) -> Result<Uuid, ProjectError> {
        // RLS scope (ADR-0008): company is on the parameter — bind it for the whole instantiation, so
        // the template reads and every nested create/add write are fenced (the nested calls re-bind the
        // same company, which is a no-op).
        company_scope::with_company_scope(Some(company_id), async move {
        let tpl = self.templates.find_for_instantiate(&self.pool, template_id).await?
            .ok_or(ProjectError::NotFound("template"))?;
        if !tpl.is_active {
            return Err(ProjectError::InvalidState("template is not active"));
        }
        let project_type = tpl.project_type;
        let project_id = self
            .create_project(NewProject {
                company_id,
                project_name,
                project_type,
                customer_id,
                source_so_id: None,
                currency: None,
            })
            .await?;
        let tasks = self.template_tasks.list_by_template(&self.pool, template_id).await?;
        for row in tasks {
            self.add_task(NewTask {
                project_id,
                parent_task_id: None,
                subject: row.subject,
                task_type: row.task_type,
                expected_time: row.expected_time,
            })
            .await?;
        }
        Ok(project_id)
        })
        .await
    }

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

    /// Bill a submitted timesheet — the ONE outbound seam. Hands the billable lines to billing as a
    /// service Sales Invoice (idempotent per timesheet), then transition-gates `submitted → billed` with
    /// the echoed `invoice_id` and rolls the project's `total_billed_amount` up. Bills **at most once**.
    pub async fn bill_timesheet(
        &self,
        timesheet_id: Uuid,
        billing: &dyn BillingPort,
        sink: &dyn ProjectEventSink,
    ) -> Result<BillOutcome, ProjectError> {
        // RLS scope (ADR-0008), ID-only pattern: identified by the timesheet id alone. Under HTTP the
        // request-dedicated connection carries the scope. When driven by an EVENT/job, the caller must
        // wrap this in `with_company_scope(Some(company_id))` — otherwise these reads fail closed.
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
        let company_id = ts.company_id;
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
        // RLS scope (ADR-0008): bind the company read off the timesheet row onto this tx, so the
        // billed-flip + the project roll-up both pass the fence.
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
    }

    /// Complete a project — terminal. Emits `ProjectCompleted` with the final cost/billable roll-ups.
    pub async fn complete_project(
        &self,
        project_id: Uuid,
        sink: &dyn ProjectEventSink,
    ) -> Result<(), ProjectError> {
        // RLS scope (ADR-0008), ID-only pattern: no company argument — this UPDATE…RETURNING rides the
        // request-dedicated connection, so RLS fences it to the caller's tenant (another company's
        // project matches 0 rows, reported below as not-open).
        let row = self.projects.complete(&self.pool, project_id).await?;
        match row {
            Some(r) => {
                sink.publish(&ProjectEvent::ProjectCompleted(ProjectCompleted {
                    project_id,
                    company_id: r.company_id,
                    total_billable_amount: r.total_billable_amount,
                    total_costing_amount: r.total_costing_amount,
                }));
                Ok(())
            }
            None => Err(ProjectError::InvalidState("project is not open")),
        }
    }
}
