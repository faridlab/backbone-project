-- Down: drop project.tasks table
DROP TABLE IF EXISTS project.tasks CASCADE;
DROP FUNCTION IF EXISTS project.tasks_audit_timestamp() CASCADE;
