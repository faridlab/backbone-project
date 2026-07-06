-- Down: drop project.projects table
DROP TABLE IF EXISTS project.projects CASCADE;
DROP FUNCTION IF EXISTS project.projects_audit_timestamp() CASCADE;
