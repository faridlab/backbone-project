-- Child-table company RLS fence for project module (ADR-0010 Decision A)
--
-- Backfills the DENORMALIZED company_id onto the two unfenced child tables and forces RLS on them,
-- closing the cross-tenant hole in the project module's ADR-0008 fence. Mirrors the standard child
-- fence pattern already in place across billing/inventory/manufacturing/payroll:
--   * company_id is a denormalized column (NOT a parent-join EXISTS policy);
--   * no hard SQL FK to organization.companies (logical FK only, per the module boundary);
--   * backfill is deterministic because both parents (timesheets, project_templates) are already
--     fenced and carry a non-null company_id.
--
-- The scope var is the same one the parent fence uses: `set_config('app.company_id', <uuid>, true)`;
-- an unset var sees zero rows (fail-closed).

-- ============================================================================
-- project.timesheet_details  (parent: project.timesheets via timesheet_id)
-- ============================================================================
ALTER TABLE project.timesheet_details
    ADD COLUMN IF NOT EXISTS company_id UUID;

UPDATE project.timesheet_details AS td
   SET company_id = ts.company_id
  FROM project.timesheets AS ts
 WHERE td.timesheet_id = ts.id
   AND td.company_id IS NULL;

ALTER TABLE project.timesheet_details
    ALTER COLUMN company_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_timesheet_details_company_id
    ON project.timesheet_details (company_id);

ALTER TABLE project.timesheet_details ENABLE ROW LEVEL SECURITY;
ALTER TABLE project.timesheet_details FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS timesheet_details_company_isolation ON project.timesheet_details;
CREATE POLICY timesheet_details_company_isolation ON project.timesheet_details
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);

-- ============================================================================
-- project.project_template_tasks  (parent: project.project_templates via template_id)
-- ============================================================================
ALTER TABLE project.project_template_tasks
    ADD COLUMN IF NOT EXISTS company_id UUID;

UPDATE project.project_template_tasks AS ptt
   SET company_id = pt.company_id
  FROM project.project_templates AS pt
 WHERE ptt.template_id = pt.id
   AND ptt.company_id IS NULL;

ALTER TABLE project.project_template_tasks
    ALTER COLUMN company_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_project_template_tasks_company_id
    ON project.project_template_tasks (company_id);

ALTER TABLE project.project_template_tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE project.project_template_tasks FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS project_template_tasks_company_isolation ON project.project_template_tasks;
CREATE POLICY project_template_tasks_company_isolation ON project.project_template_tasks
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
