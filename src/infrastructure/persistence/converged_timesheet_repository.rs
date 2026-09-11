//! Cross-schema repository over the converged analytic row (hand-authored, user-owned).
//!
//! Logged effort lives in ONE place: `timesheet.timesheets`, owned by the timesheet
//! module. This module holds NO table of its own for it — every statement here reads or
//! stamps that row cross-schema inside one host database. The module is tenant-agnostic
//! (ADR-0029): statements carry no scoping predicate of their own — pool reads ride the
//! tenant-agnostic helpers (request-dedicated connection when the caller bound one, plain
//! pool otherwise) and tx statements run on the connection the caller already relayed any
//! ambient org scope onto.
//!
//! The row carries no state and no approval of its own: `timesheet.timesheet_approvals`
//! (per employee/year/month) is the billability gate, and the per-row billing artifact is
//! the `invoice_id` link — a link, not a status. This repository holds the SQL for exactly
//! those touchpoints:
//!
//! - the approval gate read (billability),
//! - the billable period-line read + the `invoice_id` stamp (billing exit),
//! - the invoice clear (reversal),
//! - the project financial sums (derived roll-ups),
//! - live-row existence (delete guards + the hybrid task-status derivation).

use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use backbone_orm::{company_scope, org_scope};

/// One billable converged row of a period slice — what the billing exit hands to billing.
pub struct ConvergedBillableLine {
    pub row_id: Uuid,
    pub activity_type_id: Option<Uuid>,
    pub description: Option<String>,
    pub hours: Decimal,
    pub billing_rate: Decimal,
    pub billable_amount: Decimal,
}

/// What the invoice stamp learned in one statement: how many rows this call claimed, and the
/// exact billed total of exactly those rows.
pub struct StampInvoiceRow {
    pub rows_stamped: u64,
    pub amount: Decimal,
}

/// What the invoice-clear (reversal) path learned in one statement.
pub struct ClearInvoiceRow {
    pub project_id: Uuid,
    pub rows_cleared: u64,
}

/// The summed financials of a project over its live converged rows.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectFinancialSums {
    pub total_costing_amount: Decimal,
    pub total_billable_amount: Decimal,
    pub total_billed_amount: Decimal,
}

/// Stateless repository: the table belongs to the timesheet module's schema, so there is
/// no `GenericCrudRepository` to wrap — only named statements.
#[derive(Debug, Default, Clone, Copy)]
pub struct ConvergedTimesheetRepository;

pub const LIVE_ROW: &str = "(metadata->>'deleted_at') IS NULL";

impl ConvergedTimesheetRepository {
    pub fn new() -> Self {
        Self
    }
}

impl ConvergedTimesheetRepository {
    /// The billability gate: the status of the covering per-(employee, year, month)
    /// approval cycle. `Ok(None)` = no cycle row exists at all (nothing approved — fails
    /// closed).
    ///
    /// The scalar read twin lives only in the legacy `company_scope` module (ADR-0029);
    /// with no ambient legacy scope bound it runs as a plain pool read.
    pub async fn approval_status(
        &self,
        pool: &PgPool,
        employee_id: Uuid,
        year: i32,
        month: i32,
    ) -> Result<Option<String>, sqlx::Error> {
        company_scope::fetch_optional_scalar_scoped(
            pool,
            sqlx::query_scalar::<_, String>(
                r#"SELECT status::text FROM timesheet.timesheet_approvals
                   WHERE employee_id=$1 AND year=$2 AND month=$3
                     AND (metadata->>'deleted_at') IS NULL"#,
            )
            .bind(employee_id)
            .bind(year)
            .bind(month),
        )
        .await
    }

