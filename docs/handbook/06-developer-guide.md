<!-- Reader: App developer · Mode: Tutorial → How-to -->
# Developer Guide

Get from a checkout of `backbone-project` to a running module with all seven entities exposed over
REST, then wire the one operation that makes this module worth composing: **billing a timesheet**.
The tutorial part holds your hand once; the recipes assume you know your way around.

Commands here were run against `metaphor 0.2.0`. Where the top-level [README](../../README.md) shows a
`backbone-schema`/`backbone` command, use the `metaphor` form below — those are the ones that work
today.

## Prerequisites

- **Rust** (2021 edition toolchain) and **Cargo**.
- The **`metaphor`** CLI on your `PATH` (`metaphor --version` → `metaphor 0.2.0` or newer).
- A reachable **PostgreSQL** instance.

## What this module is

`backbone-project` owns **project delivery**: `Project`s, their `Task`s, and `Timesheet`s of logged
effort. It gives margin visibility through cost/billing **roll-up fields** and **posts no GL** — money
moves only when a billable timesheet is handed to `backbone-billing`. It *reads* the Customer from
`backbone-party` and the originating Sales Order from `backbone-selling` by **logical foreign key**.
See [Philosophy](01-philosophy.md) for the north star and [Architecture](04-architecture.md) for the
shape.

| Entity | Table (`project` schema) | What it is |
|--------|--------------------------|------------|
| `Project` | `project.projects` | A customer delivery engagement with cost/billing roll-up |
| `Task` | `project.tasks` | A unit of work in a project (adjacency-list tree via `parent_task_id`) |
| `Timesheet` (+ `TimesheetDetail`) | `project.timesheets`, `project.timesheet_details` | Logged effort; a submitted billable sheet is the outbound seam |
| `ActivityType` | `project.activity_types` | A kind of work with default `billing_rate` + `costing_rate` |
| `ProjectTemplate` (+ `ProjectTemplateTask`) | `project.project_templates`, `project.project_template_tasks` | A reusable task list you instantiate onto a new project |

## Quickstart — prove the toolchain end to end

```bash
# From the backbone-project directory:
export DATABASE_URL="postgresql://root:password@localhost:5432/project_dev"

# 1. Validate the schema (all model YAML under schema/models/).
metaphor schema schema validate

# 2. Apply the module's migrations (own `project` schema, 7 tables, enums, audit triggers).
metaphor migration run

# 3. Run the module's tests — the 12-test correctness gate.
metaphor dev test
```

Expected: validation passes; migrations report the `project` schema and its tables created; the test
run is green. The gate is real database-backed tests (`tests/project_golden_cases.rs`,
`tests/integrity_probes.rs`, `tests/project_billing_seam.rs`) — not a placeholder.

