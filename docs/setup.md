# Setup

## Prerequisites

- Nix (with flakes) — the devShell provides rust, prettier, uv, and postgresql
- Postgres — optional, for the explicit native/scale profile

## Native dev stack (no containers — Linux & macOS)

The default path is local SQLite and needs no database service:

```bash
make run
```

For the optional Postgres profile, run Postgres directly from the devShell
against a project-local, disposable cluster (`.pgdata/`, gitignored) on port
`55432`:

```bash
make db-up        # init (first run) + start Postgres, create the miku database
make dev          # start the DB if needed, then run the server (foreground)
make dev-tmux     # same, in a tmux session (pane 0: server, pane 1: pg log)
make db-psql      # open psql against the local cluster
make db-down      # stop Postgres
make db-reset     # stop + delete .pgdata (index is rebuilt from miku_docs/**/*.md)
```

The app defaults to the local Rust-built SQLite index at `miku_docs/.miku-index.sqlite`. `make dev` selects the explicit Postgres profile and sets `DATABASE_URL=postgres://miku@localhost:55432/miku`
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
make run          # build Tailwind CSS (bun) then run the server (default local SQLite index)
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

Containers are only for the service-backed **scale profile** — the default `memory`/`sqlite` runtime is a pure local binary (`make run`), so it needs no image. The image (`Containerfile`) is built
with the `postgres,valkey` features and pairs the app with a Postgres service via `compose.yml`.

```bash
make stack-up          # podman compose up -d (Postgres + app on postgres backend)
make stack-build       # rebuild + recreate the miku image
make stack-logs        # follow logs
make stack-down        # stop the stack
```

`COMPOSE` defaults to `podman compose`; there is no hard Docker requirement. Override for Docker Desktop: `COMPOSE="docker compose" make stack-up` (Docker needs `-f Containerfile`, which `compose.yml`
already sets via `dockerfile:`). The native stack above is preferred for day-to-day local development.

## Database

The SQLite index is stored at `miku_docs/.miku-index.sqlite` by default.
Postgres migrations live under `crates/miku-index-postgres/migrations/` and are
used only for the explicit Postgres profile. Both indexes are fully rebuildable
from `miku_docs/**/*.md`; dropping and recreating either loses no user data.
