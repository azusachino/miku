---
id: ADR-0003
type: adr
title: ADR-0003 — Write conflicts & auth
slug: write-conflicts-auth
status: Accepted
updated: 2026-06-26
date-proposed: 2026-06-25
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:write-conflicts-auth
supersedes: []
superseded-by:
relates-to: [ADR-0004]
considered-rejected: [loro-crdt]
impacts: [src/handlers/save, src/store]
tags: [concurrency, optimistic-lock, auth, single-user]
---

# ADR-0003 — Write conflicts & auth

## Decision

**Write conflicts — optimistic concurrency.** The edit view embeds the file's **content hash** as a hidden field; `POST` recomputes the hash before renaming. If it changed since load → **409
Conflict** with a "file changed underneath you" prompt (show both versions). Cheap (one read+hash before rename), guards the _file_, never involves the indexer. Hash over mtime (mtime is coarse and
lies across `git`/`rsync`).

**Auth — no user system.** Miku stays **single-user and login-less**; network protection is the _deployment's_ job. Two modes cover every persona: `MIKU_READONLY` (view-only, no write routes) for
publishing, and a writable network deploy behind an authenticating reverse proxy.

## Why

Last-write-wins (atomic rename) can silently clobber two tabs or a browser edit racing a `git pull`. The content-hash 409 is the minimal guard consistent with the single-writer model. Accounts/RBAC
reinvents Notion and breaks "keep it simple."

## Trade-offs / Rejected

**Loro / CRDTs — considered, deferred.** Loro solves concurrent multi-writer / offline merge — a problem Miku deliberately doesn't have. Its authoritative state is an operation log that can't be
reconstructed from a plain `.md` snapshot, so it **conflicts with files-are-truth**. It becomes the right choice _only if_ we commit to real-time collaborative editing — at which point it earns its
own ADR.
