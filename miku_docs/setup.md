# Setup

## Prerequisites

- Nix (with flakes) — the devShell provides rust, prettier, uv, and postgresql
- Postgres — optional, for the explicit native/scale profile

## Native dev stack (no containers — Linux & macOS)

The default path is SQLite durability with a MemoryIndex/Tantivy hot projection:

```bash
make run
```

For the optional Postgres profile, run Postgres directly from the devShell against a project-local, disposable cluster (`.pgdata/`, gitignored) on port `55432`:

```bash
make db-up        # init (first run) + start Postgres, create the miku database
make dev          # start the DB if needed, then run the server (foreground)
make dev-tmux     # same, in a tmux session (pane 0: server, pane 1: pg log)
make db-psql      # open psql against the local cluster
make db-down      # stop Postgres
make db-reset     # stop + delete .pgdata (index is rebuilt from miku_docs/**/*.md)
```

The crate default features include both the rebuildable Rust-built MemoryIndex/Tantivy projection and SQLite. The runtime defaults to the composed SQLite + MemoryIndex tier. `MIKU_INDEX_BACKEND=memory` remains available for an explicit disposable run. `make dev` selects the explicit Postgres profile and sets `DATABASE_URL=postgres://miku@localhost:55432/miku`
(trust auth, no password); migrations run on startup. Override with `MIKU_INDEX_BACKEND=…`, `MIKU_INDEX_PATH=…`, `PGPORT=…`, `PGDATA=…`, or `DATABASE_URL=…`.

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
make css          # build static/tailwind.generated.css via bun (bun run css)
make run          # run the native local stack (default memory/Tantivy projection)
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

Containers are only for the service-backed **scale profile** — the default `memory` runtime is a pure local binary (`make run`), so it needs no image. Podman and Podman Compose are intentionally host-provided tools, not Nix prerequisites.

The Compose profile runs PostgreSQL 18 and Valkey 9 with the `postgres-valkey` Miku feature set. Prepare Podman (including its VM on macOS) yourself, then run the complete lifecycle experiment through uv:

```bash
make compose-experiments # build, start, smoke-test, and tear down
# or inspect it manually:
podman-compose up -d --build
podman-compose logs -f
podman-compose down
```

The native stack above is preferred for day-to-day local development. Docker Compose can still be used when its compatibility with `compose.yml` is verified separately.

## Database

The legacy SQLite index is stored at `miku_docs/.miku-index.sqlite` when `MIKU_INDEX_BACKEND=sqlite` is selected. Postgres migrations live under `crates/miku-index-postgres/migrations/` and are used only for the explicit Postgres profile.
Both indexes are fully rebuildable from `miku_docs/**/*.md`; dropping and recreating either loses no user data.
