---
id: ADR-0007
title: ADR-0007 — Frontend rendering & client-JS budget
slug: frontend-rendering
status: Accepted
date-proposed: 2026-06-26
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:frontend-rendering
supersedes: []
superseded-by:
supersedes-doc: ["product.md: server-side syntect highlighting"]
relates-to: [ADR-0002, ADR-0005]
impacts: [src/templates, src/handlers/api, static/js/vendor, static/css]
config-keys: [theme, home]
tags: [frontend, alpine, prism, mermaid, no-bundler, mvp]
---

# ADR-0007 — Frontend rendering & client-JS budget

## Decision

Server-rendered HTML, **no JS bundler**, with a deliberate client-JS budget of vendored, locally-served (`/static/js/vendor/`, offline-capable) scripts:

- **Alpine.js — interactive widgets only.** The `Ctrl-K` command palette, `[[ ]]` / page autocomplete, modals and dropdowns. Alpine holds local DOM state and calls the JSON API (`/api/v1/quickswitch`,
  search). It is **not** a general SPA layer — server rendering still owns pages, the read view, the index, and the nav explorer (ADR-5).
- **Mermaid.js — lazy, index-flagged.** Injected **only** when the indexer set `tb_pages.has_mermaid = true`; every other page stays Mermaid-free. Diagram **theme comes from the Themes config**, not
  hardcoded.
- **Code highlighting — client-side Prism.js, for the MVP.** Prism runs in the browser (`Prism.highlightAll()`, triggered via Alpine `x-init` so it re-runs after dynamic swaps). Themes are Prism theme
  CSS in the themes folder.
- **Native server features, no plugin system:** Themes (swappable CSS + `theme` key), Homepage (`home` key), Callouts (`[!type]` post-process).
- **Frontmatter first-class:** indexer parses the `---` YAML; interpreted keys `tags` / `aliases` / `title`, everything else opaque `frontmatter JSONB`.

## Why

Obsidian's power-via-plugins is also its Electron-weight tax; Miku's wedge is server-owned indexing with the browser as a thin client. "No _bundler_" keeps that promise while allowing a few vendored
`<script>` tags. The palette and autocomplete are genuinely interactive (keyboard, debounced fetch, dynamic lists) — server rendering cannot do them, so Alpine earns its place. Prism is chosen for the
MVP because it is near-zero backend work (two tags + a call), letting the MVP ship the editor/index/search first.

## Trade-offs / Rejected

- **Prism reverses `product.md`'s server-side `syntect` decision** — accepted consciously for MVP speed (latest decision wins). **Cost:** code blocks FOUC until JS runs, and JS-disabled /
  `MIKU_READONLY`-published readers get no highlighting. **`syntect` is deferred, not rejected:** the post-MVP path is highlight-at-save + cache rendered HTML by content hash (ADR-2), serving JS-free
  classed HTML. Revisit when the publish / no-JS persona is in scope.
- Rejected: a general JS plugin system (the Electron tax); MDX/JSX (ADR-2).
- Deferred: Dataview-style queries, templates, daily-notes.
