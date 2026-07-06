use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;

use super::TaskStatus;
use super::AuditMetadata;

/// Strongly-typed ID for Task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TaskId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for TaskId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<TaskId> for Uuid {
    fn from(id: TaskId) -> Self { id.0 }
}

impl AsRef<Uuid> for TaskId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for TaskId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Task {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Uuid,
    pub parent_task_id: Option<Uuid>,
    pub subject: String,
    pub task_type: Option<String>,
    pub status: TaskStatus,
    pub expected_time: Decimal,
    pub progress: Decimal,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Task {
    /// Create a builder for Task
    pub fn builder() -> TaskBuilder {
        TaskBuilder::default()
    }

    /// Create a new Task with required fields
    pub fn new(company_id: Uuid, project_id: Uuid, subject: String, status: TaskStatus, expected_time: Decimal, progress: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            project_id,
            parent_task_id: None,
            subject,
            task_type: None,
            status,
            expected_time,
            progress,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> TaskId {
        TaskId(self.id)
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
    pub fn status(&self) -> &TaskStatus {
        &self.status
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the parent_task_id field (chainable)
    pub fn with_parent_task_id(mut self, value: Uuid) -> Self {
        self.parent_task_id = Some(value);
        self
    }

    /// Set the task_type field (chainable)
    pub fn with_task_type(mut self, value: String) -> Self {
        self.task_type = Some(value);
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
                "parent_task_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.parent_task_id = v; }
                }
                "subject" => {
                    if let Ok(v) = serde_json::from_value(value) { self.subject = v; }
                }
                "task_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.task_type = v; }
                }
                "status" => {
                    if let Ok(v) = serde_json::from_value(value) { self.status = v; }
                }
                "expected_time" => {
                    if let Ok(v) = serde_json::from_value(value) { self.expected_time = v; }
                }
                "progress" => {
                    if let Ok(v) = serde_json::from_value(value) { self.progress = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Task {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Task"
    }
}

impl backbone_core::PersistentEntity for Task {
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

impl backbone_orm::EntityRepoMeta for Task {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("project_id".to_string(), "uuid".to_string());
        m.insert("parent_task_id".to_string(), "uuid".to_string());
        m.insert("status".to_string(), "task_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["subject"]
    }
}

/// Builder for Task entity
///
/// Provides a fluent API for constructing Task instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct TaskBuilder {
    company_id: Option<Uuid>,
    project_id: Option<Uuid>,
    parent_task_id: Option<Uuid>,
    subject: Option<String>,
    task_type: Option<String>,
    status: Option<TaskStatus>,
    expected_time: Option<Decimal>,
    progress: Option<Decimal>,
}

impl TaskBuilder {
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

    /// Set the parent_task_id field (optional)
    pub fn parent_task_id(mut self, value: Uuid) -> Self {
        self.parent_task_id = Some(value);
        self
    }

    /// Set the subject field (required)
    pub fn subject(mut self, value: String) -> Self {
        self.subject = Some(value);
        self
    }

    /// Set the task_type field (optional)
    pub fn task_type(mut self, value: String) -> Self {
        self.task_type = Some(value);
        self
    }

    /// Set the status field (default: `TaskStatus::default()`)
    pub fn status(mut self, value: TaskStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Set the expected_time field (default: `Decimal::from(0)`)
    pub fn expected_time(mut self, value: Decimal) -> Self {
        self.expected_time = Some(value);
        self
    }

    /// Set the progress field (default: `Decimal::from(0)`)
    pub fn progress(mut self, value: Decimal) -> Self {
        self.progress = Some(value);
        self
    }

    /// Build the Task entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Task, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let project_id = self.project_id.ok_or_else(|| "project_id is required".to_string())?;
        let subject = self.subject.ok_or_else(|| "subject is required".to_string())?;

        Ok(Task {
            id: Uuid::new_v4(),
            company_id,
            project_id,
            parent_task_id: self.parent_task_id,
            subject,
            task_type: self.task_type,
            status: self.status.unwrap_or(TaskStatus::default()),
            expected_time: self.expected_time.unwrap_or(Decimal::from(0)),
            progress: self.progress.unwrap_or(Decimal::from(0)),
            metadata: AuditMetadata::default(),
        })
    }
}
