// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

pub mod project_events;
pub mod project_ports;
pub mod project_write_service;
// The write surface, chunked: each is an `impl ProjectWriteService` block over the vocabulary that
// stays in `project_write_service` (so the `project_write_service::{NewProject, ...}` import paths
// are unchanged).
pub mod project_lifecycle;
pub mod project_task;
pub mod project_financials;
pub mod project_billing;
pub mod project_service_delivery;
pub mod project_delete_guards;
pub use project_events::{
    LoggingSink, ProjectCompleted, ProjectEvent, ProjectEventSink, TimesheetBilled,
    TimesheetBillingReversed,
};
pub use project_ports::{
    BillingPort, InvoiceAck, InvoiceFromTimesheetPeriod, InvoiceLineFromTimesheet, ProjectRejected,
};
pub use project_write_service::{
    BillPeriodOutcome, NewProject, NewTask, ProjectError, ProjectFinancials, ProjectWriteService,
    ServiceDeliveryLine, ServiceDeliveryLineOutcome, ServiceDeliveryRequest, ServiceTrackingRung,
};
