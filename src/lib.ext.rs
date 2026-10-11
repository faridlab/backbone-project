//! The module's extension (hand-written; ADR-0031): what the module adds beside its
//! generated services. The generated module and builder carry `ModuleExt` and
//! `ModuleBuilderExt` and dereference to them, so their fields read as `module.field`.

#[allow(unused_imports)]
use super::*;

/// State the module adds; read through the generated module's `Deref`.
pub struct ModuleExt {
    /// Inbound read-port for sibling modules (`use project::exports::ProjectQueryService`).
    /// `pub` (not `pub(crate)`) so siblings can obtain the trait object — clone the Arc
    /// to hold `Arc<dyn ProjectQueryService>`. Lives in the CUSTOM FIELDS block (regen-safe).
    pub query_service: std::sync::Arc<dyn crate::exports::ProjectQueryService>,
    /// The validated write surface (project/task lifecycle, hybrid task status, the billing
    /// exit, the confirm-mint, guarded deletes, the derived financial reads). Composing hosts
    /// drive this; the generic-CRUD services above remain the trusted/admin escape hatch.
    pub write_service: std::sync::Arc<crate::application::service::ProjectWriteService>,
}

/// State the builder adds.
pub struct ModuleBuilderExt {
}

impl Default for ModuleBuilderExt {
    fn default() -> Self {
        Self {
        }
    }
}

impl ModuleBuilderExt {
    /// Build the extension's state from what the generated build made.
    #[allow(unused_variables, clippy::redundant_clone)]
    pub(crate) fn build(self, parts: &ModuleParts<'_>) -> anyhow::Result<ModuleExt> {
        let db_pool = parts.db_pool.clone();
        let activity_type_service = parts.activity_type_service.clone();
        let project_service = parts.project_service.clone();
        let project_template_service = parts.project_template_service.clone();
        let project_template_task_service = parts.project_template_task_service.clone();
        let task_service = parts.task_service.clone();
        let query_service: std::sync::Arc<dyn crate::exports::ProjectQueryService> =
            std::sync::Arc::new(crate::exports::ProjectQueryServiceImpl::new(
                activity_type_service.clone(),
                project_service.clone(),
                project_template_service.clone(),
                project_template_task_service.clone(),
                task_service.clone(),
            ));
        let write_service = std::sync::Arc::new(
            crate::application::service::ProjectWriteService::new(db_pool.clone()),
        );
        Ok(ModuleExt {
            query_service,
            write_service,
        })
    }
}
