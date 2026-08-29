//! Task writes (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk: add a task to a project's adjacency-list tree (parent must
//! be in the same project), and drive the task's HYBRID status (ADR-0016 pattern 3 — a forward
//! compute with an inverse):
//!
//! - **Forward** ([`ProjectWriteService::refresh_task_status`]): a non-latched task derives
//!   `working` iff live converged analytic rows reference it, else `open`.
//! - **Inverse** ([`ProjectWriteService::set_task_status`]): a hand-set value persists; setting a
//!   closed value (`completed`/`cancelled`) LATCHES — the compute never re-derives over it, so new
//!   logged rows never silently reopen a done task; setting `open`/`working` clears the latch and
//!   re-derives at once.
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on `TaskRepository`
//! and `ConvergedTimesheetRepository` (the derivation's EXISTS probe over the timesheet module's
//! rows).

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

    /// Hand-set a task's status — the hybrid INVERSE. Setting a closed value (`completed` /
    /// `cancelled`) LATCHES: the derivation never re-derives over it (new converged rows never
    /// silently reopen a done task). Setting `open`/`working` clears the latch and re-derives at
    /// once, so a hand-set `working` only survives while the rows justify it.
    ///
    /// Returns the task's status after the write (which may differ from the requested value when
    /// the immediate re-derivation overrides it).
    ///
    /// **Named hook point:** the close transition below (either closed value) is where recurring
    /// task generation attaches when it ports — generate-the-next-occurrence rides the latch, not
    /// the derivation.
    pub async fn set_task_status(
        &self,
        task_id: Uuid,
        status: &str,
        progress: Decimal,
    ) -> Result<String, ProjectError> {
        if progress < Decimal::ZERO || progress > Decimal::from(100) {
            return Err(ProjectError::Invalid("progress must be 0..100".into()));
        }
        match status {
            "open" | "working" | "completed" | "cancelled" => {}
            _ => return Err(ProjectError::Invalid("unknown task status".into())),
        }
        // RLS scope (ADR-0008), ID-only pattern: the read rides the request-dedicated connection;
        // the company it returns is bound onto the write tx below.
        let scope = self.tasks.find_scope_by_id(&self.pool, task_id).await?
            .ok_or(ProjectError::NotFound("task"))?;
        let company_id = scope.company_id;

        company_scope::with_company_scope(Some(company_id), async move {
            let mut tx = self.pool.begin().await?;
            company_scope::bind_company_on(&mut tx, company_id).await?;
            let moved = self.tasks.set_status(&mut tx, task_id, status, progress).await?;
            if moved != 1 {
                tx.rollback().await?;
                return Err(ProjectError::NotFound("task"));
            }
            // Open/working values are NOT sticky: clear the latch by re-deriving right away.
            if status == "open" || status == "working" {
                let has_rows = self.rows.task_has_live_rows(&mut tx, company_id, task_id).await?;
                self.tasks.derive_status(&mut tx, task_id, has_rows).await?;
            }
            tx.commit().await?;
            let after = self.tasks.find_scope_by_id(&self.pool, task_id).await?
                .ok_or(ProjectError::NotFound("task"))?;
            Ok(after.status)
        })
        .await
    }

    /// Derive a task's status from the converged rows — the hybrid FORWARD compute. A non-latched
    /// task derives `working` iff live converged analytic rows reference it, else `open`; a
    /// LATCHED task (closed by hand) is skipped — its hand-set value stands.
    ///
    /// Returns the task's status after the derivation. Drive this whenever converged rows
    /// referencing the task are written or removed; a composing host wires it onto the timesheet
    /// write events at compose time (the verb is safe to drive directly too).
    pub async fn refresh_task_status(&self, task_id: Uuid) -> Result<String, ProjectError> {
        // RLS scope (ADR-0008), ID-only pattern: the read rides the request-dedicated connection;
        // the company it returns is bound onto the derivation tx below.
        let scope = self.tasks.find_scope_by_id(&self.pool, task_id).await?
            .ok_or(ProjectError::NotFound("task"))?;
        let company_id = scope.company_id;

        company_scope::with_company_scope(Some(company_id), async move {
            let mut tx = self.pool.begin().await?;
            company_scope::bind_company_on(&mut tx, company_id).await?;
            let has_rows = self.rows.task_has_live_rows(&mut tx, company_id, task_id).await?;
            self.tasks.derive_status(&mut tx, task_id, has_rows).await?;
            tx.commit().await?;
            let after = self.tasks.find_scope_by_id(&self.pool, task_id).await?
                .ok_or(ProjectError::NotFound("task"))?;
            Ok(after.status)
        })
        .await
    }
}
