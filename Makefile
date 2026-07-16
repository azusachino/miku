.PHONY: dev fmt fmt-check css lint test check check-all-features check-integration experiments compose-experiments \
  check-blackbox check-ux-smoke check-ux-soak check-ux-browser benchmark \
  benchmark-real-vault release validate

dev:
	uv run python scripts/dev.py

fmt:
	uv run python scripts/orchestrate.py fmt

fmt-check:
	uv run python scripts/orchestrate.py fmt-check

css:
	bun install --frozen-lockfile
	bun run css

lint:
	uv run python scripts/orchestrate.py lint

test:
	uv run python scripts/orchestrate.py test

check:
	uv run python scripts/orchestrate.py check

check-all-features:
	uv run python scripts/orchestrate.py check-all-features

check-integration:
	uv run python scripts/orchestrate.py check-integration

experiments:
	uv run python scripts/orchestrate.py experiments

compose-experiments:
	uv run python scripts/orchestrate.py compose-experiments

check-blackbox:
	MIKU_UX_AUTOSTART=1 uv run python scripts/orchestrate.py check-blackbox

check-ux-smoke:
	MIKU_UX_AUTOSTART=1 uv run python scripts/orchestrate.py check-ux-smoke

check-ux-soak:
	MIKU_UX_AUTOSTART=1 uv run python scripts/orchestrate.py check-ux-soak

check-ux-browser:
	MIKU_UX_AUTOSTART=1 uv run python scripts/orchestrate.py check-ux-browser

benchmark:
	uv run python scripts/orchestrate.py benchmark

benchmark-real-vault:
	MIKU_BENCHMARK_VAULT="$(CURDIR)/miku_docs" cargo test -p miku --release --lib -- --ignored --nocapture benchmark_real_vault_reconcile

release:
	uv run python scripts/orchestrate.py release

validate:
	uv run python scripts/orchestrate.py validate
