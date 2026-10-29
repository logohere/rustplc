#!/usr/bin/env python3
"""Compare stable generated-task STATS observations."""

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


TIMING_FIELDS = (
    "scan_time_min",
    "scan_time_max",
    "scan_time_avg",
    "cycle_time_min",
    "cycle_time_max",
    "cycle_time_avg",
    "cycle_latency_min",
    "cycle_latency_max",
    "cycle_latency_avg",
)


def request(client: socket.socket, command: str) -> str:
    client.sendall((command + "\n").encode("ascii"))
    return response_frame(client)


def task_observations(response: str) -> list[dict]:
    if not response.startswith("STATS:"):
        raise ValueError(f"invalid STATS response: {response}")
    payload = json.loads(response[6:])
    tasks = payload.get("tasks")
    if not isinstance(tasks, list):
        raise ValueError("STATS tasks must be an array")
    observations = []
    for task in tasks:
        if not isinstance(task, dict) or not isinstance(task.get("name"), str):
            raise ValueError("STATS task needs a name")
        count = task.get("scan_count")
        overruns = task.get("overruns")
        observations.append(
            {
                "name": task["name"],
                "scan_count_positive": type(count) is int and count > 0,
                "timing_fields_numeric": all(type(task.get(key)) is int for key in TIMING_FIELDS),
                "overruns_nonnegative": type(overruns) is int and overruns >= 0,
            }
        )
    return observations


def wait_live_stats(client: socket.socket, timeout: float) -> list[dict]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        tasks = task_observations(request(client, "STATS"))
        if tasks and all(task["scan_count_positive"] for task in tasks):
            return tasks
        time.sleep(0.01)
    raise ValueError("generated STATS did not report a live task")


def upstream(runtime: Path, cwd: Path, timeout: float) -> list[dict]:
    SOCKET_PATH.unlink(missing_ok=True)
    process = subprocess.Popen(
        [str(runtime), "--safe-mode"], cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT
    )
    try:
        wait_ready(process, timeout)
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(timeout)
            client.connect(str(SOCKET_PATH))
            if request(client, "START") != "START:OK":
                raise ValueError("pinned OpenPLC rejected START")
            settle_state(client, "RUNNING", timeout)
            running = wait_live_stats(client, timeout)
            if request(client, "STOP") != "STOP:OK":
                raise ValueError("pinned OpenPLC rejected STOP")
            settle_state(client, "STOPPED", timeout)
            stopped = task_observations(request(client, "STATS"))
        return [{"state": "running", "tasks": running}, {"state": "stopped", "tasks": stopped}]
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        SOCKET_PATH.unlink(missing_ok=True)


def rust(executable: Path, library: Path, timeout: float) -> list[dict]:
    result = subprocess.run(
        [str(executable), str(library)], capture_output=True, text=True, timeout=timeout, check=True
    )
    lines = [line for line in result.stdout.splitlines() if line]
    if len(lines) != 2:
        raise ValueError("Rust generated STATS trace needs two responses")
    return [
        {"state": "running", "tasks": task_observations(lines[0])},
        {"state": "stopped", "tasks": task_observations(lines[1])},
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", type=Path)
    parser.add_argument("--runtime", type=Path)
    parser.add_argument("--cwd", type=Path)
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--library", type=Path)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    try:
        fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
        baseline = fixture.get("baseline_commit")
        if not isinstance(baseline, str) or len(baseline) != 40:
            raise ValueError("fixture baseline_commit must be a 40-character SHA")
        if args.runtime and args.cwd and not args.executable and not args.library:
            observations = upstream(args.runtime.resolve(), args.cwd.resolve(), args.timeout)
            runner = "pinned-openplc-generated-stats"
        elif args.executable and args.library and not args.runtime and not args.cwd:
            observations = rust(args.executable.resolve(), args.library.resolve(), args.timeout)
            runner = "rustplc-generated-stats"
        else:
            raise ValueError("select runtime/cwd or executable/library")
        metadata = dict(fixture["metadata"])
        metadata.update(
            {
                "baseline_commit": baseline,
                "os": platform.system(),
                "architecture": platform.machine(),
                "runner": runner,
            }
        )
        print(json.dumps({"metadata": metadata, "observations": observations}))
    except (OSError, ValueError, json.JSONDecodeError, subprocess.SubprocessError) as exc:
        print(f"generated STATS trace failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
