// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// The hand-written project SQL's parameter/projection types (see the `*_repository` modules, all
// declared `user_owned` in metaphor.codegen.yaml). `converged_timesheet_repository` is declared
// HERE (inside the CUSTOM block) because the module declaration list above is regenerated —
// the repo itself is user-owned and never regenerated.
pub mod converged_timesheet_repository;
pub use converged_timesheet_repository::{
    ClearInvoiceRow, ConvergedBillableLine, ConvergedTimesheetRepository, ProjectFinancialSums,
    StampInvoiceRow,
};
pub use project_repository::{NewProjectRow, ProjectCompletionRow, ProjectFinancialsRow};
pub use project_template_repository::TemplateRow;
pub use project_template_task_repository::TemplateTaskRow;
pub use task_repository::NewTaskRow;
