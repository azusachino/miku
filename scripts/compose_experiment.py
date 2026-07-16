#!/usr/bin/env python3
"""Exercise the optional PostgreSQL 18 + Valkey 9 Compose profile.

The caller owns Podman installation and VM lifecycle. This script only
controls the project stack, and always removes its containers on exit.
"""

from __future__ import annotations

import http.client
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMPOSE = os.environ.get("MIKU_COMPOSE", "podman-compose")
BASE_URL = os.environ.get("MIKU_COMPOSE_URL", "http://127.0.0.1:3000")


def run(
    command: list[str], environment: dict[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    print("+ " + " ".join(command), flush=True)
    return subprocess.run(command, cwd=ROOT, check=True, text=True, env=environment)


def request(path: str, timeout: float = 3) -> bytes:
    with urllib.request.urlopen(f"{BASE_URL}{path}", timeout=timeout) as response:
        if response.status != 200:
            raise RuntimeError(f"{path} returned HTTP {response.status}")
        return response.read()


def wait_for_api() -> None:
    deadline = time.monotonic() + 300
    while time.monotonic() < deadline:
        try:
            request("/readyz")
            print("Miku index is ready", flush=True)
            return
        except (
            http.client.RemoteDisconnected,
            ConnectionAbortedError,
            ConnectionResetError,
            urllib.error.HTTPError,
            urllib.error.URLError,
            TimeoutError,
        ):
            time.sleep(1)
    raise TimeoutError(f"Miku index did not become ready at {BASE_URL}")


def main() -> int:
    environment = os.environ.copy()
    environment.setdefault("MIKU_UID", str(os.getuid()))
    environment.setdefault("MIKU_GID", str(os.getgid()))
    started = False
    try:
        run([COMPOSE, "config"], environment)
        run([COMPOSE, "up", "-d", "--build"], environment)
        started = True
        run([COMPOSE, "ps"], environment)
        run(
            ["podman", "exec", "miku-postgres", "pg_isready", "-U", "miku", "-d", "miku"],
            environment,
        )
        run(["podman", "exec", "miku-valkey", "valkey-cli", "ping"], environment)
        wait_for_api()
        request("/api/v1/workspace", timeout=60)
        print("service experiment passed: Postgres 18 + Valkey 9 + Miku API")
        return 0
    finally:
        if started:
            subprocess.run([COMPOSE, "down"], cwd=ROOT, check=False, env=environment)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        http.client.RemoteDisconnected,
        ConnectionAbortedError,
        ConnectionResetError,
        subprocess.CalledProcessError,
        TimeoutError,
        urllib.error.HTTPError,
        urllib.error.URLError,
    ) as error:
        print(f"service experiment failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
