#!/usr/bin/env python3
import hashlib
import json
import re
import subprocess
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
PRIVATE_PARTS = {".git", "target", "artifacts", "__pycache__", ".solidstate", ".codex", ".aws", ".ssh"}
RULES = {
    "machine home path": re.compile(r"(?<![A-Za-z0-9:/])/(?:Users|home)/[^/\s\"']+|[A-Za-z]:\\\\?Users\\\\?[^\\\s]+"),
    "other owner repository": re.compile(r"\blogohere/(?!rustplc(?:\.git)?(?![A-Za-z0-9_.-]))[A-Za-z0-9_.-]+"),
    "private key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----"),
    "GitHub token": re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,})\b"),
    "AWS access key": re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
    "API key": re.compile(r"\bsk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{35,}\b"),
}


def check_blob(name: str, data: bytes) -> list[str]:
    path = PurePosixPath(name)
    failures = []
    if path.is_absolute() or ".." in path.parts or any(part in PRIVATE_PARTS for part in path.parts):
        failures.append("non-publishable path")
    if any((part == ".env" or part.startswith(".env.")) and part != ".env.example" for part in path.parts):
        failures.append("environment file")
    if path.suffix.lower() in {".p12", ".pfx", ".key", ".pem", ".log"}:
        failures.append("sensitive artifact type")
    if b"\0" in data:
        return failures + ["binary requires separate publication review"]
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return failures + ["non-UTF-8 data requires separate publication review"]
    for label, pattern in RULES.items():
        if pattern.search(text) or pattern.search(name):
            failures.append(label)
    return failures


def source_paths(root: Path) -> list[Path]:
    if (root / ".git").exists():
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
            check=True, capture_output=True,
        )
        return [root / name.decode("utf-8") for name in sorted(set(result.stdout.split(b"\0"))) if name]
    return [path for path in sorted(root.rglob("*")) if path.is_file() or path.is_symlink()]


def check_vendors(root: Path) -> list[str]:
    root = root.resolve()
    failures = []
    for name, expected in json.loads((root / "docs/vendored-sha256.json").read_text()).items():
        path = root / name
        if not path.resolve().is_relative_to(root) or not path.is_file():
            failures.append(f"{name}: invalid vendored path")
        elif hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            failures.append(f"{name}: vendored bytes changed; provenance review required")
    return failures


def main() -> None:
    failures = check_vendors(ROOT)
    checked = 0
    for path in source_paths(ROOT):
        relative = path.relative_to(ROOT)
        if path.is_symlink():
            failures.append(f"{relative}: symlink requires publication review")
            continue
        if not path.resolve().is_relative_to(ROOT.resolve()):
            failures.append(f"{relative}: source resolves outside the publication root")
            continue
        if not path.is_file():
            failures.append(f"{relative}: missing or non-regular source")
            continue
        checked += 1
        failures.extend(f"{relative}: {failure}" for failure in check_blob(relative.as_posix(), path.read_bytes()))
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"Public source patterns: {checked} publishable files checked; no findings. Not a complete secret or provenance audit.")


if __name__ == "__main__":
    main()
