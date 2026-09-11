-- Hand-authored (user-owned). Not regenerated.
--
-- Strip every company-fence artifact from the project tables (ADR-0029): the module is
-- tenant-agnostic; org scoping is installed by the COMPOSING service's tenancy decorator,
-- never by the module. Dropped here, per table: the company-leading indexes, the
-- <table>_company_isolation RLS policy, and the company_id column itself.
--
-- Ordering guard (the decorator must run FIRST on any database with data): the module
-- never moves tenancy data. A table is safe to strip when EITHER
--   a) it carries org_unit_id with no NULLs — the decorator backfilled it from company_id —
--      or b) it is empty (a fresh database: the earlier chain files created it empty).
-- Otherwise the strip RAISEs, naming the decorator step, rather than dropping a column
-- that still holds the only tenancy key. The file is re-runnable (every drop is IF EXISTS
-- and the tracker has no checksums), so a failed run retries cleanly after the decorator
-- lands.
--
-- RLS enable/force flags are deliberately NOT touched: the decorator owns those now.
--
-- Mint-backstop note: the service-delivery idempotency uniques uq_projects_source_so /
-- uq_tasks_origin_sale_line formerly led with company_id. The origin ids (sales-order and
-- sales-order-line ids) are globally unique, so the invariants return here tenant-free —
-- same names, same predicate, bare origin key — and keep the module's confirm-mint
-- at-most-once behavior working on an undecorated database (declared G-PRJ-3/G-PRJ-4 in
-- schema/hooks/convergence_guards.hook.yaml). A composing decorator therefore has no
-- org-scoped variants of these two to install.

DO $$
DECLARE
    t text;
    has_org boolean;
    org_nulls bigint;
    total bigint;
    offenders text := '';
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'activity_types', 'projects',
        'project_templates', 'project_template_tasks',
        'tasks'
    ]
    LOOP
        IF to_regclass(format('project.%I', t)) IS NULL THEN
            CONTINUE; -- chain not fully applied on this database; nothing to strip
        END IF;

        SELECT EXISTS (
                   SELECT 1 FROM information_schema.columns
                   WHERE table_schema = 'project' AND table_name = t AND column_name = 'org_unit_id'
               )
        INTO has_org;

        EXECUTE format('SELECT count(*) FROM project.%I', t) INTO total;

        IF has_org THEN
            EXECUTE format(
                'SELECT count(*) FROM project.%I WHERE org_unit_id IS NULL', t)
            INTO org_nulls;
        ELSE
            org_nulls := total; -- no org column: every row's only tenancy key is company_id
        END IF;

        IF has_org AND org_nulls = 0 THEN
            CONTINUE; -- decorator backfilled: safe
        END IF;
        IF total = 0 THEN
            CONTINUE; -- empty table (fresh database): safe
        END IF;
        offenders := offenders || format(' project.%s (%s rows, %s rows not covered by org_unit_id);', t, total, org_nulls);
    END LOOP;

    IF offenders <> '' THEN
        RAISE EXCEPTION 'refusing to strip company_id — these tables are not yet covered by the tenancy decorator:%. Apply the composing service''s tenancy decorator (it backfills org_unit_id from company_id) and re-run; it is the only step that moves tenancy data.', offenders;
    END IF;
END $$;

-- ── activity_types ─────────────────────────────────────────────────────────────
DROP INDEX IF EXISTS project.idx_activity_types_company_id_is_active;
DROP INDEX IF EXISTS project.idx_activity_types_company_id_status;
DROP POLICY IF EXISTS activity_types_company_isolation ON project.activity_types;
ALTER TABLE project.activity_types DROP COLUMN IF EXISTS company_id;

-- ── projects ───────────────────────────────────────────────────────────────────
DROP INDEX IF EXISTS project.idx_projects_company_id_status;
DROP INDEX IF EXISTS project.uq_projects_source_so;
DROP POLICY IF EXISTS projects_company_isolation ON project.projects;
ALTER TABLE project.projects DROP COLUMN IF EXISTS company_id;

-- ── project_templates ──────────────────────────────────────────────────────────
DROP INDEX IF EXISTS project.idx_project_templates_company_id_is_active;
DROP INDEX IF EXISTS project.idx_project_templates_company_id_status;
DROP POLICY IF EXISTS project_templates_company_isolation ON project.project_templates;
ALTER TABLE project.project_templates DROP COLUMN IF EXISTS company_id;

-- ── project_template_tasks ─────────────────────────────────────────────────────
DROP INDEX IF EXISTS project.idx_project_template_tasks_company_id;
DROP POLICY IF EXISTS project_template_tasks_company_isolation ON project.project_template_tasks;
ALTER TABLE project.project_template_tasks DROP COLUMN IF EXISTS company_id;

-- ── tasks ──────────────────────────────────────────────────────────────────────
DROP INDEX IF EXISTS project.uq_tasks_origin_sale_line;
DROP POLICY IF EXISTS tasks_company_isolation ON project.tasks;
ALTER TABLE project.tasks DROP COLUMN IF EXISTS company_id;

-- Restore the tenant-free mint invariants (same names, no tenant leg).
CREATE UNIQUE INDEX IF NOT EXISTS uq_projects_source_so
    ON project.projects (source_so_id)
    WHERE source_so_id IS NOT NULL AND (metadata->>'deleted_at') IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_tasks_origin_sale_line
    ON project.tasks (origin_sale_line_id)
    WHERE origin_sale_line_id IS NOT NULL AND (metadata->>'deleted_at') IS NULL;
