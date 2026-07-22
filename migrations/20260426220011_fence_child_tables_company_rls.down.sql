-- Down: reverse the child-table company RLS fence for project module (ADR-0010 Decision A)

-- Reverse the fence for project.timesheet_details
DROP POLICY IF EXISTS timesheet_details_company_isolation ON project.timesheet_details;
ALTER TABLE project.timesheet_details NO FORCE ROW LEVEL SECURITY;
ALTER TABLE project.timesheet_details DISABLE ROW LEVEL SECURITY;
DROP INDEX IF EXISTS project.idx_timesheet_details_company_id;
ALTER TABLE project.timesheet_details DROP COLUMN IF EXISTS company_id;

-- Reverse the fence for project.project_template_tasks
DROP POLICY IF EXISTS project_template_tasks_company_isolation ON project.project_template_tasks;
ALTER TABLE project.project_template_tasks NO FORCE ROW LEVEL SECURITY;
ALTER TABLE project.project_template_tasks DISABLE ROW LEVEL SECURITY;
DROP INDEX IF EXISTS project.idx_project_template_tasks_company_id;
ALTER TABLE project.project_template_tasks DROP COLUMN IF EXISTS company_id;
