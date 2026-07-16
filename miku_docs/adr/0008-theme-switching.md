---
id: ADR-0008
title: Theme switching (palette × mode)
slug: theme-switching
status: Accepted
date-proposed: 2026-06-26
date-accepted: 2026-06-26
deciders: [haru]
mirror: asobi:miku:decision:theme-switching
supersedes: []
superseded-by:
extends: [ADR-0007]
relates-to: [ADR-0003, ADR-0007]
impacts: [static/css/themes, src/templates, static/js]
config-keys: [theme]
tags: [frontend, themes, alpine, catppuccin, dark-mode]
---

# ADR-0008 — Theme switching (palette × mode)

## Decision

A theme switcher on **two orthogonal axes**, both persisted to `localStorage` (per-browser display prefs, not content):

- **Palette:** `default` | `catppuccin` → `data-palette` on `<html>`.
- **Mode:** `system` | `light` | `dark` → resolved to `data-mode` of `light`/`dark` on `<html>`. `system` reads `prefers-color-scheme`, with a `matchMedia` listener that live-updates on an OS flip.

CSS is **custom-variable sets** selected by the attribute pair — four base combinations: `default+light`, `default+dark`, `catppuccin+light` (Latte), `catppuccin+dark` (Mocha). Selectors like
`[data-palette="catppuccin"][data-mode="dark"]` override the `--vars`; all themed surfaces (callouts, links, code) read from the variables.

**Default for a fresh visitor:** `default` palette + `system` mode. The ADR-7 `theme` config key supplies the server-side default when no `localStorage` pref exists; no new config keys.

## Mechanics

1. **Inline pre-paint script** in `<head>` sets `data-palette`/`data-mode` from `localStorage` before first paint — avoids the theme flash (FOUC). One small inline `<script>`, in the spirit of ADR-7's
   client-JS budget.
2. **Alpine** drives the dropdown, persistence, and the `matchMedia` listener.
3. **Prism** code themes are swapped to match the resolved palette+mode (Catppuccin ships official Prism themes).

## Why

Palette and mode are genuinely independent, and the Catppuccin family maps exactly onto the mode axis (Latte = light, Mocha = dark), so it inherits System/Light/Dark for free — "Catppuccin + System" =
Latte by day, Mocha by night with zero extra UI. Fits the decided no-bundler + Alpine stance (ADR-7).

## Trade-offs / Rejected

Four CSS variable sets to maintain plus matching Prism themes. One new inline pre-paint `<script>` — justified by FOUC, kept minimal. `localStorage` is per-browser and **not synced** — acceptable for
a display preference. Rejected: a flat single-list of themes (loses palette×mode independence); server-side per-user theme storage (no user system — ADR-3).
