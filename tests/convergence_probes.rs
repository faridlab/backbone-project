//! Convergence probes — the project module's NEW posture over the converged analytic row
//! (`timesheet.timesheets`, owned by the timesheet module):
//!
//! - PJ-1 — the financial triple refreshes from the converged rows (both directions of change).
//! - PJ-2 — the hybrid task status (both directions): derives `working` from live rows, a hand-set
//!          close LATCHES, reopening re-derives, and the derivation skips latched tasks.
//! - PJ-3 — delete guards: a project/task with live converged rows refuses deletion; clean ones go.
//! - PJ-4 — the confirm-mint (`mint_service_delivery`): per-rung shapes, per-line idempotency,
//!          template forking, and the refusal paths.
//! - PJ-5 — the origin-key uniques are DATABASE backstops (a raw duplicate insert cannot survive).
//! - PB-6 — the period billability gate refuses every non-approved cycle state.
//!
//! The former PJ-7 company-fence probe retired with the tenancy strip (ADR-0029): the module
//! ships no fence of its own anymore — the undecorated posture (RLS flags armed, zero module
//! policies, ambient org scope riding the reads) is pinned by `tenancy_posture_probe.rs`, and
//! row isolation under a decorated host is the composing service's decorator probes.

mod common;

use backbone_project::application::service::project_write_service::{
    NewProject, NewTask, ProjectError, ProjectWriteService, ServiceDeliveryLine,
    ServiceDeliveryRequest, ServiceTrackingRung,
};
use common::*;
use rust_decimal::Decimal;
use uuid::Uuid;

async fn an_activity(pool: &sqlx::PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.activity_types (id, name, billing_rate, costing_rate, status)
           VALUES ($1,'Consulting',$2,$3,'active')"#,
    )
    .bind(id).bind(dec("500000")).bind(dec("300000"))
    .execute(pool).await.unwrap();
    id
}
fn ext_project() -> NewProject {
    NewProject {
        project_name: "P".into(), project_type: "external".into(),
        customer_id: Some(Uuid::new_v4()), source_so_id: None, currency: Some("IDR".into()),
    }
}

