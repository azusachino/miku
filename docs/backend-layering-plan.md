# Backend Layering Plan

Drafted 2026-07-14. This supersedes the current Postgres-only implementation
direction for the next backend milestone; it does not change the UX roadmap.

## Goal

Allow Miku to run in two intentional deployment tiers:

1. a backend-neutral core API;
2. a default personal tier with an in-process memory cache/read model over a
   durable local SQLite/Turso index;
3. a high-end tier with an in-process memory cache, optional Valkey cache/event
   layer, and durable Postgres index.

An isolated in-memory backend still exists for tests and disposable runs, but it
is not the normal default because it loses the rebuildable index on restart.

The Markdown tree remains the only source of truth. Every backend stores a
rebuildable projection of `miku_docs/**/*.md`.

## Current constraint

The backend boundary does not exist yet. `AppState` owns `sqlx::PgPool`, route
handlers issue Postgres SQL directly, and `IndexerQueue` owns Postgres-specific
transactions, FTS, link resolution, and tag writes. The first task is therefore
an extraction of application operations, not a driver swap.

## Proposed layers

```text
HTTP routes / templates / SSE
              |
        Core application API
              |
     IndexStore + capabilities
        /         |          \
   memory     sqlite/turso     postgres
                  |
          optional Valkey cache/events
```

## Proposed workspace layout

Use a Cargo workspace with one binary crate and focused library crates. Cargo
workspaces share a lockfile and target directory while still allowing each
package to be checked independently.

```text
Cargo.toml                 # virtual workspace, shared metadata/dependencies
crates/
  miku-domain/             # domain types, IndexStore contract, capabilities
  miku-markdown/           # parsing, rendering, index projection extraction
  miku-indexer/            # notify watcher, reconcile loop, index events
  miku-index-memory/       # deterministic reference index and test mode
  miku-index-turso/        # local SQLite/Turso implementation and migrations
  miku-index-postgres/     # current Postgres implementation and migrations
  miku-cache-valkey/       # optional cache/event decorator
  miku-app/                # axum binary, templates, HTTP routes, startup
```

Do not create a crate for every module. In particular, keep HTTP DTOs and route
handlers in `miku-app`; “core API” means the application/index contract, not a
separate HTTP package. If `miku-markdown` remains tiny after extraction, it can
start inside `miku-domain` and split later.

The dependency direction should be one-way:

```text
miku-app -> miku-indexer -> miku-domain
        -> miku-markdown -> miku-domain
        -> one selected index crate -> miku-domain
```

Index crates must never depend on `miku-app`. The app selects an index store
at startup through a small factory; feature flags should prevent unused drivers
from entering the default binary.

## Tokio and Rust community conventions to practice

- Keep `#[tokio::main]` in `miku-app` only. Library crates expose async
  functions but do not create or own a runtime.
- Make long-lived tasks owned: return an indexer handle and provide explicit
  shutdown rather than spawning detached tasks that outlive the application.
- Pass a `CancellationToken` into the watcher/reconcile loops and track spawned
  tasks with `TaskTracker`; shutdown should cancel, close intake, and await all
  tasks.
- Keep bounded channels for filesystem events and make overflow/reconcile
  behavior explicit.
- Use `thiserror` for public library errors and `anyhow` only at the binary
  startup/route orchestration boundary.
- Add tracing spans around backend calls, indexing batches, and shutdown; never
  log credentials or raw page content.
- Require public contract types to be `Send`/`Sync` where possible and keep
  errors meaningful, documented, and source-preserving.
- Prefer integration tests at crate boundaries and a shared backend contract
  test module over tests that inspect private SQL implementation details.

For the first `IndexStore` trait, use an object-safe async boundary if runtime
selection requires `Arc<dyn IndexStore>`. A small `async_trait` dependency is
acceptable here; revisit native async-in-trait once the object-safety and MSRV
choices are explicit.

### Core application API

Keep the first contract small and expressed in domain types rather than SQL
rows or driver types:

- `PageSummary` — path, title, metadata, mtime;
- `PageIndex` — parsed page projection including links, aliases, tags, FTS text;
- `LinkRecord` and `MentionRecord`;
- `SearchRequest` / `SearchResult`;
- `IndexEvent` — changed, deleted, or reconciled page;
- `BackendInfo` — backend name, durability, capabilities, and health.

The application-facing operations should cover the existing routes and indexer:

- list/search pages;
- load page relationships and metadata;
- replace one page's index projection atomically;
- delete one page's projection;
- reconcile dangling links after a page change;
- list tags, backlinks, and unlinked mentions;
- report capabilities and health.

