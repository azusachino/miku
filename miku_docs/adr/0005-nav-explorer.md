---
id: ADR-0005
type: adr
title: ADR-0005 — Navigation explorer
status: Superseded by ADR-0017
updated: 2026-07-15
tags: [frontend, navigation, workspace, historical]
---

# ADR-0005 — Navigation explorer

## Status

Superseded by ADR-0017. The original server-rendered explorer concept is kept
as historical context; the current explorer is the React workspace tree.

## Historical decision

Miku should expose a filesystem-shaped explorer with lazy folder expansion,
stable paths, and an active-note focus. The current implementation preserves
those semantics through the workspace API and browser tree.
