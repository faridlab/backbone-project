-- Down: drop project.timesheet_details table
DROP TABLE IF EXISTS project.timesheet_details CASCADE;
DROP FUNCTION IF EXISTS project.timesheet_details_audit_timestamp() CASCADE;