Do not expose `PgPool`, SQLite connections, Valkey clients, SQL strings, or
backend-specific row structs above this layer.

### Capability model

Start with one `IndexStore` contract plus a small `IndexCapabilities` value.
Split into multiple traits only when a backend genuinely cannot implement a
method. Likely capability flags are:

- `full_text_search`;
- `fuzzy_page_search`;
- `transactions`;
- `durable`;
- `distributed_cache`;
- `remote_sync`.

The HTTP API should remain stable. Unsupported optional behavior should degrade
to a documented implementation, such as substring search instead of pretending
that every backend has Postgres `tsvector` or trigram ranking.

## Backend roles

### Memory cache and test index

Use `HashMap`/`BTreeMap` projections as a bounded L1 cache/read model in both
deployment tiers. It is warmed from the durable primary and invalidated from
committed index events. Also keep a standalone memory implementation for
contract tests and disposable development. It is not durable and must never be
the silent production fallback.

### SQLite / Turso

Use one SQLite-compatible schema and migration set for the durable personal
backend. Prefer FTS5 and ordinary indexed columns over Postgres-only features.
Keep the local file path backend-qualified, for example `miku.sqlite.db`, so a
future backend cannot accidentally open or overwrite another backend's data.

The Turso/libSQL driver decision needs a spike before the main refactor:

- local SQLite-compatible operation;
- local durable file operation;
- remote Turso URL/auth operation;
- FTS5 behavior;
- transactions and concurrent index writes;
- bounded handling of `BUSY`/snapshot contention;
- migration and backup behavior.

Do not make remote Turso availability a requirement for local Miku startup.

### Postgres

Retain the current Postgres schema and use Postgres-native FTS/trigram ranking
where available. The implementation must satisfy the same core contract and
golden fixtures as SQLite, but does not need identical ranking scores.

### Valkey and Postgres high-end tier

Implement Valkey as an optional L2 decorator around the in-process memory cache,
with Postgres as the primary durable index. It is not an `IndexStore` and it
does not replace Postgres as the source of the index projection.
It may cache:

- quick-switch results;
- page summaries and tag lists;
- expensive search results;
- short-lived rendered fragments if measurement proves this useful.

The indexer remains authoritative for backend writes. Cache invalidation follows
committed `IndexEvent`s, and a Valkey outage must fall back to the primary
backend without making the wiki unavailable. SSE should remain in-process first;
distributed pub/sub is a later capability, not a prerequisite.

## Configuration shape

Use an explicit deployment tier and primary selector rather than inferring from
whichever URL exists:

```text
MIKU_TIER=local|scale
MIKU_PRIMARY=memory|turso|postgres
MIKU_DB_PATH=miku.sqlite.db
TURSO_DATABASE_URL=...
TURSO_AUTH_TOKEN=...
DATABASE_URL=...
VALKEY_URL=...
```

Startup should log backend identity and capabilities, never credentials. Invalid
combinations should fail with an actionable configuration error.

## Implementation order

### BE-01 — Contract and fixture extraction

- Define domain records and the `IndexStore` contract.
- Move route query shapes out of `src/main.rs` into `miku-app`.
- Create a backend-neutral fixture vault covering pages, aliases, tags,
  backlinks, dangling links, mentions, and FTS text.
- Add contract tests that every backend must pass.

### BE-02 — Memory cache, reference backend, and application wiring

- Implement the reference index without a database.
- Move `AppState` and `IndexerQueue` to depend on the `IndexStore` contract.
- Make route and indexer tests run without Postgres.
- Add an explicit disposable `MIKU_PRIMARY=memory` mode for tests/development.
- Add the in-process cache interface used by both deployment tiers.

### BE-03 — SQLite/Turso foundation spike

- Select and pin the Rust driver after the foundation checks pass.
- Add SQLite migrations and the durable file backend.
- Implement FTS5 search and bounded write contention handling.
- Add local backup/rebuild verification.

### BE-04 — Default local tier: memory cache + SQLite/Turso

- Make SQLite/Turso the durable primary when no primary is specified.
- Warm the in-process memory cache from SQLite/Turso on startup and update it
  from committed index events.
- Keep standalone `memory` explicit and disposable.
- Update setup, compose, and native development commands.
- Add a backend/capabilities health endpoint for operators and smoke scripts.

### BE-05 — High-end tier: memory + Valkey + Postgres

- Move the existing SQL into the Postgres index adapter.
- Compose the in-process cache with optional Valkey and Postgres.
- Preserve Postgres FTS/trigram behavior behind the same contract.
- Run the shared fixture suite plus a Postgres-specific integration suite.

### BE-06 — Cache and event hardening

