use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;
use super::AuditMetadata;

/// Strongly-typed ID for ActivityType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivityTypeId(pub Uuid);

impl ActivityTypeId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for ActivityTypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ActivityTypeId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for ActivityTypeId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<ActivityTypeId> for Uuid {
    fn from(id: ActivityTypeId) -> Self { id.0 }
}

impl AsRef<Uuid> for ActivityTypeId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for ActivityTypeId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActivityType {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub billing_rate: Decimal,
    pub costing_rate: Decimal,
    pub is_active: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl ActivityType {
    /// Create a builder for ActivityType
    pub fn builder() -> ActivityTypeBuilder {
        ActivityTypeBuilder::default()
    }

    /// Create a new ActivityType with required fields
    pub fn new(company_id: Uuid, name: String, billing_rate: Decimal, costing_rate: Decimal, is_active: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            name,
            billing_rate,
            costing_rate,
            is_active,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> ActivityTypeId {
        ActivityTypeId(self.id)
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
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "company_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.company_id = v; }
                }
                "name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.name = v; }
                }
                "billing_rate" => {
                    if let Ok(v) = serde_json::from_value(value) { self.billing_rate = v; }
                }
                "costing_rate" => {
                    if let Ok(v) = serde_json::from_value(value) { self.costing_rate = v; }
                }
                "is_active" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_active = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for ActivityType {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "ActivityType"
    }
}

impl backbone_core::PersistentEntity for ActivityType {
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

impl backbone_orm::EntityRepoMeta for ActivityType {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for ActivityType entity
///
/// Provides a fluent API for constructing ActivityType instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct ActivityTypeBuilder {
    company_id: Option<Uuid>,
    name: Option<String>,
    billing_rate: Option<Decimal>,
    costing_rate: Option<Decimal>,
    is_active: Option<bool>,
}

impl ActivityTypeBuilder {
    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
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

    /// Set the is_active field (default: `true`)
    pub fn is_active(mut self, value: bool) -> Self {
        self.is_active = Some(value);
        self
    }

    /// Build the ActivityType entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<ActivityType, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(ActivityType {
            id: Uuid::new_v4(),
            company_id,
            name,
            billing_rate: self.billing_rate.unwrap_or(Decimal::from(0)),
            costing_rate: self.costing_rate.unwrap_or(Decimal::from(0)),
            is_active: self.is_active.unwrap_or(true),
            metadata: AuditMetadata::default(),
        })
    }
}
