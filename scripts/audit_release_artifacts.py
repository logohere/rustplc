#!/usr/bin/env python3
import argparse
from pathlib import Path

from check_public_source import PRIVATE_PARTS, RULES


def audit(root: Path) -> list[str]:
    failures = []
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if path.is_symlink():
            failures.append(f"{relative}: symlink requires artifact review")
            continue
        if not path.is_file():
            continue
        if any(part in PRIVATE_PARTS for part in relative.parts) or path.name == ".DS_Store":
            failures.append(f"{relative}: private/build metadata")
        if path.name == ".env" or path.name.startswith(".env.") or path.suffix.lower() in {".p12", ".pfx", ".key", ".pem", ".log"}:
            failures.append(f"{relative}: sensitive artifact type")
        data = path.read_bytes()
        for encoding in ("utf-8", "utf-16-le", "utf-16-be"):
            text = data.decode(encoding, errors="ignore")
            for label, pattern in RULES.items():
                if pattern.search(text):
                    failures.append(f"{relative}: {label}")
    return sorted(set(failures))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle", type=Path)
    args = parser.parse_args()
    if not args.bundle.is_dir() or args.bundle.is_symlink():
        raise SystemExit("artifact audit requires a real bundle directory")
    failures = audit(args.bundle)
    if failures:
        raise SystemExit("\n".join(failures))
    print("Artifact privacy patterns: no findings; signing and actual UI acceptance remain separate gates.")


if __name__ == "__main__":
    main()
