-- Down: drop enum types for project module
DROP TYPE IF EXISTS timesheet_status CASCADE;
DROP TYPE IF EXISTS task_status CASCADE;
DROP TYPE IF EXISTS project_status CASCADE;
DROP TYPE IF EXISTS project_type CASCADE;
