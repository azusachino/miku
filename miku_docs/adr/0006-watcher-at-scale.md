---
id: ADR-0006
type: adr
title: ADR-0006 — Filesystem watcher at scale
slug: watcher-at-scale
status: Accepted
updated: 2026-06-26
date-proposed: 2026-06-26
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:watcher-at-scale
supersedes: []
superseded-by:
relates-to: []
rejects: [rocksdb-primary-store, rocksdb-work-queue]
impacts: [src/indexer, src/watcher, miku_docs/setup.md]
config-keys: [fs.inotify.max_user_watches]
tags: [watcher, notify, inotify, scale]
---

# ADR-0006 — Filesystem watcher at scale

## Decision

Keep v1's `notify` watcher as the **sole index trigger**; scale it by watching **directories, not files**. The watch budget equals **directory count, not file count**. Three levers, in order:

1. **Recursive `notify` (default)** — one watch per directory; the crate auto-registers a watch when a new subdir appears.
2. **Raise `fs.inotify.max_user_watches`** — documented in setup (the standard "increase watches" note every IDE ships). Covers the rare deep-tree case.
3. **`PollWatcher` fallback** — zero inotify watches, periodic mtime scan, traded for latency. Switched on only past an extreme directory-count threshold (archive / `MIKU_READONLY` territory).

The startup mtime+hash reconcile sweeps anything missed across the new-subdir registration race or process downtime.

## Why

The 100k-file watch limit was **misdiagnosed**. inotify watches are **per-directory**, and `notify`'s recursive mode adds one watch per subdirectory, so a wiki with shallow foldering never approaches
the limit (100k files across ~200 folders ≈ 200 watches; default cap 65k–524k; macOS
FSEvents has no per-file limit at all). The watcher's only irreplaceable job is **live
pickup of external edits**
(git pull, another editor) — exactly the files-are-truth payoff.

## Trade-offs / Rejected

**RocksDB** as a durable work-queue or primary store (the former `dataflow_v2.md`) is rejected: it solves a problem Miku doesn't have, adds a second store, and risks the core invariant
(files-are-truth). The event-driven, single-writer v1 model is retained unchanged. See `miku_docs/dataflow.md` §8 (folder-scoped watching).
