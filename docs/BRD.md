# backbone-project — BRD

## Documents
Project (customer delivery + roll-up) · Task (adjacency-list tree) · Timesheet (+ TimesheetDetail) ·
ActivityType (rate master) · ProjectTemplate (+ ProjectTemplateTask). Own Postgres schema `project`.
Posts NO GL.

## Business rules

**BR-1 (open a project).** `create_project` requires a name; an **external (billable) project requires
a customer** (logical FK to party). `internal` projects need none. Currency defaults IDR. Status `open`.

**BR-2 (task tree).** `add_task` attaches a task to an **open** project; a `parent_task_id`, if given,
must belong to the same project (adjacency-list tree). `advance_task` moves an open/working task's
status + progress (0..100); a completed/cancelled task is terminal.

**BR-3 (log time).** `log_time` creates a **submitted** Timesheet with ≥ 1 line of positive hours.
Each line snapshots its billing + costing rate (explicit, else the ActivityType's default at log time,
so a later rate change never rewrites logged history). `billable = hours·billing_rate` (billable lines
only) and `costing = hours·costing_rate` (all lines). In the SAME transaction it rolls the project's
`total_billable_amount` and `total_costing_amount` up, and that roll-up UPDATE is **status-gated**
(`WHERE status='open'`): a `complete_project` racing in the gap makes the log roll back whole, so time
never lands on a completed project and the completion event's totals stay final (maturity council
2026-07-06). Cannot log to a closed project. Emits `TimeLogged`.

**BR-3a (cancel a timesheet — the reverse gear).** `cancel_timesheet` voids a **submitted** timesheet
(a mis-entry) and REVERSES its billable + costing off the project roll-up in one status-gated tx
(`WHERE status='open'`, so a completed project's totals stay final). Idempotent (a re-cancel is a
no-op); a **billed** timesheet cannot be cancelled (reverse via billing). Without this the roll-up was a
one-way ratchet — a single typo poisoned the margin number forever (completeness council 2026-07-06).
Emits `TimesheetCancelled`.

**BR-4 (bill a timesheet — the one outbound seam).** `bill_timesheet` requires a **submitted**
timesheet with a positive billable total and a project **customer**, drives
`BillingPort::create_service_invoice` (idempotent per timesheet), then transition-gates `submitted →
billed` with the echoed `invoice_id` and rolls the project's `total_billed_amount` up in the same
transaction. Hands off **at most once** (a retry returns the existing invoice). Only `is_billable`
lines are invoiced. **Posts no GL** — billing builds the Sales Invoice and posts (Dr A/R · Cr Service
Revenue · Cr PPN). Emits `TimesheetBilled`.

**BR-5 (template).** `instantiate_template` creates a project from an **active** template and
materializes its template tasks (in `sequence` order) as real tasks.

**BR-6 (complete).** `complete_project` transitions an **open** project → `completed` (terminal) and
emits `ProjectCompleted` with the final cost/billable roll-ups.

## Events
`TimeLogged`, `TimesheetBilled`, `TimesheetCancelled`, `ProjectCompleted`.

## Deferred (with reason)
Project-cost GL posting (roll-up only until an SMB needs project P&L), resource scheduling, Gantt/kanban
UI, per-employee rate overrides, email-to-timesheet ingestion, service-PPN rate logic (tax overlay).
