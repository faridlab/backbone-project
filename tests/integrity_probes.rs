//! Integrity probes — the delivery + billing invariants that keep the funnel honest under retry,
//! over the CONVERGED analytic row (`timesheet.timesheets`, owned by the timesheet module).

mod common;

use backbone_project::application::service::project_events::ProjectEvent;
use backbone_project::application::service::project_write_service::{NewProject, ProjectError, ProjectWriteService};
use common::*;
use rust_decimal::Decimal;
use uuid::Uuid;

async fn an_activity(pool: &sqlx::PgPool, company: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.activity_types (id, company_id, name, billing_rate, costing_rate, status)
           VALUES ($1,$2,'Consulting',$3,$4,'active')"#,
    )
    .bind(id).bind(company).bind(dec("500000")).bind(dec("300000"))
    .execute(pool).await.unwrap();
    id
}
fn ext_project(company: Uuid) -> NewProject {
    NewProject {
        company_id: company, project_name: "P".into(), project_type: "external".into(),
        customer_id: Some(Uuid::new_v4()), source_so_id: None, currency: Some("IDR".into()),
    }
}

/// IP-1 — a period bills AT MOST ONCE: a retry returns the same invoice, drives billing once, and
/// rolls the project's billed total up only once.
#[tokio::test]
async fn ip1_bill_idempotent_per_period() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_approval(&pool, company, employee, 2026, 7, "approved").await;

    let a = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await.unwrap();
    assert!(!a.already);
    assert_eq!(a.amount, dec("2000000.00"));
    let b = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await.unwrap();
    assert!(b.already, "second bill short-circuits");
    assert_eq!(a.invoice_id, b.invoice_id, "same invoice");
    assert_eq!(billing.invoice_count(), 1, "billing driven exactly once");
    let billed: Decimal = sqlx::query_scalar(
        "SELECT total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed, dec("2000000.00"), "billed rolled up exactly once");
}

/// IP-2 — the reversal half: `unbill_invoice` clears the rows' invoice link and rolls the credited
/// amount back off the project's billed total; the rows re-open and a re-bill mints a fresh invoice.
/// Idempotent: an invoice nothing carries anymore is a no-op.
#[tokio::test]
async fn ip2_unbill_reverses_and_reopens() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    let row = seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_approval(&pool, company, employee, 2026, 7, "approved").await;

    let first = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await.unwrap();
    // The reversal event a credit note would drive (credited amount = the invoice's slice total).
    svc.unbill_invoice(first.invoice_id, company, first.amount, &sink).await.unwrap();

    let link: Option<Uuid> = sqlx::query_scalar(
        "SELECT invoice_id FROM timesheet.timesheets WHERE id=$1")
        .bind(row).fetch_one(&pool).await.unwrap();
    assert_eq!(link, None, "row re-opened for editing/re-billing");
    let billed: Decimal = sqlx::query_scalar(
        "SELECT total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed, dec("0.00"), "credited amount rolled back off");

    // Idempotent: reversing the same invoice again changes nothing.
    svc.unbill_invoice(first.invoice_id, company, first.amount, &sink).await.unwrap();
    let billed2: Decimal = sqlx::query_scalar(
        "SELECT total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed2, dec("0.00"));

    // A re-bill of the re-opened slice mints a FRESH invoice (the adapter renumbers -R{n}).
    billing.reverse(&PeriodKey::of(company, project, employee, 2026, 7));
    let second = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await.unwrap();
    assert!(!second.already);
    assert_ne!(second.invoice_id, first.invoice_id, "re-bill is a new invoice");
    assert_eq!(second.amount, first.amount);
    let billed3: Decimal = sqlx::query_scalar(
        "SELECT total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed3, dec("2000000.00"), "re-bill rolls up again");
}

/// IP-3 — a completed project is terminal: the refresh verb refuses it, its totals stay final, and
/// it cannot be re-completed.
#[tokio::test]
async fn ip3_completed_project_terminal() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    let fin = svc.refresh_project_financials(company, project).await.unwrap();
    svc.complete_project(project, &sink).await.unwrap();

    let refresh = svc.refresh_project_financials(company, project).await;
    assert!(matches!(refresh, Err(ProjectError::InvalidState(_))), "cannot refresh a closed project");
    let stored: Decimal = sqlx::query_scalar(
        "SELECT total_billable_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(stored, fin.total_billable_amount, "completed totals stay final");
    let recomplete = svc.complete_project(project, &sink).await;
    assert!(matches!(recomplete, Err(ProjectError::InvalidState(_))), "cannot re-complete");
}

/// IP-4 — an invoiced analytic row is WRITE-PROTECTED at the database: the timesheet schema's
/// invoiced-row guard refuses any pricing/anchoring change while the invoice link is present
/// (corrections must flow through the reversal, which clears the link first).
#[tokio::test]
async fn ip4_invoiced_row_write_guard() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    let row = seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_approval(&pool, company, employee, 2026, 7, "approved").await;
    let out = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await.unwrap();

    let reprice = sqlx::query("UPDATE timesheet.timesheets SET unit_amount=99 WHERE id=$1")
        .bind(row).execute(&pool).await;
    assert!(reprice.is_err(), "an invoiced row cannot change its pricing columns");

    // The reversal clears the link; the row is editable again by construction.
    svc.unbill_invoice(out.invoice_id, company, out.amount, &sink).await.unwrap();
    let editable = sqlx::query("UPDATE timesheet.timesheets SET remark='fixed typo' WHERE id=$1")
        .bind(row).execute(&pool).await;
    assert!(editable.is_ok(), "a cleared row is editable again");
}

/// IP-5 — only billable rows are invoiced + rolled into billed; non-billable rows cost but never
/// bill, and the refresh keeps both halves honest.
#[tokio::test]
async fn ip5_non_billable_excluded_from_billing() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_row(&pool, company, employee, project, None, 2026, 7, 7, dec("6"),
        dec("500000"), dec("300000"), false, Some(act)).await;
    seed_approval(&pool, company, employee, 2026, 7, "approved").await;

    let out = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await.unwrap();
    // Only the 4 billable hours: 4·500000 = 2,000,000. The 6 non-billable hours never bill.
    assert_eq!(out.amount, dec("2000000.00"), "only billable hours invoiced");
    let fin = svc.refresh_project_financials(company, project).await.unwrap();
    assert_eq!(fin.total_billed_amount, dec("2000000.00"), "billed = billable slice only");
    assert_eq!(fin.total_billable_amount, dec("2000000.00"), "non-billable rows contribute no billable");
    assert_eq!(fin.total_costing_amount, dec("3000000.00"), "all 10 hours cost 10·300000");
}

