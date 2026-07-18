---
title: Topcoat web-layer investigation
type: investigation
status: active
tags: [miku, topcoat, frontend, architecture, pitfall]
updated: 2026-07-18
---

# Topcoat web-layer investigation

## Question

Can Miku replace the current React/Vite workspace with Topcoat without compromising the visual design, interaction model, rich Markdown reader, editor, or filesystem freshness behavior?

## Short answer

Not established. Topcoat is a plausible candidate for a Rust-native web surface, but it has not demonstrated parity with Miku 0.0.3's stateful workspace. The current frontend is the product
baseline, not disposable implementation detail.

The migration decision must be driven by behavioral parity and lower total complexity, not by the appeal of using Rust for both server and browser-facing components.

## What the tag comparison shows

Miku v0.0.2 used server-rendered templates with Alpine.js, htmx, custom JavaScript, and browser libraries for the command palette, navigation, search, rich Markdown, and editor surfaces. The base
template and route module accumulated responsibility for both page composition and browser interaction.

Miku v0.0.3 changed the boundary deliberately. It introduced a persistent React/Vite workspace with client-owned tabs, splits, selection, focus, history, lazy tree loading, CodeMirror source
editing, rich Markdown integrations, and a domain-oriented Rust API. ADR-0017 records the server-rendered reader as rejected for the stateful workspace it needed to support.

This is evidence that the primary problem in v0.0.2 was not Alpine.js alone. The problem was representing a continuous workspace as server-rendered fragments coordinated by several independent browser
mechanisms.

## What Topcoat could improve

Topcoat may reduce:

- the Rust/TypeScript model duplication;
- handwritten route and component glue;
- the separate API/client boundary when there is only one official UI;
- asset and page routing setup;
- simple server-rendered page interactions.

Its documented primitives include typed HTML views, async components, signals, server-rendered shards, module-based routes, asset bundling, Tailwind support, and htmx integration.

These benefits apply most directly to page composition and simple interactions. They do not prove that Topcoat can own Miku's workspace state or rich browser integrations cleanly.

## Parity risks

The following are required product behaviors, not optional implementation details:

| 0.0.3 behavior | Topcoat status |
| --- | --- |
| Persistent tabs and split panes | Unproven |
| Tree expansion without losing workspace state | Unproven |
| URL, selection, focus, and history coordination | Unproven |
| CodeMirror lifecycle and source editing | Requires browser integration |
| Prism, Mermaid, and KaTeX rendering | Still requires browser libraries |
| Optimistic revision checks and conflict UI | Feasible, but custom |
| Filesystem freshness and editor refresh behavior | Feasible, but custom |
| Existing visual design and CSS | Likely portable |

Topcoat's server-rendered shard model could recreate the old failure mode if frequent interactions become request → rerender → DOM replacement cycles. A typed Rust view does not remove the need to
decide which state is local, which state is server-owned, and how state survives replacement.

Topcoat is also early-stage and experimental. Its current framework APIs should be treated as moving dependencies rather than a stable foundation.

## Decision boundary

Do not replace `miku-web` merely to remove React, TypeScript, Bun, or Vite. Replace it only if a Topcoat implementation demonstrates all of the following:

1. The 0.0.3 visual and interaction baseline remains intact.
2. Tabs, splits, tree state, focus, history, and URL state are no harder to reason about.
3. CodeMirror and rich Markdown integrations remain first-class rather than awkward escape hatches.
4. The total amount of custom JavaScript and server-fragment coordination decreases.
5. Browser acceptance checks cover the same workflows and remain reliable.
6. The resulting architecture keeps `miku-domain`, `miku-vault`, `miku-markdown`, `miku-indexer`, and projection backends independent of the web framework.

## Recommended experiment

Build a Topcoat vertical slice in a separate web crate against `miku-app`. Do not delete or substantially disturb `miku-web` during the experiment.

The slice must include:

- the workspace shell;
- lazy tree navigation;
- two opened notes and tab switching;
- search and URL synchronization;
- Markdown with code, Mermaid, and math;
- CodeMirror editing;
- optimistic save conflicts;
- an external filesystem edit reflected in the active note.

Compare it with the current frontend on interaction fidelity, code size, client JavaScript, request count, startup/build complexity, and test reliability. A static reader or a hello-world
prototype is not sufficient evidence.

## Current conclusion

Topcoat is worth investigating because Miku is unreleased and the web boundary is still changeable. It is not yet justified as a replacement for the 0.0.3 frontend. The stable choice is to preserve
the current UX contract and make Topcoat earn a migration through a difficult, feature-complete vertical slice.