    /// The live, billable, not-yet-invoiced rows of one (project, employee, year, month)
    /// slice — the exact set the billing exit invoices. Rates are plain-stored snapshots
    /// (never recomputed here); `billable_amount > 0` implies a non-NULL billing rate.
    ///
    /// The multi-row read twin lives only in the legacy `company_scope` module (ADR-0029);
    /// with no ambient legacy scope bound it runs as a plain pool read.
    pub async fn list_billable_period_lines(
        &self,
        pool: &PgPool,
        project_id: Uuid,
        employee_id: Uuid,
        year: i32,
        month: i32,
    ) -> Result<Vec<ConvergedBillableLine>, sqlx::Error> {
        let rows = company_scope::fetch_all_rows_scoped(
            pool,
            sqlx::query(
                r#"SELECT id, activity_type_id, remark, unit_amount, billing_rate, billable_amount
                   FROM timesheet.timesheets
                   WHERE project_id=$1 AND employee_id=$2
                     AND year=$3 AND month=$4
                     AND is_billable AND billable_amount > 0 AND invoice_id IS NULL
                     AND (metadata->>'deleted_at') IS NULL
                   ORDER BY date, id"#,
            )
            .bind(project_id)
            .bind(employee_id)
            .bind(year)
            .bind(month),
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ConvergedBillableLine {
                row_id: r.get("id"),
                activity_type_id: r.get("activity_type_id"),
                description: r.get("remark"),
                hours: r.get("unit_amount"),
                billing_rate: r.get("billing_rate"),
                billable_amount: r.get("billable_amount"),
            })
            .collect())
    }

    /// The invoice already stamped on a period slice, with the total of the rows carrying it —
    /// the billing exit's already-billed pre-check (a repeat of a billed slice reports the prior
    /// invoice instead of cutting a second one). `Ok(None)` = nothing billed for the key yet.
    pub async fn find_invoice_for_period(
        &self,
        pool: &PgPool,
        project_id: Uuid,
        employee_id: Uuid,
        year: i32,
        month: i32,
    ) -> Result<Option<(Uuid, Decimal)>, sqlx::Error> {
        org_scope::fetch_optional_row_scoped(
            pool,
            sqlx::query(
                r#"SELECT invoice_id, COALESCE(SUM(billable_amount), 0) AS amount
                   FROM timesheet.timesheets
                   WHERE project_id=$1 AND employee_id=$2
                     AND year=$3 AND month=$4
                     AND invoice_id IS NOT NULL AND (metadata->>'deleted_at') IS NULL
                   GROUP BY invoice_id
                   LIMIT 1"#,
            )
            .bind(project_id)
            .bind(employee_id)
            .bind(year)
            .bind(month),
        )
        .await
        .map(|row| {
            row.map(|r| (r.get::<uuid::Uuid, _>("invoice_id"), r.get::<rust_decimal::Decimal, _>("amount")))
        })
    }

    /// Stamp the echoed invoice onto every live, billable, not-yet-invoiced row of the period
    /// slice, returning how many rows THIS call claimed and their exact billed total — a racing
    /// caller's rows are already stamped and simply don't match (`invoice_id IS NULL`), which is
    /// what makes a period bill AT MOST once.
    ///
    /// Takes the CALLER'S connection so the stamp and the project's billed roll-up commit
    /// as ONE unit.
    pub async fn stamp_invoice_on_period(
        &self,
        conn: &mut sqlx::PgConnection,
        project_id: Uuid,
        employee_id: Uuid,
        year: i32,
        month: i32,
        invoice_id: Uuid,
    ) -> Result<StampInvoiceRow, sqlx::Error> {
        let rows = sqlx::query(
            r#"UPDATE timesheet.timesheets SET invoice_id=$5
               WHERE project_id=$1 AND employee_id=$2
                 AND year=$3 AND month=$4
                 AND is_billable AND billable_amount > 0 AND invoice_id IS NULL
                 AND (metadata->>'deleted_at') IS NULL
               RETURNING billable_amount"#,
        )
        .bind(project_id)
        .bind(employee_id)
        .bind(year)
        .bind(month)
        .bind(invoice_id)
        .fetch_all(&mut *conn)
        .await?;
        let amount = rows
            .iter()
            .map(|r| r.get::<rust_decimal::Decimal, _>("billable_amount"))
            .sum();
        Ok(StampInvoiceRow { rows_stamped: rows.len() as u64, amount })
    }

    /// Clear the invoice link off every live row carrying it — the reversal half of the
    /// billing exit. Returns the owning project (an invoice belongs to one period slice,
    /// hence one project) and how many rows re-opened; `rows_cleared = 0` means nothing
    /// carried the link (an idempotent re-call).
    ///
    /// Takes the CALLER'S connection so the clear and the project's billed roll-down
    /// commit as ONE unit.
    pub async fn clear_invoice(
        &self,
        conn: &mut sqlx::PgConnection,
        invoice_id: Uuid,
    ) -> Result<ClearInvoiceRow, sqlx::Error> {
        let row = sqlx::query(
            r#"UPDATE timesheet.timesheets SET invoice_id=NULL
               WHERE invoice_id=$1
                 AND (metadata->>'deleted_at') IS NULL
               RETURNING project_id"#,
        )
        .bind(invoice_id)
        .fetch_all(&mut *conn)
        .await?;
        let rows_cleared = row.len() as u64;
        let project_id = row
            .first()
            .map(|r| r.get::<uuid::Uuid, _>("project_id"))
            .unwrap_or_else(Uuid::nil);
        Ok(ClearInvoiceRow { project_id, rows_cleared })
    }

    /// Sum a project's live converged rows into the derived financial triple. Plain SUMs
    /// of plain-stored columns — aggregation, not repricing; reads never recompute a rate.
    ///
    /// Runs on the CALLER'S connection (the refresh verb's tx) so the sums and the write
    /// back onto `project.projects` settle as ONE unit against concurrent row writes.
    pub async fn sum_project_financials(
        &self,
        conn: &mut sqlx::PgConnection,
        project_id: Uuid,
    ) -> Result<ProjectFinancialSums, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT
                 COALESCE(SUM(costing_amount), 0)  AS total_costing_amount,
                 COALESCE(SUM(billable_amount), 0) AS total_billable_amount,
                 COALESCE(SUM(billable_amount) FILTER (WHERE invoice_id IS NOT NULL), 0)
                                                   AS total_billed_amount
               FROM timesheet.timesheets
               WHERE project_id=$1
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(project_id)
        .fetch_one(&mut *conn)
        .await?;
        Ok(ProjectFinancialSums {
            total_costing_amount: row.get("total_costing_amount"),
            total_billable_amount: row.get("total_billable_amount"),
            total_billed_amount: row.get("total_billed_amount"),
        })
    }

    /// How many live converged rows reference a project — the delete guard's probe.
    ///
    /// The scalar read twin lives only in the legacy `company_scope` module (ADR-0029);
    /// with no ambient legacy scope bound it runs as a plain pool read.
    pub async fn count_live_rows_for_project(
        &self,
        pool: &PgPool,
        project_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        company_scope::fetch_one_scalar_scoped(
            pool,
            sqlx::query_scalar::<_, i64>(
                r#"SELECT count(*) FROM timesheet.timesheets
                   WHERE project_id=$1
                     AND (metadata->>'deleted_at') IS NULL"#,
            )
            .bind(project_id),
        )
        .await
    }

    /// How many live converged rows reference a task — the delete guard's probe.
    /// Legacy-scoped as [`Self::count_live_rows_for_project`].
    pub async fn count_live_rows_for_task(
        &self,
        pool: &PgPool,
        task_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        company_scope::fetch_one_scalar_scoped(
            pool,
            sqlx::query_scalar::<_, i64>(
                r#"SELECT count(*) FROM timesheet.timesheets
                   WHERE task_id=$1
                     AND (metadata->>'deleted_at') IS NULL"#,
            )
            .bind(task_id),
        )
        .await
    }

    /// Whether any live converged row references the task — the hybrid task-status
    /// derivation input. Runs on the CALLER'S connection so the derivation and the row
    /// reads it derives from settle as ONE unit.
    pub async fn task_has_live_rows(
        &self,
        conn: &mut sqlx::PgConnection,
        task_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT EXISTS (
                 SELECT 1 FROM timesheet.timesheets
                 WHERE task_id=$1
                   AND (metadata->>'deleted_at') IS NULL
               ) AS has_rows"#,
        )
        .bind(task_id)
        .fetch_one(&mut *conn)
        .await?;
        Ok(row.get("has_rows"))
    }
}
