-- Down: drop project.timesheets table
DROP TABLE IF EXISTS project.timesheets CASCADE;
DROP FUNCTION IF EXISTS project.timesheets_audit_timestamp() CASCADE;