To see the HTTP surface, compose the module into a service (see [Recipe: mount the module](#how-do-i-mount-this-module-in-a-service))
and `metaphor dev serve`, then create a project:

```bash
curl -s -X POST localhost:8080/api/v1/projects \
  -H 'content-type: application/json' \
  -d '{"companyId":"…uuid…","projectName":"Website revamp","projectType":"external","currency":"IDR"}'
# → 201 { "id":"…", "projectName":"Website revamp", "status":"open", "metadata":{ "createdAt":"…" } }
```

Note the JSON is **camelCase** (`projectName`, `createdAt`) even though the Rust and SQL are
snake_case — that is the generated `#[serde(rename_all = "camelCase")]` at work.

## Key concepts

Five ideas carry you the rest of the way. One line each; the linked page explains *why*.

- **Schema YAML is the source of truth.** You edit [`schema/models/*.model.yaml`](../schema/RULE_FORMAT_MODELS.md);
  the entity, DTOs, migration, repository, service, handler, and routes are generated from it.
  ([Philosophy](01-philosophy.md).)
- **A module is a library, not a service.** It has no `main.rs`. A `backend-service` composes it via
  `ProjectModule::builder().with_database(pool).build()?` and mounts its router.
  ([Architecture](04-architecture.md).)
- **Twelve endpoints come free per entity.** `BackboneCrudHandler` gives list / create / get / update /
  patch / soft_delete / restore / empty_trash / bulk_create / upsert / find_by_id / list_deleted,
  mounted under `/api/v1/<collection>`.
- **Generated CRUD is unguarded; the real verbs live in `ProjectWriteService`.** The generated routes
  write tables directly and skip roll-up and status gates. Roll-up, `submitted → billed`, and the
  billing hand-off live in `ProjectWriteService` — call it directly (see the seam recipe).
  ([ADR-0004](adr/adr-0004-project-boundary-billing-seam.md).)
- **Custom code survives regeneration** if it sits in `// <<< CUSTOM` markers, `*_custom.rs` files, or a
  `user_owned` path (`docs/**`, `tests/features/**`). Anything else is overwritten by `generate --force`.
  ([ADR-0003](adr/adr-0003-custom-markers.md).)

## Recipes

### How do I mount this module in a service?

`ProjectModule` is the composition root (in [`src/lib.rs`](../../src/lib.rs)). Build it with a pool and
merge its router:

```rust
use backbone_project::ProjectModule;

let project = ProjectModule::builder()
    .with_database(pool.clone())
    .build()?;

// The FULL, UNGUARDED CRUD surface — 12 endpoints on each of the 7 entities.
// Fine for trusted/admin/seeding; compose a guarded router for production.
let app = axum::Router::new().nest("/api/v1", project.all_crud_routes());
```

> `ProjectModule::routes()` is `#[deprecated]` — it aliases `all_crud_routes()` but its name hides that
> it mounts *unvalidated* CRUD. Call `all_crud_routes()` explicitly, or compose read + validated-write
> routers for production.

### How do I bill a timesheet? (the outbound seam)

This is the module's defining operation, and it is **not** an HTTP route — you drive it from the
composing service through `ProjectWriteService`. The steps:

1. **Implement `BillingPort`** over your real billing module. The port is the only outbound edge; the
   shipped library has *zero* Cargo dependency on `backbone-billing`.

   ```rust
   // In the composing service — the adapter that bridges project → billing.
   use backbone_project::application::service::{BillingPort, InvoiceFromTimesheet, InvoiceAck, ProjectRejected};

   struct RealBilling { /* handle to backbone-billing's write service */ }

   #[async_trait::async_trait]
   impl BillingPort for RealBilling {
       async fn create_service_invoice(&self, req: &InvoiceFromTimesheet)
           -> Result<InvoiceAck, ProjectRejected> {
           // Call backbone-billing; it builds the Sales Invoice and posts the GL.
           // MUST be idempotent on the stable key req carries (TS-<timesheet id>).
       }
   }
   ```

2. **Log time**, then **bill**, via `ProjectWriteService` (raw verbs, transactional):

   ```rust
   use backbone_project::application::service::{ProjectWriteService, LoggingSink};

   let write = ProjectWriteService::new(pool.clone());
   let sink = LoggingSink;                 // or a bus-backed ProjectEventSink

   // logs a *submitted* timesheet + detail lines, snapshotting each activity's
   // billing_rate/costing_rate, and rolls the project's totals in one tx.
   // Returns the new timesheet id.
   let timesheet_id = write.log_time(project_id, new_timesheet, &sink).await?;

   // hands the billable sheet to billing, gates submitted → billed, records invoice_id:
   write.bill_timesheet(timesheet_id, &real_billing, &sink).await?;
   ```

   Signatures (from [`project_write_service.rs`](../../src/application/service/project_write_service.rs)):
   `log_time(&self, project_id: Uuid, ts: NewTimesheet, sink: &dyn ProjectEventSink) -> Result<Uuid, ProjectError>`
   and `bill_timesheet(&self, timesheet_id: Uuid, billing: &dyn BillingPort, sink: &dyn ProjectEventSink) -> Result<BillOutcome, ProjectError>`.

The hand-off bills **once** (idempotent on `TS-<id>`, gated `submitted → billed`), and `project` records
the echoed `invoice_id` while `backbone-billing` posts the ledger. See
[Architecture §4b](04-architecture.md#4b-the-domain-flow--log-time-then-bill-it-the-outbound-seam) for
the traced sequence and [ADR-0004](adr/adr-0004-project-boundary-billing-seam.md) for the decision.

> **Why not a generated CRUD `PATCH /timesheets/{id}` to set `status:"billed"`?** Because that path
> bypasses the roll-up and the idempotent invoice hand-off — you would mark a sheet billed with no
> invoice and a stale margin. The write-service *is* the guard.

### How do I instantiate a project from a template?

`ProjectTemplate` + `ProjectTemplateTask` define a reusable task list; `ProjectWriteService::instantiate_template`
stamps those tasks onto a new project. Create the template rows via generated CRUD (or a seeder), then
call the verb from the composing service — see golden case PGC-3 in
[`docs/business-flows/golden-cases.md`](../business-flows/golden-cases.md).

### How do I reference the Customer, Sales Order, or an employee?

By **logical foreign key**, declared in the schema — never by copying the other module's table in.
`backbone-project` already does this:

```yaml
# schema/models/project.model.yaml
customer_id:
  type: uuid?
  attributes: ["@exclude_from_foreign_key_check"]   # logical FK to party.Party.id
source_so_id:
  type: uuid?
  attributes: ["@exclude_from_foreign_key_check"]   # logical FK to selling.SalesOrder.id
```

The column carries the reference; there is no database constraint, so the modules stay independently
deployable. Audit actors (`created_by`, …) reference `sapiens.User.id` the same way.

### How do I add a business rule (e.g. "a completed project rejects new time")?

That rule already lives in `ProjectWriteService` (the roll-up UPDATE is gated `WHERE status='open'`).
For a *new* rule, extend the write service or add a custom method — put it in a `*_custom.rs` or inside
a `// <<< CUSTOM` marker so regeneration cannot eat it. See the
[Maintainer Guide → the write-service pattern](05-maintainer-guide.md#the-billing-seam-the-write-service-pattern).

### How do I seed sample data?

Seeders live in `src/seeders/` (Rust) with SQL fixtures under `migrations/seeds/`:

```bash
metaphor migration seed project            # run the Rust seeders
metaphor migration generate-seeds project  # (re)emit SQL seed files
```

## Configuration

Defaults live in [`config/application.yml`](../../config/application.yml); override per environment and
at runtime.

| Option | Default | When to change |
|--------|---------|----------------|
| `server.host` | `0.0.0.0` | Bind to a specific interface. |
| `server.port` | `8080` | Port conflicts / multi-service hosts. |
| `database.url` | `postgresql://root:password@localhost:5432/project_dev` (dev) | **Always** in real deployments — override with the `DATABASE_URL` env var, which takes precedence. |
| `database.max_connections` | `10` | Tune to your Postgres pool budget. |
| `logging.level` | `info` | `debug`/`trace` when diagnosing; `warn` in noisy prod. |
| `features.workflows` | `true` | Toggle module workflow orchestration. |

Layered files: `application.yml` (base, `skeletondb`) → `application-dev.yml` (`project_dev`) /
`application-prod.yml` (`${ENV}` interpolation). `DATABASE_URL` in the environment always wins over the
YAML.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `backbone-schema: command not found` | Following the stale README | Use `metaphor schema schema …`. `backbone-schema` is not a separate binary here. |
| `metaphor migration run` can't connect | `DATABASE_URL` unset or Postgres down | `export DATABASE_URL=postgresql://…`; confirm Postgres is reachable. |
| A timesheet is `billed` but the project margin/`total_billed_amount` is stale | Set status via generated CRUD `PATCH`, bypassing the write service | Bill through `ProjectWriteService::bill_timesheet` — the generated CRUD path skips roll-up + the idempotent hand-off. |
| `cannot resolve backbone_billing` when building the library | Tried to import billing from module code | You don't — implement `BillingPort` in the *composing service*. billing is a dev-only edge; the library has no Cargo dependency on it. |
| My custom method vanished after regen | Code sat outside a protected region | Move it inside a `// <<< CUSTOM` marker, a `*_custom.rs` file, or a `user_owned` glob ([Maintainer Guide](05-maintainer-guide.md#regen-safety--the-rules-that-keep-your-logic-alive)). |
| New endpoint returns 404 | Route not composed, or module not mounted | Mount `project.all_crud_routes()`; merge custom routes in `routes/`. |
| Schema change ignored | Edited generated Rust instead of the YAML | Revert the Rust, edit `schema/models/*.model.yaml`, regenerate. |
| JSON field names look wrong (`created_at` vs `createdAt`) | Expecting snake_case on the wire | DTOs are `camelCase` by design; snake_case is DB/Rust only. |

---

Next: [Contributing](07-contributing.md) to send a change back, or the [Glossary](08-glossary.md) to
pin down a term.
