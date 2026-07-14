# Runtime workflow

This document describes the real local runtime when a browser is using Miku while the filesystem indexer is working. It is the contract for route behavior, background tasks, backend composition, and
black-box verification.

## Invariants

- `miku_docs/**/*.md` is the source of truth.
- The database and in-process index are rebuildable projections.
- The HTTP server must bind and serve page routes without waiting for the initial filesystem reconciliation to finish.
- `/p/{path}` remains accessible while reconciliation is running. A temporary indexing failure is logged and must not turn an otherwise readable page into a startup failure.
- `index_ready=false` means “the initial reconciliation has not completed”; it is an operator/smoke-test signal, not a page-route gate.
- The Turso driver connection is single-use at a time. Backend code serializes durable Turso operations, while the HTTP read model remains available.

## Startup sequence

```text
process
  -> resolve runtime from env + Cargo features
  -> compose IndexApi in miku-app
  -> load durable Turso projection into MemoryIndex
  -> create IndexerQueue and spawn its consumer/ticker tasks
  -> build Axum router and bind HTTP listener
  -> browser may request /p/Index immediately
                         \
                          -> background reconcile miku_docs
                               -> parse Markdown
                               -> durable Turso batch write
                               -> update MemoryIndex
                               -> emit /events page refresh
                               -> set index_ready=true
```

`IndexerQueue::new_with_writer` starts the consumer before the listener binds; the consumer is deliberately independent of request handling. The initial reconcile walks the vault, skips unchanged
files by mtime, writes changed pages in batches, removes stale projections, and then marks readiness.

## Browser page request while indexing

For `GET /p/Index`:

1. Axum routes the request to `page_handler`.
2. The handler reads the Markdown source from `miku_docs/` and renders the page directly; it does not wait for `index_ready`.
3. Relationship data is read through the backend-neutral `IndexApi`.
4. Unlinked-mention candidates use backend search, then the handler confirms the match against the source Markdown before rendering it.
5. If the indexer is currently writing, Turso search falls back to the in-process projection rather than waiting on the backend mutex or returning `concurrent use forbidden`.
6. The response is returned independently of whether the background reconcile has finished. New index events are delivered to browsers through SSE at `/events`.

The important distinction is that the page source is filesystem-owned, while search, backlinks, tags, and navigation metadata are projection-backed. A partially rebuilt projection can be temporarily
stale; it must not make the source page inaccessible.

## Thread/task ownership

| Actor                 | Owns                                                   | Blocking boundary                                    |
| --------------------- | ------------------------------------------------------ | ---------------------------------------------------- |
| Tokio HTTP tasks      | page, search, edit, API, and SSE requests              | filesystem reads and `IndexApi` calls                |
| Indexer consumer task | startup reconcile, watcher events, periodic reconcile  | Markdown parsing and `IndexWriter` calls             |
| Notify callback       | converts filesystem events to bounded-channel messages | `try_send`; never performs database work             |
| Reconcile ticker task | queues a coalesced full reconcile                      | bounded channel/atomic flag                          |
| Turso backend mutex   | one durable driver operation at a time                 | only the Turso connection, not the whole HTTP server |
| MemoryIndex           | process-local read projection                          | short `RwLock` critical sections                     |

No route owns or spawns a second indexer. `IndexerQueue::shutdown` aborts and awaits its owned tasks during process shutdown.

## Verification workflow

The Rust tests cover backend contracts and driver-level concurrency. The uv suite covers the actual running application:

```bash
make run                         # starts Turso + background indexer
make check-blackbox              # waits for /api/health index_ready=true
MIKU_BLACKBOX_URL=... make check-blackbox
```

The live blackbox sequence must exercise `/api/health`, `/`, `/p/{path}`, `/p/{path}/edit`, `/search`, `/tags`, and folder routes when the fixture has them. It also sends a title-case `Miku` body
search because this is the same class of query used by page-view unlinked mentions. The blackbox must be run against a real process and corpus; helper-only tests are not sufficient proof of this
workflow.

When diagnosing startup behavior, inspect both signals:

- `index_ready=false` with successful `/p/...`: expected background work;
- HTTP 500, `concurrent use forbidden`, or an FTS parse error: regression.
