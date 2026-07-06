//! Project domain events (hand-authored, user-owned) — the public extension surface.
//!
//! backbone-project posts NO GL. Its one outbound seam points at billing (`TimesheetBilled` — a Sales
//! Invoice now exists for the delivered time); the rest are read-side funnel signals for cost/margin
//! analytics. A consuming service supplies the sink (bus, outbox, …).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Time was logged against a project (a timesheet was created/submitted with its roll-ups).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeLogged {
    pub timesheet_id: Uuid,
    pub project_id: Uuid,
    pub company_id: Uuid,
    pub billable_amount: Decimal,
    pub costing_amount: Decimal,
}

/// A submitted timesheet was handed to billing, which created the Sales Invoice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimesheetBilled {
    pub timesheet_id: Uuid,
    pub project_id: Uuid,
    pub company_id: Uuid,
    pub invoice_id: Uuid,
    pub billed_amount: Decimal,
}

/// A submitted timesheet was cancelled — its billable + costing were reversed off the project roll-up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimesheetCancelled {
    pub timesheet_id: Uuid,
    pub project_id: Uuid,
    pub company_id: Uuid,
    pub reversed_billable: Decimal,
    pub reversed_costing: Decimal,
}

/// A project was completed (delivery closed).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectCompleted {
    pub project_id: Uuid,
    pub company_id: Uuid,
    pub total_billable_amount: Decimal,
    pub total_costing_amount: Decimal,
}

/// The project domain-event union.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ProjectEvent {
    TimeLogged(TimeLogged),
    TimesheetBilled(TimesheetBilled),
    TimesheetCancelled(TimesheetCancelled),
    ProjectCompleted(ProjectCompleted),
}

/// Sink the write path publishes to. A consuming service supplies its own (bus, outbox, …).
pub trait ProjectEventSink: Send + Sync {
    fn publish(&self, event: &ProjectEvent);
}

/// A no-op/logging sink for tests and single-process composition.
#[derive(Debug, Default, Clone)]
pub struct LoggingSink;

impl ProjectEventSink for LoggingSink {
    fn publish(&self, event: &ProjectEvent) {
        tracing::info!(?event, "project event");
    }
}
