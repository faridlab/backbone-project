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

use uuid::Uuid;

use super::project_write_service::{relay_ambient_scope, ProjectError, ProjectFinancials, ProjectWriteService};

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
    /// Tenant-agnostic (ADR-0029): the sums and the write run on one plain tx; if the composing
    /// service has bound an ambient org scope it is relayed onto that tx so the composed fence
    /// sees it.
    pub async fn refresh_project_financials(
        &self,
        project_id: Uuid,
    ) -> Result<ProjectFinancials, ProjectError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let sums = self.rows.sum_project_financials(&mut tx, project_id).await?;
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
    }

    /// Read the project's stored financial triple — the plain-column read (no live compute, ever).
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
