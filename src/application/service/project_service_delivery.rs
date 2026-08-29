//! The confirm-mint seam (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk: when the selling module CONFIRMS a sales order, its
//! fulfillment port hands the service lines here, and this verb mints the tracking shapes they
//! ask for — idempotently, so a repeated confirm (or an event replay) mints nothing new:
//!
//! - `task_global_project` — one task per line, under the line's fixed global project.
//! - `task_in_project` — one project per ORDER (keyed `source_so_id`), forked from the line's
//!   template when one is given, plus one task per line.
//! - `project_only` — the per-order project, no tasks.
//! - `manual` (or no rung) — nothing.
//!
//! The idempotency is by DATABASE, not bookkeeping: the per-order project lands on the partial
//! unique index `uq_projects_source_so` and the per-line task on `uq_tasks_origin_sale_line`;
//! both inserts are `ON CONFLICT … DO NOTHING` with a re-select arm, so the second confirm of
//! the same order resolves to the first mint's ids and reports `minted: false` per line.
//!
//! The whole mint is ONE transaction: the project insert, the template-task materialization, and
//! every per-line task land or none do. Template reads happen before the tx opens (pure reads).
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on
//! `ProjectRepository` and `TaskRepository`.

use backbone_orm::company_scope;
use uuid::Uuid;

use crate::infrastructure::persistence::NewProjectRow;

use super::project_write_service::{
    ProjectError, ProjectWriteService, ServiceDeliveryLineOutcome, ServiceDeliveryRequest,
    ServiceTrackingRung,
};

