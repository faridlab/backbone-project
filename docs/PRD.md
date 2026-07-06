# backbone-project — PRD

Tier 4 · Service Delivery pillar · posts **no GL** (hands billable time to backbone-billing).

## Why
An Indonesia SMB that sells *services* (agencies, consultancies, IT shops, contractors) needs to
track delivery work and bill for time — without dragging in SAP-PS / NetSuite-PSA complexity. This is
the **lean project-delivery core**: capture a project, break it into tasks, log time at billable +
costing rates, and turn billable time into a real Sales Invoice through the same billing/GL spine the
rest of Metaphor uses. It is the second proof — from a *service* angle — of the `AccountingPost`
contract (the first was order-to-cash).

## Scope (KEEP — pillar brief §4)
- **Project** — a customer delivery engagement with cost/billing/billed roll-up fields (margin
  visibility) and a clean identity boundary: it *reads* its Customer (party) and originating Sales
  Order (selling) as logical FKs, owns neither.
- **Task** — a lightweight adjacency-list tree (`parent_task_id`), status + progress + effort estimate.
  No full WBS / critical path.
- **Timesheet (+ TimesheetDetail)** — logged effort; each line snapshots a billing + costing rate
  (from an ActivityType) at log time, so `billable = Σ hours·billing_rate` and `costing = Σ
  hours·costing_rate`.
- **ActivityType** — the rate master (default billing + costing rate per kind of work).
- **ProjectTemplate (+ ProjectTemplateTask)** — a reusable blueprint; instantiating it stamps out a
  project + its task tree in one shot.
- **The one outbound seam** — a submitted, billable Timesheet is handed to **backbone-billing**, which
  builds the timesheet-based Sales Invoice and posts the GL (Dr A/R · Cr Service Revenue · Cr PPN).
  Projects only records the echoed `invoice_id` and marks the timesheet `billed`.

## Non-goals (CUT / DEFER — brief §4)
- **GL cost posting** — project cost stays a read-model roll-up; promote to a `backbone-project →
  AccountingPost` emit only when an SMB needs project P&L (open question in the brief).
- Resource scheduling / capacity planning, Gantt / kanban board views (UI, not domain).
- `activity_cost` per-employee rate overrides, project-cost GL, WBS / critical-path.
- Multichannel / email-to-timesheet ingestion.
- Service-PPN rate logic — lives in the backbone-tax overlay, not here.

## Success criteria
- Time logs roll up billable + costing to the project exactly (golden cases).
- A billable timesheet produces exactly one real billing Sales Invoice for the project's customer,
  idempotently (proven against REAL backbone-billing).
- Zero normal Cargo edge to billing (the seam is a port; billing is a dev-dependency only).
- Survives a full codegen regen with the hand-authored seam intact (§5).
