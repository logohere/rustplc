#!/usr/bin/env python3
"""Deterministic harness runner; not an OpenPLC behavior oracle."""

from __future__ import annotations

import argparse
import json
import platform
from pathlib import Path


def trace(fixture: dict, mismatch: bool) -> dict:
    state = "STOPPED"
    observations = []
    for index, event in enumerate(fixture["events"]):
        command = event["command"]
        if command == "start":
            state = "ERROR" if mismatch else "RUNNING"
        elif command == "stop":
            state = "STOPPED"
        elif command != "status":
            raise ValueError(f"unsupported self-test command: {command}")
        observations.append({"step": index, "command": command, "state": state, "diagnostics": []})
    metadata = {
        "baseline_commit": fixture["baseline_commit"],
        "os": platform.system(),
        "architecture": platform.machine(),
        **fixture["metadata"],
        "runner": "harness-self-test",
    }
    return {"metadata": metadata, "observations": observations}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("fixture", type=Path)
    parser.add_argument("--mismatch", action="store_true")
    args = parser.parse_args()
    fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
    print(json.dumps(trace(fixture, args.mismatch), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
