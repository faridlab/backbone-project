use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;
use super::AuditMetadata;

/// Strongly-typed ID for TimesheetDetail
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimesheetDetailId(pub Uuid);

impl TimesheetDetailId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for TimesheetDetailId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TimesheetDetailId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for TimesheetDetailId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<TimesheetDetailId> for Uuid {
    fn from(id: TimesheetDetailId) -> Self { id.0 }
}

impl AsRef<Uuid> for TimesheetDetailId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for TimesheetDetailId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimesheetDetail {
    pub id: Uuid,
    pub timesheet_id: Uuid,
    pub activity_type_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub description: Option<String>,
    pub hours: Decimal,
    pub billing_rate: Decimal,
    pub costing_rate: Decimal,
    pub is_billable: bool,
    pub billable_amount: Decimal,
    pub costing_amount: Decimal,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl TimesheetDetail {
    /// Create a builder for TimesheetDetail
    pub fn builder() -> TimesheetDetailBuilder {
        TimesheetDetailBuilder::default()
    }

    /// Create a new TimesheetDetail with required fields
    pub fn new(timesheet_id: Uuid, hours: Decimal, billing_rate: Decimal, costing_rate: Decimal, is_billable: bool, billable_amount: Decimal, costing_amount: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            timesheet_id,
            activity_type_id: None,
            task_id: None,
            description: None,
            hours,
            billing_rate,
            costing_rate,
            is_billable,
            billable_amount,
            costing_amount,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> TimesheetDetailId {
        TimesheetDetailId(self.id)
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


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the activity_type_id field (chainable)
    pub fn with_activity_type_id(mut self, value: Uuid) -> Self {
        self.activity_type_id = Some(value);
        self
    }

    /// Set the task_id field (chainable)
    pub fn with_task_id(mut self, value: Uuid) -> Self {
        self.task_id = Some(value);
        self
    }

    /// Set the description field (chainable)
    pub fn with_description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "timesheet_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.timesheet_id = v; }
                }
                "activity_type_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.activity_type_id = v; }
                }
                "task_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.task_id = v; }
                }
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "hours" => {
                    if let Ok(v) = serde_json::from_value(value) { self.hours = v; }
                }
                "billing_rate" => {
                    if let Ok(v) = serde_json::from_value(value) { self.billing_rate = v; }
                }
                "costing_rate" => {
                    if let Ok(v) = serde_json::from_value(value) { self.costing_rate = v; }
                }
                "is_billable" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_billable = v; }
                }
                "billable_amount" => {
                    if let Ok(v) = serde_json::from_value(value) { self.billable_amount = v; }
                }
                "costing_amount" => {
                    if let Ok(v) = serde_json::from_value(value) { self.costing_amount = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for TimesheetDetail {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "TimesheetDetail"
    }
}

impl backbone_core::PersistentEntity for TimesheetDetail {
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

impl backbone_orm::EntityRepoMeta for TimesheetDetail {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("timesheet_id".to_string(), "uuid".to_string());
        m.insert("activity_type_id".to_string(), "uuid".to_string());
        m.insert("task_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for TimesheetDetail entity
///
/// Provides a fluent API for constructing TimesheetDetail instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct TimesheetDetailBuilder {
    timesheet_id: Option<Uuid>,
    activity_type_id: Option<Uuid>,
    task_id: Option<Uuid>,
    description: Option<String>,
    hours: Option<Decimal>,
    billing_rate: Option<Decimal>,
    costing_rate: Option<Decimal>,
    is_billable: Option<bool>,
    billable_amount: Option<Decimal>,
    costing_amount: Option<Decimal>,
}

impl TimesheetDetailBuilder {
    /// Set the timesheet_id field (required)
    pub fn timesheet_id(mut self, value: Uuid) -> Self {
        self.timesheet_id = Some(value);
        self
    }

    /// Set the activity_type_id field (optional)
    pub fn activity_type_id(mut self, value: Uuid) -> Self {
        self.activity_type_id = Some(value);
        self
    }

    /// Set the task_id field (optional)
    pub fn task_id(mut self, value: Uuid) -> Self {
        self.task_id = Some(value);
        self
    }

    /// Set the description field (optional)
    pub fn description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the hours field (required)
    pub fn hours(mut self, value: Decimal) -> Self {
        self.hours = Some(value);
        self
    }

    /// Set the billing_rate field (default: `Decimal::from(0)`)
    pub fn billing_rate(mut self, value: Decimal) -> Self {
        self.billing_rate = Some(value);
        self
    }

    /// Set the costing_rate field (default: `Decimal::from(0)`)
    pub fn costing_rate(mut self, value: Decimal) -> Self {
        self.costing_rate = Some(value);
        self
    }

    /// Set the is_billable field (default: `true`)
    pub fn is_billable(mut self, value: bool) -> Self {
        self.is_billable = Some(value);
        self
    }

    /// Set the billable_amount field (default: `Decimal::from(0)`)
    pub fn billable_amount(mut self, value: Decimal) -> Self {
        self.billable_amount = Some(value);
        self
    }

    /// Set the costing_amount field (default: `Decimal::from(0)`)
    pub fn costing_amount(mut self, value: Decimal) -> Self {
        self.costing_amount = Some(value);
        self
    }

    /// Build the TimesheetDetail entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<TimesheetDetail, String> {
        let timesheet_id = self.timesheet_id.ok_or_else(|| "timesheet_id is required".to_string())?;
        let hours = self.hours.ok_or_else(|| "hours is required".to_string())?;

        Ok(TimesheetDetail {
            id: Uuid::new_v4(),
            timesheet_id,
            activity_type_id: self.activity_type_id,
            task_id: self.task_id,
            description: self.description,
            hours,
            billing_rate: self.billing_rate.unwrap_or(Decimal::from(0)),
            costing_rate: self.costing_rate.unwrap_or(Decimal::from(0)),
            is_billable: self.is_billable.unwrap_or(true),
            billable_amount: self.billable_amount.unwrap_or(Decimal::from(0)),
            costing_amount: self.costing_amount.unwrap_or(Decimal::from(0)),
            metadata: AuditMetadata::default(),
        })
    }
}
