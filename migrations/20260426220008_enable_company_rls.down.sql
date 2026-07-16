-- Down: remove the company RLS fence for project module

-- Reverse the company RLS fence for project.activity_types
DROP POLICY IF EXISTS activity_types_company_isolation ON project.activity_types;
ALTER TABLE project.activity_types NO FORCE ROW LEVEL SECURITY;
ALTER TABLE project.activity_types DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for project.projects
DROP POLICY IF EXISTS projects_company_isolation ON project.projects;
ALTER TABLE project.projects NO FORCE ROW LEVEL SECURITY;
ALTER TABLE project.projects DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for project.project_templates
DROP POLICY IF EXISTS project_templates_company_isolation ON project.project_templates;
ALTER TABLE project.project_templates NO FORCE ROW LEVEL SECURITY;
ALTER TABLE project.project_templates DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for project.tasks
DROP POLICY IF EXISTS tasks_company_isolation ON project.tasks;
ALTER TABLE project.tasks NO FORCE ROW LEVEL SECURITY;
ALTER TABLE project.tasks DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for project.timesheets
DROP POLICY IF EXISTS timesheets_company_isolation ON project.timesheets;
ALTER TABLE project.timesheets NO FORCE ROW LEVEL SECURITY;
ALTER TABLE project.timesheets DISABLE ROW LEVEL SECURITY;

