//! The Postgres type names the entities' enums decode against.
//!
//! sqlx checks a column's type by name before decoding it. `task_status` is
//! declared by two modules (lifecycle and project), each in its own schema, so
//! a bare name matches neither column and every task read fails to decode. The
//! enum names its schema in `schema/models/task.model.yaml`; this keeps a
//! regeneration from dropping it again.

use backbone_project::TaskStatus;
use sqlx::{Postgres, Type, TypeInfo};

#[test]
fn task_status_decodes_against_the_project_schema_type() {
    assert_eq!(
        <TaskStatus as Type<Postgres>>::type_info().name(),
        "project.task_status"
    );
}
