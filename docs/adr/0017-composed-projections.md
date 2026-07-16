---
id: ADR-0017
title: Composed durable and hot projections
slug: composed-projections
status: Accepted
date-proposed: 2026-07-16
date-accepted: 2026-07-16
deciders: [haru]
relates-to: [ADR-0009, ADR-0012, ADR-0016]
impacts: [crates/miku-domain, crates/miku-app, crates/miku-index-memory, crates/miku-index-sqlite, miku_docs]
tags: [architecture, projections, tantivy, sqlite, valkey, postgres]
---

# ADR-0017 — Composed durable and hot projections

## Decision

`miku_docs/` is Miku's authoritative Markdown filesystem. `miku_vault::Vault` is only the filesystem adapter; it is not the source-of-truth concept.

Miku composes independent projection roles:

```text
miku_docs
  -> filesystem watcher and indexer
  -> durable projection: SQLite or PostgreSQL
  -> hot projection: MemoryIndex (page graph + Tantivy)
  -> optional shared result cache: Valkey
```

Tantivy is part of the hot `MemoryIndex` projection. The current implementation uses
`Index::create_in_ram`, so its index is rebuildable and process-local. Tantivy may later use a
filesystem directory, but it remains a derived search projection rather than the authoritative
Markdown store.

The default runtime is:

```text
miku_docs + SQLite durable projection + MemoryIndex/Tantivy hot projection
```

PostgreSQL and Valkey are optional deployment layers. Valkey does not replace Tantivy; it may
cache serialized results or coordinate a shared hot path when multiple processes justify its
network and operational cost.

## Required trait boundaries

The runtime composition layer must separate:

- `DocumentSource`: read, write, and watch `miku_docs` through the filesystem adapter.
- `ProjectionReader` and `ProjectionWriter`: backend-neutral indexed page, graph, tag, mention,
  and search operations.
- `DurableProjection`: SQLite or PostgreSQL implementations.
- `SearchProjection`: MemoryIndex's page graph and Tantivy search index.
- `ResultCache`: an optional best-effort Valkey cache; cache failure must not make the source or
  durable projection unavailable.
- `RuntimeComposer`: resolves the selected durable backend and composes the hot/cache layers.

The existing `IndexReader`/`IndexWriter` contracts are the starting projection contract, but the
current `compose_index` implementation still selects one complete store. It must be refactored
before SQLite + MemoryIndex and Valkey composition can be considered implemented.

## Consistency contract

For a filesystem change:

```text
miku_docs write
  -> durable projection commit
  -> MemoryIndex/Tantivy update
  -> Valkey invalidation, when enabled
```

The indexer remains the sole projection writer. A durable commit precedes publication to the hot
projection. A failed optional cache operation degrades cache freshness, not document correctness.

## Consequences

- The default path gets local low-latency graph and full-text reads without requiring Valkey.
- SQLite/PostgreSQL durability can change without changing the web/API traits.
- Valkey is justified only for shared cache/process scaling, not as a faster replacement for local
  memory or Tantivy.
- Restart behavior must be measured separately for source scan, durable projection recovery, and
  hot Tantivy rebuild.

## Implementation status

The ADR is accepted as the target design. The current code has the backend-neutral reader/writer
traits and individual implementations, but its runtime composer still needs the default
`SQLite + MemoryIndex/Tantivy` composition refactor.