/// PJ-1 — `refresh_project_financials` re-derives the stored triple from the live converged rows in
/// BOTH directions: rows added grow the totals, rows soft-deleted shrink them back.
#[tokio::test]
async fn pj1_rollups_refresh_from_converged_rows() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let employee = Uuid::new_v4();
    let act = an_activity(&pool).await;
    let project = svc.create_project(ext_project()).await.unwrap();

    let a = seed_row(&pool, employee, project, None, 2026, 6, 2, dec("3"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_row(&pool, employee, project, None, 2026, 7, 3, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;

    let fin = svc.refresh_project_financials(project).await.unwrap();
    assert_eq!(fin.total_billable_amount, dec("3500000.00"), "7 billable hours across two months");
    assert_eq!(fin.total_costing_amount, dec("2100000.00"));

    sqlx::query(
        r#"UPDATE timesheet.timesheets
           SET metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(NOW()))
           WHERE id=$1"#)
        .bind(a).execute(&pool).await.unwrap();
    let shrunk = svc.refresh_project_financials(project).await.unwrap();
    assert_eq!(shrunk.total_billable_amount, dec("2000000.00"), "deleted row no longer counts");
    assert_eq!(shrunk.total_costing_amount, dec("1200000.00"));

    // The plain read returns the stored columns unchanged (no hidden live compute).
    let reread = svc.project_financials(project).await.unwrap();
    assert_eq!(reread, shrunk);
}

/// PJ-2 — the hybrid task status (lifecycle shape `hybrid`, sticky closes), both directions:
/// derivation flips open→working when a live row references the task; a hand-set `completed`
/// latches (new rows never reopen it); reopening (`set open`) re-derives at once.
#[tokio::test]
async fn pj2_hybrid_status_both_directions() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let employee = Uuid::new_v4();
    let act = an_activity(&pool).await;
    let project = svc.create_project(ext_project()).await.unwrap();
    let task = svc.add_task(NewTask {
        project_id: project, parent_task_id: None, subject: "Build".into(),
        task_type: None, expected_time: Decimal::ZERO,
    }).await.unwrap();

    // Forward: no rows → open; a live row → working.
    assert_eq!(svc.refresh_task_status(task).await.unwrap(), "open");
    seed_row(&pool, employee, project, Some(task), 2026, 7, 6, dec("2"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    assert_eq!(svc.refresh_task_status(task).await.unwrap(), "working");

    // Inverse: a hand-set close latches — even new rows never reopen it.
    assert_eq!(svc.set_task_status(task, "completed", dec("100")).await.unwrap(), "completed");
    seed_row(&pool, employee, project, Some(task), 2026, 7, 7, dec("2"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    assert_eq!(svc.refresh_task_status(task).await.unwrap(), "completed", "latched task is skipped");

    // Reopen clears the latch: open is not sticky, so the write re-derives immediately.
    assert_eq!(svc.set_task_status(task, "open", dec("0")).await.unwrap(), "working",
        "reopen re-derives from the still-live rows");
}

/// PJ-3 — delete guards: a project or task with live converged rows refuses deletion (the rows
/// survive in the timesheet schema and would dangle); once the rows are gone (or never existed),
/// the delete goes through.
#[tokio::test]
async fn pj3_delete_guards() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let employee = Uuid::new_v4();
    let act = an_activity(&pool).await;
    let project = svc.create_project(ext_project()).await.unwrap();
    let task = svc.add_task(NewTask {
        project_id: project, parent_task_id: None, subject: "Build".into(),
        task_type: None, expected_time: Decimal::ZERO,
    }).await.unwrap();
    let row = seed_row(&pool, employee, project, Some(task), 2026, 7, 6, dec("2"),
        dec("500000"), dec("300000"), true, Some(act)).await;

    let guarded_task = svc.delete_task(task).await;
    assert!(matches!(guarded_task, Err(ProjectError::Guarded(_))), "task with rows refuses");
    let guarded_project = svc.delete_project(project).await;
    assert!(matches!(guarded_project, Err(ProjectError::Guarded(_))), "project with rows refuses");

    // The rows move away (the timesheet module's own soft delete) — the guards open.
    sqlx::query(
        r#"UPDATE timesheet.timesheets
           SET metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(NOW()))
           WHERE id=$1"#)
        .bind(row).execute(&pool).await.unwrap();
    svc.delete_task(task).await.unwrap();
    svc.delete_project(project).await.unwrap();

    // A virgin project deletes cleanly (never any rows).
    let virgin = svc.create_project(ext_project()).await.unwrap();
    svc.delete_project(virgin).await.unwrap();
    let gone: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM project.projects WHERE id=$1 AND (metadata->>'deleted_at') IS NULL")
        .bind(virgin).fetch_optional(&pool).await.unwrap();
    assert_eq!(gone, None, "soft-deleted project is no longer live");
}

/// PJ-4 — the confirm-mint: one call mints the rung shapes; a REPEATED call mints nothing new and
/// observes the same stable ids. `task_in_project` forks from the template (its tasks
/// materialize), `project_only` creates the project with no tasks, `manual` mints nothing, and
/// `task_global_project` without its fixed project refuses.
#[tokio::test]
async fn pj4_mint_idempotent_per_line() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let customer = Uuid::new_v4();

    // The fork blueprint: active template with two tasks.
    let tpl = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.project_templates (id, template_name, project_type, status)
           VALUES ($1,'Managed Service','external','active')"#,
    ).bind(tpl).execute(&pool).await.unwrap();
    for (i, subj) in ["Setup", "Support"].iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO project.project_template_tasks (id, template_id, subject, expected_time, sequence)
               VALUES ($1,$2,$3,0,$4)"#,
        ).bind(Uuid::new_v4()).bind(tpl).bind(subj).bind(i as i32)
            .execute(&pool).await.unwrap();
    }
    // A fixed global project for the task_global_project rung.
    let global = svc.create_project(NewProject {
        project_name: "Global Service Desk".into(),
        project_type: "external".into(), customer_id: Some(customer),
        source_so_id: None, currency: Some("IDR".into()),
    }).await.unwrap();

    let order = Uuid::new_v4();
    let mut n = 0u32;
    let mut line = |rung: ServiceTrackingRung| {
        n += 1;
        ServiceDeliveryLine {
            sale_line_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            quantity: dec("10"),
            description: Some(format!("Service line {}", n)),
            rung,
            fixed_project_id: if rung == ServiceTrackingRung::TaskGlobalProject { Some(global) } else { None },
            template_id: if rung == ServiceTrackingRung::TaskInProject { Some(tpl) } else { None },
        }
    };
    let lines = vec![
        line(ServiceTrackingRung::TaskInProject),
        line(ServiceTrackingRung::ProjectOnly),
        line(ServiceTrackingRung::TaskGlobalProject),
        line(ServiceTrackingRung::Manual),
    ];
    let req = ServiceDeliveryRequest {
        order_id: order, customer_id: customer,
        order_number: "SO-042".into(), currency: "IDR".into(), lines,
    };

    let outcomes = svc.mint_service_delivery(&req).await.unwrap();
    assert_eq!(outcomes.len(), 4);

    // task_in_project: the per-order project exists, keyed source_so_id, with template + line tasks.
    let (tip_project, tip_minted) = (outcomes[0].project_id.unwrap(), outcomes[0].minted);
    assert!(tip_minted);
    assert!(outcomes[0].task_id.is_some(), "one task per line");
    let (src, name, status): (Option<Uuid>, String, String) = sqlx::query_as(
        "SELECT source_so_id, project_name, status::text FROM project.projects WHERE id=$1")
        .bind(tip_project).fetch_one(&pool).await.unwrap();
    assert_eq!(src, Some(order), "per-order project keyed on the source sales order");
    assert_eq!(name, "SO SO-042");
    assert_eq!(status, "open");
    let task_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project.tasks WHERE project_id=$1 AND (metadata->>'deleted_at') IS NULL")
        .bind(tip_project).fetch_one(&pool).await.unwrap();
    assert_eq!(task_count, 3, "two template tasks + the line's task");

    // project_only: the SAME per-order project (one project per order), no task for this line.
    assert_eq!(outcomes[1].project_id, Some(tip_project), "one project per order across rungs");
    assert!(outcomes[1].minted, "project minted by this order");
    assert_eq!(outcomes[1].task_id, None, "project_only mints no task");

    // task_global_project: task under the fixed project.
    assert_eq!(outcomes[2].project_id, Some(global));
    assert!(outcomes[2].task_id.is_some());
    assert!(outcomes[2].minted);

    // manual: nothing.
    assert_eq!(outcomes[3], backbone_project::application::service::project_write_service::ServiceDeliveryLineOutcome {
        sale_line_id: req.lines[3].sale_line_id, minted: false, project_id: None, task_id: None,
    });

    // REPEAT confirm: stable ids, nothing new, billing of template materialization not re-run.
    let again = svc.mint_service_delivery(&req).await.unwrap();
    assert_eq!(again[0].project_id, Some(tip_project));
    assert_eq!(again[0].task_id, outcomes[0].task_id);
    assert!(!again[0].minted, "prior mint observed");
    assert!(!again[1].minted, "project already existed");
    assert_eq!(again[2].task_id, outcomes[2].task_id);
    assert!(!again[2].minted);
    let task_count_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project.tasks WHERE project_id=$1 AND (metadata->>'deleted_at') IS NULL")
        .bind(tip_project).fetch_one(&pool).await.unwrap();
    assert_eq!(task_count_after, 3, "no duplicate tasks on re-confirm");

    // task_global_project without its fixed project refuses the whole mint.
    let orphan = ServiceDeliveryRequest {
        order_id: Uuid::new_v4(), customer_id: customer,
        order_number: "SO-043".into(), currency: "IDR".into(),
        lines: vec![ServiceDeliveryLine {
            sale_line_id: Uuid::new_v4(), item_id: Uuid::new_v4(), quantity: dec("1"),
            description: None, rung: ServiceTrackingRung::TaskGlobalProject,
            fixed_project_id: None, template_id: None,
        }],
    };
    let refused = svc.mint_service_delivery(&orphan).await;
    assert!(matches!(refused, Err(ProjectError::Invalid(_))), "missing fixed project refuses");
}

