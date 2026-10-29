#!/usr/bin/env python3
"""Run the Rust lifecycle socket fixture and emit one normalized trace."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from openplc_socket_trace import trace


def wait_socket(path: Path, process: subprocess.Popen, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            return
        if process.poll() is not None:
            raise ValueError(f"Rust trace server exited with {process.returncode}")
        time.sleep(0.01)
    raise ValueError("Rust trace server did not create its socket")


def run(fixture: dict, server: Path, library: Path, timeout: float) -> dict:
    with tempfile.TemporaryDirectory(prefix="rustplc-trace-") as directory:
        socket_path = Path(directory) / "runtime.sock"
        process = subprocess.Popen(
            [str(server), str(socket_path), str(library)], stdout=subprocess.DEVNULL
        )
        try:
            wait_socket(socket_path, process, timeout)
            result = trace(fixture, str(socket_path), timeout, True)
            process.wait(timeout=timeout)
            return result
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", type=Path)
    parser.add_argument("--server", required=True, type=Path)
    parser.add_argument("--library", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=10.0)
    args = parser.parse_args()
    try:
        fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
        print(
            json.dumps(
                run(fixture, args.server.resolve(), args.library.resolve(), args.timeout)
            )
        )
    except (OSError, ValueError, json.JSONDecodeError, subprocess.TimeoutExpired) as exc:
        print(f"Rust lifecycle trace failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
