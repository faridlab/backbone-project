//! Golden numeric cases — the project delivery oracle over the CONVERGED analytic row. Exact
//! cost/billable refresh from `timesheet.timesheets`, the period billing hand-off amount, template
//! instantiation, and the input gates. Money is IDR, 2dp.

mod common;

use backbone_project::application::service::project_write_service::{
    NewProject, NewTask, ProjectError, ProjectWriteService,
};
use common::*;
use rust_decimal::Decimal;
use uuid::Uuid;

async fn an_activity(pool: &sqlx::PgPool, company: Uuid, billing: &str, costing: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.activity_types (id, company_id, name, billing_rate, costing_rate, status)
           VALUES ($1,$2,'Consulting',$3,$4,'active')"#,
    )
    .bind(id).bind(company).bind(dec(billing)).bind(dec(costing))
    .execute(pool).await.unwrap();
    id
}

fn a_project(company: Uuid, customer: Option<Uuid>) -> NewProject {
    NewProject {
        company_id: company, project_name: "Website Build".into(), project_type: "external".into(),
        customer_id: customer, source_so_id: None, currency: Some("IDR".into()),
    }
}

/// PGC-1 — the refresh verb derives the project triple from the converged rows. 8 billable h +
/// 2 non-billable h at billing 500000 / costing 300000: billable = 8·500000 = 4,000,000 (billable
/// rows only); costing = 10·300000 = 3,000,000 (all rows); billed = 0 (nothing invoiced yet).
#[tokio::test]
async fn pgc1_refresh_rolls_up_from_converged_rows() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company, "500000", "300000").await;
    let project = svc.create_project(a_project(company, Some(Uuid::new_v4()))).await.unwrap();

    seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("8"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_row(&pool, company, employee, project, None, 2026, 7, 7, dec("2"),
        dec("500000"), dec("300000"), false, Some(act)).await;

    let fin = svc.refresh_project_financials(company, project).await.unwrap();
    assert_eq!(fin.total_billable_amount, dec("4000000.00"));
    assert_eq!(fin.total_costing_amount, dec("3000000.00"));
    assert_eq!(fin.total_billed_amount, dec("0.00"), "nothing billed yet");

    // The stored columns carry exactly the derived triple (derived read returns the same).
    let stored = svc.project_financials(project).await.unwrap();
    assert_eq!(stored, fin, "stored columns == refreshed triple");
}

/// PGC-2 — billing an APPROVED period slice hands off exactly the billable amount, stamps the rows
/// with the echoed invoice id, and rolls the project's billed total up. A repeat is a no-op.
#[tokio::test]
async fn pgc2_bill_period_rolls_billed() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company, "500000", "300000").await;
    let project = svc.create_project(a_project(company, Some(Uuid::new_v4()))).await.unwrap();
    let row = seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("8"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_approval(&pool, company, employee, 2026, 7, "approved").await;

    let out = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink)
        .await.unwrap();
    assert!(!out.already);
    assert_eq!(out.amount, dec("4000000.00"));

    let (inv, billable): (Option<Uuid>, Decimal) = sqlx::query_as(
        "SELECT invoice_id, billable_amount FROM timesheet.timesheets WHERE id=$1")
        .bind(row).fetch_one(&pool).await.unwrap();
    assert_eq!(inv, Some(out.invoice_id), "row carries the invoice link");
    assert_eq!(billable, dec("4000000.00"));

    let billed: Decimal = sqlx::query_scalar(
        "SELECT total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed, dec("4000000.00"), "project billed roll-up");

    // A repeat of the same slice reports the prior invoice without driving billing again.
    let again = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink)
        .await.unwrap();
    assert!(again.already, "second bill short-circuits");
    assert_eq!(again.invoice_id, out.invoice_id, "same invoice");
    assert_eq!(billing.invoice_count(), 1, "billing driven exactly once");
}

