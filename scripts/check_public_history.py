#!/usr/bin/env python3
import argparse
import re
import subprocess
from pathlib import Path

from check_public_source import RULES, check_blob

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_EMAIL = re.compile(r"(?:[^@\s]+@users\.noreply\.github\.com|(?:noreply|support)@github\.com)", re.IGNORECASE)
EMAIL = re.compile(r"[A-Za-z0-9_.+\[\]-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")


def git(root: Path, *args: str) -> bytes:
    return subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True).stdout


def metadata_failures(root: Path, commit: str) -> list[str]:
    fields = git(root, "show", "--no-patch", "--format=%an%x00%ae%x00%cn%x00%ce%x00%B", commit).decode("utf-8").split("\0", 4)
    failures = []
    for email in (fields[1], fields[3]):
        if email and not PUBLIC_EMAIL.fullmatch(email):
            failures.append("commit email requires a public noreply identity")
    for address in EMAIL.findall("\n".join(fields)):
        if not PUBLIC_EMAIL.fullmatch(address):
            failures.append("commit metadata email requires a public noreply identity")
    for label, pattern in RULES.items():
        if pattern.search("\n".join(fields)):
            failures.append(f"commit metadata: {label}")
    return failures


def history_failures(root: Path, revisions: list[str]) -> tuple[list[str], int, int]:
    commits = git(root, "rev-list", "--reverse", *revisions).decode().splitlines()
    failures = []
    checked = set()
    blobs = {}
    for commit in commits:
        prefix = commit[:12]
        failures.extend(f"{prefix}: {failure}" for failure in metadata_failures(root, commit))
        for entry in git(root, "ls-tree", "-r", "-z", commit).split(b"\0"):
            if not entry:
                continue
            info, raw_name = entry.split(b"\t", 1)
            mode, kind, object_id = info.decode().split()
            name = raw_name.decode("utf-8")
            if (object_id, name) in checked:
                continue
            checked.add((object_id, name))
            if kind != "blob" or mode not in {"100644", "100755"}:
                failures.append(f"{prefix} {name!r}: non-regular historical source")
                continue
            if object_id not in blobs:
                blobs[object_id] = git(root, "cat-file", "blob", object_id)
            failures.extend(f"{prefix} {name!r}: {failure}" for failure in check_blob(name, blobs[object_id]))
    return failures, len(commits), len(checked)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base")
    parser.add_argument("--head")
    args = parser.parse_args()
    revisions = ["--all"]
    if args.base or args.head:
        if not args.base or not args.head or not all(re.fullmatch(r"[0-9a-f]{40}", sha) for sha in (args.base, args.head)):
            parser.error("base and head must both be full commit IDs")
        base = git(ROOT, "merge-base", args.base, args.head).decode().strip()
        revisions = [f"{base}..{args.head}"]
    if git(ROOT, "rev-parse", "--is-shallow-repository").strip() == b"true":
        raise SystemExit("history checks require a complete checkout")
    failures, commits, blobs = history_failures(ROOT, revisions)
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"Public history: {commits} commits and {blobs} unique path/blob pairs checked; no findings.")


if __name__ == "__main__":
    main()
