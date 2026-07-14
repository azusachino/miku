---
id: ADR-0004
title: Rename / delete & assets
slug: rename-delete-assets
status: Accepted
date-proposed: 2026-06-25
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:rename-delete-assets
supersedes: []
superseded-by:
relates-to: [ADR-0003]
depends-on: [ADR-0002]
impacts: [src/handlers/rename, src/handlers/delete, migrations]
config-keys: [MIKU_TRASH_TTL]
tags: [rename, soft-delete, assets, backlinks]
---

# ADR-0004 — Rename / delete & assets

## Decision

**Rename = first-class operation.** `POST /page/rename` (1) atomically renames the file, (2) finds referrers via `tb_links.target_id`, (3) rewrites `[[Old]]→[[New]]` in each referrer through the
normal atomic-save path (preserving aliases). Each rewrite fires `notify` → normal reindex. The one operation that writes many files — best-effort at FS level; partial failures self-heal via startup
reconcile. UX: "this will update N backlinks" confirm.

**Delete = soft-delete with 7-day archive.** The file is **moved to `miku_docs/.trash/<original-path>@<deleted-at>.md`**; `.trash/` is excluded from the watcher and index, so the page vanishes from
search/backlinks at once (row removed, inbound links go dangling via `ON DELETE SET NULL`) while the bytes survive. GC purges trash older than **`MIKU_TRASH_TTL` (default 7 days)**. Restore = move the
file back → reindex.

**Assets.** Live in `miku_docs/assets/`; upload writes atomically, deduped by content hash (`name-<short-hash>.ext`). `![[image.png]]` resolves by basename. Orphans are **reported, never
auto-deleted** (auto-GC of user files violates files-are-truth) — `tb_links(kind='asset' AND is_embed)` minus disk.

## Why

A bare rename leaves every `[[OldName]]` dangling; `notify` as the sole trigger keeps both the link-rewrites and the file move on the normal reindex path. Trash lives inside the content root so it
travels with backups/PVC, but is ignore-listed to keep the live index pure.

## Trade-offs / Rejected

No new asset table — asset targets already live in `tb_links`. Auto-deletion of orphan assets rejected outright.
