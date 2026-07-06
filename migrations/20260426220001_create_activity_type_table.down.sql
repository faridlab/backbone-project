-- Down: drop project.activity_types table
DROP TABLE IF EXISTS project.activity_types CASCADE;
DROP FUNCTION IF EXISTS project.activity_types_audit_timestamp() CASCADE;
