use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;

use super::TimesheetStatus;
use super::AuditMetadata;

/// Strongly-typed ID for Timesheet
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimesheetId(pub Uuid);

impl TimesheetId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for TimesheetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TimesheetId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for TimesheetId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<TimesheetId> for Uuid {
    fn from(id: TimesheetId) -> Self { id.0 }
}

impl AsRef<Uuid> for TimesheetId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for TimesheetId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Timesheet {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Uuid,
    pub employee_id: Option<Uuid>,
    pub currency: String,
    pub status: TimesheetStatus,
    pub total_hours: Decimal,
    pub total_billable_amount: Decimal,
    pub total_costing_amount: Decimal,
    pub invoice_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Timesheet {
    /// Create a builder for Timesheet
    pub fn builder() -> TimesheetBuilder {
        TimesheetBuilder::default()
    }

    /// Create a new Timesheet with required fields
    pub fn new(company_id: Uuid, project_id: Uuid, currency: String, status: TimesheetStatus, total_hours: Decimal, total_billable_amount: Decimal, total_costing_amount: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            project_id,
            employee_id: None,
            currency,
            status,
            total_hours,
            total_billable_amount,
            total_costing_amount,
            invoice_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> TimesheetId {
        TimesheetId(self.id)
    }

    /// Get when this entity was created
    pub fn created_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.created_at.as_ref()
    }

    /// Get when this entity was last updated
    pub fn updated_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.updated_at.as_ref()
    }

    /// Check if this entity is soft deleted
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted_at.is_some()
    }

    /// Check if this entity is active (not deleted)
    pub fn is_active(&self) -> bool {
        self.metadata.deleted_at.is_none()
    }

    /// Get when this entity was deleted
    pub fn deleted_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.deleted_at.as_ref()
    }

    /// Get who created this entity
    pub fn created_by(&self) -> Option<&Uuid> {
        self.metadata.created_by.as_ref()
    }

    /// Get who last updated this entity
    pub fn updated_by(&self) -> Option<&Uuid> {
        self.metadata.updated_by.as_ref()
    }

    /// Get who deleted this entity
    pub fn deleted_by(&self) -> Option<&Uuid> {
        self.metadata.deleted_by.as_ref()
    }

    /// Get the current status
    pub fn status(&self) -> &TimesheetStatus {
        &self.status
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the employee_id field (chainable)
    pub fn with_employee_id(mut self, value: Uuid) -> Self {
        self.employee_id = Some(value);
        self
    }

    /// Set the invoice_id field (chainable)
    pub fn with_invoice_id(mut self, value: Uuid) -> Self {
        self.invoice_id = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "company_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.company_id = v; }
                }
                "project_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.project_id = v; }
                }
                "employee_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.employee_id = v; }
                }
                "currency" => {
                    if let Ok(v) = serde_json::from_value(value) { self.currency = v; }
                }
                "status" => {
                    if let Ok(v) = serde_json::from_value(value) { self.status = v; }
                }
                "total_hours" => {
                    if let Ok(v) = serde_json::from_value(value) { self.total_hours = v; }
                }
                "total_billable_amount" => {
                    if let Ok(v) = serde_json::from_value(value) { self.total_billable_amount = v; }
                }
                "total_costing_amount" => {
                    if let Ok(v) = serde_json::from_value(value) { self.total_costing_amount = v; }
                }
                "invoice_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.invoice_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Timesheet {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Timesheet"
    }
}

impl backbone_core::PersistentEntity for Timesheet {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            self.id = uuid;
        }
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.created_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.created_at = Some(ts);
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.updated_at
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.updated_at = Some(ts);
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.deleted_at
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.metadata.deleted_at = ts;
    }
}

impl backbone_orm::EntityRepoMeta for Timesheet {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("project_id".to_string(), "uuid".to_string());
        m.insert("employee_id".to_string(), "uuid".to_string());
        m.insert("invoice_id".to_string(), "uuid".to_string());
        m.insert("status".to_string(), "timesheet_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["currency"]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// Builder for Timesheet entity
///
/// Provides a fluent API for constructing Timesheet instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct TimesheetBuilder {
    company_id: Option<Uuid>,
    project_id: Option<Uuid>,
    employee_id: Option<Uuid>,
    currency: Option<String>,
    status: Option<TimesheetStatus>,
    total_hours: Option<Decimal>,
    total_billable_amount: Option<Decimal>,
    total_costing_amount: Option<Decimal>,
    invoice_id: Option<Uuid>,
}

impl TimesheetBuilder {
    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Set the project_id field (required)
    pub fn project_id(mut self, value: Uuid) -> Self {
        self.project_id = Some(value);
        self
    }

    /// Set the employee_id field (optional)
    pub fn employee_id(mut self, value: Uuid) -> Self {
        self.employee_id = Some(value);
        self
    }

    /// Set the currency field (default: `"IDR".to_string()`)
    pub fn currency(mut self, value: String) -> Self {
        self.currency = Some(value);
        self
    }

    /// Set the status field (default: `TimesheetStatus::default()`)
    pub fn status(mut self, value: TimesheetStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Set the total_hours field (default: `Decimal::from(0)`)
    pub fn total_hours(mut self, value: Decimal) -> Self {
        self.total_hours = Some(value);
        self
    }

    /// Set the total_billable_amount field (default: `Decimal::from(0)`)
    pub fn total_billable_amount(mut self, value: Decimal) -> Self {
        self.total_billable_amount = Some(value);
        self
    }

    /// Set the total_costing_amount field (default: `Decimal::from(0)`)
    pub fn total_costing_amount(mut self, value: Decimal) -> Self {
        self.total_costing_amount = Some(value);
        self
    }

    /// Set the invoice_id field (optional)
    pub fn invoice_id(mut self, value: Uuid) -> Self {
        self.invoice_id = Some(value);
        self
    }

    /// Build the Timesheet entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Timesheet, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let project_id = self.project_id.ok_or_else(|| "project_id is required".to_string())?;

        Ok(Timesheet {
            id: Uuid::new_v4(),
            company_id,
            project_id,
            employee_id: self.employee_id,
            currency: self.currency.unwrap_or("IDR".to_string()),
            status: self.status.unwrap_or(TimesheetStatus::default()),
            total_hours: self.total_hours.unwrap_or(Decimal::from(0)),
            total_billable_amount: self.total_billable_amount.unwrap_or(Decimal::from(0)),
            total_costing_amount: self.total_costing_amount.unwrap_or(Decimal::from(0)),
            invoice_id: self.invoice_id,
            metadata: AuditMetadata::default(),
        })
    }
}
