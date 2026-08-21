-- Migration: replace the two project lifecycle booleans with status enums
-- activity_types and project_templates each carried
-- `is_active BOOLEAN NOT NULL DEFAULT TRUE`; the tree-wide convention is one
-- `status` enum field per lifecycle (see docs/refactoring-schema in the serpa
-- workspace). Each boolean migrates only rows deviating from its own column
-- default. The enum types are created unqualified so they land beside the
-- module's other enum types (public), where the generated sqlx type_name
-- resolves. The old is_active indexes die with their column; status indexes
-- take their place.

DO $$ BEGIN
    CREATE TYPE activity_type_status AS ENUM ('active', 'inactive');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN
    CREATE TYPE project_template_status AS ENUM ('active', 'inactive');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

ALTER TABLE project.activity_types ADD COLUMN status activity_type_status NOT NULL DEFAULT 'active';
UPDATE project.activity_types SET status = 'inactive' WHERE NOT is_active;
ALTER TABLE project.activity_types DROP COLUMN is_active;

ALTER TABLE project.project_templates ADD COLUMN status project_template_status NOT NULL DEFAULT 'active';
UPDATE project.project_templates SET status = 'inactive' WHERE NOT is_active;
ALTER TABLE project.project_templates DROP COLUMN is_active;

CREATE INDEX IF NOT EXISTS idx_activity_types_company_id_status ON project.activity_types (company_id, status);
CREATE INDEX IF NOT EXISTS idx_project_templates_company_id_status ON project.project_templates (company_id, status);
