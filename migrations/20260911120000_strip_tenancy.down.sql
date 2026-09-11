-- Hand-authored (user-owned). Not regenerated.
--
-- Best-effort restore sketch for the tenancy strip (ADR-0029). This is a breaking module
-- release against dev-stage databases: the down re-adds the company_id column as nullable
-- with its plain indexes and the company isolation policy shape, but restores NO data —
-- rows written after the strip (or after the decorator re-keyed them) carry org_unit_id
-- only. The composing service's tenancy decorator remains the live fence; treat this
-- down as a schema-shape sketch for archaeology, not a usable rollback.
--
-- The restored mint uniques lead with company_id again (the pre-strip shape). With the
-- column NULL on all restored rows, PostgreSQL treats NULLs as distinct — the unique
-- does not bind until a backfill gives every row a company again.
--
-- Index restorations match the CURRENT pre-strip chain shape: the activity-type and
-- template listing indexes read (company_id, status) — their (company_id, is_active)
-- ancestors died with the is_active columns in the status-lifecycle migration.

DROP INDEX IF EXISTS project.uq_projects_source_so;
DROP INDEX IF EXISTS project.uq_tasks_origin_sale_line;

ALTER TABLE project.activity_types        ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE project.projects              ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE project.project_templates     ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE project.project_template_tasks ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE project.tasks                 ADD COLUMN IF NOT EXISTS company_id uuid;

CREATE INDEX IF NOT EXISTS idx_activity_types_company_id_status
    ON project.activity_types (company_id, status);
CREATE INDEX IF NOT EXISTS idx_projects_company_id_status
    ON project.projects (company_id, status);
CREATE INDEX IF NOT EXISTS idx_project_templates_company_id_status
    ON project.project_templates (company_id, status);
CREATE INDEX IF NOT EXISTS idx_project_template_tasks_company_id
    ON project.project_template_tasks (company_id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_projects_source_so
    ON project.projects (company_id, source_so_id)
    WHERE source_so_id IS NOT NULL AND (metadata->>'deleted_at') IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_tasks_origin_sale_line
    ON project.tasks (company_id, origin_sale_line_id)
    WHERE origin_sale_line_id IS NOT NULL AND (metadata->>'deleted_at') IS NULL;

-- The pre-strip policies read a NOT NULL company_id; on this nullable sketch they make
-- NULL-company rows invisible to every scoped connection until data is restored by hand.
CREATE POLICY activity_types_company_isolation ON project.activity_types
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY projects_company_isolation ON project.projects
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY project_templates_company_isolation ON project.project_templates
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY project_template_tasks_company_isolation ON project.project_template_tasks
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY tasks_company_isolation ON project.tasks
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
