#!/usr/bin/env python3
"""Smoke the runnable RustPLC process with a pinned generated library."""

from __future__ import annotations

import argparse
import json
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from openplc_socket_trace import response_frame, settle_state


def connect(process: subprocess.Popen, path: Path, timeout: float) -> socket.socket:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise ValueError(f"rustplc exited with {process.returncode}")
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.settimeout(timeout)
        try:
            client.connect(str(path))
            client.sendall(b"PING\n")
            if response_frame(client) == "PING:OK":
                return client
            client.close()
        except OSError:
            client.close()
        time.sleep(0.01)
    raise ValueError("rustplc command socket did not become ready")


def run_mode(binary: Path, library: Path, safe: bool, timeout: float) -> str:
    with tempfile.TemporaryDirectory(prefix="rustplc-app-") as directory:
        path = Path(directory) / "runtime.sock"
        command = [str(binary), "--program", str(library), "--socket", str(path)]
        if safe:
            command.append("--safe-mode")
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            with connect(process, path, timeout) as client:
                settle_state(client, "STOPPED" if safe else "RUNNING", timeout)
                if not safe:
                    client.sendall(b"STATS\n")
                    if '"name":"MAIN_TASK"' not in response_frame(client):
                        raise ValueError("running app did not publish MAIN_TASK statistics")
            process.terminate()
            stdout, stderr = process.communicate(timeout=timeout)
            if process.returncode != 0:
                raise ValueError(f"rustplc shutdown failed: {stderr.strip()}")
            if path.exists():
                raise ValueError("rustplc left its command socket behind")
            return stdout.strip()
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--library", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=10.0)
    args = parser.parse_args()
    try:
        version = subprocess.run(
            [str(args.binary.resolve()), "--version"], check=True, capture_output=True, text=True
        ).stdout.strip()
        result = {
            "version": version,
            "running": run_mode(args.binary.resolve(), args.library.resolve(), False, args.timeout),
            "safe_mode": run_mode(args.binary.resolve(), args.library.resolve(), True, args.timeout),
        }
        print(json.dumps(result, sort_keys=True))
        return 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"RustPLC app smoke failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
