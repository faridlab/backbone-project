<!--
Date: 2026-08-03
Repo type: module
Unit: backbone-project
Focus lens: bounded-context-cleanliness
Roster (standing): chair, skeptic, steelman, yagni-business
Roster (context): ddd-bounded-context, contract-seat
Roster (invited): domain-expert (project-delivery domain rules present)
Subagent seats: steelman, skeptic, chair (isolated, read-only)
Question: "is this repo complete, nothing missing?"
-->

# Council — module:backbone-project — focus: bounded-context-cleanliness

## Best call
Re-land a module-owned `ProjectQueryService` impl in the designated `// <<< CUSTOM SERVICES` block (`exports/services.rs:93-95`) or a sibling `*_custom.rs`, wired in `ProjectModule::build()` and exposed via a `pub fn query_service(&self) -> Arc<dyn ProjectQueryService>` accessor. The trait's own header (`services.rs:5-6`, "provide the public API for other modules") plus the documented consumer pattern (`exports/mod.rs:11`) makes this an INBOUND port the module must fulfill — not a host-supplied adapter. The Steelman's BillingPort analogy is architecturally flawed: outbound ports (consumer-side, dependency inversion) can defer impl to the host; inbound read-ports (provider-side) cannot, because only this module owns its repos.

- Residual negative value: ~4-6 hours of mechanical work (21 trait methods, each a 1-3 line repo delegation; all 7 repos already exist and are wired in `ProjectModule::build()`). Until landed, the inbound contract is hollow — any sibling following the documented `use project::exports::ProjectQueryService` pattern imports a trait with no constructable value, and the `pub mod exports;` publication (newly added in this WIP) advertises a contract that does not exist. Negative compounds if a sibling starts coding against it before the gap is closed: ~1-2 days of integration rework plus a forced contract redesign under pressure.
- Reversibility: easy — one impl block behind an existing trait plus one accessor on `ProjectModule`; delete both to revert. The trait stays as-is (it is correctly shaped and already published).
- What would flip this: a doc statement parallel to `handbook/04-architecture.md:114-116` (which documents the WRITE side's host-construction as intentional) explicitly stating ProjectQueryService is host-supplied AND naming where the adapter lives; OR a sibling repo that publishes its own `impl ProjectQueryService` over repo accessors this module exposes (which would require this module to export repos — it does not). Neither exists today.

## Disagreement map
1. Read-port impl ownership — THE crux. Steelman says trait-here/impl-in-composition-root mirrors BillingPort discipline and the deletion is a move TOWARD encapsulation. Skeptic + Contract-seat say the module's own docs make it a contract the MODULE must fulfill; a host cannot implement reads over repos it does not own. DECIDED FOR SKEPTIC: the analogy conflates outbound (consumer-side) and inbound (provider-side) port directions, which are not symmetric. Verified: BillingPort has 2 impls + ADR + dev-guide sample + maintainer-guide statement; ProjectQueryService has zero of all four.
2. Aggregate children exposed as first-class CRUD roots — DDD-seat flags that TimesheetDetail and ProjectTemplateTask are aggregate CHILDREN (timesheet total_hours = Σ line.hours) but `all_crud_routes()` (`lib.rs:82-101`) mounts full 12-endpoint unguarded CRUD on both, letting a caller mutate a detail line directly and bypass the timesheet invariant. Generated-symmetry seat treats this as acceptable (codegen treats all entities uniformly). Real 🟠 should-fix, ranked #2 below — it is a genuine cleanliness gap but does not block "complete & usable" the way the hollow inbound contract does.
3. Event-sourcing layer behind default-OFF `events = []` — YAGNI-seat calls it the biggest ahead-of-scale bet (per-entity `domain/event/*_events.rs` + handlers for all 7, no consumer shown). Substance-seat defends it as gated, zero compile cost off-feature. Gated and invisible to the default build, so low blast radius — parked.

## Recommendations (ranked by leverage)
| # | Move | Leverage | Residual negative | Reversibility | Evidence to flip |
|---|------|----------|-------------------|---------------|------------------|
| 1 | Re-land module-owned `ProjectQueryService` impl + `pub fn query_service()` accessor on `ProjectModule` | Closes the only hollow published contract; unblocks documented sibling integration; finishes the encapsulation refactor the WIP started | ~4-6h mechanical; zero design risk (restoring a deletion, repos exist) | easy (delete impl + accessor to revert; trait stays) | A doc statement parallel to write-side `handbook/04-architecture.md:114-116` naming a host adapter location; or a sibling repo publishing its own impl over this module's exported repos (none exported) |
| 2 | Stop mounting full CRUD on aggregate children: drop TimesheetDetail + ProjectTemplateTask from `all_crud_routes()`, expose them only through validated write paths that preserve the timesheet/template invariants | Protects `total_hours = Σ line.hours` and template-task consistency; removes a real invariant-bypass surface | ~1 day to route the validated paths + update tests; temporary friction if any seeder relied on direct detail CRUD | medium (route composition change; tests follow) | A finding that no caller can reach detail endpoints in practice (audited access logs) — then it is latent, defer |
| 3 | Decide the `events = []` feature's fate: either point at a consumer or delete the per-entity event handlers | Removes a maintenance surface with no demonstrated consumer; OR commits to the layer with a real subscriber | Low if kept gated (zero compile cost off-feature); maintenance drift if left undecided | easy (feature-flagged; delete the flag + handlers) | A consumer repo found in the workspace that subscribes to `ProjectEvent` via `ProjectEventSink` |
| 4 | Replace `tests/features/example.feature` skeleton with executable BDD for the billing handoff (`submitted → billed`) and template instantiation | Locks the two highest-value domain flows as executable spec; currently only golden-cases (Rust + prose) cover them | ~1-2 days to wire a step runner; pure additive, no code change | easy (additive test artifact) | A decision that golden cases in Rust are the canonical executable spec and BDD is out of scope |

## Parking lot
- BDD feature skeleton is placeholder (`tests/features/example.feature`) — Domain-expert; scope: test-artifact completeness, not bounded-context cleanliness.
- `#[deprecated] routes()` alias (`lib.rs:103-111`) still ships — could be removed once all consumers migrate to `all_crud_routes()` or guarded compositions; Contract-seat; scope: API surface hygiene.
- Version-transform / VersionedResponse infrastructure is symmetric and complete across all 7 entities but its consumer (a versioned gateway) was not located — Contract-seat; scope: cross-cutting versioning, out of focus.
- Company RLS fence is modeled (schema comments + domain_policy) but not verified against a live multi-tenant deploy — Domain-expert; scope: runtime security, out of focus for a crate-completeness review.
