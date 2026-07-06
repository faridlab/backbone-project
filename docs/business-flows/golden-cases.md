# backbone-project — business flows & golden cases

## Flow: capture → deliver → log → bill (project-to-cash, service angle)
```
create_project (external → needs customer)
   │
   ▼  add_task* (adjacency tree)  ·  instantiate_template (project + tasks in one shot)
   │
   ▼  log_time  → submitted Timesheet (rates snapshotted) → rolls billable + costing onto Project
   │
   ▼  bill_timesheet  → BillingPort::create_service_invoice (REAL billing Sales Invoice)
                        → gate submitted→billed + roll total_billed_amount → TimesheetBilled
   │
   ▼  complete_project → ProjectCompleted (final roll-ups)
```
Only `bill_timesheet` reaches money — and it posts NO GL itself; billing builds the Sales Invoice and
posts (Dr A/R · Cr Service Revenue · Cr PPN).

## Golden cases (`tests/project_golden_cases.rs`)
- **PGC-1 — log-time roll-up.** 8 billable h + 2 non-billable h at billing 500000 / costing 300000:
  timesheet `billable = 4,000,000` (billable lines only), `costing = 3,000,000` (all lines), `hours =
  10`; the project's `total_billable_amount` / `total_costing_amount` match, `total_billed_amount = 0`.
- **PGC-2 — bill roll-up.** Billing a submitted 8h timesheet hands off `4,000,000`, marks it `billed`
  with the echoed `invoice_id`, and rolls the project's `total_billed_amount` to `4,000,000`.
- **PGC-3 — template instantiation.** A 3-task template stamps out a project with all 3 tasks.
- **PGC-4 — validation.** External project needs a customer; timesheet needs a line; hours must be
  positive; a task needs a subject.

## Integrity probes (`tests/integrity_probes.rs`)
- **IP-1 — bill idempotent.** Retry returns the same invoice, drives billing once, rolls billed once.
- **IP-2 — nothing billable refused.** A wholly non-billable timesheet cannot be billed; billing not
  driven.
- **IP-3 — completed project terminal.** No time logged, cannot re-complete.
- **IP-4 — bill requires customer.** An internal (customer-less) project's billable time cannot be
  billed (fails closed before driving billing).
- **IP-5 — non-billable excluded.** Only billable lines invoice + roll into billed; non-billable lines
  cost but never bill.
- **IP-6 — complete races log-time, conserved (maturity).** Racing `complete_project` against `log_time`:
  a completed project's stored `total_billable_amount` equals the total its `ProjectCompleted` event
  reported, and no `submitted` timesheet is stranded on it (the roll-up UPDATE is status-gated in-tx).
- **IP-7 — cancel reverses the roll-up (completeness).** A fat-fingered 40h sheet inflates the roll-up;
  `cancel_timesheet` reverses it back to exactly the good sheet's contribution; re-cancel is a no-op; a
  billed sheet cannot be cancelled.

## Seam (`tests/project_billing_seam.rs`)
- **PBSEAM-1 — timesheet → REAL billing.** `bill_timesheet` creates a real `billing.sales_invoices` row
  (`TS-<id>`) for the project's customer at the billable total; idempotent re-bill creates no second
  invoice.

## §5 round-trip (`scripts/project_billing_seam_roundtrip.sh`)
Regen (`--force`) leaves the seam files byte-identical; the oracle + seam re-run green.