/// PJ-5 — the origin-key uniques are DATABASE backstops, not bookkeeping: a raw second live project
/// for the same source sales order, or task for the same origin sale line, cannot be inserted even
/// by hand. Tenant-free since the tenancy strip (ADR-0029): the origin ids are globally unique, so
/// the bare-keyed partial uniques serve every tenant.
#[tokio::test]
async fn pj5_origin_key_uniques() {
    let pool = pool().await;
    let customer = Uuid::new_v4();
    let order = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.projects
             (id, project_name, project_type, customer_id, source_so_id, currency, status)
           VALUES ($1,'First','external',$2,$3,'IDR','open')"#,
    ).bind(Uuid::new_v4()).bind(customer).bind(order)
        .execute(&pool).await.unwrap();

    let dup_project = sqlx::query(
        r#"INSERT INTO project.projects
             (id, project_name, project_type, customer_id, source_so_id, currency, status)
           VALUES ($1,'Second','external',$2,$3,'IDR','open')"#,
    ).bind(Uuid::new_v4()).bind(customer).bind(order)
        .execute(&pool).await;
    assert!(dup_project.is_err(), "a second live project per source sales order cannot be inserted");

    let project: Uuid = sqlx::query_scalar(
        "SELECT id FROM project.projects WHERE source_so_id=$1")
        .bind(order).fetch_one(&pool).await.unwrap();
    let sale_line = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.tasks (id, project_id, subject, origin_sale_line_id)
           VALUES ($1,$2,'First task',$3)"#,
    ).bind(Uuid::new_v4()).bind(project).bind(sale_line)
        .execute(&pool).await.unwrap();
    let dup_task = sqlx::query(
        r#"INSERT INTO project.tasks (id, project_id, subject, origin_sale_line_id)
           VALUES ($1,$2,'Second task',$3)"#,
    ).bind(Uuid::new_v4()).bind(project).bind(sale_line)
        .execute(&pool).await;
    assert!(dup_task.is_err(), "a second live task per origin sale line cannot be inserted");
}

