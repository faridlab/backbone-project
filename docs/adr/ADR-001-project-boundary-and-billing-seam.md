# ADR-001 — Project boundary and the timesheet-billing seam

Status: accepted · 2026-07-06 · Tier 4 (Service Delivery pillar; posts no GL)

## Context
Where the Financials pillar is the *hub* (one GL contract, many emitters), Service Delivery is a *leaf*
pillar: near-self-contained contexts that mostly READ other modules and only lightly touch the GL.
backbone-project is Tier-4a — the only one of the three with a real outbound seam (timesheet → billing),
so it doubles as a second proof of the `AccountingPost` contract from a *service* angle. The real work
is the identity + money boundary: a Project is delivery intent + cost/billing visibility; it is NOT the
ledger, NOT the Customer, NOT the invoice.

## Decision
1. **Project owns delivery + roll-up; party owns the Customer; billing owns the invoice + GL.** A
   Project *reads* `customer_id` (party) and `source_so_id` (selling) as logical FKs. Roll-up fields
   (`total_costing_amount` / `total_billable_amount` / `total_billed_amount`) give margin visibility
   without posting anything.
2. **One outbound seam, via a port + events (zero normal Cargo edge).** `bill_timesheet →
   BillingPort::create_service_invoice` hands a billable timesheet to billing, which builds the Sales
   Invoice and posts (Dr A/R · Cr Service Revenue · Cr PPN). A composing service wires the real billing
   behind the port; project never imports backbone-billing.
3. **The hand-off is idempotent + transition-gated.** A timesheet bills **once** (invoice idempotent per
   timesheet `TS-<id>`; gate on `submitted → billed`). A retry returns the existing invoice. The
   project's `total_billed_amount` rolls up in the same transaction as the gate.
4. **Rates are snapshotted at log time.** Each timesheet line copies the ActivityType's billing +
   costing rate when logged, so a later rate change never rewrites logged history.
5. **Posts no GL.** Money moves only once billing takes over the billable timesheet. Project-cost GL
   posting is deferred (roll-up only) until an SMB needs project P&L.

## Consequences
- The delivery context is self-contained; turn project off and no ledger changes. It only *feeds* the
  order-to-cash spine that already exists — now from a service/time angle.
- Proven end-to-end (`tests/project_billing_seam.rs` drives the REAL billing write path) and survives
  regen (§5).

## Parking lot (each with a gate)
- **Roll-up had no reverse gear** — FIXED (completeness council 2026-07-06): `log_time` only ever ADDED
  to the project roll-up and no verb reached the reserved `TimesheetStatus::cancelled`, so a mis-logged
  timesheet poisoned the margin number forever. Added `cancel_timesheet` (`submitted → cancelled`) which
  reverses the sheet's billable + costing off the roll-up in the same open-project-gated tx; idempotent;
  a billed sheet cannot be cancelled (IP-7, proven-by-revert).
- **Log-time roll-up races completion** — FIXED (maturity council 2026-07-06): `log_time` read project
  status outside its tx then rolled the project up unguarded, so a `complete_project` committing in the
  gap left time on a completed project (roll-up drifting past the `ProjectCompleted` totals + a billable
  orphan sheet). The roll-up UPDATE is now status-gated in-tx (`WHERE status='open'`); a racing
  completion makes the whole log roll back (IP-6, proven-by-revert).
- **Project-cost GL posting** — cost stays a read-model roll-up; gate: an SMB needing project P&L →
  promote to a `backbone-project → AccountingPost` emit (mirrors the financials "Cost Center promoted on
  demand" pattern).
- **Bill-before-gate crash window** — `bill_timesheet` calls billing before the status gate; the gate
  makes the project's *record* bill-once, but "at most one downstream invoice" is delegated to the
  adapter deduping on the stable key (`TS-<id>`), which must be UNIQUE-backed. Gate: a go-live
  outbox/saga (same class as the other producers' parked event-bus idempotency).
- **Task dependencies (`depends_on` DAG)**, per-employee rate overrides, resource scheduling, Gantt —
  deferred (PRD non-goals / brief park list).
