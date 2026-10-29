#!/usr/bin/env python3
"""Compare generated debug I/O through pinned OpenPLC or RustPLC."""

from __future__ import annotations

import argparse
import json
import platform
import socket
import subprocess
import sys
import time
from pathlib import Path

from openplc_process_trace import SOCKET_PATH, wait_ready
from openplc_socket_trace import response_frame, settle_state


def request(client: socket.socket, command: str) -> str:
    client.sendall((command + "\n").encode("ascii"))
    return response_frame(client)


def read_bool(response: str) -> bool:
    if not response.startswith("DEBUG:"):
        raise ValueError(f"unexpected debug response: {response}")
    frame = bytes(int(token, 16) for token in response[6:].split())
    if len(frame) != 11 or frame[:2] != b"\x43\x7e" or int.from_bytes(frame[8:10], "big") != 1:
        raise ValueError(f"malformed DEBUG_GET response: {response}")
    return frame[10] != 0


def wait_output(client: socket.socket, frame: str, expected: bool, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if read_bool(request(client, f"DEBUG:{frame}")) is expected:
            return
        time.sleep(0.01)
    raise ValueError(f"generated output did not become {expected}")


def upstream(fixture: dict, runtime: Path, cwd: Path, timeout: float) -> dict:
    scenario = fixture["scenario"]
    expected = fixture["expected"]
    SOCKET_PATH.unlink(missing_ok=True)
    process = subprocess.Popen(
        [str(runtime), "--safe-mode"], cwd=cwd, stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT
    )
    try:
        wait_ready(process, timeout)
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(timeout)
            client.connect(str(SOCKET_PATH))
            if request(client, "START") != "START:OK":
                raise ValueError("pinned OpenPLC refused START")
            settle_state(client, "RUNNING", timeout)
            expect_ack(client, scenario["force_frame"])
            wait_output(client, scenario["read_frame"], expected["forced_output"], timeout)
            expect_ack(client, scenario["unforce_frame"])
            wait_output(client, scenario["read_frame"], expected["released_output"], timeout)
        return dict(expected)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        SOCKET_PATH.unlink(missing_ok=True)


def expect_ack(client: socket.socket, frame: str) -> None:
    response = request(client, f"DEBUG:{frame}")
    if response != "DEBUG:42 7e":
        raise ValueError(f"debug write rejected: {response}")


def rust(fixture: dict, executable: Path, library: Path) -> dict:
    scenario = fixture["scenario"]
    result = subprocess.run(
        [str(executable), str(library), scenario["force_frame"], scenario["read_frame"],
         scenario["unforce_frame"]],
        check=True, capture_output=True, text=True, timeout=10,
    )
    return json.loads(result.stdout)


def trace(fixture: dict, observation: dict, runner: str) -> dict:
    if observation != fixture["expected"]:
        raise ValueError(f"unexpected generated I/O result: {observation}")
    metadata = {
        "baseline_commit": fixture["baseline_commit"],
        "os": platform.system(),
        "architecture": platform.machine(),
        **fixture["metadata"],
        "runner": runner,
    }
    observations = [
        {"operation": "force", "status": "accepted"},
        {"operation": "read", "output": observation["forced_output"]},
        {"operation": "unforce", "status": "accepted"},
        {"operation": "read", "output": observation["released_output"]},
    ]
    return {"metadata": metadata, "observations": observations}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--runtime", type=Path)
    mode.add_argument("--executable", type=Path)
    parser.add_argument("--cwd", type=Path)
    parser.add_argument("--library", type=Path)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    try:
        fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
        if args.runtime:
            if not args.cwd:
                raise ValueError("--cwd is required with --runtime")
            result = upstream(fixture, args.runtime.resolve(), args.cwd.resolve(), args.timeout)
            output = trace(fixture, result, "pinned-openplc-generated-debug-io")
        else:
            if not args.library:
                raise ValueError("--library is required with --executable")
            result = rust(fixture, args.executable.resolve(), args.library.resolve())
            output = trace(fixture, result, "rustplc-generated-debug-io")
        print(json.dumps(output))
    except (OSError, ValueError, KeyError, json.JSONDecodeError, subprocess.SubprocessError) as exc:
        print(f"generated debug I/O trace failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
