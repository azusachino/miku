# Unlinked mentions: implementation plan

## Goal

Keep the useful part of the Obsidian-style feature—discovering ordinary text that could become a page link—without making `/p/:path` pay for a vault-wide search. Obsidian exposes this as a secondary
Backlinks section alongside linked mentions; Miku will keep the same distinction while making the feature asynchronous and disposable. [Obsidian Backlinks](https://obsidian.md/help/plugins/backlinks)

## Current path and measured problem

The page handler currently loads explicit backlinks, then calls `unlinked_mentions`. That helper performs a body search, reads each hit from the filesystem, reparses Markdown, and verifies the first
plain-text occurrence before rendering. On the real `miku_docs` corpus, this produced approximately:

| Operation                        | Observed time |
| -------------------------------- | ------------: |
| Page view unlinked-mention phase |   1.60–1.62 s |
| Body search                      |   1.62–1.80 s |
| Quick switch search              |        0.70 s |
| Navigation/folder query          |        0.05 s |

The same work also contends with Turso's single connection while the background indexer is committing. The feature is therefore a secondary-index problem, not a page-render problem.

## Target dataflow

```text
Markdown file change
  -> parse PageIndex
  -> commit page projection and explicit links
  -> update title/alias dictionary
  -> enqueue changed source for mention extraction
  -> scan source body against known page names
  -> commit derived mention rows
  -> publish one refresh event

GET /p/:path
  -> read source Markdown
  -> read explicit backlinks
  -> read derived mentions by target_path
  -> render immediately
```

The mention relation is allowed to lag. It must never block page content, navigation, ordinary search, or link following.

## Document signals and “key insights”

The pre-index stage is also the right place to extract lightweight document signals, but “key insights” must be split into two levels:

### Deterministic signals — first implementation

These are cheap, explainable, reproducible, and rebuildable from Markdown:

```text
DocumentSignals {
  lead: first meaningful paragraph,
  headings: [heading text and level],
  title: resolved page title,
  aliases: frontmatter aliases,
  tags: inline and frontmatter tags,
  links: outgoing page links,
  word_count: visible body word count,
}
```

The first “insight” surface should therefore be a compact **Highlights** block made from the lead paragraph and the most relevant headings. It should not claim to be a semantic summary. The parser
already has the frontmatter split, title resolution, heading extraction, tags, and wikilink projection needed to produce these fields during one parse.

### Semantic summaries — later and optional

Actual prose such as “this document argues that…” is a summarization feature, not an index feature. It should be an asynchronous, explicitly configured provider with its own cache, model/version,
failure state, and privacy controls. It must never be required for search, backlinks, or page rendering, and it must never overwrite Markdown source.

### Tantivy boundary

Tantivy can help rank existing terms and retrieve relevant passages, but it is not by itself a key-insight extractor. Its tokenization, term frequency, positions, and BM25-style ranking are useful
inputs for later extractive ranking; they do not understand document claims, causality, or correctness. The initial Highlights block should come from Markdown structure, not from an opaque score.

## Proposed projection

The backend contract should expose a bounded lookup by target page, while the storage implementation owns the schema:

```text
unlinked_mentions(
  target_path,
  source_path,
  source_title,
  snippet,
  matched_text,
  source_mtime,
  target_revision
)
```

The first durable implementation can use a Turso table with a composite primary key `(target_path, source_path, matched_text)` and an index on `target_path`. The row is derived and may be deleted and
rebuilt without touching Markdown files or the primary page projection.

The in-memory backend should maintain the same observable behavior using a target-keyed map. It must not implement the current full `BTreeMap` body scan as the production behavior; that implementation
is retained only as a small contract-test reference until the derived relation exists.

## Matching pipeline

### Phase 1: exact titles

1. Build a normalized title dictionary from indexed `PageIndex.summary.title` values.
2. Exclude empty, ambiguous, and self titles.
3. Scan each changed page body once.
4. Ignore frontmatter, fenced code, inline code, existing wikilinks, and Markdown links.
5. Emit at most one relation per `(source, target, matched text)` with a short context snippet.
6. Replace all rows for the changed source in one transaction.

An Aho–Corasick-style multi-pattern matcher is the preferred implementation if the title dictionary is large: it scans the source body once for all known names. A simpler exact matcher is acceptable
for the first benchmark, provided it has the same exclusions and emits deterministic results.

### Phase 2: aliases and invalidation

Add frontmatter aliases after title-only matching is measured. A title or alias change invalidates the affected target's mention rows; a source edit invalidates only that source's rows. A full
relation rebuild remains available for database recovery and matcher/schema changes.

### Ambiguity policy

If two pages share the same title or alias, do not guess. Mark the candidate ambiguous or omit it from promotion results. Existing explicit wikilinks remain resolved by the normal link resolver.

## Tantivy's role

Tantivy is useful in four bounded ways:

1. **Ordinary search.** Its tokenized inverted index is the right engine for user-entered full-text queries.
2. **Candidate retrieval during migration.** For a temporary implementation, a normalized title query can narrow candidate pages before exact Markdown verification.
3. **Incremental indexing mechanics.** Its `IndexWriter` accepts additions and deletions, publishes changes on commit, and merges immutable segments in the background. This maps well to a background
   batch indexer. [Tantivy IndexWriter](https://docs.rs/tantivy/latest/tantivy/indexer/struct.IndexWriter.html)
4. **Tokenizer and position support.** Custom tokenizers and indexed positions can support exact term/phrase candidates, but multilingual matching needs an explicit tokenizer decision; the default
   tokenizer is not a complete CJK phrase matcher. [Tantivy tokenizers](https://docs.rs/tantivy/latest/tantivy/tokenizer/index.html)

Tantivy does not replace the derived relation. It does not know whether a match is inside a wikilink, code block, frontmatter, or a safe promotion span. It also does not provide the target-oriented
relation that the page view needs without another mapping layer.

## Implementation sequence

1. Add a backend-neutral `replace_mentions_for_source` and `mentions_for_target` contract, or the smallest equivalent extension to `IndexWriter`/`IndexReader`.
2. Add Turso schema, transaction helpers, and contract tests for insert, replacement, deletion, restart, and stale-row cleanup.
3. Move Markdown matching into `miku-indexer` so parsing and mention extraction share the same source bytes and exclusion rules.
4. Add a deterministic title dictionary and exact title matcher; benchmark simple matching against a multi-pattern matcher before choosing the implementation.
5. Wire changed-source and changed-target invalidation into reconcile and watcher events.
6. Remove synchronous mention discovery from `page_view`; render the relation lookup as optional secondary data.
7. Add blackbox coverage for a visible mention, an existing wikilink, code/frontmatter false positives, ambiguous titles, changed source, changed target, restart, and an indexer-in-progress page
   request.
8. Add metrics: `mention_scan_pages_total`, `mention_scan_duration_seconds`, `mention_rows`, `mention_rebuild_total`, and page-view `mentions_lookup_ms`.
9. Compare before/after on the 14k-page corpus. The page route must not regress when the relation is unavailable; the desired steady-state lookup is single-digit milliseconds locally.
10. Add `DocumentSignals` only after mention extraction is stable; persist compact signals separately from the full page payload so they can evolve without invalidating the primary projection.

## Explicit non-goals

- No Bloom filter as the primary result structure.
- No automatic conversion without an explicit user action.
- No semantic/entity linking; matching is title/alias text only.
- No requirement that unlinked mentions be current while the background indexer is busy.
