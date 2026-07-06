# ADR-0004: The project boundary and the one outbound timesheet→billing seam

- **Status:** Accepted
- **Date:** 2026-07-06
- **Deciders:** backbone-project maintainers (maturity + completeness councils)
- **Scope:** module-specific (not a framework-wide decision like [0001](adr-0001-schema-yaml-ssot.md)–[0003](adr-0003-custom-markers.md))

> This is the handbook's record of the decision. The **extended write-up** — full context, the
> parking lot, and the two council fixes — lives in
> [`docs/adr/ADR-001`](../../adr/ADR-001-project-boundary-and-billing-seam.md); the runnable proof lives
> in [`docs/business-flows/golden-cases.md`](../../business-flows/golden-cases.md) and the `tests/` suite.

## Context

`backbone-project` is a Tier-4a **Service Delivery** module: a near-self-contained context that mostly
*reads* other modules and only lightly touches money. The hard question is not its CRUD — that is
[generated](adr-0002-generic-crud.md) — but its **identity and money boundary**. A Project looks like it
could own the Customer, the invoice, and a slice of the ledger. If it did, the delivery context would
become a second billing system, and turning it off would move the GL.

## Decision

1. **Project owns delivery intent + roll-up; siblings own the rest.** A Project *reads* `customer_id`
   (party) and `source_so_id` (selling) as logical FKs. Roll-up fields (`total_costing_amount`,
   `total_billable_amount`, `total_billed_amount`) give margin visibility **without posting anything**.
2. **One outbound seam, via a port — zero normal Cargo edge.** `ProjectWriteService::bill_timesheet`
   calls `BillingPort::create_service_invoice`; a composing service wires the real `backbone-billing`
   behind the port. The library never imports billing (it is a `[dev-dependencies]` test edge only).
3. **The hand-off is idempotent + transition-gated.** A timesheet bills **once** — invoice keyed
   `TS-<id>`, gate `submitted → billed`, and `total_billed_amount` rolled in the *same* transaction as
   the gate. A retry returns the existing invoice.
4. **Rates are snapshotted at log time.** Each timesheet line copies the `ActivityType`'s billing +
   costing rate when logged; a later rate change never rewrites logged history.
5. **Posts no GL.** Money moves only once billing takes over the billable timesheet. Project-cost GL
   posting is deferred (roll-up only) until an SMB needs project P&L.

## Alternatives considered

- **Let project build the invoice / post the GL itself.** Duplicates billing, couples the delivery
  context to the ledger, and breaks the "turn it off and nothing in the GL changes" property. Rejected.
- **A normal Cargo dependency on `backbone-billing`.** Simpler to call, but makes project un-buildable
  without billing and couples their release cadence. Rejected in favor of the `BillingPort` trait.
- **Expose the hand-off as a generated CRUD `PATCH /timesheets/{id}` → `status: billed`.** Bypasses the
  roll-up and the idempotent invoice hand-off; would mark a sheet billed with no invoice and a stale
  margin. Rejected — the write service *is* the guard.

## Consequences

**Easier:** the delivery context is self-contained and independently deployable; it *feeds* the
existing order-to-cash spine from a service/time angle without owning any of it; the seam is proven
end-to-end (`tests/project_billing_seam.rs` drives the real billing write path) and survives regen (the
seam files sit behind `// <<< CUSTOM` markers).

**Harder / to live with:** the seam has **no HTTP route and is not wired into `ProjectModule`** — a
composing service must construct `ProjectWriteService` and implement `BillingPort` itself (documented in
the [Developer Guide](../06-developer-guide.md#how-do-i-bill-a-timesheet-the-outbound-seam) and
[Maintainer Guide](../05-maintainer-guide.md#the-billing-seam-the-write-service-pattern)); the generated
CRUD routes can still mutate `project.timesheets` directly and bypass every gate, so a deployment must
compose a guarded router rather than expose `all_crud_routes()` naïvely; "at most one downstream invoice"
across a `bill_timesheet` crash window is delegated to the adapter's UNIQUE-backed `TS-<id>` (a go-live
outbox/saga is parked). See the parking lot in
[`docs/adr/ADR-001`](../../adr/ADR-001-project-boundary-and-billing-seam.md) for the full list.
