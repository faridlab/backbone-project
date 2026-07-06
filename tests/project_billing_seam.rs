//! The billing seam, end-to-end: **backbone-project → the REAL backbone-billing**.
//!   PBSEAM-1 (billable Timesheet → Sales Invoice): `bill_timesheet` drives the REAL billing write path
//!            to create a service Sales Invoice from the billable lines; `invoice_id` → a real
//!            `billing.sales_invoices` row for the project's customer, at the billable total. Idempotent.
//! This edge is a dev-dependency ONLY — the shipped project library depends on neither billing nor GL.

mod common;

use backbone_project::application::service::project_write_service::{
    NewProject, NewTimeLine, NewTimesheet, ProjectWriteService,
};
use common::*;
use rust_decimal::Decimal;
use uuid::Uuid;

/// PBSEAM-1 — billing a timesheet creates a REAL billing Sales Invoice for the project's customer.
#[tokio::test]
async fn pbseam1_timesheet_bills_to_real_sales_invoice() {
    let pool = pool().await;
    let svc = ProjectWriteService::new(pool.clone());
    let billing = RealBilling {
        svc: backbone_billing::application::service::billing_write_service::BillingWriteService::new(pool.clone()),
        pool: pool.clone(),
    };
    let sink = LoggingSink;
    let company = Uuid::new_v4();
    let customer = Uuid::new_v4();
    let act = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO project.activity_types (id, company_id, name, billing_rate, costing_rate, is_active)
           VALUES ($1,$2,'Consulting',$3,$4,true)"#,
    ).bind(act).bind(company).bind(dec("500000")).bind(dec("300000")).execute(&pool).await.unwrap();

    let project = svc.create_project(NewProject {
        company_id: company, project_name: "Acme Site".into(), project_type: "external".into(),
        customer_id: Some(customer), source_so_id: None, currency: Some("IDR".into()),
    }).await.unwrap();
    let ts = svc.log_time(project, NewTimesheet {
        employee_id: None, currency: None,
        lines: vec![NewTimeLine { activity_type_id: Some(act), task_id: None, description: Some("Build".into()),
            hours: dec("8"), billing_rate: None, costing_rate: None, is_billable: true }],
    }, &sink).await.unwrap();

    let out = svc.bill_timesheet(ts, &billing, &sink).await.unwrap();
    assert!(!out.already);
    assert_eq!(out.amount, dec("4000000.00"));

    // A REAL billing Sales Invoice exists for the customer, at the billable total.
    let (cust, number, net): (Uuid, String, Decimal) = sqlx::query_as(
        "SELECT customer_id, invoice_number, net_total FROM billing.sales_invoices WHERE id=$1")
        .bind(out.invoice_id).fetch_one(&pool).await.unwrap();
    assert_eq!(cust, customer, "invoice is for the project's customer");
    assert_eq!(number, format!("TS-{ts}"));
    assert_eq!(net, dec("4000000.00"), "net = 8h · 500000");
    let line_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing.sales_invoice_lines WHERE invoice_id=$1")
        .bind(out.invoice_id).fetch_one(&pool).await.unwrap();
    assert_eq!(line_count, 1, "the billable line carried into the invoice");

    // Idempotent: a second bill hands off no second invoice.
    let again = svc.bill_timesheet(ts, &billing, &sink).await.unwrap();
    assert!(again.already);
    assert_eq!(again.invoice_id, out.invoice_id);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing.sales_invoices WHERE invoice_number=$1")
        .bind(format!("TS-{ts}")).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1, "exactly one invoice for the timesheet");
}