/// IP-6 — the refresh is a pure derivation of the live converged rows: soft-deleting a fat-fingered
/// row (the timesheet module's delete path) and re-running the refresh restores the honest margin.
/// Without the explicit-verb posture, a stored roll-up would drift forever after such a delete.
#[tokio::test]
async fn ip6_refresh_tracks_row_lifecycle() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();

    // A correct 4h row, then a fat-fingered 40h row (both billable at 500000 / costing 300000).
    seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    let typo = seed_row(&pool, company, employee, project, None, 2026, 7, 7, dec("40"),
        dec("500000"), dec("300000"), true, Some(act)).await;

    let inflated = svc.refresh_project_financials(company, project).await.unwrap();
    assert_eq!(inflated.total_billable_amount, dec("22000000.00"), "(4+40)·500000 before the fix");
    assert_eq!(inflated.total_costing_amount, dec("13200000.00"));

    // The timesheet module's own delete path soft-deletes its row; the refresh re-derives.
    sqlx::query(
        r#"UPDATE timesheet.timesheets
           SET metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(NOW()))
           WHERE id=$1"#)
        .bind(typo).execute(&pool).await.unwrap();
    let fixed = svc.refresh_project_financials(company, project).await.unwrap();
    assert_eq!(fixed.total_billable_amount, dec("2000000.00"), "roll-up returned to the good row");
    assert_eq!(fixed.total_costing_amount, dec("1200000.00"));

    // The event trail records what the billing path published (spot-check the union shape).
    let cap = CapturingSink::new();
    let billing = FakeBilling::new();
    seed_approval(&pool, company, employee, 2026, 7, "approved").await;
    svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &cap).await.unwrap();
    assert!(cap.events().iter().any(|e| matches!(e, ProjectEvent::TimesheetBilled(_))));
}

/// IP-7 — completing a project captures the final totals into its event even after the
/// convergence (the completion path reads the STORED triple, so a refresh before completing is
/// what fixes the announced numbers).
#[tokio::test]
async fn ip7_complete_announces_refreshed_totals() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let cap = CapturingSink::new();
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;

    let fin = svc.refresh_project_financials(company, project).await.unwrap();
    svc.complete_project(project, &cap).await.unwrap();
    let announced = cap.events().into_iter().find_map(|e| match e {
        ProjectEvent::ProjectCompleted(c) => Some(c.total_billable_amount),
        _ => None,
    });
    assert_eq!(announced, Some(fin.total_billable_amount),
        "ProjectCompleted announces the refreshed stored total");
}