impl ProjectWriteService {
    /// Mint the tracking shapes a confirmed sales order's service lines ask for — at most once per
    /// order line. Returns one outcome per input line, in input order, carrying the STABLE ids a
    /// repeat confirm observes (`minted` false there — nothing was created).
    ///
    /// RLS scope (ADR-0008): the company is on the request DTO; it is bound for the whole mint and
    /// again onto the tx, so every write satisfies the `app.company_id` fence.
    pub async fn mint_service_delivery(
        &self,
        req: &ServiceDeliveryRequest,
    ) -> Result<Vec<ServiceDeliveryLineOutcome>, ProjectError> {
        company_scope::with_company_scope(Some(req.company_id), async move {
            // Does ANY line want a per-order project? (task_in_project / project_only.)
            let wants_order_project = req.lines.iter().any(|l| {
                matches!(l.rung, ServiceTrackingRung::TaskInProject | ServiceTrackingRung::ProjectOnly)
            });

            // The fork blueprint, when a line carries one: the first template id among the
            // per-order-project lines wins (one project per ORDER, so one fork shape per order).
            let template_id = req.lines.iter().find_map(|l| match l.rung {
                ServiceTrackingRung::TaskInProject | ServiceTrackingRung::ProjectOnly => l.template_id,
                _ => None,
            });

            // Read the template BEFORE the tx opens — pure reads, no need to hold the write lock
            // through them. A named template that is missing or inactive is a broken fulfillment
            // payload: refuse the whole mint rather than fork silently without the blueprint.
            let mut template_project_type = "external".to_string();
            let mut template_tasks = Vec::new();
            if let Some(tid) = template_id {
                let tpl = self.templates.find_for_instantiate(&self.pool, tid).await?
                    .ok_or_else(|| ProjectError::Invalid(
                        "the order's service template was not found — cannot mint its delivery".into(),
                    ))?;
                if tpl.status != "active" {
                    return Err(ProjectError::InvalidState("the order's service template is not active"));
                }
                template_project_type = tpl.project_type;
                template_tasks = self.template_tasks.list_by_template(&self.pool, tid).await?;
            }

            let mut tx = self.pool.begin().await?;
            // RLS scope (ADR-0008): bind the company onto the tx — every mint write below rides it.
            company_scope::bind_company_on(&mut tx, req.company_id).await?;

            // The per-order project: at most one live project per (company, source sales order).
            // The insert IS the idempotency probe — Some(id) means THIS call created it (and owes
            // the template materialization); None means a prior mint already had it.
            let mut order_project: Option<(Uuid, bool)> = None; // (project_id, created_here)
            if wants_order_project {
                let fresh = Uuid::new_v4();
                let project_name = format!("SO {}", req.order_number);
                let inserted = self
                    .projects
                    .insert_project_for_so(&mut tx, &NewProjectRow {
                        id: fresh,
                        company_id: req.company_id,
                        project_name: &project_name,
                        project_type: &template_project_type,
                        customer_id: Some(req.customer_id),
                        source_so_id: Some(req.order_id),
                        currency: &req.currency,
                    })
                    .await?;
                match inserted {
                    Some(id) => {
                        // Materialize the blueprint's tasks into the fresh project (sequence
                        // order preserved by the listing) — inside the same tx.
                        for row in &template_tasks {
                            self.tasks
                                .insert_task_in_tx(&mut tx, &crate::infrastructure::persistence::NewTaskRow {
                                    id: Uuid::new_v4(),
                                    company_id: req.company_id,
                                    project_id: id,
                                    parent_task_id: None,
                                    subject: &row.subject,
                                    task_type: row.task_type.as_deref(),
                                    expected_time: row.expected_time,
                                })
                                .await?;
                        }
                        order_project = Some((id, true));
                    }
                    None => {
                        // Raced or repeated: the order's project already exists (the conflicting
                        // row is committed by the time DO NOTHING returns). Re-select it.
                        let id = self
                            .projects
                            .find_id_by_source_so(&self.pool, req.company_id, req.order_id)
                            .await?
                            .ok_or(ProjectError::InvalidState(
                                "the order's project exists but could not be re-read",
                            ))?;
                        order_project = Some((id, false));
                    }
                }
            }

            let mut outcomes = Vec::with_capacity(req.lines.len());
            for line in &req.lines {
                let outcome = match line.rung {
                    ServiceTrackingRung::Manual => ServiceDeliveryLineOutcome {
                        sale_line_id: line.sale_line_id,
                        minted: false,
                        project_id: None,
                        task_id: None,
                    },
                    ServiceTrackingRung::TaskGlobalProject => {
                        let fixed = line.fixed_project_id.ok_or_else(|| ProjectError::Invalid(
                            "a task_global_project line needs its fixed project".into(),
                        ))?;
                        // The fixed project must be live and in this company — the scoped read
                        // makes a cross-company project simply not found.
                        self.projects.find_scope_by_id(&self.pool, fixed).await?
                            .ok_or_else(|| ProjectError::Invalid(
                                "the line's fixed project was not found for this company".into(),
                            ))?;
                        let task = self.mint_line_task(
                            &mut tx, req, fixed, line, &format!("SO {}", req.order_number),
                        ).await?;
                        ServiceDeliveryLineOutcome {
                            sale_line_id: line.sale_line_id,
                            minted: task.0.is_some(),
                            project_id: Some(fixed),
                            task_id: task.1.or(task.0),
                        }
                    }
                    ServiceTrackingRung::TaskInProject => {
                        let (project_id, created) = order_project
                            .ok_or(ProjectError::InvalidState(
                                "the order project was not minted for a task_in_project line",
                            ))?;
                        let task = self.mint_line_task(
                            &mut tx, req, project_id, line, &format!("SO {}", req.order_number),
                        ).await?;
                        ServiceDeliveryLineOutcome {
                            sale_line_id: line.sale_line_id,
                            minted: created || task.0.is_some(),
                            project_id: Some(project_id),
                            task_id: task.1.or(task.0),
                        }
                    }
                    ServiceTrackingRung::ProjectOnly => {
                        let (project_id, created) = order_project
                            .ok_or(ProjectError::InvalidState(
                                "the order project was not minted for a project_only line",
                            ))?;
                        ServiceDeliveryLineOutcome {
                            sale_line_id: line.sale_line_id,
                            minted: created,
                            project_id: Some(project_id),
                            task_id: None,
                        }
                    }
                };
                outcomes.push(outcome);
            }

            tx.commit().await?;
            Ok(outcomes)
        })
        .await
    }

    /// Mint one line's task (the shared arm of the two task-bearing rungs). Returns
    /// `(inserted_id, reselected_id)`: exactly one is `Some` — the first when this call minted,
    /// the second when the origin-key unique index already had a task for the line.
    async fn mint_line_task(
        &self,
        tx: &mut sqlx::PgConnection,
        req: &ServiceDeliveryRequest,
        project_id: Uuid,
        line: &super::project_write_service::ServiceDeliveryLine,
        order_name: &str,
    ) -> Result<(Option<Uuid>, Option<Uuid>), ProjectError> {
        // Planned hours mirror the ordered quantity (services are bought in hour units); the
        // line's description is the subject, falling back to the order's name.
        let subject = line.description.clone().unwrap_or_else(|| order_name.to_string());
        let inserted = self
            .tasks
            .insert_task_for_sale_line(
                tx,
                Uuid::new_v4(),
                req.company_id,
                project_id,
                line.sale_line_id,
                &subject,
            )
            .await?;
        if let Some(id) = inserted {
            return Ok((Some(id), None));
        }
        let existing = self
            .tasks
            .find_id_by_origin_sale_line(tx, req.company_id, line.sale_line_id)
            .await?;
        Ok((None, existing))
    }
}
