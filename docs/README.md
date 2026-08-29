# backbone-project — Documentation

The documentation set for **`backbone-project`**, the Backbone Framework **project-delivery** domain
module (v0.1.3). It owns `Project`s, their `Task`s, and `Task` templates; it gives margin
visibility through derived cost/billing roll-ups and **posts no GL** — money moves only when an
approved timesheet period crosses its one outbound seam into `backbone-billing`.

> **Posture change — converged analytic row.** This module no longer owns a timesheet table.
> Logged effort lives in ONE place: the timesheet module's `timesheet.timesheets` row, and this
> module holds only derived reads over it (the financial refresh verb), the period billing exit,
> the per-rung service-delivery mint from confirmed sales orders, guarded deletes, and hybrid task
> statuses. See [`profitability-sections.md`](profitability-sections.md) for how the P&L sections
> assemble without double-counting. Handbook pages written before the convergence still narrate the
> retired local timesheet model in places — where they disagree with the schema YAML (the source of
> truth), the schema wins and the page is due a refresh.

Every handbook page names **one reader** and **one mode** (Diátaxis) at its top. Find your reader,
follow the path.

## Find your path

| You are… | You want to… | Start here |
|----------|--------------|-----------|
| **Evaluator** | Decide whether to build on this | [Philosophy](handbook/01-philosophy.md) → [Background](handbook/02-background.md) → [Technology](handbook/03-technology.md) |
| **App developer** | Integrate the module and bill a timesheet | [Developer Guide](handbook/06-developer-guide.md) |
| **Maintainer** | Understand the machine and extend it safely | [Architecture](handbook/04-architecture.md) → [Maintainer Guide](handbook/05-maintainer-guide.md) |
| **Contributor** | Open a correct PR | [Contributing](handbook/07-contributing.md) |
| **Anyone** | Agree on what a word means | [Glossary](handbook/08-glossary.md) |

## The handbook

1. [Philosophy & motivation](handbook/01-philosophy.md) — *Evaluator.* The worldview, this module's north star (posts no GL, one billing seam), and the non-goals.
2. [Background & prior art](handbook/02-background.md) — *Evaluator.* What came before (hand-rolled CRUD, ORMs, scaffolders) and what this rejects.
3. [Technology & the "why"](handbook/03-technology.md) — *Evaluator + Maintainer.* The stack, each choice with a rationale and a rejected alternative.
4. [Architecture](handbook/04-architecture.md) — *Maintainer.* C4 view: context, containers, the DDD 4-layer shape, and the timesheet→billing flow traced end-to-end.
5. [Maintainer Guide](handbook/05-maintainer-guide.md) — *Maintainer.* Schema-YAML SSoT, regeneration, `// <<< CUSTOM` markers, where code goes per layer, the write-service/seam pattern, release flow.
6. [Developer Guide](handbook/06-developer-guide.md) — *App developer.* Install → quickstart → recipes (incl. billing a timesheet) → configuration → troubleshooting.
7. [Contributing](handbook/07-contributing.md) — *Contributor.* Dev setup, commit/PR conventions, tests and lint, review checklist.
8. [Glossary](handbook/08-glossary.md) — *All.* One term, one meaning — framework terms and the project-delivery ubiquitous language.
9. [Architecture Decision Records](handbook/adr/) — *Maintainer.* Why this design, not another ([0004](handbook/adr/adr-0004-project-boundary-billing-seam.md) is the module's defining decision).

## Alongside the handbook

The handbook is the *narrative*. These reference and product sets live beside it — link out, don't
duplicate:

- **[Schema DSL reference](schema/README.md)** — the exact YAML grammar: [types](schema/TYPES.md), [model rules](schema/RULE_FORMAT_MODELS.md), [generation targets](schema/GENERATION.md), [error codes](schema/ERROR_CODES.md), [examples](schema/EXAMPLES.md). The *Reference* corner of Diátaxis; the handbook explains the *why*.
- **[Business flows](business-flows/README.md)** — one doc per business flow (actors, preconditions, rules, postconditions), each linked to its executable BDD oracle; [golden cases](business-flows/golden-cases.md) mirror the `tests/` suite (PGC / IP / PBSEAM).
- **[ADR-001 — project boundary & billing seam](adr/ADR-001-project-boundary-and-billing-seam.md)** — the extended decision write-up behind handbook [ADR-0004](handbook/adr/adr-0004-project-boundary-billing-seam.md), including the full parking lot.
- **Product docs** — [BRD](BRD.md) · [PRD](PRD.md) · [FSD](FSD.md) · [Extension guide](extension-guide.md).

## Conventions this handbook follows

- **Reader + mode named** at the top of every page.
- **Commands are real.** Every `metaphor …` command was run against `metaphor 0.2.0` while writing. Where a command in the top-level [README](../README.md) is stale, the handbook flags it and gives the working form.
- **Code wins over docs.** When a doc and the schema/code disagree, the schema YAML (the source of truth) wins — the doc is the bug.
