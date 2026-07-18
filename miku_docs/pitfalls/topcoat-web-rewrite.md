---
title: Topcoat rewrite is not automatically simpler
type: pitfall
status: active
tags: [miku, topcoat, frontend, architecture]
updated: 2026-07-18
---

# Pitfall: Topcoat rewrite is not automatically simpler

## Warning

Do not infer that replacing React/Vite/TypeScript with Topcoat will fix the problems experienced by Miku 0.0.2. Fewer languages and fewer named tools do not necessarily mean less product
complexity.

## Evidence

Miku 0.0.2 already combined server-rendered HTML, Alpine.js, htmx, custom JavaScript, and rich browser libraries. Its weakness was the coordination model for a stateful workspace, not only the
choice of Alpine.js.

Miku 0.0.3 moved tabs, splits, focus, selection, history, lazy loading, and rich reader/editor behavior into an explicit React frontend. That frontend currently defines the accepted visual and UX
baseline.

## Do instead

Treat Topcoat as an unproven web implementation candidate. Preserve the 0.0.3 behavior as a compatibility target, prototype the hardest interactions first, and migrate only when the experiment
reduces total coordination complexity without requiring a return to server-fragment choreography.

Keep the domain, vault, Markdown, indexer, and projection contracts independent of Topcoat so the experiment remains reversible.
