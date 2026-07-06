# backbone-project — Extension Guide

## Public surface (stable)
- **Events** (`application::service::project_events`): `TimeLogged`, `TimesheetBilled`,
  `TimesheetCancelled`, `ProjectCompleted`, the `ProjectEvent` union, and `ProjectEventSink` (a consuming service supplies its
  own sink — bus, outbox, …). These are the read-side hooks for cost/margin analytics.
- **Port** (`application::service::project_ports`): `BillingPort` + its DTOs (`InvoiceFromTimesheet`,
  `InvoiceLineFromTimesheet`, `InvoiceAck`, `ProjectRejected`). This is the seam a composing service
  implements over backbone-billing. **Zero normal Cargo edge** — the DTOs are the wire contract,
  duplicated per consumer by design.
- **Write path** (`application::service::project_write_service::ProjectWriteService`): the guarded verbs
  (`create_project`, `add_task`, `advance_task`, `instantiate_template`, `log_time`, `cancel_timesheet`,
  `bill_timesheet`, `complete_project`) — the only supported way to mutate delivery/billing state.

## How a consuming service wires the billing seam
Implement `BillingPort` over the real `backbone_billing::...::BillingWriteService` (create a service
Sales Invoice keyed on a stable `TS-<timesheet_id>` number for idempotency), and pass it to
`bill_timesheet`. See `tests/common/mod.rs` (`RealBilling`) for the reference adapter.

## Not a contract
- The 12 generated CRUD endpoints per entity (`BackboneCrudHandler`) are convenience scaffolding. Do
  **not** mutate delivery/billing state through the generic PATCH surface — it bypasses the roll-up +
  transition gates. Use `ProjectWriteService`.
- `// <<< CUSTOM` blocks inside generated files preserve local edits only; they are not a cross-module
  extension point.

## Invariants a consumer must not break
- A timesheet bills at most once; the adapter's `TS-<id>` invoice number MUST be UNIQUE-backed for the
  crash-window idempotency to hold.
- Rates are snapshotted at log time; do not recompute `billable_amount` from the live ActivityType.
