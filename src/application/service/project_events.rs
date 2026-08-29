//! Project domain events (hand-authored, user-owned) — the public extension surface.
//!
//! backbone-project posts NO GL. Its one outbound seam points at billing: an APPROVED
//! (project, employee, year, month) slice of the converged analytic row was handed to billing
//! (`TimesheetBilled` — a Sales Invoice now exists), or such an invoice was reversed
//! (`TimesheetBillingReversed` — the rows re-opened). The rest are read-side funnel signals for
//! cost/margin analytics. A consuming service supplies the sink (bus, outbox, …).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An approved (project, employee, year, month) slice of the converged analytic rows was handed
/// to billing, which created the Sales Invoice. The rows now carry the `invoice_id` link.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimesheetBilled {
    pub project_id: Uuid,
    pub employee_id: Uuid,
    pub company_id: Uuid,
    pub year: i32,
    pub month: i32,
    pub invoice_id: Uuid,
    pub billed_amount: Decimal,
}

/// A timesheet-based Sales Invoice was reversed (credited): the `invoice_id` link was cleared off
/// its rows, which re-open for editing and re-billing, and the project's billed total rolled down.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimesheetBillingReversed {
    pub project_id: Uuid,
    pub company_id: Uuid,
    pub invoice_id: Uuid,
    pub credited_amount: Decimal,
    pub rows_reopened: u64,
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
    TimesheetBilled(TimesheetBilled),
    TimesheetBillingReversed(TimesheetBillingReversed),
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
