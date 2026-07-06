-- Down: drop project.project_template_tasks table
DROP TABLE IF EXISTS project.project_template_tasks CASCADE;
DROP FUNCTION IF EXISTS project.project_template_tasks_audit_timestamp() CASCADE;
