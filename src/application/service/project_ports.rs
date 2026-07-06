//! Project's one outbound port (hand-authored, user-owned) — the seam where delivered time becomes a
//! transaction. Projects holds only this trait + its own DTOs; a composing service wires the real
//! backbone-billing behind it. **Zero normal Cargo edge** to billing — the DTOs are the wire contract,
//! duplicated per consumer by design. Projects never posts to the GL: it hands a billable timesheet to
//! billing, which builds the Sales Invoice and posts (Dr A/R · Cr Service Revenue · Cr PPN).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One billable timesheet line carried into the Sales Invoice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvoiceLineFromTimesheet {
    /// Logical item id representing the activity/service billed (from the activity type).
    pub item_id: Uuid,
    pub description: Option<String>,
    /// Hours become the invoice quantity.
    pub hours: Decimal,
    /// The snapshotted billing rate becomes the unit price.
    pub rate: Decimal,
}

/// Hand a submitted, billable timesheet off to billing as a service Sales Invoice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvoiceFromTimesheet {
    pub company_id: Uuid,
    pub project_id: Uuid,
    pub timesheet_id: Uuid,
    pub customer_id: Uuid,
    pub currency: String,
    pub lines: Vec<InvoiceLineFromTimesheet>,
}

/// The created Sales Invoice id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvoiceAck {
    pub invoice_id: Uuid,
}

/// The billing seam — a composing service implements it over backbone-billing.
#[async_trait::async_trait]
pub trait BillingPort: Send + Sync {
    async fn create_service_invoice(
        &self,
        req: &InvoiceFromTimesheet,
    ) -> Result<InvoiceAck, ProjectRejected>;
}

/// A downstream rejection surfaced to projects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRejected {
    pub code: String,
    pub message: String,
}
