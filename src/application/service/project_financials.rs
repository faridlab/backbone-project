//! The derived financial reads (hand-authored, user-owned).
//!
//! An `impl ProjectWriteService` chunk: the project's cost/billing roll-ups are DERIVED reads over
//! the converged analytic row (`timesheet.timesheets`, the timesheet module's table). They are
//! plain-stored columns on `project.projects` — the profitability surface — and the ONLY write path
//! into them is the explicit refresh verb below. Reads never compute, never reprice: a read returns
//! the stored triple as-is (TSM-1 — no live repricing reads).
//!
//! Per the module's 4-layer rule this file holds no SQL — the statements live on
//! `ConvergedTimesheetRepository` (the cross-schema sums) and `ProjectRepository` (the stored-column
//! write + read); the sums read and the write back run on ONE tx so a concurrent row write cannot
//! split them.

use backbone_orm::company_scope;
use uuid::Uuid;

use super::project_write_service::{ProjectError, ProjectFinancials, ProjectWriteService};

impl ProjectWriteService {
    /// Recompute the project's financial triple from its live converged rows and store it:
    /// `total_costing_amount = Σ costing_amount`, `total_billable_amount = Σ billable_amount`,
    /// `total_billed_amount = Σ billable_amount WHERE invoice_id IS NOT NULL`. Plain SUMs of
    /// plain-stored columns — aggregation, not repricing.
    ///
    /// Gated on an OPEN project (a completed project's "final" totals stay final — the same posture
    /// the retired incremental roll-up carried): a closed project refuses with `InvalidState`.
    /// Call this after any converged-row write that touches money; a composing host wires it onto
    /// the timesheet write events at compose time (the verb is safe to drive directly too).
    ///
    /// `company_id` scopes the whole path up front (ADR-0008): the sums and the write run on one tx
    /// with `app.company_id` bound from the parameter, satisfying BOTH schemas' fences.
    pub async fn refresh_project_financials(
        &self,
        company_id: Uuid,
        project_id: Uuid,
    ) -> Result<ProjectFinancials, ProjectError> {
        company_scope::with_company_scope(Some(company_id), async move {
            let mut tx = self.pool.begin().await?;
            // RLS scope (ADR-0008): the surrounding task-local never reaches a pooled
            // connection, so the company must be bound onto THIS transaction for the
            // cross-schema sums and the roll-up write to pass both fences.
            company_scope::bind_company_on(&mut tx, company_id).await?;
            let sums = self.rows.sum_project_financials(&mut tx, company_id, project_id).await?;
            let moved = self
                .projects
                .set_financials_open(
                    &mut tx,
                    project_id,
                    sums.total_costing_amount,
                    sums.total_billable_amount,
                    sums.total_billed_amount,
                )
                .await?;
            if moved != 1 {
                tx.rollback().await?;
                return Err(ProjectError::InvalidState("cannot refresh a closed project"));
            }
            tx.commit().await?;
            Ok(ProjectFinancials {
                total_costing_amount: sums.total_costing_amount,
                total_billable_amount: sums.total_billable_amount,
                total_billed_amount: sums.total_billed_amount,
            })
        })
        .await
    }

    /// Read the project's stored financial triple — the plain-column read (no live compute, ever).
    ///
    /// ID-only (ADR-0008): under HTTP the request-dedicated connection carries the caller's
    /// `app.company_id`; an event/job caller wraps this in `with_company_scope(Some(company_id))`
    /// or a cross-tenant project is simply not found.
    pub async fn project_financials(
        &self,
        project_id: Uuid,
    ) -> Result<ProjectFinancials, ProjectError> {
        let row = self
            .projects
            .read_financials(&self.pool, project_id)
            .await?
            .ok_or(ProjectError::NotFound("project"))?;
        Ok(ProjectFinancials {
            total_costing_amount: row.total_costing_amount,
            total_billable_amount: row.total_billable_amount,
            total_billed_amount: row.total_billed_amount,
        })
    }
}
