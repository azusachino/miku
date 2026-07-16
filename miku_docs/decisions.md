# Design Decisions (ADRs)

Resolutions to the open design corners. Each is a decision, not code. Mirrored in asobi (`miku:decision:*`). See `architecture.md` for the prose design.

> **Status:** ADR-1 … ADR-5 below are **verified** and now canonical in `miku_docs/adr/` (`0001`–`0005`). This file is the **draft/staging** pool for proposed decisions; the verified copies in `miku_docs/adr/`
> win on any conflict.

---

## ADR-1 — Full-text search (English)

**Decision.** Use Postgres' built-in **`english`** FTS config: `to_tsvector('english', title ‖ body)` with **title weighted A, body B**, ranked by `ts_rank`, snippets via `ts_headline('english', …)`.
Content and titles are English in practice; the app name (ミク) is branding, not a content-language requirement.

**Why this is now trivial.** No extension, no Rust tokenizer, no spike. Postgres stays **vanilla** so the index is disposable/portable and identical across compose and k8s. This was the only ADR that
needed a spike — it's eliminated.

**Deferred, not rejected.** If meaningful CJK content ever shows up, revisit with app-side `lindera` tokenization or `pgroonga`. Switching is just a reindex — cheap, because the index is fully
rebuildable from files.

---

## ADR-2 — Markdown & wikilink grammar

**Renderer — `comrak`.** `comrak` (Rust cmark-gfm) provides, as built-in extension options: GFM (tables, strikethrough, task lists, footnotes, autolinks), **native wikilinks** (`[[Target|Display]]`),
and **native GitHub alerts** (`> [!NOTE]` callouts). That keeps custom grammar work to two extractors — `#tags` and `![[embed]]` — that the renderer does not provide natively. Render speed is
acceptable for a personal wiki, and rendered HTML can be cached by content hash later if needed. No MDX/JSX (rejected — JS runtime, breaks portability; `miku:decision:no-mdx`).

**Editor vs renderer — CM6 is a separate axis.** CodeMirror 6 is a _client-side editor_; it does **not** render the read view or feed the index — the server still parses Markdown (comrak) for both.
What CM6 buys is the _editing_ experience: in-editor syntax highlighting and a natural home for `[[ ]]` autocomplete and live preview. So: **comrak for server-side render/index now; CM6 for the editor
(roadmap)** — orthogonal, not either/or.

**Tag grammar.** `#tag` where tag = Unicode letters/digits plus `_ - /` (so hierarchical `#area/health` works). Recognized only in text (not code, URL fragments, or `# ATX headings`); `#` immediately
followed by a tag char, no space. Stored whole (`area/health`); ancestor grouping (`area`) is a **query-time prefix match**, not index-time expansion.

**Callouts — conflict resolved:** Obsidian/GitHub `> [!type]` wins (now _native_ via comrak alerts); `:::` directive syntax dropped; `architecture.md` updated. **Transclusion** `![[Page]]` /
`![[image.png]]`: custom, server-side, with a recursion/cycle depth limit (roadmap render; grammar fixed now).

**Latest decision wins.** If a later ADR supersedes an earlier one, the later ADR is the canonical decision. See `miku_docs/adr/` for verified, timestamped decisions.

---

## ADR-3 — Write conflicts & auth

**Write conflicts.** Saves are atomic rename = last-write-wins, so two tabs (or a browser edit racing a `git pull`) can silently clobber. **Decision: optimistic concurrency.** The edit view embeds the
file's **content hash** as a hidden field; `POST` recomputes the hash before renaming. If it changed since load → **409 Conflict** with a "file changed underneath you" prompt (show both versions).
Cheap (one read+hash before rename), guards the _file_, and never involves the indexer — consistent with single-writer. Hash over mtime (mtime is coarse and lies across `git`/`rsync`).

**On CRDTs (`loro-dev/loro`) — considered, deferred.** Loro is excellent rich-text CRDT tech, but it solves _concurrent multi-writer / offline merge_ — a problem Miku deliberately doesn't have
(single-user, single-writer). The deeper blocker: it **conflicts with files-are-truth**. A CRDT's authoritative state is its operation log, which cannot be reconstructed from a plain `.md` snapshot,
so adopting it forces a fork — either the oplog becomes canonical (breaking "just Markdown files you own") or we keep two sources of truth and must pick a winner. **Decision: keep the
content-hash 409.** Loro becomes the right choice _only if_ we commit to real-time collaborative editing, at which point it reshapes the core invariant and earns its own ADR. Not a single-user need.

