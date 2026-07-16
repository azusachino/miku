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
import time
import urllib.error
import urllib.request


def run(
    command: list[str],
    environment: dict[str, str] | None = None,
    cwd: str | None = None,
) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, check=True, env=environment, cwd=cwd)


def server_ready(url: str) -> bool:
    try:
        with urllib.request.urlopen(f"{url}/healthz", timeout=1) as response:
            return response.status == 200
    except (urllib.error.URLError, TimeoutError):
        return False


def cargo(*args: str) -> None:
    run(["cargo", *args])


def check() -> None:
    run(["bun", "install", "--frozen-lockfile"], cwd="miku-web")
    run(["bun", "run", "check"], cwd="miku-web")
    run(["bun", "run", "build"], cwd="miku-web")
    run(["cargo", "fmt", "--all", "--", "--check"])
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


# Real publishing order: leaf → root. `cargo publish` requires the whole non-dev
# dependency closure (including the optional postgres/valkey backends) to already
# be on crates.io, so a real release walks this list top-to-bottom, one crate at
# a time, waiting for each to land on the index before the next.
PUBLISH_ORDER = (
    "miku-domain",
    "miku-markdown",
    "miku-indexer",
    "miku-index-memory",
    "miku-index-sqlite",
    "miku-index-postgres",
    "miku-cache-valkey",
    "miku-app",
    "miku",
)

# Only the leaves (no internal path deps) can be dry-run before publishing:
# `cargo publish --dry-run` resolves every dependency against the crates.io index
# during packaging, so a crate whose deps are not yet published cannot be
# validated ahead of time. This target packages the leaves as a metadata/include
# pre-flight; the rest are covered by the real publish and by `make check`.
DRY_RUN_LEAVES = ("miku-domain", "miku-markdown")


def release() -> None:
    for package in DRY_RUN_LEAVES:
        cargo("publish", "--dry-run", "--allow-dirty", "-p", package)
    print("publish order (real release, leaf → root): " + " -> ".join(PUBLISH_ORDER))


def scale() -> None:
    run([sys.executable, "scripts/index_scale_test.py"])


def blackbox() -> None:
    run_ux_script("scripts/blackbox.py")


def ux_smoke() -> None:
    run_ux_script("scripts/ux_smoke.py")


def ux_soak() -> None:
    run_ux_script("scripts/ux_soak.py")


def ux_browser() -> None:
    run_ux_script("scripts/ux_browser.py")


def run_ux_script(script: str) -> None:
    environment = os.environ.copy()
    base_url = environment.get("MIKU_BLACKBOX_URL", "http://127.0.0.1:3000").rstrip("/")
    if server_ready(base_url):
        run([sys.executable, script], environment)
        return

    if environment.get("MIKU_UX_AUTOSTART") != "1":
        run([sys.executable, script], environment)
        return

    port = environment.get("MIKU_UX_AUTOSTART_PORT", "3001")
    base_url = f"http://127.0.0.1:{port}"
    environment["MIKU_BIND"] = f"127.0.0.1:{port}"
    environment["MIKU_BLACKBOX_URL"] = base_url
    print(f"+ cargo run (default features) on {base_url}", flush=True)
    server = subprocess.Popen(["cargo", "run"], env=environment)
    try:
        deadline = time.monotonic() + 60
        while not server_ready(base_url):
            if server.poll() is not None:
                raise subprocess.CalledProcessError(server.returncode or 1, ["cargo", "run"])
            if time.monotonic() >= deadline:
                raise TimeoutError(f"Miku did not become ready at {base_url}")
            time.sleep(0.5)
        run([sys.executable, script], environment)
    finally:
        if server.poll() is None:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()


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
    "check-ux-soak": ux_soak,
    "check-ux-browser": ux_browser,
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
