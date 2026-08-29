//! Project's one outbound port (hand-authored, user-owned) — the seam where delivered time becomes a
//! transaction. Projects holds only this trait + its own DTOs; a composing service wires the real
//! backbone-billing behind it. **Zero normal Cargo edge** to billing — the DTOs are the wire contract,
//! duplicated per consumer by design. Projects never posts to the GL: it hands an approved period
//! slice of the converged analytic rows to billing, which builds the Sales Invoice and posts
//! (Dr A/R · Cr Service Revenue · Cr PPN).
//!
//! **Adapter duty — invoice numbering and the reversal retry.** The billing unit is the
//! (project, employee, year, month) slice, so the natural idempotency key is
//! `TS-{project_id}-{employee_id}-{YYYYMM}`. When a reversed slice is re-billed, the new invoice
//! retries with the suffix `-R{n}` (n = prior invoices on that key, discovered by the
//! duplicate-number lookup: on a duplicate number, check whether the found invoice is still linked
//! to live rows; if it is, return it — the idempotent case; if it is not, it is a reversed prior
//! invoice, so mint the next `-R{n}`). The module deliberately knows none of this — the wire
//! contract carries the slice, the adapter owns billing's vocabulary.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One billable converged row carried into the Sales Invoice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvoiceLineFromTimesheet {
    /// The converged analytic row the line bills (for traceability).
    pub row_id: Uuid,
    /// Logical item id representing the activity/service billed (from the activity type).
    pub item_id: Uuid,
    pub description: Option<String>,
    /// Hours become the invoice quantity.
    pub hours: Decimal,
    /// The plain-stored billing-rate snapshot becomes the unit price.
    pub rate: Decimal,
}

/// Hand an approved (project, employee, year, month) slice of billable converged rows off to
/// billing as a service Sales Invoice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvoiceFromTimesheetPeriod {
    pub company_id: Uuid,
    pub project_id: Uuid,
    pub employee_id: Uuid,
    pub year: i32,
    pub month: i32,
    pub customer_id: Uuid,
    pub currency: String,
    pub lines: Vec<InvoiceLineFromTimesheet>,
}

/// The created Sales Invoice id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvoiceAck {
    pub invoice_id: Uuid,
}

/// The billing seam — a composing service implements it over backbone-billing. The call is
/// idempotent per period slice (see the adapter-duty note on the module).
#[async_trait::async_trait]
pub trait BillingPort: Send + Sync {
    async fn create_service_invoice(
        &self,
        req: &InvoiceFromTimesheetPeriod,
    ) -> Result<InvoiceAck, ProjectRejected>;
}

/// A downstream rejection surfaced to projects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRejected {
    pub code: String,
    pub message: String,
}
