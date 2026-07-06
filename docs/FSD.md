# backbone-project — FSD

## Entities
Project (`project_type`, `customer_id`/`source_so_id` logical FKs, `status`, roll-ups
`total_costing_amount`/`total_billable_amount`/`total_billed_amount`) · Task (`parent_task_id`
adjacency tree, `status`, `progress`, `expected_time`) · Timesheet (`status`, `total_hours`,
`total_billable_amount`, `total_costing_amount`, `invoice_id`) · TimesheetDetail (`activity_type_id`,
`hours`, `billing_rate`/`costing_rate` snapshots, `is_billable`, `billable_amount`/`costing_amount`) ·
ActivityType (`billing_rate`/`costing_rate`) · ProjectTemplate (+ ProjectTemplateTask `sequence`).
Enums: ProjectType {external, internal}, ProjectStatus {open, completed, cancelled}, TaskStatus {open,
working, completed, cancelled}, TimesheetStatus {draft, submitted, billed, cancelled}.

## Write path (`ProjectWriteService`, hand-authored, user-owned)
- `create_project` / `add_task` / `advance_task` / `instantiate_template` / `complete_project`
- `log_time(project_id, NewTimesheet, sink)` → submitted timesheet + lines + project roll-up (one tx,
  status-gated)
- `cancel_timesheet(timesheet_id, sink)` → voids a submitted sheet + reverses its roll-up (the reverse
  gear); idempotent, status-gated
- `bill_timesheet(timesheet_id, &dyn BillingPort, sink)` → the seam; idempotent + transition-gated

`money()` = IDR 2dp half-away-from-zero. Errors: `ProjectError {Db, NotFound, InvalidState, Invalid,
BillingRejected}`.

## Seam (port — zero normal Cargo edge)
- **Billing emit (proven, PBSEAM-1):** `bill_timesheet` drives the REAL backbone-billing write path to
  create a service Sales Invoice from the billable lines; `invoice_id` links a real
  `billing.sales_invoices` row for the project's customer, at the billable total. Idempotent per
  timesheet (`TS-<id>`). Projects marks the timesheet `billed`. ADR-001.
- **Inbound:** none — Project *reads* Customer (party) + Sales Order (selling) as logical FKs; it posts
  no GL itself (billing does).

## Test oracle
`project_golden_cases` (4: log-time roll-up, bill roll-up, template instantiation, validation),
`integrity_probes` (7: bill idempotent, nothing-billable refused, completed-project terminal,
bill-requires-customer, non-billable excluded from billing, IP-6 complete-races-log-time conserves the
roll-up, IP-7 cancel_timesheet reverses the roll-up), `project_billing_seam` (1: billable timesheet →
REAL billing Sales Invoice, idempotent) + §5 round-trip. **12 tests.**

> The generated `integration_tests.rs` hits an external HTTP server (`API_BASE_URL`, default
> `127.0.0.1:3000`) and is environmental scaffolding, not part of this module's correctness gate; the
> hand-authored oracle above + §5 is the gate.
