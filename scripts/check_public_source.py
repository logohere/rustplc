#!/usr/bin/env python3
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXCLUDE = {".git", "target", "artifacts", "__pycache__"}
RULES = {
    "machine home path": re.compile(r"(?<![A-Za-z0-9:/])/(?:Users|home)/[^/\s\"']+|[A-Za-z]:\\\\?Users\\\\?[^\\\s]+"),
    "other owner repository": re.compile(r"\blogohere/(?!rustplc(?:\.git)?(?![A-Za-z0-9_.-]))[A-Za-z0-9_.-]+"),
    "private key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----"),
    "GitHub token": re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,})\b"),
    "AWS access key": re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
    "API key": re.compile(r"\bsk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{35,}\b"),
}


def main() -> None:
    failures = []
    checked = 0
    for name, expected in json.loads((ROOT / "docs/vendored-sha256.json").read_text()).items():
        path = ROOT / name
        if not path.resolve().is_relative_to(ROOT) or not path.is_file():
            failures.append(f"{name}: invalid vendored path")
        elif hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            failures.append(f"{name}: vendored bytes changed; provenance review required")
    for path in sorted(ROOT.rglob("*")):
        relative = path.relative_to(ROOT)
        if any(part in EXCLUDE for part in relative.parts):
            continue
        if path.is_symlink():
            failures.append(f"{relative}: symlink requires publication review")
            continue
        if not path.is_file():
            continue
        if (path.name == ".env" or path.name.startswith(".env.")) and path.name != ".env.example":
            failures.append(f"{relative}: environment file")
        if path.suffix in {".p12", ".pfx", ".key", ".pem", ".log"}:
            failures.append(f"{relative}: sensitive artifact type")
        data = path.read_bytes()
        if b"\0" in data:
            failures.append(f"{relative}: binary requires separate publication review")
            continue
        text = data.decode("utf-8")
        checked += 1
        for name, pattern in RULES.items():
            if pattern.search(text):
                failures.append(f"{relative}: {name}")
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"Public source patterns: {checked} text files checked; no matches. Not a complete secret or provenance audit.")


if __name__ == "__main__":
    main()
