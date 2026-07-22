use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;

use super::ProjectType;
use super::ProjectStatus;
use super::AuditMetadata;

/// Strongly-typed ID for Project
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(pub Uuid);

impl ProjectId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ProjectId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for ProjectId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<ProjectId> for Uuid {
    fn from(id: ProjectId) -> Self { id.0 }
}

impl AsRef<Uuid> for ProjectId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for ProjectId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_name: String,
    pub project_type: ProjectType,
    pub customer_id: Option<Uuid>,
    pub source_so_id: Option<Uuid>,
    pub currency: String,
    pub status: ProjectStatus,
    pub expected_start_date: Option<DateTime<Utc>>,
    pub expected_end_date: Option<DateTime<Utc>>,
    pub total_costing_amount: Decimal,
    pub total_billable_amount: Decimal,
    pub total_billed_amount: Decimal,
    pub notes: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Project {
    /// Create a builder for Project
    pub fn builder() -> ProjectBuilder {
        ProjectBuilder::default()
    }

    /// Create a new Project with required fields
    pub fn new(company_id: Uuid, project_name: String, project_type: ProjectType, currency: String, status: ProjectStatus, total_costing_amount: Decimal, total_billable_amount: Decimal, total_billed_amount: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            project_name,
            project_type,
            customer_id: None,
            source_so_id: None,
            currency,
            status,
            expected_start_date: None,
            expected_end_date: None,
            total_costing_amount,
            total_billable_amount,
            total_billed_amount,
            notes: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> ProjectId {
        ProjectId(self.id)
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
    pub fn status(&self) -> &ProjectStatus {
        &self.status
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the customer_id field (chainable)
    pub fn with_customer_id(mut self, value: Uuid) -> Self {
        self.customer_id = Some(value);
        self
    }

    /// Set the source_so_id field (chainable)
    pub fn with_source_so_id(mut self, value: Uuid) -> Self {
        self.source_so_id = Some(value);
        self
    }

    /// Set the expected_start_date field (chainable)
    pub fn with_expected_start_date(mut self, value: DateTime<Utc>) -> Self {
        self.expected_start_date = Some(value);
        self
    }

    /// Set the expected_end_date field (chainable)
    pub fn with_expected_end_date(mut self, value: DateTime<Utc>) -> Self {
        self.expected_end_date = Some(value);
        self
    }

    /// Set the notes field (chainable)
    pub fn with_notes(mut self, value: String) -> Self {
        self.notes = Some(value);
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
                "project_name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.project_name = v; }
                }
                "project_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.project_type = v; }
                }
                "customer_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.customer_id = v; }
                }
                "source_so_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.source_so_id = v; }
                }
                "currency" => {
                    if let Ok(v) = serde_json::from_value(value) { self.currency = v; }
                }
                "status" => {
                    if let Ok(v) = serde_json::from_value(value) { self.status = v; }
                }
                "expected_start_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.expected_start_date = v; }
                }
                "expected_end_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.expected_end_date = v; }
                }
                "total_costing_amount" => {
                    if let Ok(v) = serde_json::from_value(value) { self.total_costing_amount = v; }
                }
                "total_billable_amount" => {
                    if let Ok(v) = serde_json::from_value(value) { self.total_billable_amount = v; }
                }
                "total_billed_amount" => {
                    if let Ok(v) = serde_json::from_value(value) { self.total_billed_amount = v; }
                }
                "notes" => {
                    if let Ok(v) = serde_json::from_value(value) { self.notes = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Project {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Project"
    }
}

impl backbone_core::PersistentEntity for Project {
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

impl backbone_orm::EntityRepoMeta for Project {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("customer_id".to_string(), "uuid".to_string());
        m.insert("source_so_id".to_string(), "uuid".to_string());
        m.insert("project_type".to_string(), "project_type".to_string());
        m.insert("status".to_string(), "project_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["project_name", "currency"]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// Builder for Project entity
///
/// Provides a fluent API for constructing Project instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct ProjectBuilder {
    company_id: Option<Uuid>,
    project_name: Option<String>,
    project_type: Option<ProjectType>,
    customer_id: Option<Uuid>,
    source_so_id: Option<Uuid>,
    currency: Option<String>,
    status: Option<ProjectStatus>,
    expected_start_date: Option<DateTime<Utc>>,
    expected_end_date: Option<DateTime<Utc>>,
    total_costing_amount: Option<Decimal>,
    total_billable_amount: Option<Decimal>,
    total_billed_amount: Option<Decimal>,
    notes: Option<String>,
}

impl ProjectBuilder {
    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Set the project_name field (required)
    pub fn project_name(mut self, value: String) -> Self {
        self.project_name = Some(value);
        self
    }

    /// Set the project_type field (default: `ProjectType::default()`)
    pub fn project_type(mut self, value: ProjectType) -> Self {
        self.project_type = Some(value);
        self
    }

    /// Set the customer_id field (optional)
    pub fn customer_id(mut self, value: Uuid) -> Self {
        self.customer_id = Some(value);
        self
    }

    /// Set the source_so_id field (optional)
    pub fn source_so_id(mut self, value: Uuid) -> Self {
        self.source_so_id = Some(value);
        self
    }

    /// Set the currency field (default: `"IDR".to_string()`)
    pub fn currency(mut self, value: String) -> Self {
        self.currency = Some(value);
        self
    }

    /// Set the status field (default: `ProjectStatus::default()`)
    pub fn status(mut self, value: ProjectStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Set the expected_start_date field (optional)
    pub fn expected_start_date(mut self, value: DateTime<Utc>) -> Self {
        self.expected_start_date = Some(value);
        self
    }

    /// Set the expected_end_date field (optional)
    pub fn expected_end_date(mut self, value: DateTime<Utc>) -> Self {
        self.expected_end_date = Some(value);
        self
    }

    /// Set the total_costing_amount field (default: `Decimal::from(0)`)
    pub fn total_costing_amount(mut self, value: Decimal) -> Self {
        self.total_costing_amount = Some(value);
        self
    }

    /// Set the total_billable_amount field (default: `Decimal::from(0)`)
    pub fn total_billable_amount(mut self, value: Decimal) -> Self {
        self.total_billable_amount = Some(value);
        self
    }

    /// Set the total_billed_amount field (default: `Decimal::from(0)`)
    pub fn total_billed_amount(mut self, value: Decimal) -> Self {
        self.total_billed_amount = Some(value);
        self
    }

    /// Set the notes field (optional)
    pub fn notes(mut self, value: String) -> Self {
        self.notes = Some(value);
        self
    }

    /// Build the Project entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Project, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let project_name = self.project_name.ok_or_else(|| "project_name is required".to_string())?;

        Ok(Project {
            id: Uuid::new_v4(),
            company_id,
            project_name,
            project_type: self.project_type.unwrap_or(ProjectType::default()),
            customer_id: self.customer_id,
            source_so_id: self.source_so_id,
            currency: self.currency.unwrap_or("IDR".to_string()),
            status: self.status.unwrap_or(ProjectStatus::default()),
            expected_start_date: self.expected_start_date,
            expected_end_date: self.expected_end_date,
            total_costing_amount: self.total_costing_amount.unwrap_or(Decimal::from(0)),
            total_billable_amount: self.total_billable_amount.unwrap_or(Decimal::from(0)),
            total_billed_amount: self.total_billed_amount.unwrap_or(Decimal::from(0)),
            notes: self.notes,
            metadata: AuditMetadata::default(),
        })
    }
}
