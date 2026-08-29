//! The billing seam, end-to-end: **backbone-project → the REAL backbone-billing**, over the
//! converged analytic row.
//!   PBSEAM-1 (approved period → Sales Invoice): `bill_timesheet_period` drives the REAL billing
//!              write path to create a service Sales Invoice from the billable slice; `invoice_id` →
//!              a real `billing.sales_invoices` row for the project's customer, at the billable
//!              total, numbered TS-{project}-{employee}-{YYYYMM}. Idempotent per slice.
//!   PBSEAM-2 (reversal → -R re-bill): after `unbill_invoice` clears the link, a re-bill mints a
//!              FRESH invoice numbered {base}-R1 (the adapter's stale-link retry duty).
//! This edge is a dev-dependency ONLY — the shipped project library depends on neither billing nor GL.

mod common;

use backbone_project::application::service::project_write_service::{NewProject, ProjectWriteService};
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

/// PBSEAM-1 — billing an approved period creates a REAL billing Sales Invoice for the project's
/// customer, numbered by the adapter's stable period convention.
#[tokio::test]
async fn pbseam1_period_bills_to_real_sales_invoice() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = RealBilling {
        svc: backbone_billing::application::service::billing_write_service::BillingWriteService::new(pool.clone()),
        pool: pool.clone(),
    };
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let customer = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;

    let project = svc.create_project(NewProject {
        company_id: company, project_name: "Acme Site".into(), project_type: "external".into(),
        customer_id: Some(customer), source_so_id: None, currency: Some("IDR".into()),
    }).await.unwrap();
    seed_row(&pool, company, employee, project, None, 2026, 7, 6, dec("8"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_approval(&pool, company, employee, 2026, 7, "approved").await;

    let out = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await.unwrap();
    assert!(!out.already);
    assert_eq!(out.amount, dec("4000000.00"));

    // A REAL billing Sales Invoice exists for the customer, at the billable total.
    let (cust, number, net): (Uuid, String, Decimal) = sqlx::query_as(
        "SELECT customer_id, invoice_number, net_total FROM billing.sales_invoices WHERE id=$1")
        .bind(out.invoice_id).fetch_one(&pool).await.unwrap();
    let key = PeriodKey::of(company, project, employee, 2026, 7);
    assert_eq!(cust, customer, "invoice is for the project's customer");
    assert_eq!(number, key.number(), "adapter numbers by the stable period convention");
    assert_eq!(net, dec("4000000.00"), "net = 8h · 500000");
    let line_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing.sales_invoice_lines WHERE invoice_id=$1")
        .bind(out.invoice_id).fetch_one(&pool).await.unwrap();
    assert_eq!(line_count, 1, "the billable row carried into the invoice");

    // Idempotent: a second bill hands off no second invoice.
    let again = svc.bill_timesheet_period(project, employee, 2026, 7, company, &billing, &sink).await.unwrap();
    assert!(again.already);
    assert_eq!(again.invoice_id, out.invoice_id);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing.sales_invoices WHERE invoice_number=$1")
        .bind(key.number()).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1, "exactly one invoice for the period");
}

/// PBSEAM-2 — the reversal + re-bill cycle through the REAL billing tables: `unbill_invoice` clears
/// the converged rows' link; a re-bill finds the old number STALE (no live row carries its invoice)
/// and retries with `-R1`, minting a fresh invoice at the same total.
#[tokio::test]
async fn pbseam2_unbill_then_rebill_mints_retry_number() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = RealBilling {
        svc: backbone_billing::application::service::billing_write_service::BillingWriteService::new(pool.clone()),
        pool: pool.clone(),
    };
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let customer = Uuid::new_v4();
    let employee = Uuid::new_v4();
    let act = an_activity(&pool, company).await;

    let project = svc.create_project(NewProject {
        company_id: company, project_name: "Acme Site 2".into(), project_type: "external".into(),
        customer_id: Some(customer), source_so_id: None, currency: Some("IDR".into()),
    }).await.unwrap();
    seed_row(&pool, company, employee, project, None, 2026, 8, 3, dec("5"),
        dec("500000"), dec("300000"), true, Some(act)).await;
    seed_approval(&pool, company, employee, 2026, 8, "approved").await;
    let key = PeriodKey::of(company, project, employee, 2026, 8);

    let first = svc.bill_timesheet_period(project, employee, 2026, 8, company, &billing, &sink).await.unwrap();
    svc.unbill_invoice(first.invoice_id, company, first.amount, &sink).await.unwrap();

    let second = svc.bill_timesheet_period(project, employee, 2026, 8, company, &billing, &sink).await.unwrap();
    assert!(!second.already);
    assert_ne!(second.invoice_id, first.invoice_id, "the re-bill is a fresh invoice");
    assert_eq!(second.amount, first.amount, "same slice total");

    let (number, net): (String, Decimal) = sqlx::query_as(
        "SELECT invoice_number, net_total FROM billing.sales_invoices WHERE id=$1")
        .bind(second.invoice_id).fetch_one(&pool).await.unwrap();
    assert_eq!(number, format!("{}-R1", key.number()), "retry numbering after a reversal");
    assert_eq!(net, dec("2500000.00"));

    // Both invoices exist in billing (the original + the retry); only the retry carries live links.
    let linked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM timesheet.timesheets WHERE invoice_id=$1")
        .bind(first.invoice_id).fetch_one(&pool).await.unwrap();
    assert_eq!(linked, 0, "the reversed invoice carries no live analytic rows");
    let billed: Decimal = sqlx::query_scalar(
        "SELECT total_billed_amount FROM project.projects WHERE id=$1")
        .bind(project).fetch_one(&pool).await.unwrap();
    assert_eq!(billed, dec("2500000.00"), "billed total reflects the re-bill only");
}