/// PB-6 — the period billability gate: `pending`, `rejected`, and NO cycle at all all refuse the
/// billing exit with the guarded error, and billing is never driven.
#[tokio::test]
async fn pb6_period_gate_refuses_non_approved() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = FakeBilling::new();
    let sink = LoggingSink;
    let act = an_activity(&pool).await;
    let project = svc.create_project(ext_project()).await.unwrap();

    for (employee, status) in [
        (Uuid::new_v4(), "pending"),
        (Uuid::new_v4(), "rejected"),
    ] {
        seed_row(&pool, employee, project, None, 2026, 7, 6, dec("4"),
            dec("500000"), dec("300000"), true, Some(act)).await;
        seed_approval(&pool, employee, 2026, 7, status).await;
        let refused = svc.bill_timesheet_period(project, employee, 2026, 7, &billing, &sink).await;
        assert!(matches!(refused, Err(ProjectError::Guarded(_))), "{status} cycle refuses");
    }

    // No cycle row at all (a month nobody submitted) — fails closed.
    let nobody = Uuid::new_v4();
    seed_row(&pool, nobody, project, None, 2026, 8, 4, dec("4"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    let no_cycle = svc.bill_timesheet_period(project, nobody, 2026, 8, &billing, &sink).await;
    assert!(matches!(no_cycle, Err(ProjectError::Guarded(_))), "absent cycle refuses");
    assert_eq!(billing.invoice_count(), 0, "billing never driven");
}
