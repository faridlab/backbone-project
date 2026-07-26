//! Integrity probes — the delivery + billing invariants that keep the funnel honest under retry.

mod common;

use backbone_project::application::service::project_events::ProjectEvent;
use backbone_project::application::service::project_write_service::{
    NewProject, NewTimeLine, NewTimesheet, ProjectError, ProjectWriteService,
};
use common::*;
use rust_decimal::Decimal;
use uuid::Uuid;

async fn an_activity(pool: &sqlx::PgPool, company: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.activity_types (id, company_id, name, billing_rate, costing_rate, is_active)
           VALUES ($1,$2,'Consulting',$3,$4,true)"#,
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
async fn billable_ts(svc: &ProjectWriteService, project: Uuid, act: Uuid, sink: &LoggingSink) -> Uuid {
    svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
            hours: dec("4"), billing_rate: None, costing_rate: None, is_billable: true }],
    }, sink).await.unwrap()
}

/// IP-1 — a timesheet bills AT MOST ONCE: a retry returns the same invoice, drives billing once,
/// and rolls the project's billed total up only once.
#[tokio::test]
async fn ip1_bill_idempotent() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    let ts = billable_ts(&svc, project, act, &sink).await;

    let a = svc.bill_timesheet(ts, company, &billing, &sink).await.unwrap();
    assert!(!a.already);
    let b = svc.bill_timesheet(ts, company, &billing, &sink).await.unwrap();
    assert!(b.already, "second bill short-circuits");
    assert_eq!(a.invoice_id, b.invoice_id, "same invoice");
    assert_eq!(billing.invoice_count(), 1, "billing driven exactly once");
    let billed: Decimal = sqlx::query_scalar(
        "SELECT total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed, dec("2000000.00"), "billed rolled up exactly once");
}

/// IP-2 — a timesheet with nothing billable cannot be billed (billing is never driven).
#[tokio::test]
async fn ip2_nothing_billable_refused() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    let ts = svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
            hours: dec("4"), billing_rate: None, costing_rate: None, is_billable: false }],
    }, &sink).await.unwrap();

    let err = svc.bill_timesheet(ts, company, &billing, &sink).await.unwrap_err();
    assert!(matches!(err, ProjectError::Invalid(_)), "nothing billable → refused");
    assert_eq!(billing.invoice_count(), 0, "billing not driven");
}

/// IP-3 — a completed project is terminal: no more time logged, cannot be re-completed.
#[tokio::test]
async fn ip3_completed_project_terminal() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    svc.complete_project(project, &sink).await.unwrap();

    let relog = svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
            hours: dec("1"), billing_rate: None, costing_rate: None, is_billable: true }],
    }, &sink).await;
    assert!(matches!(relog, Err(ProjectError::InvalidState(_))), "cannot log to a closed project");
    let recomplete = svc.complete_project(project, &sink).await;
    assert!(matches!(recomplete, Err(ProjectError::InvalidState(_))), "cannot re-complete");
}

/// IP-4 — a project with no customer cannot be billed (the guard fails closed before driving billing).
#[tokio::test]
async fn ip4_bill_requires_customer() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    // An internal project has no customer, but time can still be logged with an explicit billing rate.
    let project = svc.create_project(NewProject {
        company_id: company, project_name: "Internal".into(), project_type: "internal".into(),
        customer_id: None, source_so_id: None, currency: Some("IDR".into()),
    }).await.unwrap();
    let ts = billable_ts(&svc, project, act, &sink).await;

    let err = svc.bill_timesheet(ts, company, &billing, &sink).await.unwrap_err();
    assert!(matches!(err, ProjectError::Invalid(_)), "no customer → refused");
    assert_eq!(billing.invoice_count(), 0, "billing not driven");
}

/// IP-5 — only billable lines are invoiced + rolled into billed; non-billable lines cost but never bill.
#[tokio::test]
async fn ip5_non_billable_excluded_from_billing() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();
    let ts = svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![
            NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
                hours: dec("4"), billing_rate: None, costing_rate: None, is_billable: true },
            NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
                hours: dec("6"), billing_rate: None, costing_rate: None, is_billable: false },
        ],
    }, &sink).await.unwrap();

    let out = svc.bill_timesheet(ts, company, &billing, &sink).await.unwrap();
    // Only the 4 billable hours: 4·500000 = 2,000,000. The 6 non-billable hours never bill.
    assert_eq!(out.amount, dec("2000000.00"), "only billable hours invoiced");
    let (billed, costing): (Decimal, Decimal) = sqlx::query_as(
        "SELECT total_billed_amount, total_costing_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed, dec("2000000.00"), "billed = billable only");
    assert_eq!(costing, dec("3000000.00"), "all 10 hours cost 10·300000");
}

