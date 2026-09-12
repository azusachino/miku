---
title: KB conventions
created: 2026-07-21 19:17
modified: 2026-07-21 19:17
type: documentation
status: maintained
maturity: stable
tags:
  - meta
  - documentation
---

## KB conventions

Miku's tracked first-party notes are the source of truth. These conventions apply to Markdown files directly under `miku_docs/` and `miku_docs/adr/`. Imported course corpora are preserved as source material and are not renamed or semantically normalized.

### Frontmatter

Every note should have these fields:

```yaml
---
title: A descriptive title
created: YYYY-MM-DD HH:MM
modified: YYYY-MM-DD HH:MM
type: concept
status: active
maturity: seed
aliases:
  - optional-alias
tags:
  - optional-tag
---
```

Required fields are `title`, `created`, `modified`, `type`, `status`, `maturity`, and `tags`.

`type` answers “what is this?” Use one of:

- `concept`, `article`, `book`, `course`, `collection`, or `person` for knowledge notes
- `journal`, `inbox`, `map`, `plan`, `index`, or `documentation` for operational notes

`status` answers “what am I doing with it?” The supported workflow values are:

- `inbox`: captured but not processed
- `active`: currently being processed or developed
- `paused`: intentionally deprioritized
- `maintained`: stable enough to keep and occasionally review

`maturity` answers “how developed is the knowledge?” The supported values are:

- `seed`: an initial capture or incomplete understanding
- `developing`: useful, but still being expanded or clarified
- `stable`: coherent knowledge that does not currently need substantial work

Additional fields such as `source`, `author`, `aliases`, `date`, and book-specific metadata are allowed when they describe the note. Avoid introducing presentation-only or duplicate fields.

Frontmatter uses the canonical order shown above, followed by optional descriptive, publishing, progress, and finally unknown fields. Sequence values use block-list syntax (`  - value`), never flow syntax (`[value]`). Historical daily activity-hour mappings use `time_spent`; the misleading `timer` key and Tolaria's `_width` and `_organized` presentation keys are not supported.

Frontmatter migration is intentionally reviewable rather than automatic. Run `make kb-check` to validate maintained filenames and local-link casing.

#### Filenames and aliases

- Markdown filenames use lowercase kebab-case, for example `cognitive-load-theory.md`. Daily notes use the ISO date form `YYYY-MM-DD.md`; weekly and monthly notes retain their existing ISO-derived forms such as `2026-w31.md` and `2026-07.md`.
- A filename is a stable identifier, not a display title. Keep capitalization, punctuation, non-English names, and other human-readable forms in `title` or `aliases`.
- When renaming a note, add the old stem as an alias unless it was meaningless or incorrect. Update resolvable repository links in the same change and run `make audit`.
- Aliases represent names a reader might genuinely search for: former filenames, abbreviations, translations, or established alternate names. Do not add spelling mistakes, duplicate the title, or repeat an alias with different capitalization.
- Imported non-canonical filenames are legacy inventory. Do not rename them as part of first-party maintenance.

#### Sources

- Use `source` when a note summarizes, quotes, imports, or closely derives from an external work. Original journals, plans, and independently developed concept notes do not need a source merely to satisfy metadata.
- Prefer the most direct durable URL available. If no URL exists, use a precise bibliographic reference or a wikilink to a source note.
- Use `author` when attribution is known and useful. Keep source title, author, and publication details in the note body when a bare URL would not preserve enough context.
- Mark quotations in the body and include a page, chapter, timestamp, or section when available. A `source` field identifies the work but does not replace local quotation context.
- Never leave `source` as `todo`, `tbd`, `unknown`, or an empty field. Omit it until the provenance is known.

#### Tags

Tags describe the note's subject or collection. They are not a copy of every keyword in the body.

- Use one or more tags for every note. `status` remains the workflow field; do not use tags such as `#inbox` or `#active` as a replacement.
- Tags use lowercase kebab-case without the `#` prefix in YAML, for example `cloud-native` or `personal-finance`. Words are separated by one hyphen; spaces and underscores are not canonical. Unicode letters and numbers are allowed, so non-English subject tags do not need transliteration.
- Canonicalization trims surrounding whitespace, lowercases letters, replaces runs of whitespace, underscores, or hyphens with one hyphen, and removes leading or trailing hyphens. Unsupported punctuation is reported rather than silently deleted.
- Prefer stable domain tags over narrow one-off phrases. Add a second tag only when it identifies a meaningful subdomain, format, or collection.
- Do not encode dates or calendar periods in tags. Values such as `week-2026-31` duplicate journal filenames and are not useful subjects; use the note path, title, or date metadata for temporal queries.
- Directory names are useful evidence but do not determine a tag by themselves; inspect the note title and content before assigning a topic.
- Existing tags with legacy capitalization, spacing, or language are preserved until their note batch is reviewed. Normalize them only as part of an explicit batch.
- Add tags in frontmatter rather than inline body tags so Obsidian searches and maintenance scripts have one canonical metadata source.
- Use `tags` as the only subject-classification field. Do not add `categories`; `make frontmatter-format` migrates legacy category values into `tags`, and the tag formatter then canonicalizes them.

Tag migration should remain reviewable. Canonical spelling is mechanical, but choosing, merging, or removing tag concepts requires inspecting the affected notes; do not make semantic taxonomy decisions as part of a filename-only change.

### Titles and links

- Journal titles include their date, for example `日志 2026.07.21`.
- Weekly titles use `YYYY-wNN`.
- Evergreen notes use a descriptive human title.
- Prefer wikilinks for local notes and assets. Run `make kb-check` after renaming or deleting maintained notes.
- Do not create a wikilink for an idea that does not yet have a note; use ordinary list text until the note exists.

### Formatting and callouts

Use native Obsidian callouts for notes, quotes, summaries, and warnings:

```md
> [!note]+ Note The first paragraph can contain multiple sentences on one physical line. If it is long, leave it long; Prettier will not hard-wrap it.
>
> Start a second paragraph with another quoted blank line.
>
> - Lists also need the `>` prefix.
> - Keep the prefix on every physical line.
```

Callout content is a Markdown blockquote. Every physical line must begin with `> `, and blank lines inside the callout must be represented by a lone `>`. Without those prefixes, the content ends the callout.

Use the repository's normal formatter for source code. Markdown body formatting remains manual so long prose and examples are not rewritten unexpectedly.

### Maintenance

Run `make kb-check` after changing maintained note filenames or links. The gate checks lowercase kebab-case names, unresolved Markdown links within the maintained set, and wikilinks that use non-canonical casing. `make check` includes this gate.
