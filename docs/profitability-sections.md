# Project profitability sections

How a project's P&L is assembled from analytic sections, and the discipline that keeps the
sections from double-counting the same money. This is a CONVENTION (documentation), not code:
nothing in this module computes section sums at read time — every number named below lands on
`project.projects` via an explicit write and is read back as a plain column.

## The stored triple

`project.projects` carries three derived money columns:

| Column | Meaning | Refreshed by |
|---|---|---|
| `total_costing_amount` | internal cost of all converged analytic rows on the project | `ProjectWriteService::refresh_project_financials` |
| `total_billable_amount` | billable value of those rows (approved-period lines) | same verb, same tx |
| `total_billed_amount` | value actually handed to billing as Sales Invoices | the billing exit's stamp/reverse txs |

The first two are recomputed from the converged analytic row (`timesheet.timesheets`, owned by
the timesheet module) by the refresh verb — they are never live-computed on read, and a completed
project's totals stay final (the refresh refuses a closed project). The third moves only when the
billing exit stamps rows (`bill_timesheet_period`) or a reversal clears them (`unbill_invoice`).

## The sections

A project's full profitability picture is the union of independently-owned sections. Each section
is owned by exactly one module; no section is computed by this module at read time.

| # | Section | Owner | Sign | Notes |
|---|---|---|---|---|
| 10 | Sale — timesheet-based service invoices | timesheet rows via this module's billing exit | revenue (negative in analytic sign convention) | the stamped rows' `billable_amount`, surfaced as `total_billed_amount` |
| 11 | Vendor bills — subcontracted delivery | buying module | expense | materials/services bought for the project |
| 12 | Manufacturing + material consumption | manufacturing over inventory | expense | work orders and stock moves consumed by the project |
| 13 | Expenses | expense module | expense | employee out-of-pocket costs allocated to the project |
| 14 | Analytic lines — income split | analytic module | income (credit) | the income half of sign-split analytic entries |
| 15 | Analytic lines — expense split | analytic module | expense (debit) | the expense half; 14 and 15 partition the same journal postings, never both counted as new money |

Sections 10 and 14 can describe overlapping value: a timesheet-based invoice posts both a
receivable (revenue) and, when analytic accounting is on, an analytic income line. When a report
sums sections, it must pick ONE of the two lenses:

- **Invoiced lens**: sections 10 + 11 + 12 + 13 (cash-anchored: what was billed vs spent).
- **Analytic lens**: sections 14 + 15 + 11 + 12 + 13 (entry-anchored: what was posted vs spent).

Summing 10 and 14 together double-counts revenue. This is the one rule of the convention that a
report author can silently get wrong, so it is written down here.

## Mutual-exclusion discipline

- Every money fact belongs to exactly ONE section. If a module wants to surface a fact another
  section already carries, it does so as a reference (an id link), not a second amount — the way
  this module stamps `invoice_id` onto the converged rows it billed instead of copying the
  invoice total anywhere else.
- The converged analytic row is the single source of effort truth: this module derives
  cost/billable/billed from it and writes nothing back. The timesheet module owns the row's
  lifecycle; the approval cycle (`timesheet.timesheet_approvals`, per company + employee +
  year + month) is the gate that makes a period billable.
- Reversals flow back through the same door the money left by: a credit note clears the rows'
  `invoice_id` link and rolls `total_billed_amount` back — it never writes a negative section.
