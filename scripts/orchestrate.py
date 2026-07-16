#!/usr/bin/env python3
"""Project orchestration entry point; run it through ``uv``."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from ci import (
    all_features,
    blackbox,
    check,
    integration,
    release,
    scale,
    ux_browser,
    ux_smoke,
    ux_soak,
    validate,
)

ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], cwd: Path = ROOT) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def format_code() -> None:
    run(["cargo", "fmt", "--all"])
    run([sys.executable, "-m", "ruff", "format", "scripts"])
    run(["bun", "run", "format"], cwd=ROOT / "miku-web")


def format_check() -> None:
    run(["cargo", "fmt", "--all", "--", "--check"])
    run([sys.executable, "-m", "ruff", "format", "--check", "scripts"])
    run(["bun", "run", "format:check"], cwd=ROOT / "miku-web")


def lint() -> None:
    run(["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])


def test() -> None:
    run(["cargo", "test", "--workspace"])


def experiments() -> None:
    """Run isolated model proofs and the real-vault benchmark."""
    run([sys.executable, "experiments/hybrid_projection_probe.py"])
    run([sys.executable, "experiments/compare_vault_models.py", "--root", "miku_docs"])
    all_features()


def compose_experiments() -> None:
    """Run the opt-in PostgreSQL 18 + Valkey 9 Compose experiment."""
    run([sys.executable, "scripts/compose_experiment.py"])


COMMANDS = {
    "fmt": format_code,
    "fmt-check": format_check,
    "lint": lint,
    "test": test,
    "check": check,
    "check-all-features": all_features,
    "check-integration": integration,
    "experiments": experiments,
    "compose-experiments": compose_experiments,
    "benchmark": scale,
    "check-blackbox": blackbox,
    "check-ux-smoke": ux_smoke,
    "check-ux-soak": ux_soak,
    "check-ux-browser": ux_browser,
    "release": release,
    "validate": validate,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=sorted(COMMANDS))
    args = parser.parse_args()
    COMMANDS[args.command]()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
