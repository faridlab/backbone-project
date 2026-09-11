//! Delete guards (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk: guarded soft-delete for project and task. A project or task
//! that live converged analytic rows still reference refuses loudly — the rows live in the timesheet
//! module's schema and survive the delete, so deleting their anchor would dangle them. A database FK
//! cannot cross module schemas by design (modules compose via serialized ports), so the service
//! guard IS the enforcement — declared G-PRJ-1/G-PRJ-2 in schema/hooks/convergence_guards.hook.yaml.
//!
//! The generic CRUD soft-delete surface (`all_crud_routes`) remains the trusted/admin escape hatch;
//! this verb is the validated path a composing host mounts.
//!
//! Tenant-agnostic (ADR-0029): no scope read precedes the guard — the probe runs unscoped and the
//! soft-delete runs on a plain tx; if the composing service has bound an ambient org scope it is
//! relayed onto that tx so the composed fence sees it.
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on
//! `ConvergedTimesheetRepository` (the EXISTS probes) and `ProjectRepository`/`TaskRepository`
//! (the soft-deletes); the probe and the delete run on ONE tx so a row written in between cannot
//! slip past the guard.

use uuid::Uuid;

use super::project_write_service::{relay_ambient_scope, ProjectError, ProjectWriteService};

impl ProjectWriteService {
    /// Soft-delete a project — refused while live converged analytic rows reference it.
    pub async fn delete_project(&self, project_id: Uuid) -> Result<(), ProjectError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let live = self.rows.count_live_rows_for_project(&self.pool, project_id).await?;
        if live > 0 {
            tx.rollback().await?;
            return Err(ProjectError::Guarded(
                "project still has logged time — delete or move the analytic rows first",
            ));
        }
        let moved = self.projects.soft_delete(&mut tx, project_id).await?;
        if moved != 1 {
            tx.rollback().await?;
            return Err(ProjectError::NotFound("project"));
        }
        tx.commit().await?;
        Ok(())
    }

    /// Soft-delete a task — refused while live converged analytic rows reference it.
    pub async fn delete_task(&self, task_id: Uuid) -> Result<(), ProjectError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let live = self.rows.count_live_rows_for_task(&self.pool, task_id).await?;
        if live > 0 {
            tx.rollback().await?;
            return Err(ProjectError::Guarded(
                "task still has logged time — delete or move the analytic rows first",
            ));
        }
        let moved = self.tasks.soft_delete(&mut tx, task_id).await?;
        if moved != 1 {
            tx.rollback().await?;
            return Err(ProjectError::NotFound("task"));
        }
        tx.commit().await?;
        Ok(())
    }
}