**Auth — no user system.** Miku stays **single-user and login-less**; network protection is the _deployment's_ job. Two modes cover every persona:

- **`MIKU_READONLY`** (roadmap flag) — serves view-only, no edit/save routes. For publishing; no auth needed because nothing writes.
- **Writable network deploy** — put Miku behind an **authenticating reverse proxy** (oauth2-proxy / basic auth / Tailscale). Documented, not built.

Building accounts/RBAC is explicitly rejected — it reinvents Notion and breaks "keep it simple." Local-first personas bind to localhost and need nothing.

---

## ADR-4 — Rename/delete & assets

**Status: superseded for v0.0.2.** Miku does not expose rename, move, delete, Trash, restore, or purge operations. The file tree is read-only; users manage paths and file removal with their
filesystem, editor, scripts, or git. The watcher reconciles those external changes into the disposable index.

**Assets.** Live in `miku_docs/assets/`. Upload (roadmap): `POST /assets` writes atomically, keeping the original name but **deduping by content hash** (`name-<short-hash>.ext`) to avoid collisions.
`![[image.png]]` resolves by basename in `assets/`; served with caching headers.

**Orphan assets — never auto-deleted.** Auto-GC of user files violates files-are-truth (an asset may be referenced from outside Miku, or kept deliberately). Instead, a **report**: assets on disk minus
`tb_links(kind = 'asset' AND is_embed)` = unreferenced. Manual cleanup only. No new table — asset targets already live in `tb_links`.

---

## ADR-5 — Navigation explorer (folder/file tree)

**Decision.** A Quartz-style collapsible **explorer** in the sidebar, rendered **server-side** from `tb_pages.path` using native `<details>`/`<summary>` — **zero client JS, no schema change**. Folders
are derived **path-prefixes**, not rows; the tree is a pure function of the existing index, so the read path stays **Postgres-only** (consistent with the single-writer, read-only-handler model).

**Why no JS.** This fits the decided frontend stance (`product.md` → _Rendering: "no JS bundler / server-first" ≠ "zero JS"_): Mermaid is the **one** deliberate client-JS exception; everything else
stays server-rendered. `<details>` gives collapse/expand for free, works with JS disabled, and needs no bundler.

**Why physical folders, not tags.** A tag-hierarchy tree (`#area/sub`) is more "on-brand" per `architecture.md` ("the graph carries the meaning"), but the explorer's job is **spatial orientation** —
"where does this file live" — which folders answer directly. Folders remain the "loose filing cabinet" (`architecture.md`); the explorer just makes that cabinet navigable. A tag-tree is a _different_
view and can ship later at `/tags` reusing the same `<details>` renderer over `tb_tags`.

**Scale.** Full server-render for typical vaults (hundreds–low thousands). Past a threshold, render top levels collapsed and **lazy-load** a folder's children on expand from a small `/tree?prefix=…`
endpoint — never emit 100k nodes eagerly (same "never load the full set at once" rule as backlinks). The active page's ancestor folders get `open`; siblings stay collapsed.

**Ordering.** Folders-before-files, alphabetical (free from a `BTreeMap` walk, `title.is_none()` first). Frontmatter `order`/`nav` hints are **deferred** — not needed for v0.

**Rejected.** (1) A client-side JS tree widget — reintroduces the bundler/Electron tax ADR-2 and `product.md` exist to avoid. (2) A `tb_folders` table — folders are derivable from `path`;
materializing them duplicates truth and invites drift. Mirrored in asobi `miku:decision:nav-explorer`.

---

## ADR-6 — Filesystem watcher at scale

> **Status: Verified** → canonical in `miku_docs/adr/0006-watcher-at-scale.md`. Lifts the `miku_docs/dataflow.md` §8 resolution into a decision so the RocksDB detour is not re-litigated.

**Decision.** Keep v1's `notify` watcher as the sole index trigger; scale it by watching **directories, not files**. Watch budget = directory count, not file count. Three levers in order: (1)
recursive `notify` (one watch per directory, default); (2) document raising `fs.inotify.max_user_watches` in setup; (3) a `PollWatcher` fallback (zero inotify watches, periodic mtime scan) past an
extreme directory-count threshold. The startup mtime+hash reconcile sweeps any events missed across the new-subdir registration race or process downtime.

