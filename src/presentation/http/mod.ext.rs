// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

/// Guarded route composition for production.
///
/// This module owns NO analytic-row tables anymore — logged effort lives in the timesheet
/// module's `timesheet.timesheets` (the converged row), reached only through
/// `ProjectWriteService` verbs (the billing exit, the derived financial refresh, the hybrid
/// task-status derivation). What mounts here is this module's own tables:
///
/// - The aggregate-leaf child **ProjectTemplateTask** is mounted **READ-ONLY**: its writes go
///   through template instantiation, so generic child CRUD could bypass the fork shape.
/// - Tasks stay full generic CRUD for reads/masters, but the validated status writes
///   (the hybrid latch/derive) and guarded deletes go through `module.write_service`.
///
/// Prefer this over `ProjectModule::all_crud_routes()`, which mounts full unvalidated CRUD
/// on every entity and is reserved for trusted / admin / seeding.
pub fn create_guarded_project_routes(module: &crate::ProjectModule) -> axum::Router {
    axum::Router::new()
        // Aggregate-leaf child: READ-ONLY (writes via template instantiation).
        .merge(create_project_template_task_read_routes(module.project_template_task_service.clone()))
        // Aggregate roots + masters: full generic CRUD.
        .merge(create_activity_type_routes(module.activity_type_service.clone()))
        .merge(create_project_routes(module.project_service.clone()))
        .merge(create_project_template_routes(module.project_template_service.clone()))
        .merge(create_task_routes(module.task_service.clone()))
}
