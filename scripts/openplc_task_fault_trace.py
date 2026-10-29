#!/usr/bin/env python3
"""Run a real pinned OpenPLC task fault and emit normalized evidence."""

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

UNSET_SCAN_TIME = (1 << 63) - 1


def request(client: socket.socket, command: str) -> str:
    client.sendall((command + "\n").encode("ascii"))
    return response_frame(client)


def observe(client: socket.socket, fixture: dict, timeout: float) -> dict:
    if request(client, "START") != "START:OK":
        raise ValueError("pinned OpenPLC refused START")
    settle_state(client, "RUNNING", timeout)
    scenario = fixture["scenario"]
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        response = request(client, "STATS")
        if not response.startswith("STATS:"):
            raise ValueError(f"unexpected STATS response: {response}")
        tasks = {task["name"]: task for task in json.loads(response[6:])["tasks"]}
        fault = tasks.get(scenario["fault_task"])
        healthy = tasks.get(scenario["healthy_task"])
        healthy_done = healthy is not None and healthy["scan_time_min"] != UNSET_SCAN_TIME
        if fault is not None and healthy_done:
            if fault["scan_count"] != 1 or fault["scan_time_min"] != UNSET_SCAN_TIME:
                raise ValueError("fault task timing does not show one uncompleted scan")
            state = request(client, "STATUS").removeprefix("STATUS:")
            return {"state": state, "fault_task": "isolated", "healthy_task": "scanning"}
    raise ValueError("healthy task made no scan progress before timeout")


def run(fixture: dict, runtime: Path, cwd: Path, timeout: float) -> dict:
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
            observation = observe(client, fixture, timeout)
        metadata = {
            "baseline_commit": fixture["baseline_commit"],
            "os": platform.system(),
            "architecture": platform.machine(),
            **fixture["metadata"],
            "runner": "pinned-openplc-task-fault",
        }
        return {"metadata": metadata, "observations": [observation]}
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
        print(json.dumps(run(fixture, args.runtime.resolve(), args.cwd.resolve(), args.timeout)))
    except (OSError, ValueError, KeyError, json.JSONDecodeError, subprocess.SubprocessError) as exc:
        print(f"pinned OpenPLC task-fault trace failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
