# UX Polish Tasks

Drafted 2026-07-14 from a review of the current Miku UI and interaction patterns in Obsidian, SilverBullet, VS Code, Zed, and Notion.

This is a polish sequence, not a storage or collaboration roadmap. The file-owned Markdown invariant and disposable Postgres index remain unchanged.

## Product principles

1. The note is the primary surface; chrome should explain itself and then recede.
2. Every frequent action has one obvious keyboard path and one discoverable mouse path.
3. Navigation, editing, and feedback should feel continuous rather than route-driven.
4. Destructive actions need confirmation, recovery, and visible completion state.

## Ordered tasks

### UX-01 — Establish a calm reading surface (M)

Reduce the visual competition between the note and the application shell in `src/templates/base.html`, `src/templates/page.html`, and `static/miku.css`.

- Remove or substantially quiet animated orbs, gradient-heavy controls, and decorative equalizer elements.
- Make the reading column a stable 65–75ch measure with predictable heading, paragraph, list, table, and code-block rhythm.
- Keep one accent color for actions and links; reserve the destructive accent for destructive actions and missing links.
- Make the right rail optional and visually secondary, not a second dashboard.
- Verify light/dark themes, reduced motion, narrow windows, and long titles.

Done when a page screenshot reads as a document first and an application second.

### UX-02 — Make navigation history and focus predictable (M)

Improve the shell and page navigation behavior in `base.html`, `miku.js`, and the Axum navigation handlers.

- Preserve scroll position when returning to a page or closing the palette.
- Add explicit Back, Forward, and Recent pages behavior; do not make the user reconstruct history from the browser alone.
- Keep focus in the command palette after search results update.
- Restore focus to the editor after toggling from reading to editing.
- Define one shortcut for page switching and one for commands; show both in the UI instead of presenting a generic “jump or run” hint.

Done when keyboard navigation works from page open through edit, save, and return without unexpected focus loss or full-shell flicker.

### UX-03 — Turn the palette into a reliable quick-pick (S–M)

Polish the existing quick switcher and command palette rather than adding more commands.

- Use clear command names with optional categories and visible shortcuts.
- Show page title, path, and a useful recent/favorite signal in results.
- Support `Esc`, arrow keys, Enter, Home/End, and empty/error/loading states.
- Use a consistent result row height and active-row treatment.
- Keep search and command modes visually distinct while sharing the same picker interaction.

Done when a user can open any page or action in under three keystrokes and can always tell what Enter will do.

### UX-04 — Finish the inline editor contract (M)

Make the current CodeMirror editor feel trustworthy in `page.html` and the save handlers.

- Show a clear saved / saving / unsaved / failed state near the editor.
- Warn before leaving with unsaved content; preserve the draft after a preview or network failure.
- Keep the reading surface and editor aligned to the same content width.
- Make `Cmd/Ctrl-E` a true mode toggle and ensure the fallback editor remains usable with JavaScript disabled.
- Replace the current “live preview” ambiguity with an explicit preview error state and retry action.

Done when an interrupted save cannot silently discard text and the user always knows whether the file on disk is current.

### UX-05 — Make links feel like first-class navigation (M)

Complete the linking interaction across the editor, renderer, and page routes.

- Selecting a missing `[[page]]` result should offer “Create page” explicitly; creation should happen only after the user confirms or enters content.
- Keep missing links visually distinct but calm, with a direct create action.
- Show link target and page preview on hover where practical.
- Preserve the existing unlinked-mention promotion flow, but make the action explain exactly which text will be converted.

Done when creating, following, and promoting links feels reversible and requires no manual path repair.

### UX-06 — Simplify the sidebar and hierarchy (M)

Refine the folder tree and right rail in `_nav_macro.html`, `folder.html`, and `page.html`.

- Default to a small number of useful sections: pages, search, and tags.
- Lazy-load deeper tree levels without making the first level feel empty.
- Add a clear collapse/expand affordance and remember the user's choice.
- Move rename, move, and trash actions into a consistent contextual menu.
- Make the current page, current folder, and breadcrumb hierarchy unmistakable.

Done when users can locate a page spatially without needing the quick switcher, while the quick switcher remains faster for known destinations.

### UX-07 — Replace silent mutations with lightweight feedback (S)

Audit all create, save, move, rename, trash, restore, and promote actions.

- Show a short success confirmation for completed mutations.
- Show an actionable error with retry or recovery where possible.
- Use confirmation only for destructive or difficult-to-reverse operations.
- Keep the notification system unobtrusive and compatible with reduced motion.

Done when every mutation has a visible outcome and no operation depends on the user inferring success from a page refresh.

### UX-08 — Add a real UX acceptance harness (M)

Add browser-level smoke coverage for the flows above, alongside existing Rust unit/template tests.

- Cover quick switch, command execution, edit toggle, preview failure, save, missing-link creation, mention promotion, tree navigation, and trash recovery.
- Test keyboard paths and reduced-motion rendering where the browser runner supports them.
- Keep the smoke fixture small and deterministic; do not use a personal vault.
- Document the exact local runtime and browser command in `docs/setup.md`.

Done when UX changes have evidence beyond template-string assertions.

## Suggested first slice

Start with `UX-01`, `UX-02`, and `UX-04` as one cohesive “calm and trustworthy editing” slice. Then implement `UX-03` and `UX-07`, followed by link and sidebar polish. `UX-08` should be added
alongside the first slice, not postponed until the end.

## Product references

- [Obsidian Live Preview](https://help.obsidian.md/Live%2Bpreview%2Bupdate) — source/live-preview continuity and an explicit mode toggle.
- [SilverBullet Manual](https://v1.silverbullet.md/Manual) and [Getting Started](https://v1.silverbullet.md/Getting%20Started) — page picker, live preview, slash commands, and create-on-follow for
  missing links.
- [VS Code user interface](https://code.visualstudio.com/docs/editing/userinterface) — quick open, command palette, navigation history, and Zen mode.
- [VS Code command palette guidance](https://code.visualstudio.com/api/ux-guidelines/command-palette) — clear command names, categories, and shortcuts.
- [Notion sidebar navigation](https://www.notion.com/help/navigate-with-the-sidebar) — collapsible hierarchy, favorites, contextual page actions, and sidebar resizing/hiding.
