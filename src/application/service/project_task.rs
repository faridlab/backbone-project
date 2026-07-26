//! Task writes (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk over the vocabulary in [`super::project_write_service`]: add a
//! task to a project's adjacency-list tree (parent must be in the same project), and advance a task's
//! status / progress. Both ride the RLS ID-only pattern (ADR-0008): the request-dedicated connection
//! carries the caller's `app.company_id`, so a cross-tenant project/task is simply not found.
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on `TaskRepository` and
//! `ProjectRepository`.

use backbone_orm::company_scope;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::infrastructure::persistence::NewTaskRow;

use super::project_write_service::{NewTask, ProjectError, ProjectWriteService};

impl ProjectWriteService {
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
}
