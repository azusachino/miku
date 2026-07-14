# Miku — Product & Positioning

> Design rework written _before_ implementation. Personas drive scope; scope drives the build. See `architecture.md` for the technical contract.

## Personas — five lives, one wiki

### 1. Priya — Staff Engineer (200-person startup)

Knowledge scattered across Confluence, Notion, Slack DMs, and a `~/notes` folder. Nothing links. During an incident she follows `[[postgres-failover]]` backlinks to the runbook, the postmortem, and
the capacity note, fixes the doc, and commits all of `miku_docs/` to a private git repo — versioned, diffable notes.

- **Leans on:** backlinks, FTS, plain `.md` on disk (git/rg/sed still work), no lock-in.
- **Before:** real knowledge, unsearchable across five silos; lost on every tool migration.

### 2. Tanaka-san — Records & Compliance Officer (municipal office)

Cloud SaaS is a data-sovereignty and procurement problem. Records must live on managed, auditable storage and outlive any vendor. Runs Miku on-prem; the files are the record, Postgres is explicitly
disposable. Audit is `git log` over the notes directory.

- **Leans on:** filesystem-as-truth, self-hosted, no proprietary format, rebuildable index.
- **Before:** forbidden cloud tools, or brittle Word docs in shared drives — no linking, no search.

### 3. Mei — second-year university student

Notes across five courses; the connections exams test get lost. Tags `#thermodynamics`, links `[[entropy]]`. Two weeks before finals she revises by _following the graph_ through backlinks instead of
re-reading everything.

- **Leans on:** tags, backlinks-as-revision-tool, dead-simple textarea capture.
- **Before:** linear Google Docs; no way to see how concepts connected.

### 4. Lucas — freelance investigative journalist

Sensitive source notes; cloud sync is a liability. Works offline. Builds a web of `[[person]]` / `[[shell-company]]` pages; backlinks reveal who connects to whom. FTS finds the half-remembered quote.
Nothing leaves the machine.

- **Leans on:** local-only, offline, zero cloud dependency, FTS over a large corpus.
- **Before:** wouldn't trust Notion with sources; or an un-cross-referenceable folder.

### 5. Aiko — novelist / worldbuilder

A series with hundreds of characters and timelines that must stay consistent. Every character is a page; changing `[[Kaelen]]`'s backstory surfaces every scene that references him via backlinks.
Manuscript and wiki are the same plain files.

- **Leans on:** dense linking, backlinks-as-consistency-check, tags, large-graph performance.
- **Before:** spreadsheets + a wiki SaaS she'd pay forever just to keep reading her own world.

## What the personas change about the design (the rework)

| Tension from the stories                                                      | Design response                                                                                               |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Mei & Lucas need frictionless capture                                         | Keep v0 textarea, but make **quick-create / quick-open (fuzzy by title)** a first-class flow.                 |
| Aiko & Lucas have large, dense graphs                                         | Non-negotiable: never recompute the graph on a keystroke; paginate/virtualize backlinks.                      |
| Tanaka-san needs the rebuildable-index promise _provable_                     | "Drop DB → rebuild from files" is a real command someone can run — that demo _is_ the compliance sale.        |
| Everyone also edits files outside the editor (git pull, sed, another machine) | The `notify` watcher as the sole index trigger makes external edits reindex identically. Single-writer holds. |
| Priya wants git-native notes                                                  | Falls out of files-on-disk. Say it out loud; build nothing.                                                   |

