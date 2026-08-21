use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::ProjectType;
use super::ProjectTemplateStatus;
use super::AuditMetadata;

/// Strongly-typed ID for ProjectTemplate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectTemplateId(pub Uuid);

impl ProjectTemplateId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for ProjectTemplateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ProjectTemplateId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for ProjectTemplateId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<ProjectTemplateId> for Uuid {
    fn from(id: ProjectTemplateId) -> Self { id.0 }
}

impl AsRef<Uuid> for ProjectTemplateId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for ProjectTemplateId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectTemplate {
    pub id: Uuid,
    pub company_id: Uuid,
    pub template_name: String,
    pub project_type: ProjectType,
    pub status: ProjectTemplateStatus,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl ProjectTemplate {
    /// Create a builder for ProjectTemplate
    pub fn builder() -> ProjectTemplateBuilder {
        <ProjectTemplateBuilder as Default>::default()
    }

    /// Create a new ProjectTemplate with required fields
    pub fn new(company_id: Uuid, template_name: String, project_type: ProjectType, status: ProjectTemplateStatus) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            template_name,
            project_type,
            status,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> ProjectTemplateId {
        ProjectTemplateId(self.id)
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
    pub fn status(&self) -> &ProjectTemplateStatus {
        &self.status
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
                "template_name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.template_name = v; }
                }
                "project_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.project_type = v; }
                }
                "status" => {
                    if let Ok(v) = serde_json::from_value(value) { self.status = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for ProjectTemplate {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "ProjectTemplate"
    }
}

impl backbone_core::PersistentEntity for ProjectTemplate {
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

impl backbone_orm::EntityRepoMeta for ProjectTemplate {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("project_type".to_string(), "project_type".to_string());
        m.insert("status".to_string(), "project_template_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["template_name"]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// Builder for ProjectTemplate entity
///
/// Provides a fluent API for constructing ProjectTemplate instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct ProjectTemplateBuilder {
    company_id: Option<Uuid>,
    template_name: Option<String>,
    project_type: Option<ProjectType>,
    status: Option<ProjectTemplateStatus>,
}

impl ProjectTemplateBuilder {
    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Set the template_name field (required)
    pub fn template_name(mut self, value: String) -> Self {
        self.template_name = Some(value);
        self
    }

    /// Set the project_type field (default: `ProjectType::default()`)
    pub fn project_type(mut self, value: ProjectType) -> Self {
        self.project_type = Some(value);
        self
    }

    /// Set the status field (default: `ProjectTemplateStatus::default()`)
    pub fn status(mut self, value: ProjectTemplateStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Build the ProjectTemplate entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<ProjectTemplate, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let template_name = self.template_name.ok_or_else(|| "template_name is required".to_string())?;

        Ok(ProjectTemplate {
            id: Uuid::new_v4(),
            company_id,
            template_name,
            project_type: self.project_type.unwrap_or_default(),
            status: self.status.unwrap_or_default(),
            metadata: AuditMetadata::default(),
        })
    }
}
