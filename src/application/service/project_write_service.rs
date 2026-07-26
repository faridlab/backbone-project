//! The hand-authored project write path (user-owned; survives regen).
//!
//! Project delivery + time tracking + the ONE outbound seam. Projects posts NO GL: a submitted,
//! billable Timesheet is handed to backbone-billing through `BillingPort` (zero normal Cargo edge),
//! which builds the Sales Invoice and posts (Dr A/R · Cr Service Revenue · Cr PPN). Projects only records
//! the echoed `invoice_id` and marks the timesheet BILLED. The hand-off is idempotent + transition-gated
//! (a timesheet bills once). Money is IDR, 2dp, half-away-from-zero.
//!
//! **This file is the hub:** it holds the module's vocabulary (input structs, outcome, error) and the
//! service constructor. The rest of the write surface is chunked into focused siblings, each an
//! `impl ProjectWriteService` block over these same types:
//!
//! - [`super::project_lifecycle`] — open / instantiate-from-template / complete a project.
//! - [`super::project_task`] — add a task (adjacency-list tree) and advance its status / progress.
//! - [`super::project_timesheet`] — log effort (`log_time`) and `cancel_timesheet` (the reverse gear).
//! - [`super::project_billing`] — the ONE outbound seam: `bill_timesheet` hands a billable timesheet
//!   to backbone-billing and transition-gates `submitted → billed`.

use rust_decimal::{Decimal, RoundingStrategy};
use sqlx::PgPool;
use uuid::Uuid;

use crate::infrastructure::persistence::{
    ActivityTypeRepository, ProjectRepository, ProjectTemplateRepository,
    ProjectTemplateTaskRepository, TaskRepository, TimesheetDetailRepository, TimesheetRepository,
};

pub(super) fn money(v: Decimal) -> Decimal {
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
    pub(super) pool: PgPool,
    pub(super) projects: ProjectRepository,
    pub(super) tasks: TaskRepository,
    pub(super) templates: ProjectTemplateRepository,
    pub(super) template_tasks: ProjectTemplateTaskRepository,
    pub(super) timesheets: TimesheetRepository,
    pub(super) timesheet_details: TimesheetDetailRepository,
    pub(super) activity_types: ActivityTypeRepository,
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
}
