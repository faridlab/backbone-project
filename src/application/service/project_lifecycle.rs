//! Project lifecycle writes (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk over the vocabulary in [`super::project_write_service`]: open a
//! project, instantiate one from a template (materializing its tasks), and complete it (terminal). Each
//! method keeps its RLS scope wrapper (ADR-0008) — company on the DTO/parameter is bound so the write
//! satisfies the `app.company_id` fence.
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on
//! `ProjectRepository` / `ProjectTemplateRepository` / `ProjectTemplateTaskRepository`, and the
//! template's nested `create_project` / `add_task` calls reuse the lifecycle + task siblings.

use backbone_orm::company_scope;
use uuid::Uuid;

use crate::infrastructure::persistence::NewProjectRow;

use super::project_events::{ProjectCompleted, ProjectEvent, ProjectEventSink};
use super::project_write_service::{NewProject, NewTask, ProjectError, ProjectWriteService};

impl ProjectWriteService {
    /// Open a project.
    pub async fn create_project(&self, p: NewProject) -> Result<Uuid, ProjectError> {
        if p.project_name.trim().is_empty() {
            return Err(ProjectError::Invalid("project needs a name".into()));
        }
        if p.project_type == "external" && p.customer_id.is_none() {
            return Err(ProjectError::Invalid("an external (billable) project needs a customer".into()));
        }
        let id = Uuid::new_v4();
        let currency = p.currency.unwrap_or_else(|| "IDR".into());
        // RLS scope (ADR-0008): company on the DTO — bind it so the INSERT satisfies the WITH CHECK
        // fence on `app.company_id` (a non-request caller has no ambient scope otherwise).
        let company = p.company_id;
        company_scope::with_company_scope(
            Some(company),
            self.projects.insert_project(&self.pool, &NewProjectRow {
                id,
                company_id: p.company_id,
                project_name: &p.project_name,
                project_type: &p.project_type,
                customer_id: p.customer_id,
                source_so_id: p.source_so_id,
                currency: &currency,
            }),
        )
        .await?;
        Ok(id)
    }

    /// Instantiate a project from a template, materializing its template tasks as real tasks
    /// (preserving `sequence` order). Returns the new project id.
    pub async fn instantiate_template(
        &self,
        template_id: Uuid,
        company_id: Uuid,
        project_name: String,
        customer_id: Option<Uuid>,
    ) -> Result<Uuid, ProjectError> {
        // RLS scope (ADR-0008): company is on the parameter — bind it for the whole instantiation, so
        // the template reads and every nested create/add write are fenced (the nested calls re-bind the
        // same company, which is a no-op).
        company_scope::with_company_scope(Some(company_id), async move {
        let tpl = self.templates.find_for_instantiate(&self.pool, template_id).await?
            .ok_or(ProjectError::NotFound("template"))?;
        if !tpl.is_active {
            return Err(ProjectError::InvalidState("template is not active"));
        }
        let project_type = tpl.project_type;
        let project_id = self
            .create_project(NewProject {
                company_id,
                project_name,
                project_type,
                customer_id,
                source_so_id: None,
                currency: None,
            })
            .await?;
        let tasks = self.template_tasks.list_by_template(&self.pool, template_id).await?;
        for row in tasks {
            self.add_task(NewTask {
                project_id,
                parent_task_id: None,
                subject: row.subject,
                task_type: row.task_type,
                expected_time: row.expected_time,
            })
            .await?;
        }
        Ok(project_id)
        })
        .await
    }

    /// Complete a project — terminal. Emits `ProjectCompleted` with the final cost/billable roll-ups.
    pub async fn complete_project(
        &self,
        project_id: Uuid,
        sink: &dyn ProjectEventSink,
    ) -> Result<(), ProjectError> {
        // RLS scope (ADR-0008), ID-only pattern: no company argument — this UPDATE…RETURNING rides the
        // request-dedicated connection, so RLS fences it to the caller's tenant (another company's
        // project matches 0 rows, reported below as not-open).
        let row = self.projects.complete(&self.pool, project_id).await?;
        match row {
            Some(r) => {
                sink.publish(&ProjectEvent::ProjectCompleted(ProjectCompleted {
                    project_id,
                    company_id: r.company_id,
                    total_billable_amount: r.total_billable_amount,
                    total_costing_amount: r.total_costing_amount,
                }));
                Ok(())
            }
            None => Err(ProjectError::InvalidState("project is not open")),
        }
    }
}
