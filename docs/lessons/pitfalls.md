> Project-local fallback lessons captured for agent review. Mirror active pitfalls into asobi when possible.

## 2026-07-01 — whole-shell-htmx-boost-flicker

Status: resolved for v0.0.2

Tried: Put broad `hx-boost="true"` on the app shell and let search result links inherit default boosted navigation.

Why it failed: htmx default boosted navigation targets too much document surface, so search-result clicks can swap/repaint the full application chrome and look like a page flicker.

Do instead: For search forms and search-result page links, explicitly set `hx-target=".mk-view"`, `hx-select=".mk-view"`, `hx-swap="outerHTML show:top"`, and `hx-push-url="true"` so only the main
content pane updates.

## 2026-07-01 — text-placeholders-are-not-icons

Status: active

Tried: Use single letters such as `L`, `D`, and `/` as topbar action controls.

Why it failed: The controls read as degraded text rather than intentional icons, especially after broader visual polish.

Do instead: Use real inline SVG icons or the project's icon system, keep accessible labels, and avoid adding a network dependency for basic chrome icons.

## 2026-07-01 — loading-state-without-error-state

Status: active

Tried: Show quick-switcher `Loading...` while relying on fetch/abort cleanup alone.

Why it failed: Any client-side race, timeout, or exception could leave users seeing a permanent loading state with no recovery information.

Do instead: Track an explicit error state, race fetch with a timeout, clear loading in `finally`, and keep request ids so stale responses cannot mutate current palette state.

## 2026-07-01 — search-ui-scope-without-backend-scope

Status: active

Tried: Render `All`, `Title`, and `Body` search controls while the backend only deserialized `q`.

Why it failed: The UI implied scoped search, but all modes executed the same body-tsv query and produced misleading behavior.

Do instead: Deserialize `scope`, model it as an enum, and route title/path/slug/body queries through distinct SQL paths with visible active scope in the template.

## 2026-07-01 — inline-tree-forms

Status: active

Tried: Implement tree operations as row-level `Move/Rename` inline forms, delete confirm forms, redirecting `/api/move` and `/api/trash` handlers, and folder-only drag/drop.

Why it failed: The flow lacks the interaction model users expect from Trilium-style trees: drag hit modes before/after/into, context-menu cut/paste/move-to, undoable trash, collision preview, keyboard
movement, and JSON tree transactions.

Do instead: Keep the explorer read-only in v0.0.2. Provide reliable Markdown
content editing, and leave path changes or file removal to the filesystem and
git. Revisit file-management UX only as a separate, fully specified milestone.
