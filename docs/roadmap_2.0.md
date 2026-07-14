# Miku 2.0 — UX Roadmap

> Status: **draft** for review. Supersedes the v0/v1 "make it work" phase. The goal of 2.0 is not new storage features — the file-owned index is solid — but a **UX the author actually enjoys living in
> every day.**

## 1. Design thesis

Three principles, distilled from VS Code, Zed, Obsidian, and SilverBullet (the closest peer — a browser-based Markdown wiki). Every 2.0 decision is measured against these:

1. **Content is the hero; the chrome fades.** Zed's "less is more" — the UI should recede so the note is what you see. Decoration that competes with the text is a bug, not a feature. (Zed: _"the UI
   fades into the background"_; VS Code Zen mode.)
2. **One keyboard surface drives everything.** A real command palette + quick-switcher is the backbone all four apps share. Mouse-optional, never mouse-required. (VS Code: _"all functionality… the
   same interactive window"_; SilverBullet `Cmd-k` / `Cmd-/`.)
3. **Live preview, not modes.** Reading and editing should be the _same_ surface — Markdown syntax melts away near the text and reappears under the cursor. No hard navigation between `/p/X` and
   `/p/X/edit`. (Obsidian Live Preview; SilverBullet's contextual reveal.)

## 2. What each app teaches us

| App              | What we borrow                                                                                                                                                        | What we skip                                                                 |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| **SilverBullet** | Live preview in-place editing; `[[ ]]` autocomplete with create-on-click for missing pages; `/` slash commands; `Cmd-k` page switcher; linked mentions at page bottom | Lua end-user scripting / live queries (out of scope for a personal wiki)     |
| **Obsidian**     | Reading ⇄ editing toggle (`Cmd-E`); **properties** panel (frontmatter as structured fields); quick switcher; linked **and** unlinked mentions                         | Plugin ecosystem; tabs/panes sprawl; graph as a feature centerpiece          |
| **Zed**          | Minimalism as a _constraint_; UI recedes; speed is non-negotiable; command palette as the control center; progressive disclosure ("Tesla door handle")                | Native/GPU rendering (we're a browser app — borrow the _feel_, not the tech) |
| **VS Code**      | Command palette + quick-open as one type-to-filter surface; Zen/focus mode; sensible defaults with optional customization; everything keyboard-reachable              | Activity bar / heavy multi-panel workbench                                   |

## 3. Current-state gaps (why the UX feels off today)

- **The "Cmd K" chip is a lie.** It's an `<a href="/search">` — a full page navigation, not a palette. There is no quick-switcher and no command palette.
- **Editing is a hard mode-switch.** `Edit` navigates to a separate `/edit` route with a split CodeMirror + server-rendered preview pane. Heavy, jarring, and nothing like the live-preview peers.
- **Decorative noise competes with content.** Animated background orbs, the equalizer logo, gradient glows, and four accent themes pull attention _away_ from the note — the opposite of the thesis.
- **No editor intelligence.** Plain CodeMirror markdown: no `[[` autocomplete, no `/` slash commands, no create-missing-page flow. (An autocomplete endpoint was sketched in `frontend_design.md` but
  never built.)
- **Frontmatter is read-only display.** Properties show as a bullet list; you can't edit them without hand-typing YAML in the source.
- **Backlinks only.** We show "linked from" but no _unlinked_ mentions.

## 4. The roadmap

Ordered smallest → biggest leverage-to-effort. Each epic is independently shippable; together they are 2.0.

### Epic 1 — The command surface · _S–M_

The single highest-leverage win. Build a real overlay (Alpine, no bundler):

- **Quick-switcher** (`Cmd-K`): fuzzy-jump to any page by title/path.
- **Command palette** (`Cmd-/` or `Cmd-Shift-P`): new page, toggle theme, toggle reading/edit, go to tags, etc. — one surface, type-to-filter.
- Replace the fake search chip; keep `/search` as the full-text results page.
- Needs a lightweight `/api/quickswitch?q=` (titles+paths) endpoint — read-only, already trivially indexable from `tb_pages`.

### Epic 2 — Live preview editing (kill the mode switch) · _L_

The structural heart of 2.0. Collapse view + edit into one surface:

- CodeMirror with live-preview decorations: headings/bold/links render styled; the active line reveals raw Markdown (Obsidian/SilverBullet model).
- `Cmd-E` toggles a clean **reading view** (no syntax at all) for focused review.
- Drop the split-pane `/edit` route and the server `/preview` round-trip for the common case (keep `/preview` as a fallback / for non-JS).
- **Invariant preserved:** still saves atomic `.md`; the watcher remains the sole indexer. This is a _frontend_ change.
- ⚠️ Biggest lift — see Open Decisions for the scope fork.

### Epic 3 — Editor intelligence · _M_

On top of Epic 2's editor:

- `[[` → page autocomplete; selecting a non-existent page creates it on click (SilverBullet's orange-link pattern).
- `/` → slash menu for snippets and formatting (heading, table, callout, code block, date).
- Reuse the Epic 1 quick-switch index for `[[` suggestions.

### Epic 4 — Visual calm / minimal chrome · _M_

Make the chrome recede (the core "I don't like the UX" complaint):

- Cut/region the animated orbs, gradient glows, equalizer logo to a single tasteful mark. One refined accent, not four competing themes (keep theme switching, trim the decoration).
- Establish a real **type + spacing scale** and a reading measure (~65–75ch).
- Add a **Zen/focus mode** (hide sidebar + chrome, center the note) — VS Code Zen, toggled from the palette.

### Epic 5 — Knowledge surfaces · _M_

- **Properties panel:** frontmatter as structured, editable fields (text/date/list/checkbox), Obsidian-style — writes back to the `.md`.
- **Linked + unlinked mentions:** add unlinked text matches beneath the existing backlinks, each one-click promotable to a real `[[link]]`.
- (Stretch) local backlink graph / context snippets.

### Epic 6 — Native-feel polish · _S–M_

Honor Zed's speed principle in a browser:

- Hover-prefetch links; optimistic navigation; zero layout shift; skeletons.
- Audit first-paint weight (we ship Tailwind-in-browser, Prism, Mermaid, Alpine, htmx, CodeMirror from CDNs — measure and trim/self-host).

## 5. Non-goals for 2.0

- Multi-user / real-time collaboration (single-writer model stays).
- Plugin/scripting ecosystem (SilverBullet's Lua, Obsidian plugins).
- Mobile-first redesign (responsive polish only).
- Replacing Postgres or the file-owned invariant — **untouched.**

## 6. Open decisions (need a call before Epic 2)

1. **Editor depth.** Full live-preview decorations (SilverBullet-grade, the premium feel, large CodeMirror investment) **vs.** a lighter "edit-in-place with `Cmd-E` reading toggle" (keeps a
   visible-syntax editor but kills the route switch — ~half the work). _Recommendation: ship the lighter version first (delivers 80% of the felt improvement), then layer decorations._
2. **Bundler.** Editor intelligence (Epics 2–3) leans hard on CodeMirror extensions. Staying zero-bundler via `esm.sh` works but is fragile at this complexity. _Recommendation: keep zero-bundler
   through Epic 2; reassess a minimal build step (esbuild) if Epic 3 friction warrants it._

## 7. Task breakdown

Canonical, dispatchable task state lives in the asobi graph under the `miku:ux-2.0` epic (`asobi search "miku:ux-2.0"`). Mirror for reference:

| Task   | Title                                                          | Status  | Dep    |
| ------ | -------------------------------------------------------------- | ------- | ------ |
| task-1 | Command surface: `Cmd-K` quick-switcher + `Cmd-/` palette      | READY   | —      |
| task-2 | Live-preview editing: one surface, `Cmd-E` reading toggle      | READY\* | —      |
| task-3 | Editor intelligence: `[[ ]]` autocomplete + `/` slash commands | BLOCKED | task-2 |
| task-4 | Visual calm: recede chrome, type/spacing scale, Zen mode       | READY   | —      |
| task-5 | Knowledge surfaces: editable properties + unlinked mentions    | READY   | —      |
| task-6 | Native-feel polish: prefetch, optimistic nav, trim CDN weight  | READY   | —      |

\* task-2 is gated on the §6 open decisions (editor depth + bundler) before dispatch. Each task entity carries a `plan:` observation with concrete file paths and line numbers.

---

### Sources

- SilverBullet — [silverbullet.md](https://silverbullet.md/), [v1 manual](https://v1.silverbullet.md/), [GitHub](https://github.com/silverbulletmd/silverbullet)
- Obsidian — [Edit & read views](https://obsidian.md/help/edit-and-read), [Live preview update](https://help.obsidian.md/Live+preview+update)
- Zed — [Between Editors and IDEs](https://zed.dev/blog/between-editors-and-ides), [How we rebuilt settings](https://zed.dev/blog/settings-ui)
- VS Code — [UX Guidelines](https://code.visualstudio.com/api/ux-guidelines/overview), [Command Palette](https://code.visualstudio.com/api/ux-guidelines/command-palette),
  [Custom Layout](https://code.visualstudio.com/docs/configure/custom-layout) </content> </invoke>
