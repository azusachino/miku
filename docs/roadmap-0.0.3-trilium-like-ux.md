# Miku Note v0.0.3 — Trilium-like workspace UX

Status: proposed
Target: the next release after v0.0.2

## Product decision

Miku Note should borrow Trilium's **spatial model**, not its storage model:

- a persistent note tree gives the user a place to orient themselves;
- one focused reader/editor keeps the current note central;
- history, quick-open, breadcrumbs, and a small context rail make movement cheap;
- keyboard navigation makes the tree useful without requiring a mouse.

Miku keeps its differentiator: Markdown files under `miku_docs/` are the source
of truth, and the index is disposable. There is no proprietary note database,
clone graph, or WYSIWYG block model in this milestone.

This is consistent with TriliumNext's current emphasis on hierarchical notes,
fast navigation, full-text search, note hoisting, bookmarks, and workspace
navigation. Trilium's wider feature set (rich note types, scripting, sync,
encryption, maps, and collections) is deliberately not the 0.0.3 target.

## Why v0.0.2 still feels unlike Trilium

The current release has the right capabilities but presents them as separate
surfaces:

| Current v0.0.2 | Desired 0.0.3 behavior |
| --- | --- |
| Explorer is a collapsible sidebar utility | Explorer is the persistent home of navigation |
| A page is opened from a link or palette | Opening a page selects it in the tree and reader |
| Breadcrumbs describe the path after navigation | Breadcrumbs and tree jointly explain where the user is |
| Reader rail exposes backlinks and metadata | Context rail is predictable and collapsible |
| Search is powerful but modal/page-oriented | Quick-open is the universal jump action; search is its deeper mode |
| Zen mode hides the shell | Focus mode is an explicit, reversible reading state |
| Filesystem folders are the only hierarchy | Folders are the navigation hierarchy; links/tags remain the knowledge graph |

The missing feeling is continuity: the user should always know **where they
are, what they recently opened, and how to get back**.

## UX contract

### The desktop shell

The default reader layout is a three-region workspace:

```text
┌──────────────────────────────────────────────────────────────────────┐
│ brand · history · quick open · search · theme                         │
├───────────────────┬──────────────────────────────┬───────────────────┤
│ explorer           │ focused note                 │ context           │
│                   │ breadcrumbs                  │ backlinks         │
│ folders + pages    │ title + reader/editor        │ mentions/tags     │
│ recent/bookmarks   │                              │ page info         │
└───────────────────┴──────────────────────────────┴───────────────────┘
```

Rules:

1. The explorer is visible by default on desktop and remembers its width and
   open folders. The active page and its ancestors are always visible.
2. A page link changes the selected page in the explorer and the reader
   fragment together. It must not create a second application shell.
3. Browser back/forward and in-app previous/next history are both supported.
4. The context rail is useful but never required to read the note. It can be
   collapsed independently from the explorer.
5. On narrow screens, the explorer and context rail become drawers; the note
   remains the primary surface.
6. Readonly remains the default. Edit is an explicit mode, and save/conflict
   feedback must stay visible in the note header.

### Tree semantics

The 0.0.3 tree is a projection of the filesystem paths already in the index:

- folders are path prefixes, not new database entities;
- pages are Markdown files, displayed by frontmatter title when present;
- folders sort before pages, with stable case-insensitive ordering;
- the active path opens its ancestors;
- lazy children loading is used for large folders;
- a page can be linked from many places, but it has one physical tree location;
- tags and backlinks are secondary graph views, never mixed into the physical
  explorer.

This preserves the v0.0.2 read-only file ownership boundary. Move, rename,
delete, and cloning are not tree actions yet; users still use the filesystem,
editor, scripts, or git.

### Focus and hoist

Add a **focus mode** that temporarily narrows the explorer to the current
folder subtree and its ancestors. This is the Miku equivalent of Trilium's
note hoisting, adapted to physical folders:

- `Focus folder` is available from a folder row and the command palette;
- the focused root is shown as a dismissible scope chip;
- quick-open and tree traversal stay inside the scope;
- the reader can still follow a wikilink outside the scope, which displays a
  small “outside focus” breadcrumb and offers `Clear focus`;
- focus is URL-addressable (`?focus=<path>`) so refresh and bookmarks behave
  predictably;
- focus is a view preference, never a mutation of files or index data.

Do not call this a general graph hoist. A folder scope is explainable from the
filesystem and can be implemented without inventing a hierarchy database.

## 0.0.3 scope

### P0 — navigation continuity

1. **Persistent explorer workspace**
   - Keep the current lazy `/api/v1/nav/children` model.
   - Add explicit selected/active styling, folder disclosure state, and a
     compact recent/bookmark section.
   - Restore the last selected page only when the URL has no explicit page.
   - Preserve `miku:ui:v1` compatibility; use a versioned `miku:ui:v2` state
     migration if the shape changes.

2. **Navigation history**
   - Track page visits in the browser shell, capped to a small recent list.
   - Add back/forward actions beside quick-open and keyboard shortcuts.
   - Use `history.pushState` for intentional page selection and `replaceState`
     for startup normalization; do not duplicate browser history for fragment
     refreshes.
   - Keep the server-rendered `/p/{path}` URL canonical and reload-safe.

3. **Universal quick-open**
   - `Ctrl/Cmd-K` opens a focused switcher with Pages, Recent, Bookmarks, and
     Commands.
   - Arrow keys move, Enter opens, Escape closes, and the current selection is
     announced to assistive technology.
   - Page results use the existing quick-switch endpoint; add an optional
     folder/scope parameter instead of a second search index.

