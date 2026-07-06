# Architecture Decision Records

One decision per record: context, decision, alternatives, consequences. **Immutable once
accepted** — to change a decision, write a new ADR that supersedes the old one and update its
Status line; never edit an accepted decision in place.

| ADR | Decision | Status |
|-----|----------|--------|
| [0001](adr-0001-schema-yaml-ssot.md) | Schema YAML is the single source of truth | Accepted |
| [0002](adr-0002-generic-crud.md) | CRUD is inherited from generics, not written per entity | Accepted |
| [0003](adr-0003-custom-markers.md) | Regen-safety via CUSTOM markers and `user_owned` | Accepted |
| [0004](adr-0004-project-boundary-billing-seam.md) | The project boundary and the one outbound timesheet→billing seam | Accepted |

0001–0003 are **framework** decisions (true for every Backbone module). 0004 is **module-specific** to
`backbone-project`; its extended write-up is [`docs/adr/ADR-001`](../../adr/ADR-001-project-boundary-and-billing-seam.md).