**Deliberately deferred** (named to keep scope honest): no mobile app (browser is the surface), no real-time collab (single-writer, single-user), no built-in encryption (filesystem/disk's job), no
cloud sync (git's job). Every persona is satisfied _without_ these — signal the v0 scope is right.

## The commercial — "Notes you'll still own in 2040"

**Problem:** Knowledge tools are rented, siloed, and trapped in formats you can't read without the vendor. When the subscription lapses, your second brain is held hostage — and your real tools (git,
grep, your editor, your backup) can't touch the data.

**Why Miku — your wiki is just Markdown files; Miku is the lens, not the cage:**

- **You own the files.** Plain `.md` in one folder. Delete Miku tomorrow; your knowledge is untouched.
- **Connections, found for you.** `[[links]]` → backlinks, tags, FTS built in the background. The valuable graph, without hand-maintenance.
- **The database is disposable, on purpose.** Postgres is a cache; nuke it and Miku rebuilds from files. Nothing important lives anywhere but your disk.
- **Self-host or run local.** No account, no telemetry, no cloud.
- **It gets out of your way.** Browser editor over a textarea. No bundler, no app to learn, no migration the day you need it most.

**One line:** _Obsidian's linking and a real search engine — but the files are unarguably yours, and the index is something you can throw away._

## Product name — Miku

The project is named **Miku** (初音ミク) — Hatsune Miku, the iconic Vocaloid voice bank and cultural figure in music/tech. Like the Vocaloid engine itself, Miku lets you compose and shape knowledge
without vendor lock-in: the _content_ (Markdown files) is the source of truth, and Miku is the tool layer that renders, links, and searches — ephemeral and replaceable.

## What we learn from Notion and Obsidian

### From Notion (the polish & onboarding playbook)

- **The empty state is the product.** Notion never shows a blank page — it shows templates and a "/" menu that teaches the tool. Miku's first run should seed a welcome page that _demonstrates_
  `[[links]]` and `#tags`, not an empty textarea.
- **The "/" command palette** turns a blank box into a discoverable surface. A Miku command bar (`Ctrl-K`: quick-open, new page, search) is the single highest-leverage UI affordance — it serves Mei's
  capture and Priya's navigation at once.
- **Bidirectional context is shown, not summoned.** Notion surfaces related content inline. Miku's backlink panel should always be visible, not a click away.
- **What NOT to copy:** the proprietary block model and DB-as-truth. That's exactly the lock-in Miku exists to refuse. Notion's data is the cage; ours is files.

### From Obsidian (the local-first, file-owned playbook — our closest sibling)

- **Files-on-disk is a feature users evangelize**, not a technical detail. Obsidian's whole trust story is "it's just Markdown in a folder." Miku shares this DNA — lean into it as the headline, like
  they do.
- **`[[wikilink]]` autocomplete is the core interaction.** Typing `[[` and fuzzy-picking an existing page (or creating one inline) is what makes linking effortless enough to actually do. This is the
  one interaction Miku must nail.
- **Backlinks first, unlinked mentions second.** Explicit backlinks are a core relationship surface. Unlinked textual mentions are an optional discovery aid, maintained as a background-derived
  relation and never allowed to delay page rendering. They may be stale while indexing is in progress.
- **Local graph view** is the demo that sells it, even if rarely used daily. Defer for v0, but it's the screenshot that makes Aiko's worldbuilding click.
- **Plugins are why it's sticky — and why it's heavy.** Obsidian's power is a plugin ecosystem; its cost is a JS-heavy Electron app. Miku's bet is the opposite: server-side indexing, no bundler, the
  browser as a thin client. _Don't_ chase plugins in v0 — the server-owned index is our differentiator.
- **What NOT to copy:** Electron weight and the sync paywall. Miku is self-hosted and uses git for sync — no vault-sync subscription.

### The synthesis

Notion teaches **discoverability** (command palette, never-blank states, inline context). Obsidian teaches **ownership** (files as truth, frictionless `[[linking]]`, backlinks as the daily payoff).
Miku's wedge is taking Obsidian's ownership story and moving the _indexing_ server-side — so linking, backlinks, tags, and search are computed for you in the background instead of by a pile of client
plugins, while the files stay plainly, provably yours.

## Feature stance vs Obsidian (decided)

**Adopted as native server features (no plugin system):**

- **Themes** — swappable CSS in `static/` + a `theme` config key. Pure CSS.
- **Homepage** — a `home` config key naming the landing note.
- **Admonitions / Callouts** — post-process blockquotes starting with `[!type]` into styled callouts (render + CSS only).

**Frontmatter (first-class):** indexer parses the `---` YAML block. Interpreted keys are `tags` (merged with inline `#tags`), `aliases` (resolve `[[alias]]` → page; affects backlinks), `title`
(display name). Everything else is indexed as opaque `key → value` properties (`frontmatter JSONB` on `pages`) — searchable and the groundwork for a future Dataview-lite query, with no hardcoded
schema.

### Rendering: "no JS bundler / server-first" ≠ "zero JS"

> **Latest decision: ADR-7** (`docs/adr/0007-frontend-rendering.md`) — see there. Highlights uses **client-side Prism.js** for the MVP; **`syntect` is deferred** as a post-MVP swap. The server-side
> stance below is the _target_, not the MVP.

- **Code highlighting → server-side `syntect`.** Highlight at render time into classed spans, colored by CSS; themeable via the Themes mechanism. No client JS, works with JS disabled.
- **Prism.js → not as an engine.** Reuse only its CSS _class-name convention_ so existing Prism themes drop into the themes folder. No Prism JS dependency.
- **Mermaid → the one deliberate client-JS exception.** No pure-Rust renderer; server-rendering needs node/headless-browser (worse than a script tag). Vendor `mermaid.js`, no bundler, and **inject it
  only on pages the index says contain a mermaid block** — every other page stays JS-free. This reframes the v0 invariant: no _bundler_, not dogmatic zero-JS.

**Deferred:** Dataview-style queries (our Postgres index is the right home for it later), templates (lightweight `templates/` seed files), daily-notes/calendar (a date-named-note convention).
**Rejected:** a general JS plugin system — that is the Electron-weight tax Miku exists to avoid.
