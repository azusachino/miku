# Setup

## Prerequisites

- Nix (with flakes) — the devShell provides rust, prettier, uv, and postgresql
- Postgres — provided by the native dev stack below, or reachable via `DATABASE_URL`

## Native dev stack (no containers — Linux & macOS)

The fastest path, and the one to use on the Mac mini: run Postgres directly from
the devShell against a project-local, disposable cluster (`.pgdata/`, gitignored)
on port `55432`, then `cargo run`. No podman, no Docker, no VM — just processes.

```bash
make db-up        # init (first run) + start Postgres, create the miku database
make dev          # start the DB if needed, then run the server (foreground)
make dev-tmux     # same, in a tmux session (pane 0: server, pane 1: pg log)
make db-psql      # open psql against the local cluster
make db-down      # stop Postgres
make db-reset     # stop + delete .pgdata (index is rebuilt from miku_docs/**/*.md)
```

`make dev` sets `DATABASE_URL=postgres://miku@localhost:55432/miku` (trust auth,
no password) and the app runs its embedded sqlx migrations on startup — no
separate migrate step. Override with `PGPORT=…` / `PGDATA=…` / `DATABASE_URL=…`.

## Remote access (LAN / Tailscale)

The server binds `0.0.0.0:3000` by default, so it is reachable from other devices
on your tailnet at `http://<tailscale-ip>:3000` (or the MagicDNS name, e.g.
`http://mac-mini:3000`) — not only from localhost. No reverse proxy needed for
tailnet access.

- Restrict to local only: `MIKU_BIND=127.0.0.1:3000 make dev`.
- macOS: if the application firewall prompts, allow incoming connections for the
  miku binary; Tailscale traffic arrives over the `utun` interface.
- Optional TLS/sharing: `tailscale serve 3000` (tailnet) or `tailscale funnel
  3000` (public) put it behind Tailscale's TLS.

## Manual configure (external Postgres)

If you already run Postgres elsewhere, just point the app at it (kept out of git):

```bash
export DATABASE_URL=postgres://localhost/miku
```

## Build, run, test

```bash
nix develop       # enter the devShell (provisions all tools)
make run          # run the server (expects DATABASE_URL already set)
make check                             # default fmt + lint + tests
make all-features                      # all Cargo features
make integration                       # optional service-backed probes
make release                           # crates.io package dry-runs
make validate                          # check + release build

All quality targets are thin Make wrappers around `uv run python scripts/ci.py`,
so local and GitHub CI use the same implementation. The Python commands can also
be invoked directly when debugging a single matrix slice.

The `scripts/` suite is a first-class non-Rust test surface: `pytest` covers
black-box validation helpers, and `ruff` checks/lints the automation code. The
HTTP black-box probe requires a running app and is invoked with `make blackbox`.
```

Project automation/scripts are Python run via `uv run python scripts/<x>.py`
(root `pyproject.toml`), not bash.

## Containers (optional, Linux)

The `compose.yml` + `make stack-*` targets still provide a podman/Docker stack
(`make stack-up`, or `COMPOSE="docker compose" make stack-up`). The native stack
above is preferred for local development.

## Database

The Postgres index is a disposable cache. Migrations live under `migrations/`
and are applied via sqlx. The index is fully rebuildable from `miku_docs/**/*.md` —
dropping and re-migrating the database loses no user data.
