<!-- Reader: All · Mode: Reference -->
# Glossary — ubiquitous language

One term, one meaning, used everywhere in this handbook and in the code. When a term here names a
type or file, that name is exact. If you find a doc using a different word for one of these, the doc
is the bug.

The list has two parts: **framework terms** (true for every Backbone module) and **domain terms**
(specific to `backbone-project`'s project-delivery domain).

## Framework terms

### Aggregate / Entity
A domain object with identity and a lifecycle, defined by one `schema/models/<name>.model.yaml`. In
this module: `Project`, `Task`, `Timesheet`, `TimesheetDetail`, `ActivityType`, `ProjectTemplate`,
`ProjectTemplateTask`. Generated into `src/domain/entity/<name>.rs` with a strongly-typed id (e.g.
`TimesheetId`), a builder, `apply_patch`, and audit accessors.

### Application layer
The use-case layer (`src/application/`): services and DTOs. In this module it holds both the generated
service aliases *and* the hand-authored [billing seam](#the-seam--billingport) (`ProjectWriteService`
et al.). Depends on the domain; knows nothing about HTTP transport.

### Audit metadata
The `metadata` JSONB field (`created_at`, `updated_at`, `deleted_at`, `created_by`, `updated_by`,
`deleted_by`) added when `config.audit: true`. Timestamps are set by a Postgres trigger
(`migrations/*_add_audit_triggers.up.sql`); the `*_by` actor fields are logical FKs to `sapiens.User.id`.

### `BackboneCrudHandler`
The `backbone-core` type that produces an Axum `Router` with all **twelve** CRUD endpoints for an
entity. Invoked as `BackboneCrudHandler::<…>::routes(service, "/collection")`. You never hand-write
these routes. Note: this path is unguarded — see [twelve endpoints](#twelve-endpoints).

### Bounded context
The single business domain a module owns. `backbone-project` owns **project delivery** and nothing
else — it never edits another module's schema and references siblings by [logical FK](#logical-foreign-key).

### Composition root
`src/lib.rs` — the `ProjectModule` struct and `ProjectModuleBuilder`. Wires each of the seven services
to its repository and composes the router (`all_crud_routes()`). The one place allowed to depend on
every layer. (Note: `src/module.rs` is an orphaned skeleton stub and is **not** the composition root.)

### CUSTOM marker
A `// <<< CUSTOM … // END CUSTOM` region inside a generated file. Content between the markers survives
regeneration. Spelling varies per file (`// <<< CUSTOM METHODS START >>>`, `// <<< CUSTOM`, …) — match
what is already there.

### DTO (Data Transfer Object)
A wire-shape struct in `src/application/dto/`. Per entity: `Create…Dto`, `Update…Dto`, `Patch…Dto`,
`…ResponseDto`, `…SummaryDto`, `…ListResponseDto`. Serialized `camelCase`. Generated, with `From`/`Apply`
conversions to and from the entity.

### Domain layer
The innermost layer (`src/domain/`): entities, value objects, the module's enums, invariants, and
repository **traits** (ports). Depends on nothing.

### `GenericCrudRepository` / `GenericCrudService`
The `backbone-orm` / `backbone-core` generics that carry all standard CRUD. Each repository is a
**newtype** over `GenericCrudRepository<Entity, SoftDelete>` (with `TABLE_NAME = "project.<table>"`);
each service is a **type alias** over `GenericCrudService<Entity, CreateDto, UpdateDto, Repository>`.
Inherited, never re-implemented.

### Infrastructure layer
The adapter layer (`src/infrastructure/`): the seven repository newtypes over `GenericCrudRepository`.
Depends on domain and application.

### Logical foreign key
A cross-module reference declared as a UUID column with `@exclude_from_foreign_key_check` (or
`@foreign_key(module.Type.field)` for documentation). It records the relationship but is **not**
enforced by a database constraint, so modules stay independently deployable. Examples in this module:
`customer_id` → party, `source_so_id` → selling, `employee_id`/`*_by` → sapiens.

### `metaphor`
The workspace CLI (v0.2.0) that orchestrates the projects and dispatches to plugins (`metaphor-schema`,
`metaphor-codegen`, `metaphor-dev`). Prefer it over raw `cargo`/`sqlx`. The standalone `backbone-schema`
binary the README mentions is **not** installed; use `metaphor schema schema …`.

### Module
A **library crate** owning one bounded context in 4-layer DDD, schema-driven. `[lib]` only — no
`main.rs`. Composed into a `backend-service`; never run alone. This repo, `backbone-project`, is one.

### Own schema (per module)
Each module gets its own Postgres schema (`schema: project` in `index.model.yaml`). Migrations
`CREATE SCHEMA project` and qualify tables as `project.<table>`, so modules never collide on a table
name.

### Port / Adapter
The DDD names for the two repository shapes per entity: the **port** is the domain-layer repository
`trait` (the contract); the **adapter** is the infrastructure-layer newtype `struct` (the Postgres
implementation). See also [the seam / BillingPort](#the-seam--billingport) — the module's *outbound*
port.

### Presentation layer
The transport layer (`src/presentation/`, `src/routes/`): HTTP handlers and route composition. Depends
on the application layer.

### Regeneration (regen)
Re-running `metaphor schema schema generate … --force` to rebuild all downstream code from the schema.
Overwrites everything **outside** a protected region (CUSTOM markers, `*_custom.rs`, `user_owned` globs).

### Schema (the SSoT)
`schema/models/*.model.yaml` — the single source of truth. Every entity struct, DTO, migration,
repository, service, handler, and route is generated from it. Not to be confused with the *Postgres
schema* (the per-module `project` namespace).

### Soft delete
Marking a row deleted (`metadata.deleted_at` set) instead of removing it, enabled by
`config.soft_delete: true`. Backs the `soft_delete` / `restore` / `empty_trash` / `list_deleted`
endpoints.

### Twelve endpoints
The standard CRUD surface every entity gets from `BackboneCrudHandler`: `list`, `create`, `get`,
`update`, `patch`, `soft_delete`, `restore`, `empty_trash`, `bulk_create`, `upsert`, `find_by_id`,
`list_deleted`. Exposed all-at-once by `ProjectModule::all_crud_routes()` as the **unguarded** surface;
it bypasses domain roll-up and status gates.

### `user_owned`
The `metaphor.codegen.yaml` key listing glob paths the generator skips wholesale — never reads, merges,
or deletes. This module protects `tests/features/**` and `docs/**` (this handbook lives under one).

## Domain terms — project delivery

### Project
A customer delivery engagement (`project.projects`). Holds `project_type` (`external` = billable to a
customer / `internal`), `status` (`open → completed / cancelled`), and the [roll-up](#roll-up) fields.
Reads `customer_id` (party) and `source_so_id` (selling) by logical FK. **Posts no GL.**

### Task
A unit of work inside a project (`project.tasks`), arranged as a lightweight **adjacency-list tree**
via `parent_task_id` (NULL for a top-level task). Status `open → working → completed / cancelled`.
Deliberately *not* a WBS / critical-path / Gantt model.

### Timesheet / TimesheetDetail
`Timesheet` (`project.timesheets`) is logged effort against a project; its `TimesheetDetail` lines
(`project.timesheet_details`) each carry hours and a **snapshotted** billing + costing rate. Status
`draft → submitted → billed / cancelled`. `total_billable_amount = Σ hours·billing_rate`,
`total_costing_amount = Σ hours·costing_rate`.

### ActivityType
A kind of work (Consulting, Development, …) with default `billing_rate` (charged to the customer) and
`costing_rate` (internal cost), per hour (`project.activity_types`). A timesheet line references an
activity type and **snapshots its rates at log time**, so a later rate change never rewrites history.

### ProjectTemplate / ProjectTemplateTask
A reusable task list (`project.project_templates`, `project.project_template_tasks`) that
`ProjectWriteService::instantiate_template` stamps onto a new project.

### `billing_rate` vs `costing_rate`
The two rates on an `ActivityType` (and snapshotted per timesheet line): `billing_rate` is what the
customer is charged; `costing_rate` is the internal cost. The gap is the project's margin.

### Billable vs costing amount
`total_billable_amount` is what could be invoiced (`Σ hours·billing_rate`); `total_costing_amount` is
what it cost (`Σ hours·costing_rate`). Both are roll-up fields on both `Timesheet` and `Project`.

### Roll-up
The cost/billing totals denormalized onto the `Project` (`total_costing_amount`, `total_billable_amount`,
`total_billed_amount`) for **margin visibility without posting to the GL**. `log_time` adds to the
roll-up (gated `WHERE status='open'`); `cancel_timesheet` reverses it; `bill_timesheet` advances
`total_billed_amount`.

### The seam / `BillingPort`
The module's single **outbound** edge. `bill_timesheet` calls
`BillingPort::create_service_invoice(&InvoiceFromTimesheet) -> Result<InvoiceAck, ProjectRejected>`, and
a composing service supplies the implementation over `backbone-billing`. Defined in
`src/application/service/project_ports.rs`. **Zero normal Cargo edge** — the DTOs are the wire contract.

### `ProjectWriteService`
The hand-authored application service (`src/application/service/project_write_service.rs`) that carries
the real domain verbs — `create_project`, `add_task`, `advance_task`, `instantiate_template`,
`log_time`, `cancel_timesheet`, `bill_timesheet`, `complete_project`. It is **not** wired into
`ProjectModule` and has **no HTTP route**; a composing service constructs it and calls it directly.

### Sales Invoice
The billing artifact `backbone-billing` builds from a billable timesheet. project never builds it and
never posts its GL (Dr A/R · Cr Service Revenue · Cr PPN) — it only records the echoed `invoice_id`.

### `TS-<id>` idempotency key
The stable key the billing hand-off carries (derived from the timesheet id). Billing must dedupe on it
so a timesheet produces **at most one** downstream invoice; a retry returns the existing one.

### Transition gate
A status guard enforced in the same transaction as the state change: `log_time`'s roll-up is gated
`WHERE status='open'`; `bill_timesheet` gates `submitted → billed`; `cancel_timesheet` gates
`submitted → cancelled`. Gates make the writes idempotent and race-safe.

### Reverse gear
`cancel_timesheet` (`submitted → cancelled`) — reverses a mis-logged sheet's billable + costing amounts
off the project roll-up in the same open-project-gated tx. A **billed** sheet cannot be cancelled.

### Posts no GL
The module's boundary invariant: `backbone-project` never writes to the general ledger. It records
delivery intent + roll-up for margin; money moves only when a billable timesheet is handed to billing.

### `ProjectEvent` / `ProjectEventSink`
Domain events (`TimeLogged`, `TimesheetBilled`, `TimesheetCancelled`, `ProjectCompleted`) emitted by the
write service to a `ProjectEventSink` (`src/application/service/project_events.rs`). The default sink
logs; a composing service can supply a bus-backed sink.
