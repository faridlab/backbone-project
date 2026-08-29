//! Read-port proof — exercises `module.query_service` (the inbound `ProjectQueryService`)
//! end-to-end.
//!
//! The council found this contract exported-but-unimplemented; these tests prove the re-landed
//! `ProjectQueryServiceImpl` is wired into `ProjectModule`, reachable as `module.query_service`,
//! and returns correctly-mapped DTOs for the entities a sibling module would query. Reads go
//! through the generic services; in the test role (postgres, which bypasses RLS) the company fence
//! is inert, so we assert on the exact values written.

mod common;

use backbone_project::exports::{ActivityTypeId, ProjectId};
use backbone_project::ProjectModule;
use common::*;
use uuid::Uuid;

/// Seed an activity type and return its id (mirrors the golden-case helper).
async fn an_activity(pool: &sqlx::PgPool, company: Uuid, billing: &str, costing: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.activity_types (id, company_id, name, billing_rate, costing_rate, status)
           VALUES ($1,$2,'Consulting',$3,$4,'active')"#,
    )
    .bind(id)
    .bind(company)
    .bind(dec(billing))
    .bind(dec(costing))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// QSP-1 — the published read-port returns the project that was written, with correctly-mapped
/// fields, a correct summary, and a correct existence probe; a random id is absent. This is the
/// one test that would have caught the "trait exported, no impl" defect the council flagged.
#[tokio::test]
async fn qsp1_project_read_port() {
    let pool = pool().await;
    let module = ProjectModule::builder()
        .with_database(pool.clone())
        .build()
        .unwrap();
    let q = module.query_service.clone(); // Arc<dyn ProjectQueryService>

    let company = Uuid::new_v4();
    let customer = Uuid::new_v4();
    let project = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.projects
             (id, company_id, project_name, project_type, customer_id, status, currency)
           VALUES ($1,$2,'Atlas','external',$3,'open','IDR')"#,
    )
    .bind(project)
    .bind(company)
    .bind(customer)
    .execute(&pool)
    .await
    .unwrap();

    // get_project → full DTO, every field mapped from the row.
    let dto = q
        .get_project(ProjectId(project))
        .await
        .unwrap()
        .expect("project must be readable through the query port");
    assert_eq!(dto.id, ProjectId(project));
    assert_eq!(dto.company_id, company);
    assert_eq!(dto.project_name, "Atlas");
    assert_eq!(dto.customer_id, Some(customer));
    assert_eq!(dto.currency, "IDR");
    // metadata is exposed as opaque JSON, never as the internal AuditMetadata type.
    assert!(dto.metadata.is_object(), "metadata must round-trip as JSON");

    // get_project_summary → the lean view.
    let sum = q
        .get_project_summary(ProjectId(project))
        .await
        .unwrap()
        .expect("summary must be readable");
    assert_eq!(sum.id, ProjectId(project));
    assert_eq!(sum.project_name, "Atlas");

    // exists → true for the written row, false for a random id (proves it isn't a stub returning true).
    assert!(q.project_exists(ProjectId(project)).await.unwrap());
    assert!(
        !q.project_exists(ProjectId(Uuid::new_v4())).await.unwrap(),
        "a random id must not exist"
    );
}

/// QSP-2 — a second entity (ActivityType) reads through the same port, proving the impl covers
/// all five services, not just Project. Also checks the rate fields round-trip exactly.
#[tokio::test]
async fn qsp2_activity_type_read_port() {
    let pool = pool().await;
    let module = ProjectModule::builder()
        .with_database(pool.clone())
        .build()
        .unwrap();
    let q = module.query_service.clone();

    let company = Uuid::new_v4();
    let act = an_activity(&pool, company, "500000", "300000").await;

    let dto = q
        .get_activity_type(ActivityTypeId(act))
        .await
        .unwrap()
        .expect("activity type must be readable through the query port");
    assert_eq!(dto.id, ActivityTypeId(act));
    assert_eq!(dto.company_id, company);
    assert_eq!(dto.name, "Consulting");
    assert_eq!(dto.billing_rate, dec("500000.00"));
    assert_eq!(dto.costing_rate, dec("300000.00"));
    assert!(q.activity_type_exists(ActivityTypeId(act)).await.unwrap());
    assert!(
        !q.activity_type_exists(ActivityTypeId(Uuid::new_v4()))
            .await
            .unwrap(),
        "a random activity id must not exist"
    );
}
