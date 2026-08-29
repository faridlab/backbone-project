-- Down migration: recreate empty shells of the retired tables and drop the
-- service-delivery origin keys. The shells mirror the original create
-- migrations (minus data): restoring rows is out of scope — the analytic row
-- lives in the timesheet module now.

DROP INDEX IF EXISTS project.uq_tasks_origin_sale_line;
ALTER TABLE project.tasks DROP COLUMN IF EXISTS origin_sale_line_id;
DROP INDEX IF EXISTS project.uq_projects_source_so;

DO $$ BEGIN
    CREATE TYPE timesheet_status AS ENUM ('draft', 'submitted', 'billed', 'cancelled');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

CREATE TABLE IF NOT EXISTS project.timesheets (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    project_id UUID NOT NULL,
    employee_id UUID,
    currency TEXT NOT NULL DEFAULT 'IDR',
    status timesheet_status NOT NULL DEFAULT 'draft',
    total_hours NUMERIC(10, 2) NOT NULL DEFAULT 0 CHECK (total_hours >= 0),
    total_billable_amount NUMERIC(18, 2) NOT NULL DEFAULT 0 CHECK (total_billable_amount >= 0),
    total_costing_amount NUMERIC(18, 2) NOT NULL DEFAULT 0 CHECK (total_costing_amount >= 0),
    invoice_id UUID,
    metadata JSONB NOT NULL DEFAULT '{"created_at":null,"updated_at":null,"deleted_at":null,"created_by":null,"updated_by":null,"deleted_by":null}'::jsonb,
    PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS project.timesheet_details (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    timesheet_id UUID NOT NULL,
    activity_type_id UUID,
    task_id UUID,
    description TEXT,
    hours NUMERIC(10, 2) NOT NULL,
    billing_rate NUMERIC(18, 2) NOT NULL DEFAULT 0 CHECK (billing_rate >= 0),
    costing_rate NUMERIC(18, 2) NOT NULL DEFAULT 0 CHECK (costing_rate >= 0),
    is_billable BOOLEAN NOT NULL DEFAULT TRUE,
    billable_amount NUMERIC(18, 2) NOT NULL DEFAULT 0 CHECK (billable_amount >= 0),
    costing_amount NUMERIC(18, 2) NOT NULL DEFAULT 0 CHECK (costing_amount >= 0),
    metadata JSONB NOT NULL DEFAULT '{"created_at":null,"updated_at":null,"deleted_at":null,"created_by":null,"updated_by":null,"deleted_by":null}'::jsonb,
    PRIMARY KEY (id)
);
