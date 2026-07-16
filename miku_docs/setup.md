---
title: Miku Development Setup
type: guide
status: active
tags: [miku, setup, development]
updated: 2026-07-16
---

# Setup

## Prerequisites

- Nix (with flakes) — the devShell provides rust, prettier, uv, and postgresql
- Postgres — optional, for the explicit native/scale profile

## Native dev stack (no containers — Linux & macOS)

The default path is SQLite durability with a MemoryIndex/Tantivy hot projection:

```bash
make dev
```

The default runtime uses SQLite plus the in-memory/Tantivy projection. Postgres and Valkey are optional service-backed profiles.

Override the backend with MIKU_INDEX_BACKEND, DATABASE_URL, or VALKEY_URL.

## Remote access (LAN / Tailscale)

The server binds `0.0.0.0:3000` by default, so it is reachable from other devices on your tailnet at `http://<tailscale-ip>:3000` (or the MagicDNS name, e.g. `http://mac-mini:3000`) — not only from
localhost. No reverse proxy needed for tailnet access.

- Restrict to local only: `MIKU_BIND=127.0.0.1:3000 make dev`.
- macOS: if the application firewall prompts, allow incoming connections for the miku binary; Tailscale traffic arrives over the `utun` interface.
- Optional TLS/sharing: `tailscale serve 3000` (tailnet) or `tailscale funnel 3000` (public) put it behind Tailscale's TLS.

## Manual configure (external Postgres)

If you already run Postgres elsewhere, just point the app at it (kept out of git):

```bash
export DATABASE_URL=postgres://localhost/miku
export MIKU_INDEX_BACKEND=postgres
```

## Build, run, test

```bash
nix develop       # enter the devShell (provisions all tools)
make css          # build frontend CSS
make dev          # run Rust backend and Vite frontend together
make check                             # default fmt + lint + tests
make check-all-features                # all Cargo features
make check-integration                 # optional service-backed probes
make release                           # crates.io leaf package dry-runs
make validate                          # check + release build
make check-blackbox                    # live HTTP checks against a running app
make check-ux-browser                 # Playwright browser acceptance (install Chromium once)
MIKU_BENCH_BACKEND=sqlite make benchmark # benchmark a running local SQLite app
```

All quality targets are thin Make wrappers around `uv run python scripts/ci.py`, so local and GitHub CI use the same implementation. The Python commands can also be invoked directly when debugging a
single matrix slice.

The `scripts/` suite is a first-class non-Rust test surface: `pytest` covers black-box validation helpers, and `ruff` checks/lints the automation code. The HTTP black-box probe requires a running app
and is invoked with `make check-blackbox`.

Project automation/scripts are Python run via `uv run python scripts/<x>.py` (root `pyproject.toml`), not bash.

The browser acceptance harness uses Playwright against a real local process. Install its browser once with `uv run playwright install chromium`, then run `make check-ux-browser`. Screenshots are
written to `.artifacts/ux/` (ignored).

## Containers (Postgres/Valkey scale profile only)

Containers are only for the service-backed scale profile. The default local runtime is a native Rust binary and Vite frontend. Podman and Podman Compose are intentionally host-provided tools, not Nix
prerequisites.

The Compose profile runs PostgreSQL 18 and Valkey 9 with the `postgres-valkey` Miku feature set. Prepare Podman (including its VM on macOS) yourself, then run the complete lifecycle experiment through
uv:

```bash
make compose-experiments # build, start, smoke-test, and tear down
# or inspect it manually:
podman-compose up -d --build
podman-compose logs -f
podman-compose down
```

The native stack above is preferred for day-to-day local development. Docker Compose can still be used when its compatibility with `compose.yml` is verified separately.

## Database

The legacy SQLite index is stored at `miku_docs/.miku-index.sqlite` when `MIKU_INDEX_BACKEND=sqlite` is selected. Postgres migrations live under `crates/miku-index-postgres/migrations/` and are used
only for the explicit Postgres profile. Both indexes are fully rebuildable from `miku_docs/**/*.md`; dropping and recreating either loses no user data.