- Add measured cache points only after baseline timings exist.
- Invalidate by committed index event/version.
- Test cold cache, warm cache, stale cache, unavailable Valkey, and restart.
- Keep the application correct with Valkey disabled.

## Non-goals for the first pass

- No generic ORM or “support every SQL dialect” abstraction.
- No Valkey-primary data model.
- No cross-backend live replication.
- No migration of Markdown content into a database.
- No frontend redesign as part of this backend work.

## Acceptance gates

- `make check` passes with the memory backend and no database service.
- The same fixture suite passes for SQLite/Turso and Postgres.
- Deleting and rebuilding the index from `miku_docs/` produces the same page,
  relationship, tag, and search results.
- A backend outage has an explicit health/error response and does not corrupt
  Markdown files.
- Valkey can be removed without changing correctness.
- The API exposes backend identity/capabilities without leaking secrets.

## Runtime composition

There is one durable primary and a tier-specific cache composition:

```text
Default tier:
  AppState
    primary: Turso/SQLite
    l1: InProcessMemory
    events: InProcessEventBus

High-end tier:
  AppState
    primary: Postgres
    l1: InProcessMemory
    l2: Valkey
    events: InProcessEventBus + optional ValkeyPubSub

optional:
  ValkeyCache / ValkeyPubSub
```

The startup factory reads the deployment tier and explicit connection settings,
constructs exactly one durable primary, then composes cache/event layers around
it. The composed read path is:

```text
request -> L1 memory lookup
         -> miss: L2 Valkey lookup when enabled
         -> miss: durable primary
         -> populate L2 then L1
         -> successful write: primary commit -> invalidate L1/L2 -> publish event
```

Valkey is never consulted for correctness-critical fallback decisions. If it is
unavailable, the wrapper logs the degraded state and delegates directly to the
primary backend. Cache keys should include a schema/version namespace and an
index generation so invalidation does not depend on deleting every key.

The in-process event bus remains the default for browser SSE. A Valkey pub/sub
implementation can be added later as a separate event capability when multiple
Miku processes are a real deployment requirement.

## crates.io release shape

Do not publish the entire workspace by default. A personal application and a
reusable Rust library have different release surfaces.

### Recommended initial policy

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

# In each application-only crate:
[package]
publish = false
```

Keep `miku-app`, index adapters, cache adapters, and deployment glue unpublished until
their public API and support promises are intentional. Release the application
as a container image, binary archive, or Git tag. This avoids promising that the
internal backend traits are a stable ecosystem API.

### If we want `cargo install miku`

Create a publishable binary package named `miku` with a small, documented CLI.
Its reusable path dependencies must either also be published with versions or
be replaced by published registry dependencies at release time. Publish in
dependency order, for example:

```text
miku-domain -> miku-markdown -> miku-indexer -> miku
```

Backend adapters can remain private if the binary package owns them directly;
otherwise they must be published before the binary. `miku-app` should not be
both an opaque internal package and a public library without a deliberate API
review.

### If we want a public Rust SDK

Publish only the stable, useful libraries:

```text
miku-domain
miku-markdown
optional: miku-index-turso, miku-index-postgres
```

Keep Valkey support separate because it adds an operational dependency. Every
published crate needs complete metadata, a README/rustdoc story, a license,
repository links, SemVer discipline, and no private path-only dependency.

Use workspace-inherited versions and dependency declarations, but verify the
packaged manifest for each crate. Features should be additive; do not encode
mutually exclusive primary backends as one giant feature matrix. Separate
backend packages keep dependency graphs and release promises clearer.

### Release checklist

1. Update the crate version and changelog entries.
2. Run workspace formatting, lint, tests, and backend-specific integration tests.
3. Run `cargo package --list -p <crate>` and inspect the package contents.
4. Run `cargo publish --dry-run -p <crate>` in dependency order.
5. Publish each public crate explicitly with `cargo publish -p <crate>`.
6. Tag the released commit and publish the application binary/container.

Publishing is permanent on crates.io, so dry-run and package inspection are
required before the first upload.

## References

- [libSQL Rust Builder](https://docs.rs/libsql/latest/libsql/struct.Builder.html)
  documents local, remote, and replica database modes.
- [Turso Rust Builder](https://docs.rs/turso/latest/turso/struct.Builder.html)
  documents the current in-process Rust driver; the project describes Turso as
  beta, so the driver must be validated before becoming the default.
- [Turso database repository](https://github.com/tursodatabase/turso) describes
  its SQLite-compatible in-process model and current beta status.
- [redis Rust cluster module](https://docs.rs/redis/latest/redis/cluster/)
  demonstrates the existing Rust client path for Redis/Valkey-compatible
  clusters.
