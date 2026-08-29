//! The hand-authored project write path (user-owned; survives regen).
//!
//! Project delivery over the CONVERGED analytic row: logged effort lives in
//! `timesheet.timesheets` (the timesheet module's table — this module owns no timesheet table of
//! its own). What remains here is the derived half: the project's cost/billing roll-ups are
//! refreshed from that row by an explicit verb, the billing exit hands an APPROVED
//! (project, employee, year, month) slice to backbone-billing through `BillingPort` (zero normal
//! Cargo edge), and a confirmed sales order can mint its service delivery (project + tasks) here,
//! idempotently per order line. Projects posts NO GL. Money is IDR; amounts arrive as
//! plain-stored snapshots on the converged row (the timesheet module's write path rounds), and
//! this module's sums never re-round a stored amount.
//!
//! **This file is the hub:** it holds the module's vocabulary (input structs, outcomes, error) and
//! the service constructor. The rest of the write surface is chunked into focused siblings, each an
//! `impl ProjectWriteService` block over these same types:
//!
//! - [`super::project_lifecycle`] — open a project / instantiate one from a template / complete it.
//! - [`super::project_task`] — add a task (adjacency-list tree) and drive its hybrid status
//!   (derive from live converged rows; hand-set close latches).
//! - [`super::project_financials`] — the derived reads: `refresh_project_financials` (recompute +
//!   store) and `project_financials` (plain column read).
//! - [`super::project_billing`] — the billing exit: `bill_timesheet_period` (approved slice →
//!   Sales Invoice, at most once) and `unbill_invoice` (the reversal).
//! - [`super::project_service_delivery`] — `mint_service_delivery`: the per-rung confirm mint.
//! - [`super::project_delete_guards`] — guarded soft-delete for project/task (refuses while live
//!   converged rows reference them).

use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::infrastructure::persistence::{
    ConvergedTimesheetRepository, ProjectRepository, ProjectTemplateRepository,
    ProjectTemplateTaskRepository, TaskRepository,
};

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
    #[error("guarded: {0}")]
    Guarded(&'static str),
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

/// The derived financial triple of a project — plain stored columns on `project.projects`,
/// refreshed from the converged analytic rows by [`ProjectWriteService::refresh_project_financials`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectFinancials {
    pub total_costing_amount: Decimal,
    pub total_billable_amount: Decimal,
    pub total_billed_amount: Decimal,
}

/// What the billing exit hands back: the Sales Invoice id, the billed total, and whether this
/// call was the one that billed (a repeat of the same period slice is a no-op).
#[derive(Debug, Clone, PartialEq)]
pub struct BillPeriodOutcome {
    pub invoice_id: Uuid,
    pub amount: Decimal,
    pub already: bool,
}

/// The service-tracking rung a confirmed sales-order line carries — the mint's behavior selector.
/// Mirrors the wire vocabulary the selling module's fulfillment port publishes; duplicated here by
/// design (ports are serialized contracts, not shared types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceTrackingRung {
    /// One task per line, under a fixed global project.
    TaskGlobalProject,
    /// One project per ORDER, plus one task per line.
    TaskInProject,
    /// The per-order project, no tasks.
    ProjectOnly,
    /// Nothing is minted (absence of a rung behaves the same way).
    Manual,
}

impl ServiceTrackingRung {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceTrackingRung::TaskGlobalProject => "task_global_project",
            ServiceTrackingRung::TaskInProject => "task_in_project",
            ServiceTrackingRung::ProjectOnly => "project_only",
            ServiceTrackingRung::Manual => "manual",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "task_global_project" => Some(ServiceTrackingRung::TaskGlobalProject),
            "task_in_project" => Some(ServiceTrackingRung::TaskInProject),
            "project_only" => Some(ServiceTrackingRung::ProjectOnly),
            "manual" => Some(ServiceTrackingRung::Manual),
            _ => None,
        }
    }
}

/// One service line of a confirmed order, as the fulfillment port delivers it.
pub struct ServiceDeliveryLine {
    pub sale_line_id: Uuid,
    pub item_id: Uuid,
    pub quantity: Decimal,
    pub description: Option<String>,
    pub rung: ServiceTrackingRung,
    /// The fixed project for `task_global_project` (refuses when absent).
    pub fixed_project_id: Option<Uuid>,
    /// The blueprint for `task_in_project` / `project_only` forking.
    pub template_id: Option<Uuid>,
}

/// A confirmed order's service-delivery mint request.
pub struct ServiceDeliveryRequest {
    pub order_id: Uuid,
    pub company_id: Uuid,
    pub customer_id: Uuid,
    pub order_number: String,
    pub currency: String,
    pub lines: Vec<ServiceDeliveryLine>,
}

/// What the mint did for one order line: `minted` is true when THIS call created the project/task,
/// false when the prior mint already had them (or the rung mints nothing). The ids are always the
/// stable ones a repeat confirm must observe.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceDeliveryLineOutcome {
    pub sale_line_id: Uuid,
    pub minted: bool,
    pub project_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
}

pub struct ProjectWriteService {
    pub(super) pool: PgPool,
    pub(super) projects: ProjectRepository,
    pub(super) tasks: TaskRepository,
    pub(super) templates: ProjectTemplateRepository,
    pub(super) template_tasks: ProjectTemplateTaskRepository,
    pub(super) rows: ConvergedTimesheetRepository,
}

impl ProjectWriteService {
    pub fn new(pool: PgPool) -> Self {
        let projects = ProjectRepository::new(pool.clone());
        let tasks = TaskRepository::new(pool.clone());
        let templates = ProjectTemplateRepository::new(pool.clone());
        let template_tasks = ProjectTemplateTaskRepository::new(pool.clone());
        let rows = ConvergedTimesheetRepository::new();
        Self {
            pool, projects, tasks, templates, template_tasks, rows,
        }
    }
}
