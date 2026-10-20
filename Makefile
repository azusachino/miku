.PHONY: dev fmt fmt-check css lint test kb-check check check-all-features check-integration experiments compose-experiments \
  check-blackbox e2e-soak benchmark \
  benchmark-real-vault release validate

# Browser checks from the playwright-verify skill: make e2e, e2e-probe, e2e-install.
E2E_DIR := miku-web
include e2e.mk

# The timed API soak runs only on request (MIKU_UX_SOAK_SECONDS sets its length).
e2e-soak:
	cd $(E2E_DIR) && bunx playwright test --project=soak $(ARGS)

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

kb-check:
	uv run scripts/check_kb_conventions.py

check: kb-check
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

benchmark:
	uv run python scripts/orchestrate.py benchmark

benchmark-api:
	uv run python scripts/api_benchmark.py

benchmark-real-vault:
	MIKU_BENCHMARK_VAULT="$(CURDIR)/miku_docs" cargo test -p miku --release --lib -- --ignored --nocapture benchmark_real_vault_reconcile

benchmark-real-vault-search:
	MIKU_BENCHMARK_VAULT="$(CURDIR)/miku_docs" cargo test -p miku --release --lib -- --ignored --nocapture benchmark_real_vault_search

release:
	uv run python scripts/orchestrate.py release

validate:
	uv run python scripts/orchestrate.py validate
