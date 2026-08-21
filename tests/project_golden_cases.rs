//! Golden numeric cases — the project delivery oracle. Exact billable/costing roll-ups, the billing
//! hand-off amount, template instantiation, and the input guards. Money is IDR, 2dp.

mod common;

use backbone_project::application::service::project_write_service::{
    NewProject, NewTask, NewTimeLine, NewTimesheet, ProjectError, ProjectWriteService,
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

/// PGC-1 — logging time rolls billable + costing up exactly. 8 billable h + 2 non-billable h at
/// billing 500000 / costing 300000: billable = 8·500000 = 4,000,000 (billable lines only); costing =
/// 10·300000 = 3,000,000 (all lines). The project's roll-up fields match.
#[tokio::test]
async fn pgc1_log_time_rolls_up() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company, "500000", "300000").await;
    let project = svc.create_project(a_project(company, Some(Uuid::new_v4()))).await.unwrap();

    let ts = svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![
            NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
                hours: dec("8"), billing_rate: None, costing_rate: None, is_billable: true },
            NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
                hours: dec("2"), billing_rate: None, costing_rate: None, is_billable: false },
        ],
    }, &sink).await.unwrap();

    let (hours, billable, costing): (Decimal, Decimal, Decimal) = sqlx::query_as(
        "SELECT total_hours, total_billable_amount, total_costing_amount FROM project.timesheets WHERE id=$1")
        .bind(ts).fetch_one(&pool).await.unwrap();
    assert_eq!(hours, dec("10.00"));
    assert_eq!(billable, dec("4000000.00"));
    assert_eq!(costing, dec("3000000.00"));

    let (p_billable, p_costing, p_billed): (Decimal, Decimal, Decimal) = sqlx::query_as(
        "SELECT total_billable_amount, total_costing_amount, total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(p_billable, dec("4000000.00"), "project billable roll-up");
    assert_eq!(p_costing, dec("3000000.00"), "project costing roll-up");
    assert_eq!(p_billed, dec("0.00"), "nothing billed yet");
}

/// PGC-2 — billing a submitted timesheet hands off exactly the billable amount and rolls the project's
/// billed total up; the timesheet is marked billed with the echoed invoice id.
#[tokio::test]
async fn pgc2_bill_timesheet_rolls_billed() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company, "500000", "300000").await;
    let project = svc.create_project(a_project(company, Some(Uuid::new_v4()))).await.unwrap();
    let ts = svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
            hours: dec("8"), billing_rate: None, costing_rate: None, is_billable: true }],
    }, &sink).await.unwrap();

    let out = svc.bill_timesheet(ts, company, &billing, &sink).await.unwrap();
    assert!(!out.already);
    assert_eq!(out.amount, dec("4000000.00"));

    let (status, inv): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status::text, invoice_id FROM project.timesheets WHERE id=$1")
        .bind(ts).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "billed");
    assert_eq!(inv, Some(out.invoice_id));
    let billed: Decimal = sqlx::query_scalar(
        "SELECT total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed, dec("4000000.00"), "project billed roll-up");
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

/// PGC-4 — the input guards: an external project needs a customer; a timesheet needs a line; hours
/// must be positive; a task needs a subject.
#[tokio::test]
async fn pgc4_validation() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let sink = LoggingSink;
    let company = Uuid::new_v4();

    let no_customer = svc.create_project(NewProject {
        company_id: company, project_name: "X".into(), project_type: "external".into(),
        customer_id: None, source_so_id: None, currency: None,
    }).await;
    assert!(matches!(no_customer, Err(ProjectError::Invalid(_))), "external project needs a customer");

    let project = svc.create_project(a_project(company, Some(Uuid::new_v4()))).await.unwrap();
    let empty = svc.log_time(project, NewTimesheet { employee_id: None, currency: None, lines: vec![] }, &sink).await;
    assert!(matches!(empty, Err(ProjectError::Invalid(_))), "timesheet needs a line");

    let bad_hours = svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![NewTimeLine { activity_type_id: None, task_id: None, description: None,
            hours: dec("0"), billing_rate: Some(dec("1")), costing_rate: Some(dec("1")), is_billable: true }],
    }, &sink).await;
    assert!(matches!(bad_hours, Err(ProjectError::Invalid(_))), "hours must be positive");

    let no_subject = svc.add_task(NewTask {
        project_id: project, parent_task_id: None, subject: "  ".into(), task_type: None,
        expected_time: Decimal::ZERO,
    }).await;
    assert!(matches!(no_subject, Err(ProjectError::Invalid(_))), "task needs a subject");
}