4. **Focus mode / folder hoist**
   - Implement the scoped tree and scoped quick-open behavior described above.
   - Make the scope obvious and one-click reversible.

### P1 — context and capture

5. **Consistent note context rail**
   - Standardize the rail sections: page info, backlinks, unlinked mentions,
     tags/properties, and on-page headings.
   - Keep sections collapsed state local to the shell and do not hide backlinks
     behind a separate page.
   - Show empty states that explain what the user can do next, rather than
     removing the section unpredictably.

6. **Quick capture**
   - Add `New note` to the top bar and command palette.
   - Default creation to the focused folder/current folder when one exists;
     otherwise use the vault root.
   - The first screen asks only for a title, creates a safe Markdown path, and
     opens the normal editor. No new note database or draft store is needed.
   - Keep this behind readonly mode: `MIKU_READONLY` must hide or reject it.

7. **Bookmarks and recent notes**
   - Store only paths and timestamps in local browser state.
   - Bookmark toggling is a view preference, not frontmatter and not an index
     write.
   - Add stale-entry handling when a path disappears: retain the label briefly,
     then offer remove/refresh instead of producing a broken silent link.

### P2 — polish and verification

8. **Keyboard and responsive polish**
   - Tree: focus tree, move up/down, expand/collapse, open selected note.
   - Reader: focus explorer, focus note, toggle context, toggle focus mode.
   - Narrow screens: drawer transitions, safe focus return, no horizontal page
     overflow.
   - Keep the existing server-rendered/HTMX/Alpine architecture; do not add a
     SPA router or a general client state framework.

9. **Reader-shell acceptance suite**
   - Extend `scripts/reader_shell_contract.py` for active tree selection,
     navigation history, focus scope, and readonly gating.
   - Extend `scripts/ux_smoke.py` for the critical browser flows.
   - Verify direct URL reload, browser back/forward, keyboard quick-open,
     missing/deleted pages, and a large lazy-loaded folder.

## How to implement it

### Route and payload design

Prefer small additions to the existing routes:

| Need | Proposed boundary |
| --- | --- |
| Reader selection | existing `/p/{path}` + reader fragment swap |
| Tree children | existing `/api/v1/nav/children?dir=...` |
| Quick-open | existing `/api/v1/quickswitch?q=...&scope=...` |
| New note | `GET /p/{path}/edit` with a safe create path, then existing save flow |
| Focus scope | URL query `?focus=<path>` and server-filtered tree payload |
| Reader context | existing reader page payload; add only data needed for stable sections |

The selected page should be represented by the server-rendered `data-page-path`
attribute and a single shell event. Avoid a second client-side page model.

### Browser state

Use one versioned state object, extending the existing storage contract:

```json
{
  "version": 2,
  "theme": "dark",
  "sidebarWidth": 288,
  "railWidth": 260,
  "folders": [],
  "recent": [],
  "bookmarks": [],
  "focus": null
}
```

Keep only bounded, disposable UI state in local storage. The URL is the source
of truth for the current page and focus scope; Markdown and the index remain the
source of truth for content and relationships.

### Creation safety

Quick capture must reuse the existing path validation and atomic-save boundary.
The implementation should:

1. normalize a title to a safe relative Markdown path;
2. reject traversal, empty names, and collisions;
3. create through the same temp-file + flush + rename path as normal saves;
4. let the watcher index the file asynchronously;
5. redirect to the readonly page and show an indexing-pending state if needed.

If this cannot be done without weakening readonly or conflict guarantees, defer
quick capture rather than adding a special write path.

## Release acceptance criteria

0.0.3 is ready when a new user can complete this sequence without losing their
place:

1. Open Miku and see the current note selected in a persistent tree.
2. Expand a folder and open a child note; the tree, breadcrumb, URL, and reader
   agree on the selection.
3. Press `Cmd/Ctrl-K`, type a partial title, open a result, then use Back and
   Forward to return to the previous notes.
4. Focus the current folder, quick-open a note within it, follow one link out,
   and clear focus to return to the full vault.
5. Collapse the context rail, reload, and continue reading with the note still
   central and the active tree path visible.
6. In readonly mode, no create/edit/save control is available and direct write
   requests are rejected.
7. A missing or externally renamed note does not leave the shell in a blank or
   contradictory state; the user receives a recoverable message.

Quality gates remain `make check`, `make web-check`, and the targeted reader
shell/browser smoke checks. Visual verification is still performed by the user
in a real browser.

## Explicit non-goals

- arbitrary multi-parent tree placement or Trilium-style cloning;
- moving, renaming, deleting, trash, or restore actions in the explorer;
- WYSIWYG parity, collections, canvases, maps, scripting, or plugins;
- CRDT collaboration, accounts, sync, encryption, or mobile offline mode;
- a database-backed bookmark or hierarchy model;
- a SPA rewrite or client-side rendering of Markdown;
- graph view as a prerequisite for the navigation experience.

## Design references

- [TriliumNext README](https://github.com/TriliumNext/Trilium) — current feature
  set and hierarchical-note positioning.
- [TriliumNext user guide](https://docs.triliumnotes.org/user-guide/) — tree,
  navigation, search, bookmarks, hoisting, and workspace concepts.
- [Miku ADR-0005](adr/0005-nav-explorer.md) — filesystem-derived explorer
  decision.
- [Miku architecture](architecture.md) — files-as-truth and disposable index
  contract.
