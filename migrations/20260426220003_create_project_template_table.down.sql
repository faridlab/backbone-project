-- Down: drop project.project_templates table
DROP TABLE IF EXISTS project.project_templates CASCADE;
DROP FUNCTION IF EXISTS project.project_templates_audit_timestamp() CASCADE;
