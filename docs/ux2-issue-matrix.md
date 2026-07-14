# UX 2.0 Issue Matrix and Visual Contract

This document is the review baseline for the UX 2.0 polish epic. It records the issues visible in the current server-rendered shell and defines the visual rules that later tasks must preserve.

## Issue matrix

| Priority | Surface             | Current issue                                                                                                                                            | User impact                                                                                        | Evidence / owner                                    |
| -------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| P0       | Reading surface     | The shell, decorative gradients, right rail, and note compete for attention; the main article has no stable reading measure when the rail is present.    | A document does not feel like the primary object, especially on large screens.                     | `base.html`, `page.html`, `miku.css` reading layout |
| P0       | Typography          | Display/body fonts are named but not delivered locally, while mixed Markdown content uses several independent sizes and line-heights.                    | Font fallback changes alignment; headings, lists, tables, and code blocks feel visually unrelated. | `miku.css` font tokens and `.mk-prose` rules        |
| P0       | Navigation          | Palette requests are cancellable, but the page shell still relies on full route changes and repeated SSE/event activity can make navigation feel frozen. | Search/click/switch flows lose focus or appear stuck during background indexing.                   | `miku.js`, `base.html`, indexer readiness behavior  |
| P1       | Editor              | The editor has mode controls but no persistent, compact saved/saving/unsaved/failed status contract.                                                     | Users cannot tell whether the file on disk contains the current draft.                             | `edit.html` save and preview lifecycle              |
| P1       | Sidebar             | Files, search, tags, and trash are all first-class panels with dense controls and inconsistent hierarchy cues.                                           | Spatial navigation is harder than quick switching; the note loses width to chrome.                 | `base.html`, `_nav_macro.html`, sidebar tokens      |
| P1       | Feedback            | Toasts exist for selected tree actions, but save, preview, create, rename, move, delete, and promotion do not share one feedback language.               | Success and failure are inferred from route changes or reloads.                                    | `miku.js`, mutation handlers, toast markup          |
| P1       | Responsive behavior | Wide shell assumptions and fixed controls create pressure at tablet/narrow widths; long titles and paths compete with actions.                           | Buttons wrap or disappear before the content becomes usable.                                       | `miku.css` media rules and toolbar templates        |
| P2       | Accessibility       | Focus styling and keyboard paths are not defined as one system; reduced motion only gates selected animations.                                           | Keyboard and motion-sensitive users get inconsistent feedback.                                     | `miku.css`, `base.html`, `miku.js`                  |
| P2       | UI dependencies     | Tailwind browser CDN and ESM CodeMirror imports are useful during development but make the visual/runtime contract network-dependent.                    | Offline or slow-network use can degrade the editor and styling.                                    | `base.html`, `edit.html`, frontend architecture doc |

## Visual contract

### Hierarchy

- The note is the strongest visual surface. The shell, sidebar, and rail use quieter surfaces and borders than the article.
- One accent color communicates action, focus, selection, and links. Pink is reserved for intentional secondary emphasis and destructive states.
- Decorative gradients, animated equalizers, and background orbs are optional accents, never required to understand content or state.

### Grid and measure

- The reading column targets `68ch`–`76ch`; the article measure must remain stable when the optional rail is visible.
- Page title, lede, metadata, Markdown body, editor, and preview share the same left and right content edges.
- All controls use the same control height and radius family. Icons are optically centered, not merely centered by their line box.
- Long titles, paths, CJK text, tables, and code may wrap or scroll inside their own region; they must not move unrelated controls.

### Type

- Use one body stack, one display stack, and one monospace stack, with explicit system fallbacks that preserve approximate metrics.
- Body text uses a readable `1.65`–`1.75` line-height; headings use a tighter scale with consistent vertical rhythm.
- Metadata and labels may be smaller, but never rely on letter spacing alone to establish hierarchy.
- Font delivery must be explicitly either local/offline or a documented progressive enhancement; no critical layout may depend on a remote font.

### State

- Loading, empty, error, saved, unsaved, and unavailable states have text labels or accessible names; color alone is insufficient.
- Focus-visible rings use the accent token and remain visible against both theme surfaces.
- Reduced motion disables route pop, shimmer, floating decoration, and toast transitions while preserving state changes.

## Review gates

Before implementation moves beyond the baseline:

1. The issue matrix is kept current when a defect is fixed or a trade-off is accepted.
2. Browser checks use deterministic fixture content and a real Miku process.
3. A UI library is adopted only with a recorded size, accessibility, offline, and maintenance justification.
4. Each visual change is checked in dark/light themes and at desktop, tablet, and narrow widths.
