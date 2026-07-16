---
id: ADR-0007
type: adr
title: ADR-0007 — Frontend rendering and client JavaScript budget
status: Superseded by ADR-0017
updated: 2026-07-15
tags: [frontend, prism, mermaid, historical]
---

# ADR-0007 — Frontend rendering and client JavaScript budget

## Status

Superseded by ADR-0017. This record is retained only to explain the rejected
server-rendered and Alpine-based frontend direction.

## Historical decision

The original MVP used server-rendered HTML with a small set of locally served
scripts. Prism and Mermaid were selected for Markdown enhancements, while
Alpine was selected for local interactions.

## Current replacement

Miku now uses the separate React/Vite frontend described by ADR-0017. Tailwind
and Tailwind Typography own the shell and generic Markdown typography; Prism,
Mermaid, and KaTeX remain library-backed reader integrations.
