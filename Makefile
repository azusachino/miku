# Tools come from the nix devShell. Outside it, wrap each command in
# `nix develop --command`; inside it (IN_NIX_SHELL set), run directly.
NIX_RUN := $(if $(IN_NIX_SHELL),,nix develop --command )
PRETTIER_FILES := "**/*.{md,json,yaml,yml}"

# Container engine for the local stack. Defaults to `podman compose` (Linux
# rootless). Override for other hosts, e.g. on macOS with Docker Desktop:
#   make stack-up COMPOSE="docker compose"
# On macOS with podman, start the VM first: `podman machine init && podman machine start`.
COMPOSE ?= podman compose

.PHONY: fmt fmt-check lint test check check-all-features check-integration check-blackbox release validate benchmark inspect-index run clean daily stack-up stack-down stack-build stack-logs db-init db-up db-down db-reset db-psql dev dev-tmux

fmt:
	$(NIX_RUN)cargo fmt --all
	$(NIX_RUN)prettier --write $(PRETTIER_FILES)

fmt-check:
	$(NIX_RUN)cargo fmt --all -- --check
	$(NIX_RUN)prettier --check $(PRETTIER_FILES)

lint:
	$(NIX_RUN)cargo clippy --workspace --all-targets -- -D warnings

test:
	$(NIX_RUN)cargo test --workspace

check:
	$(NIX_RUN)uv run python scripts/ci.py check

check-all-features:
	$(NIX_RUN)uv run python scripts/ci.py check-all-features

check-integration:
	$(NIX_RUN)uv run python scripts/ci.py check-integration

check-blackbox:
	$(NIX_RUN)uv run python scripts/ci.py check-blackbox

release:
	$(NIX_RUN)uv run python scripts/ci.py release

validate:
	$(NIX_RUN)uv run python scripts/ci.py validate

benchmark:
	$(NIX_RUN)uv run python scripts/ci.py benchmark

inspect-index:
	$(NIX_RUN)cargo run -p miku-index-turso --example inspect -- "$(MIKU_INDEX_PATH)"

# --- Native (no-container) local dev stack ---------------------------------
# Runs Postgres directly from the nix devShell against a project-local cluster
# (.pgdata, gitignored) on a non-default port, then `cargo run`. No podman.
# The app runs its own embedded sqlx migrations on startup, so `make dev` is all
# you need after `make db-up`. macOS-friendly: pure processes + tmux, no VM.
PGDATA  ?= .pgdata
PGPORT  ?= 55432
PGHOST  := $(abspath $(PGDATA))
DATABASE_URL ?= postgres://miku@localhost:$(PGPORT)/miku
MIKU_INDEX_BACKEND ?= turso
MIKU_INDEX_PATH ?= miku_docs/.miku-index.turso
MIKU_RECONCILE_BATCH_SIZE ?= 512
MIKU_PARSE_CONCURRENCY ?= 8

# One-time cluster init. Superuser is `miku` and auth is trust (local dev only),
# so the DATABASE_URL needs no password and is username-agnostic across hosts.
db-init:
	@test -d $(PGDATA) || $(NIX_RUN)initdb -D $(PGDATA) \
		--username=miku --auth-local=trust --auth-host=trust --encoding=UTF8

# Start the cluster (idempotent) and ensure the miku database exists.
db-up: db-init
	@$(NIX_RUN)pg_ctl -D $(PGDATA) -l $(PGDATA)/server.log \
		-o "-p $(PGPORT) -k $(PGHOST)" -w start || true
	@$(NIX_RUN)createdb -h localhost -p $(PGPORT) -U miku miku 2>/dev/null || true

db-down:
	@$(NIX_RUN)pg_ctl -D $(PGDATA) -m fast stop || true

# The index is disposable — nuke the cluster and rebuild from miku_docs/**/*.md.
db-reset: db-down
	rm -rf $(PGDATA)

db-psql:
	$(NIX_RUN)psql "$(DATABASE_URL)"

# Start the DB (if needed) and run the server in the foreground.
dev: db-up
	MIKU_INDEX_BACKEND=postgres DATABASE_URL="$(DATABASE_URL)" $(NIX_RUN)cargo run

# Same, but in a tmux session: pane 0 = server, pane 1 = Postgres log tail.
dev-tmux: db-up
	tmux new-session -d -s miku -n miku \
		'MIKU_INDEX_BACKEND=postgres DATABASE_URL="$(DATABASE_URL)" nix develop --command cargo run'
	tmux split-window -t miku:miku -v 'tail -f $(PGDATA)/server.log'
	tmux select-pane -t miku:miku.0
	tmux attach -t miku

run:
	MIKU_INDEX_BACKEND="$(MIKU_INDEX_BACKEND)" MIKU_INDEX_PATH="$(MIKU_INDEX_PATH)" MIKU_RECONCILE_BATCH_SIZE="$(MIKU_RECONCILE_BATCH_SIZE)" MIKU_PARSE_CONCURRENCY="$(MIKU_PARSE_CONCURRENCY)" $(NIX_RUN)cargo run

clean:
	$(NIX_RUN)cargo clean

# Daily target: run quality checks and rebuild the local stack image
daily: check stack-build

# Local stack operations
stack-up:
	$(COMPOSE) up -d

stack-down:
	$(COMPOSE) down

stack-build:
	$(COMPOSE) up -d --build --force-recreate miku

stack-logs:
	$(COMPOSE) logs -f