/// IP-7 (completeness council 2026-07-06) — a mis-logged timesheet can be CANCELLED, reversing its
/// billable + costing off the project roll-up (the reverse gear the ratchet lacked). Idempotent; a
/// billed timesheet cannot be cancelled. Without cancel, a single typo poisons the margin number forever.
#[tokio::test]
async fn ip7_cancel_timesheet_reverses_rollup() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company).await;
    let project = svc.create_project(ext_project(company)).await.unwrap();

    // A correct 4h sheet, then a fat-fingered 40h sheet (both billable at 500000 / costing 300000).
    let good = billable_ts(&svc, project, act, &sink).await;
    let typo = svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
            hours: dec("40"), billing_rate: None, costing_rate: None, is_billable: true }],
    }, &sink).await.unwrap();

    // Roll-up is inflated by the typo: billable = (4+40)·500000 = 22,000,000; costing = 44·300000 = 13,200,000.
    let (b0, c0): (Decimal, Decimal) = sqlx::query_as(
        "SELECT total_billable_amount, total_costing_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(b0, dec("22000000.00"));
    assert_eq!(c0, dec("13200000.00"));

    // Cancel the typo — the roll-up returns to exactly the good sheet's contribution.
    svc.cancel_timesheet(typo, &sink).await.unwrap();
    let (b1, c1, status): (Decimal, Decimal, String) = sqlx::query_as(
        "SELECT p.total_billable_amount, p.total_costing_amount, t.status::text
         FROM project.projects p, project.timesheets t WHERE p.id=$1 AND t.id=$2")
        .bind(project).bind(typo).fetch_one(&pool).await.unwrap();
    assert_eq!(b1, dec("2000000.00"), "billable reversed to just the good 4h sheet");
    assert_eq!(c1, dec("1200000.00"), "costing reversed to just the good 4h sheet");
    assert_eq!(status, "cancelled");

    // Idempotent: a re-cancel is a no-op, roll-up unchanged.
    svc.cancel_timesheet(typo, &sink).await.unwrap();
    let b2: Decimal = sqlx::query_scalar(
        "SELECT total_billable_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(b2, dec("2000000.00"), "re-cancel changes nothing");

    // A BILLED timesheet cannot be cancelled (reverse via billing instead).
    svc.bill_timesheet(good, company, &billing, &sink).await.unwrap();
    let err = svc.cancel_timesheet(good, &sink).await.unwrap_err();
    assert!(matches!(err, ProjectError::InvalidState(_)), "billed timesheet cannot be cancelled");
}

/// IP-6 (maturity council 2026-07-06) — completing a project and logging time RACE conserved.
/// `log_time` used a non-atomic check-then-act (status read on the pool, then an unguarded roll-up
/// UPDATE), so a `complete_project` committing in the gap could leave time logged onto a completed
/// project — the stored roll-up grows past the "final" numbers `ProjectCompleted` already published, and
/// a billable orphan timesheet lands on a closed project. The roll-up UPDATE is now status-gated in the
/// same tx: if the project isn't open when it runs, the whole timesheet insert rolls back. Invariant:
/// a completed project's stored billable equals the total its `ProjectCompleted` event reported, and no
/// submitted timesheet exists on it.
#[tokio::test]
async fn ip6_complete_races_log_time_conserves_rollup() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let act = an_activity(&pool, company).await;

    for _ in 0..40 {
        let setup = ProjectWriteService::new(pool.clone());
        let project = setup.create_project(ext_project(company)).await.unwrap();
        let svc_a = ProjectWriteService::new(pool.clone());
        let svc_b = ProjectWriteService::new(pool.clone());
        let cap = CapturingSink::new();
        let logsink = LoggingSink;
        let ts = NewTimesheet {
            employee_id: None, currency: None,
            lines: vec![NewTimeLine { activity_type_id: Some(act), task_id: None, description: None,
                hours: dec("4"), billing_rate: None, costing_rate: None, is_billable: true }],
        };

        let (_c, _l) = tokio::join!(
            svc_a.complete_project(project, &cap),
            svc_b.log_time(project, ts, &logsink),
        );

        let (status, stored): (String, Decimal) = sqlx::query_as(
            "SELECT status::text, total_billable_amount FROM project.projects WHERE id=$1")
            .bind(project).fetch_one(&pool).await.unwrap();
        if status == "completed" {
            let reported = cap.events().into_iter().find_map(|e| match e {
                ProjectEvent::ProjectCompleted(c) => Some(c.total_billable_amount), _ => None,
            });
            if let Some(r) = reported {
                assert_eq!(stored, r,
                    "completed project's stored billable == the ProjectCompleted total (no post-completion drift)");
            }
            let orphan: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM project.timesheets WHERE project_id=$1 AND status='submitted'::timesheet_status")
                .bind(project).fetch_one(&pool).await.unwrap();
            assert_eq!(orphan, 0, "no billable timesheet logged onto a completed project");
        }
    }
}
