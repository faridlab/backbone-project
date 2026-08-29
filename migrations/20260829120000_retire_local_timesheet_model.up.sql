-- Migration: retire the module's own timesheet tables and add the service-delivery origin keys
--
-- Logged effort now lives in ONE place: the converged analytic row owned by the
-- timesheet module (timesheet.timesheets). This module keeps only DERIVED reads
-- over that row (project cost/billing roll-ups, refreshed by an explicit verb)
-- plus the billing exit, which bills an approved (project, employee, year,
-- month) slice. The local header/detail pair and its status band are therefore
-- dropped outright — no data migration: the module is not composed in any host,
-- so these tables only ever held test data.
--
-- The drop also removes, with the tables: their audit triggers, their RLS
-- policies, and the fence backfill that joined them. The two audit FUNCTIONS
-- outlive their tables, so they are dropped explicitly.
--
-- What replaces the vocabulary:
--   * cost/billable/billed roll-ups stay on project.projects (refreshed from
--     the converged rows by refresh_project_financials);
--   * the per-row billing artifact becomes the converged row's invoice_id link
--     (a link, not a status);
--   * service delivery minted from a confirmed sales order keys its
--     idempotency on origin keys: projects.source_so_id (one project per
--     order) and tasks.origin_sale_line_id (one task per order line).

-- Tables first (their audit triggers go with them), then the orphaned audit functions.
DROP TABLE IF EXISTS project.timesheet_details;
DROP TABLE IF EXISTS project.timesheets;

DROP FUNCTION IF EXISTS project.timesheets_audit_timestamp();
DROP FUNCTION IF EXISTS project.timesheet_details_audit_timestamp();

-- The status band type was created unqualified (it lives in public, beside the
-- module's other enum types) — see the status_lifecycle migration for the
-- convention. It belongs to the retired tables only; the timesheet module's
-- own types (timesheet_type, timesheet_approval_status) are untouched.
DROP TYPE IF EXISTS public.timesheet_status;

-- Service-delivery mint backrefs (one project per originating sales order,
-- one task per originating sales-order line; logical FKs — cross-module, no
-- DB constraint by design).
CREATE UNIQUE INDEX IF NOT EXISTS uq_projects_source_so
    ON project.projects (company_id, source_so_id)
    WHERE source_so_id IS NOT NULL AND (metadata->>'deleted_at') IS NULL;

ALTER TABLE project.tasks ADD COLUMN IF NOT EXISTS origin_sale_line_id UUID;

COMMENT ON COLUMN project.tasks.origin_sale_line_id IS
    'Sales-order line this task was minted for # logical FK to selling.SalesOrderItem.id';

CREATE UNIQUE INDEX IF NOT EXISTS uq_tasks_origin_sale_line
    ON project.tasks (company_id, origin_sale_line_id)
    WHERE origin_sale_line_id IS NOT NULL AND (metadata->>'deleted_at') IS NULL;
