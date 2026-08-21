-- Down: restore the two is_active booleans exactly as they were.
-- Only 'inactive' rows are written back as FALSE; rows at the column default
-- map to the boolean default TRUE without an UPDATE. The status indexes die
-- with their column; the original is_active indexes are recreated by name.

ALTER TABLE project.activity_types ADD COLUMN is_active BOOLEAN NOT NULL DEFAULT TRUE;
UPDATE project.activity_types SET is_active = FALSE WHERE status = 'inactive';
ALTER TABLE project.activity_types DROP COLUMN status;

ALTER TABLE project.project_templates ADD COLUMN is_active BOOLEAN NOT NULL DEFAULT TRUE;
UPDATE project.project_templates SET is_active = FALSE WHERE status = 'inactive';
ALTER TABLE project.project_templates DROP COLUMN status;

CREATE INDEX IF NOT EXISTS idx_activity_types_company_id_is_active ON project.activity_types (company_id, is_active);
CREATE INDEX IF NOT EXISTS idx_project_templates_company_id_is_active ON project.project_templates (company_id, is_active);

DROP TYPE IF EXISTS activity_type_status;
DROP TYPE IF EXISTS project_template_status;