**Why.** The 100k-file watch limit was **misdiagnosed**: inotify watches are **per-directory**, and `notify`'s recursive mode adds one watch per subdirectory, so a wiki with shallow foldering never
approaches the limit (100k files in ~200 folders ≈ 200 watches; default cap 65k–524k; macOS FSEvents has no per-file limit). The watcher's only irreplaceable job is **live pickup of external edits** —
exactly the files-are-truth payoff.

**Rejected.** RocksDB as a durable work-queue / primary store — solves a problem Miku doesn't have, adds a second store, and risks the core invariant. See `miku_docs/dataflow.md` §8 (folder-scoped
watching). Canonical decision: `miku_docs/adr/0006-watcher-at-scale.md`.

---

## ADR-7 — Frontend rendering & client-JS budget

> **Status: Verified** → canonical in `miku_docs/adr/0007-frontend-rendering.md`. **Supersedes** `product.md`'s server-side `syntect` highlighting choice (latest decision wins). Source plan:
> `miku_docs/frontend_design.md` (agy), reconciled.

**Decision (summary).** Server-rendered, **no JS bundler**, with a client-JS budget of vendored, locally-served libraries:

- **Alpine.js** — interactive widgets only (`Ctrl-K` palette, `[[ ]]` autocomplete, modals); calls the JSON API. Not a general SPA layer.
- **Mermaid.js** — lazy, injected only when `tb_pages.has_mermaid`; theme from config, not hardcoded.
- **Code highlighting → client-side Prism.js for the MVP** — a conscious reversal of the server-side `syntect` plan, accepted for MVP speed. Cost: FOUC
  - no highlighting for JS-disabled / published readers. **syntect deferred** as a post-MVP swap (highlight-at-save + cache).
- Native server features (Themes/Homepage/Callouts), first-class frontmatter — unchanged.

See the ADR for full rationale and the deferred-syntect note.

---

## ADR-8 — Theme switching

> **Status: Verified** → canonical in `miku_docs/adr/0008-theme-switching.md`. **Extends ADR-7** (Themes + Alpine budget); ADR-7 is immutable, so the switcher is a new decision.

**Decision.** A theme switcher on **two orthogonal axes**, both persisted to `localStorage` (per-browser display prefs, not content):

- **Palette:** `default` | `catppuccin` → `data-palette` on `<html>`.
- **Mode:** `system` | `light` | `dark` → resolved to a `data-mode` of `light`/`dark` on `<html>`. `system` reads `prefers-color-scheme` and a `matchMedia` listener live-updates on OS flip.

CSS is **custom-variable sets** selected by the attribute pair — four base combinations: `default+light`, `default+dark`, `catppuccin+light` (Latte), `catppuccin+dark` (Mocha).
`[data-palette="catppuccin"][data-mode="dark"]` overrides the `--vars`; everything (callouts, links, code) reads the variables.

**Mechanics.** (1) A tiny **inline pre-paint script** in `<head>` sets `data-palette`/`data-mode` from `localStorage` before first paint — avoids the theme flash (FOUC). (2) **Alpine** drives the
dropdown + persistence + the `matchMedia` listener. (3) The server `theme` config key (ADR-7) supplies the **default** when no saved pref. (4) **Prism** code themes are swapped to match the resolved
palette+mode (Catppuccin ships official Prism themes).

**Why.** Palette and mode are genuinely independent, and the Catppuccin family maps exactly onto the mode axis (Latte = light, Mocha = dark), so it inherits System/Light/Dark for free — "Catppuccin +
System" = Latte by day, Mocha by night with zero extra UI. Fits the decided no-bundler + Alpine stance.

**Trade-offs / Rejected.** Four CSS variable sets to maintain (and matching Prism themes). One new inline pre-paint `<script>` — justified by FOUC, kept minimal. `localStorage` is per-browser and
**not synced** — acceptable for a display preference. Rejected: a flat single-list of themes (loses the palette×mode independence the user chose); server-side per-user theme storage (no user system —
ADR-3). Candidate verified file: `miku_docs/adr/0008-theme-switching.md`.
