#!/usr/bin/env python3
"""Capture raw replies from a running pinned OpenPLC command socket."""

from __future__ import annotations

import argparse
import json
import platform
import socket
import sys
import time
from pathlib import Path


MAX_RESPONSE_SIZE = 65536
COMMANDS = {
    "ping": "PING",
    "status": "STATUS",
    "switch": "SWITCH",
    "stats": "STATS",
    "start": "START",
    "stop": "STOP",
}
FINAL_STATES = {"RUNNING", "STOPPED", "ERROR", "EMPTY"}


def response_frame(client: socket.socket) -> str:
    data = bytearray()
    while len(data) < MAX_RESPONSE_SIZE:
        chunk = client.recv(min(4096, MAX_RESPONSE_SIZE - len(data)))
        if not chunk:
            raise ValueError("runtime closed the socket before a newline response")
        data.extend(chunk)
        newline = data.find(b"\n")
        if newline >= 0:
            return bytes(data[:newline]).decode("ascii")
    raise ValueError("runtime response exceeded 65536 bytes")


def prepared_events(events: list, allow_state_change: bool) -> list[tuple[int, str, str, str | None]]:
    prepared = []
    for step, event in enumerate(events):
        if not isinstance(event, dict) or not isinstance(event.get("command"), str):
            raise ValueError(f"event {step} has no command")
        name = event["command"].lower()
        command = COMMANDS.get(name)
        if command is None:
            raise ValueError(f"unsupported command in adapter: {name}")
        if name in {"start", "stop"} and not allow_state_change:
            raise ValueError("START/STOP require --allow-state-change")
        settle = event.get("settle")
        if settle is not None and (name not in {"start", "stop"} or settle not in FINAL_STATES):
            raise ValueError(f"invalid settlement state at event {step}")
        prepared.append((step, name, command, settle))
    return prepared


def settle_state(client: socket.socket, expected: str, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    wanted = f"STATUS:{expected}"
    while time.monotonic() < deadline:
        client.sendall(b"STATUS\n")
        response = response_frame(client)
        if response == wanted:
            return
        if response != "STATUS:TRANSITIONING":
            raise ValueError(f"transition landed on {response}, expected {wanted}")
        time.sleep(0.01)
    raise ValueError(f"transition did not reach {wanted} within {timeout:g}s")


def trace(fixture: dict, socket_path: str, timeout: float, allow_state_change: bool) -> dict:
    if not hasattr(socket, "AF_UNIX"):
        raise ValueError("OpenPLC command adapter requires Unix-domain sockets")
    metadata = fixture.get("metadata")
    if not isinstance(metadata, dict):
        raise ValueError("fixture metadata must be an object")
    events = fixture.get("events")
    if not isinstance(events, list):
        raise ValueError("fixture events must be a list")
    baseline = fixture.get("baseline_commit")
    if not isinstance(baseline, str) or len(baseline) != 40:
        raise ValueError("fixture baseline_commit must be a 40-character SHA")
    prepared = prepared_events(events, allow_state_change)
    observations = []
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(timeout)
        client.connect(socket_path)
        for step, name, command, settle in prepared:
            client.sendall((command + "\n").encode("ascii"))
            observations.append(
                {"step": step, "command": name, "response": response_frame(client)}
            )
            if settle is not None:
                settle_state(client, settle, timeout)
    result_metadata = dict(metadata)
    result_metadata.update(
        {"baseline_commit": baseline, "os": platform.system(), "architecture": platform.machine()}
    )
    return {
        "metadata": result_metadata,
        "observations": observations,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", type=Path)
    parser.add_argument("--socket", required=True, dest="socket_path")
    parser.add_argument("--timeout", type=float, default=5.0)
    parser.add_argument("--allow-state-change", action="store_true")
    args = parser.parse_args()
    try:
        fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
        if not isinstance(fixture, dict):
            raise ValueError("fixture must be an object")
        print(json.dumps(trace(fixture, args.socket_path, args.timeout, args.allow_state_change)))
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"OpenPLC socket trace failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
