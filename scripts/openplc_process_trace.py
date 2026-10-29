#!/usr/bin/env python3
"""Boot pinned OpenPLC in safe mode and emit one lifecycle socket trace."""

from __future__ import annotations

import argparse
import json
import socket
import subprocess
import sys
import time
from pathlib import Path

from openplc_socket_trace import response_frame, trace


SOCKET_PATH = Path("/run/runtime/plc_runtime.socket")


def ping() -> bool:
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(1.0)
            client.connect(str(SOCKET_PATH))
            client.sendall(b"PING\n")
            return response_frame(client) == "PING:OK"
    except OSError:
        return False


def wait_ready(process: subprocess.Popen, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise ValueError(f"pinned OpenPLC exited with {process.returncode}")
        if ping():
            return
        time.sleep(0.01)
    raise ValueError("pinned OpenPLC command socket did not become ready")


def run(fixture: dict, runtime: Path, cwd: Path, timeout: float) -> dict:
    SOCKET_PATH.unlink(missing_ok=True)
    process = subprocess.Popen(
        [str(runtime), "--safe-mode"], cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT
    )
    try:
        wait_ready(process, timeout)
        return trace(fixture, str(SOCKET_PATH), timeout, True)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        SOCKET_PATH.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--cwd", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    try:
        fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
        result = run(fixture, args.runtime.resolve(), args.cwd.resolve(), args.timeout)
        print(json.dumps(result))
    except (OSError, ValueError, json.JSONDecodeError, subprocess.TimeoutExpired) as exc:
        print(f"pinned OpenPLC lifecycle trace failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