/// PGC-3 — instantiating a template creates a project plus its template tasks (in sequence order).
#[tokio::test]
async fn pgc3_instantiate_template() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let company = Uuid::new_v4();
    let tpl = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.project_templates (id, company_id, template_name, project_type, status)
           VALUES ($1,$2,'Onboarding','external','active')"#,
    ).bind(tpl).bind(company).execute(&pool).await.unwrap();
    for (i, subj) in ["Kickoff", "Design", "Handover"].iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO project.project_template_tasks (id, template_id, company_id, subject, expected_time, sequence)
               VALUES ($1,$2,$3,$4,0,$5)"#,
        ).bind(Uuid::new_v4()).bind(tpl).bind(company).bind(subj).bind(i as i32).execute(&pool).await.unwrap();
    }

    let project = svc.instantiate_template(tpl, company, "Acme Onboarding".into(), Some(Uuid::new_v4()))
        .await.unwrap();
    let subjects: Vec<String> = sqlx::query_scalar(
        "SELECT subject FROM project.tasks WHERE project_id=$1 ORDER BY (metadata->>'created_at')")
        .bind(project).fetch_all(&pool).await.unwrap();
    assert_eq!(subjects.len(), 3, "all template tasks materialized");
    assert!(subjects.contains(&"Kickoff".to_string()));
    assert!(subjects.contains(&"Handover".to_string()));
}

/// PGC-4 — the input gates: an external project needs a customer; the billing exit refuses an
/// unapproved period, a period with nothing billable, and a customer-less project; a task needs a
/// subject; progress must be 0..100.
#[tokio::test]
async fn pgc4_validation() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company, "500000", "300000").await;

    let no_customer = svc.create_project(NewProject {
        company_id: company, project_name: "X".into(), project_type: "external".into(),
        customer_id: None, source_so_id: None, currency: None,
    }).await;
    assert!(matches!(no_customer, Err(ProjectError::Invalid(_))), "external project needs a customer");

    let project = svc.create_project(a_project(company, Some(Uuid::new_v4()))).await.unwrap();
    seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("8"),
        dec("500000"), dec("300000"), true, Some(act)).await;

    // Unapproved cycle (seeded pending) → guarded.
    seed_approval(&pool, company, employee, 2026, 7, "pending").await;
    let unapproved = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await;
    assert!(matches!(unapproved, Err(ProjectError::Guarded(_))), "unapproved period refuses");

    // Approved but nothing billable (a non-billable row only) → invalid.
    let other = Uuid::new_v4();
    let employee2 = Uuid::new_v4();
    let project2 = svc.create_project(a_project(company, Some(other))).await.unwrap();
    seed_row(&pool, company, employee2, project2, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), false, Some(act)).await;
    seed_approval(&pool, company, employee2, 2026, 7, "approved").await;
    let nothing = svc.bill_timesheet_period(project2, employee2, 2026, 7, company, &billing, &sink).await;
    assert!(matches!(nothing, Err(ProjectError::Invalid(_))), "nothing billable refuses");

    // An internal project has no customer to bill.
    let internal = svc.create_project(NewProject {
        company_id: company, project_name: "Internal".into(), project_type: "internal".into(),
        customer_id: None, source_so_id: None, currency: Some("IDR".into()),
    }).await.unwrap();
    let employee3 = Uuid::new_v4();
    seed_row(&pool, company, employee3, internal, None, 2026, 7, 6, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_approval(&pool, company, employee3, 2026, 7, "approved").await;
    let noone = svc.bill_timesheet_period(internal, employee3, 2026, 7, company, &billing, &sink).await;
    assert!(matches!(noone, Err(ProjectError::Invalid(_))), "no customer refuses");

    assert_eq!(billing.invoice_count(), 0, "billing never driven");

    let no_subject = svc.add_task(NewTask {
        project_id: project, parent_task_id: None, subject: "  ".into(), task_type: None,
        expected_time: Decimal::ZERO,
    }).await;
    assert!(matches!(no_subject, Err(ProjectError::Invalid(_))), "task needs a subject");

    let task = svc.add_task(NewTask {
        project_id: project, parent_task_id: None, subject: "Build".into(), task_type: None,
        expected_time: Decimal::ZERO,
    }).await.unwrap();
    let bad_progress = svc.set_task_status(task, "working", dec("101")).await;
    assert!(matches!(bad_progress, Err(ProjectError::Invalid(_))), "progress must be 0..100");
    let bad_status = svc.set_task_status(task, "paused", dec("10")).await;
    assert!(matches!(bad_status, Err(ProjectError::Invalid(_))), "unknown status refuses");
}
