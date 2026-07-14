#!/usr/bin/env python3
"""Small, explicit CI matrix for Miku.

Run through uv:

    uv run python scripts/ci.py check
    uv run python scripts/ci.py check-all-features
    uv run python scripts/ci.py check-integration
    uv run python scripts/ci.py release
    uv run python scripts/ci.py check-blackbox
    uv run python scripts/ci.py benchmark

The default path never needs Postgres or Valkey. Service-backed checks are
opt-in through environment variables so local development stays lightweight.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys


def run(command: list[str]) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, check=True)


def cargo(*args: str) -> None:
    run(["cargo", *args])


def check() -> None:
    run(["cargo", "fmt", "--all", "--", "--check"])
    run(["prettier", "--check", "**/*.{md,json,yaml,yml}"])
    run(["ruff", "check", "scripts"])
    run(["ruff", "format", "--check", "scripts"])
    run(["pytest", "scripts"])
    cargo("clippy", "--workspace", "--all-targets", "--", "-D", "warnings")
    cargo("test", "--workspace")


def all_features() -> None:
    cargo("check", "--workspace", "--all-features")
    cargo("test", "--workspace", "--all-features")


def integration() -> None:
    all_features()
    database_url = os.environ.get("DATABASE_URL")
    valkey_url = os.environ.get("VALKEY_URL") or os.environ.get("REDIS_URL")
    if database_url:
        run([sys.executable, "scripts/index_scale_test.py"])
    else:
        print("skip: DATABASE_URL is not set; Postgres integration probe omitted")
    if valkey_url:
        print("info: VALKEY_URL is set; Valkey runtime smoke is covered by the app profile")
    else:
        print("skip: VALKEY_URL is not set; Valkey runtime smoke omitted")


def release() -> None:
    for package in ("miku-domain", "miku-markdown"):
        cargo("package", "--list", "-p", package)
        cargo("publish", "--dry-run", "--allow-dirty", "-p", package)


def scale() -> None:
    run([sys.executable, "scripts/index_scale_test.py"])


def blackbox() -> None:
    run([sys.executable, "scripts/blackbox.py"])


def ux_smoke() -> None:
    run([sys.executable, "scripts/ux_smoke.py"])


def validate() -> None:
    check()
    cargo("build", "--release")


COMMANDS = {
    "check": check,
    "check-all-features": all_features,
    "check-integration": integration,
    "release": release,
    "benchmark": scale,
    "check-blackbox": blackbox,
    "check-ux-smoke": ux_smoke,
    "validate": validate,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=sorted(COMMANDS))
    args = parser.parse_args()
    if shutil.which("cargo") is None:
        parser.error("cargo is not available; enter the Nix development shell first")
    try:
        COMMANDS[args.command]()
    except subprocess.CalledProcessError as error:
        return error.returncode or 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
