# ADR-0015 — Derived unlinked-mention index

- **Status:** Accepted
- **Date:** 2026-07-14
- **Mirror:** asobi `miku:decision:derived-unlinked-mention-index`

## Decision

Treat unlinked mentions as an optional, eventually consistent derived relation rather than a page-render query.

The background indexer will maintain a relation from a source page to a target page when the source contains the target's title or alias as plain Markdown text, excluding existing wikilinks, code, frontmatter, and self-references. The page route will only read this relation; it will never scan the vault, invoke body FTS, or reread candidate Markdown files to discover mentions.

The durable page projection remains the source for rebuilding the relation. The relation is disposable and may be empty or stale while reconciliation is running. Linked forward links and backlinks remain authoritative and immediately available through the normal index projection.

Tantivy, as used by the Turso FTS implementation, remains the general-purpose full-text candidate/search engine. It may accelerate a rebuild or provide a fallback candidate set, but it is not the source of truth for mention semantics or promotion safety.

## Why

The previous page path searched body FTS and then reread candidate files during every render. On the 14k-page corpus this added about 1.6 seconds to page views and competed with the single Turso connection. This violated the runtime invariant that a readable page must not wait for secondary discovery features.

The indexer already parses changed Markdown into complete `PageIndex` values. Computing mention relations in that background pipeline amortizes the work, makes the request path bounded, and lets the relation be rebuilt after deleting the database.

## Trade-offs / Rejected

- A Bloom filter alone is insufficient: it can reject some impossible matches, but cannot return the source pages or snippets needed by the UI.
- A page-token inverted index is more general than needed and can consume substantial space for a large corpus.
- Direct Tantivy body queries are fast for ordinary search, but they do not encode the application rules for wikilinks, aliases, code, frontmatter, self-references, or safe promotion.
- The relation is eventually consistent. A newly changed page may not appear in unlinked mentions until its background batch commits; this is acceptable because unlinked mentions are a secondary discovery surface.
- Exact title matching is the first implementation. Alias matching and richer context extraction follow only after the indexed relation has coverage and latency measurements.
