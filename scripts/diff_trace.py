#!/usr/bin/env python3
"""Run two trace emitters and fail on an unexplained observable difference."""

from __future__ import annotations

import argparse
import json
import shlex
import subprocess
import sys
from pathlib import Path


REQUIRED_METADATA = {"baseline_commit", "os", "architecture", "clock", "scheduler", "build_mode"}


def command_args(command: str, fixture: Path) -> list[str]:
    tokens = shlex.split(command)
    if "{fixture}" not in tokens:
        raise ValueError("runner command must contain a {fixture} token")
    return [str(fixture) if token == "{fixture}" else token for token in tokens]


def run(command: str, fixture: Path, label: str) -> dict:
    try:
        result = subprocess.run(
            command_args(command, fixture), check=True, capture_output=True, text=True, timeout=120
        )
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
        detail = getattr(exc, "stderr", "").strip()
        raise ValueError(f"{label} runner failed: {detail or exc}") from exc
    try:
        trace = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise ValueError(f"{label} runner did not emit one JSON trace: {exc}") from exc
    if not isinstance(trace, dict):
        raise ValueError(f"{label} trace must be an object")
    return trace


def normalize(trace: dict, fixture: dict, label: str) -> dict:
    metadata = trace.get("metadata")
    observations = trace.get("observations")
    if not isinstance(metadata, dict) or not isinstance(observations, list):
        raise ValueError(f"{label} trace needs metadata and observations")
    missing = REQUIRED_METADATA - metadata.keys()
    if missing:
        raise ValueError(f"{label} trace missing metadata: {', '.join(sorted(missing))}")
    if metadata["baseline_commit"] != fixture["baseline_commit"]:
        raise ValueError(f"{label} trace uses the wrong baseline commit")
    cleaned = {key: value for key, value in metadata.items() if key != "runner"}
    volatile = set(fixture.get("volatile_fields", []))
    cleaned_observations = []
    for observation in observations:
        if not isinstance(observation, dict):
            raise ValueError(f"{label} observations must be objects")
        cleaned_observations.append({key: value for key, value in observation.items() if key not in volatile})
    return {"metadata": cleaned, "observations": cleaned_observations}


def difference(left: object, right: object, path: str = "trace") -> str | None:
    if type(left) is not type(right):
        return f"{path}: {left!r} != {right!r}"
    if isinstance(left, dict):
        for key in sorted(set(left) | set(right)):
            if key not in left or key not in right:
                return f"{path}.{key}: field presence differs"
            result = difference(left[key], right[key], f"{path}.{key}")
            if result:
                return result
    elif isinstance(left, list):
        if len(left) != len(right):
            return f"{path}: length {len(left)} != {len(right)}"
        for index, (item_left, item_right) in enumerate(zip(left, right)):
            result = difference(item_left, item_right, f"{path}[{index}]")
            if result:
                return result
    elif left != right:
        return f"{path}: {left!r} != {right!r}"
    return None


def save_failure(directory: Path, fixture: dict, upstream: dict, rust: dict, detail: str) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    output = {"fixture": fixture.get("name", "unnamed"), "difference": detail, "upstream": upstream, "rust": rust}
    (directory / f"{fixture.get('name', 'trace')}.json").write_text(
        json.dumps(output, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def self_test_args(args: argparse.Namespace) -> tuple[str, str]:
    runner = Path(__file__).with_name("trace_fixture_runner.py")
    base = f"{shlex.quote(sys.executable)} {shlex.quote(str(runner))} {{fixture}}"
    rust = f"{base} --mismatch" if args.expect_mismatch else base
    return base, rust


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", nargs="?", type=Path, default=Path("fixtures/lifecycle-basic.json"))
    parser.add_argument("--upstream-cmd")
    parser.add_argument("--rust-cmd")
    parser.add_argument("--artifact-dir", type=Path, default=Path("artifacts/differential"))
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--expect-mismatch", action="store_true")
    args = parser.parse_args()
    try:
        fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
        if args.self_test:
            upstream_cmd, rust_cmd = self_test_args(args)
        elif args.upstream_cmd and args.rust_cmd:
            upstream_cmd, rust_cmd = args.upstream_cmd, args.rust_cmd
        else:
            raise ValueError("provide --upstream-cmd and --rust-cmd, or use --self-test")
        upstream = normalize(run(upstream_cmd, args.fixture, "upstream"), fixture, "upstream")
        rust = normalize(run(rust_cmd, args.fixture, "rust"), fixture, "rust")
        detail = difference(upstream, rust)
        if detail and args.expect_mismatch:
            save_failure(args.artifact_dir, fixture, upstream, rust, detail)
            print(f"expected mismatch captured: {detail}")
            return 0
        if detail:
            save_failure(args.artifact_dir, fixture, upstream, rust, detail)
            print(f"trace mismatch: {detail}", file=sys.stderr)
            return 1
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"differential trace failed: {exc}", file=sys.stderr)
        return 1
    print(f"trace parity passed: {fixture.get('name', 'unnamed')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
