#!/usr/bin/env python3
"""Start the native Miku development stack.

Run with:

    uv run python scripts/dev.py

The Rust server owns the vault, watcher, and in-memory index. Vite owns the
browser development server and proxies `/api` and `/events` to Rust.
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BACKEND_URL = "http://127.0.0.1:3000"
FRONTEND_URL = "http://127.0.0.1:5173"


def command_available(command: str) -> None:
    if shutil.which(command) is None:
        raise SystemExit(f"missing required command: {command}")


def spawn(
    command: list[str], environment: dict[str, str], working_directory: Path
) -> subprocess.Popen[bytes]:
    print("+ " + " ".join(command), flush=True)
    return subprocess.Popen(
        command,
        cwd=working_directory,
        env=environment,
        start_new_session=(os.name == "posix"),
    )


def stop(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    if os.name == "posix":
        os.killpg(process.pid, signal.SIGTERM)
    else:
        process.terminate()


def main() -> int:
    command_available("cargo")
    command_available("bun")

    environment = os.environ.copy()
    environment.setdefault("MIKU_INDEX_BACKEND", "sqlite")
    environment.setdefault("MIKU_BIND", "0.0.0.0:3000")
    environment.setdefault("MIKU_READONLY", "0")

    backend = spawn(["cargo", "run", "-p", "miku"], environment, ROOT)
    frontend = spawn(["bun", "run", "dev"], environment, ROOT / "miku-web")
    processes = [backend, frontend]

    print(f"Miku backend:  {BACKEND_URL}", flush=True)
    print(f"Miku frontend: {FRONTEND_URL}", flush=True)
    print("Press Ctrl-C to stop both processes.", flush=True)

    def shutdown(_signum: int, _frame: object) -> None:
        for process in processes:
            stop(process)

    signal.signal(signal.SIGINT, shutdown)
    signal.signal(signal.SIGTERM, shutdown)

    try:
        while True:
            exited = next((process for process in processes if process.poll() is not None), None)
            if exited is not None:
                return exited.returncode or 0
            time.sleep(0.25)
    except KeyboardInterrupt:
        shutdown(signal.SIGINT, None)
        return 130
    finally:
        for process in processes:
            stop(process)
        for process in processes:
            process.wait()


if __name__ == "__main__":
    sys.exit(main())
