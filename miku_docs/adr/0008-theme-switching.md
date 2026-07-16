---
id: ADR-0008
type: adr
title: ADR-0008 — Theme switching
status: Accepted
updated: 2026-07-16
tags: [frontend, themes, tailwind, dark-mode]
relates-to: [ADR-0017]
impacts: [miku-web/src/styles.css, miku-web/src/shared/ui.ts]
---

# ADR-0008 — Theme switching

## Decision

Miku provides light and dark themes as semantic roles rather than component specific colors. Theme choice is persisted per browser and applied to the workspace shell, reader, editor, syntax
highlighting, Mermaid, and dialogs.

## Mechanics

- CSS variables define the palette roles for each theme.
- Tailwind utilities consume the semantic roles for the shell.
- Prism uses the matching official light or dark theme.
- Mermaid receives the active theme when a diagram renders.
- React owns the toggle and local persistence.

## Consequences

Reader content remains legible when the theme changes, and the frontend does not depend on a server-side theme or a global user account.
