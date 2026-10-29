#!/usr/bin/env python3
"""Run the Rust task-fault fixture and emit normalized evidence."""

from __future__ import annotations

import argparse
import json
import platform
import subprocess
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", type=Path)
    parser.add_argument("--executable", required=True, type=Path)
    args = parser.parse_args()
    try:
        fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
        run = subprocess.run(
            [str(args.executable.resolve())], check=True, capture_output=True, text=True, timeout=10
        )
        observation = json.loads(run.stdout)
        metadata = {
            "baseline_commit": fixture["baseline_commit"],
            "os": platform.system(),
            "architecture": platform.machine(),
            **fixture["metadata"],
            "runner": "rustplc-task-fault",
        }
        print(json.dumps({"metadata": metadata, "observations": [observation]}))
    except (OSError, ValueError, KeyError, json.JSONDecodeError, subprocess.SubprocessError) as exc:
        print(f"Rust task-fault trace failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
