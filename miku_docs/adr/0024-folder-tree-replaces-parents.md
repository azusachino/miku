---
id: ADR-0024
type: adr
title: ADR-0024 — The folder tree replaces frontmatter parents and order
slug: folder-tree-replaces-parents
status: Proposed
updated: 2026-09-30
date-proposed: 2026-09-30
date-accepted:
deciders: [haru]
mirror: asobi:miku:decision:folder-tree-replaces-parents
supersedes: []
superseded-by: []
relates-to: [ADR-0017, ADR-0022, ADR-0023]
impacts: [crates/miku-domain, crates/miku-vault, crates/miku-app, crates/miku/src/http_api.rs, miku-web, scripts/blackbox.py]
tags: [domain, tree, frontmatter, performance]
---

## ADR-0024 — The folder tree replaces frontmatter parents and order

### Decision

Miku stops interpreting the `parents` and `order` frontmatter keys. **The folder structure is the only hierarchy.** This completes what ADR-0023 froze: Trilium-style placements and clones are a non-goal.

1. **Hierarchy comes from paths.** A note's parent is its folder. A folder's note is its `index.md`, the convention the workspace tree already uses. Sibling order is the tree's existing sort: folders first, then by title with numeric ordering.
2. **Note context derives parents and children from paths.** `parents` is the chain of ancestor `index.md` notes that exist. `children` is empty for an ordinary note. For an `index.md` note, `children` is the entries of its folder, the same answer the tree API gives. The `ContextResponse` fields keep their names and shapes, so the API contract and generated client do not change in this step.
3. **Existing keys become plain properties.** Files that already contain `parents` or `order` are not rewritten. The keys are shown and indexed like any other frontmatter property, with no special meaning.
4. **Miku stops writing the keys.** Note creation and ID migration no longer add `parents: []` or `order` to files.
5. **The domain drops the concepts.** `Note`, `WorkspaceFrontmatter`, and parent-list validation lose `parents` and `order`. The frontend drops the "placements" property row and `parents` from its note model.
6. **`GET /api/v1/note-children/{id}` and the tree's `parent_id` query filter are deprecated.** `GET /api/v1/tree?folder=…` answers the same question. Both stay until the frontend and `scripts/blackbox.py` stop using them, then they are removed in the same change series.

The stable frontmatter `id` is **out of scope**. It loses its main consumer here (parents referenced notes by ID), so its future gets a separate review.

### Why

- **It removes the full-vault scan behind every page view.** `note_context` finds children by reading every page's frontmatter for a matching parent ID, and resolves parents through an ID map built from every page. Both are O(vault) per request, which breaks ADR-0023 goal 1. Path-derived hierarchy needs only the note's own path and, for folder notes, one folder listing.
- **Nothing depends on it.** In the maintained vault, the only `parents:` occurrence is the example inside ADR-0017. The imported course corpora are organized by folders.
- **Plain folders cannot express placements.** A note in several places, or an order that differs from the filesystem's, must live in Miku-specific frontmatter that other editors, `ls`, and git do not understand. This contradicts "the folder stays the only source of truth".
- **Miku stops writing its own keys into user files.** Injecting `parents: []` into every created or migrated note is noise in the user's files and in their git history.

### Implementation order

1. Add a `note_context` test on a fixture vault that pins today's answers for paths, backlinks, and outgoing links. This is the first piece of the ADR-0023 rebuild conformance test.
2. Derive `parents` and `children` from paths in `miku-app` and remove the frontmatter scan.
3. Stop writing `parents` and `order` in `miku-vault`; remove them from `miku-domain`.
4. Update the frontend (the placements row, the note model, the `note-children` call) and `scripts/blackbox.py`, then remove the endpoint and the `parent_id` filter and regenerate `openapi.json` and `generated/api.ts`.
5. Benchmark `note_context` on the real vault before step 2 and after step 4.

### Trade-offs / Rejected

- **Keep `parents` as an optional "up" link.** Rejected. An upward link is already expressible as an ordinary `[[wikilink]]` in the body or in a property, and it then shows up as a backlink. A special key adds a second link mechanism for the same job.
- **Keep `parents` but index a parent-to-children map.** Rejected. It fixes the scan but keeps a hierarchy that folders cannot show, and ADR-0023 made placements a non-goal.
- **Rewrite existing files to delete the keys.** Rejected. Miku does not bulk-edit user files. The keys are harmless as plain properties, and users can remove them with their own tools.
- **Remaining cost:** outgoing-link resolution in `note_context` still builds a name index from every page on each request. This ADR removes two of the three full scans; the name index moves into the indexed projection in the follow-up relation-index ADR.
- **Cost accepted:** a note can no longer appear in two places in the tree, and custom sibling order is gone. Users who need either can rename folders and files, or link notes explicitly.
