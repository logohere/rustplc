#!/usr/bin/env python3
"""Validate the pinned OpenPLC source inventory without network access."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
STATUSES = {"not-started", "in-progress", "verified", "blocked"}


def relative(value: object, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise ValueError(f"{label} must be a non-empty string")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"{label} must be a relative path without '..'")
    return value


def load_manifest(path: Path) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read manifest: {exc}") from exc
    if not isinstance(data, dict):
        raise ValueError("manifest root must be an object")
    required = {
        "repository",
        "version",
        "commit",
        "generated_program_toolchain",
        "license",
        "scope_roots",
        "root_files",
        "files",
    }
    missing = required - data.keys()
    if missing:
        raise ValueError(f"manifest missing fields: {', '.join(sorted(missing))}")
    if not isinstance(data["repository"], str) or not data["repository"].startswith("https://"):
        raise ValueError("repository must be an HTTPS URL")
    if not isinstance(data["version"], str) or not data["version"]:
        raise ValueError("version must be a non-empty string")
    if not isinstance(data["commit"], str) or not COMMIT_RE.fullmatch(data["commit"]):
        raise ValueError("commit must be a 40-character lowercase SHA-1")
    validate_toolchain(data["generated_program_toolchain"])
    if not isinstance(data["license"], str) or not data["license"]:
        raise ValueError("license must be a non-empty string")
    roots = [relative(item, "scope_roots entry") for item in data["scope_roots"]]
    root_files = [relative(item, "root_files entry") for item in data["root_files"]]
    if len(roots) != len(set(roots)) or len(root_files) != len(set(root_files)):
        raise ValueError("scope_roots and root_files must be unique")
    expected: set[str] = set()
    for entry in data["files"]:
        if not isinstance(entry, dict):
            raise ValueError("each files entry must be an object")
        path = relative(entry.get("path"), "files.path")
        if path in expected:
            raise ValueError(f"duplicate files.path: {path}")
        if path not in root_files and not any(path.startswith(f"{root}/") for root in roots):
            raise ValueError(f"files.path is outside declared roots: {path}")
        if not isinstance(entry.get("category"), str) or not entry["category"]:
            raise ValueError(f"missing category for {path}")
        relative(entry.get("rust_target"), f"rust_target for {path}")
        if entry.get("status") not in STATUSES:
            raise ValueError(f"invalid status for {path}: {entry.get('status')}")
        expected.add(path)
    gitlinks = data.get("gitlinks", [])
    if not isinstance(gitlinks, list):
        raise ValueError("gitlinks must be a list")
    for link in gitlinks:
        if not isinstance(link, dict) or not COMMIT_RE.fullmatch(str(link.get("commit", ""))):
            raise ValueError("each gitlinks entry needs a 40-character commit")
        relative(link.get("path"), "gitlinks.path")
    return data


def validate_pin(value: object, label: str) -> dict:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    repository = value.get("repository")
    if not isinstance(repository, str) or not repository.startswith("https://"):
        raise ValueError(f"{label}.repository must be an HTTPS URL")
    if not COMMIT_RE.fullmatch(str(value.get("commit", ""))):
        raise ValueError(f"{label}.commit must be a 40-character lowercase SHA-1")
    return value


def validate_toolchain(value: object) -> None:
    if not isinstance(value, dict) or set(value) != {"editor", "strucpp"}:
        raise ValueError("generated_program_toolchain must contain editor and strucpp")
    editor = validate_pin(value["editor"], "generated_program_toolchain.editor")
    relative(editor.get("manifest"), "generated_program_toolchain.editor.manifest")
    strucpp = validate_pin(value["strucpp"], "generated_program_toolchain.strucpp")
    if not isinstance(strucpp.get("tag"), str) or not strucpp["tag"]:
        raise ValueError("generated_program_toolchain.strucpp.tag must be non-empty")
    relative(
        strucpp.get("program_base_header"),
        "generated_program_toolchain.strucpp.program_base_header",
    )


def git(source: Path, *args: str) -> str:
    try:
        result = subprocess.run(
            ["git", "-C", str(source), *args],
            check=True,
            capture_output=True,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        detail = getattr(exc, "stderr", "").strip()
        raise ValueError(f"git {' '.join(args)} failed: {detail or exc}") from exc
    return result.stdout.strip()


def tracked_files(source: Path, paths: list[str]) -> tuple[set[str], dict[str, str]]:
    raw = git(source, "ls-files", "-s", "-z", "--", *paths)
    files: set[str] = set()
    links: dict[str, str] = {}
    for record in raw.split("\0"):
        if not record:
            continue
        metadata, path = record.split("\t", 1)
        mode, object_id, _stage = metadata.split()
        if mode == "160000":
            links[path] = object_id
        else:
            files.add(path)
    return files, links


def verify_source(data: dict, source: Path) -> None:
    if not source.is_dir():
        raise ValueError(f"source is not a directory: {source}")
    if git(source, "rev-parse", "HEAD") != data["commit"]:
        raise ValueError("source HEAD does not match pinned commit")
    version = (source / "VERSION").read_text(encoding="utf-8").strip()
    if version != data["version"]:
        raise ValueError(f"VERSION is {version!r}, expected {data['version']!r}")
    license_text = (source / "LICENSE").read_text(encoding="utf-8")
    if "MIT License" not in license_text:
        raise ValueError("LICENSE does not contain the MIT License marker")
    paths = data["scope_roots"] + data["root_files"]
    actual, links = tracked_files(source, paths)
    expected = {entry["path"] for entry in data["files"]}
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    if missing or extra:
        raise ValueError(f"file inventory mismatch; missing={missing[:5]}, extra={extra[:5]}")
    declared_links = {link["path"]: link["commit"] for link in data.get("gitlinks", [])}
    if links != declared_links:
        raise ValueError(f"gitlink inventory mismatch; actual={links}, declared={declared_links}")
    for path in expected:
        if not (source / path).is_file():
            raise ValueError(f"tracked baseline file is unavailable: {path}")


def verify_toolchain(data: dict, editor: Path | None, strucpp: Path | None) -> None:
    pins = data["generated_program_toolchain"]
    for label, source in (("editor", editor), ("strucpp", strucpp)):
        if source is not None and git(source, "rev-parse", "HEAD") != pins[label]["commit"]:
            raise ValueError(f"{label} source HEAD does not match pinned commit")
    if editor is not None:
        manifest = json.loads((editor / pins["editor"]["manifest"]).read_text(encoding="utf-8"))
        compiler = manifest.get("strucpp", {})
        if compiler.get("version") != pins["strucpp"]["tag"]:
            raise ValueError("editor does not pin the declared STruC++ tag")
    if strucpp and not (strucpp / pins["strucpp"]["program_base_header"]).is_file():
        raise ValueError("pinned STruC++ ProgramBase header is unavailable")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", nargs="?", default="baselines/openplc-runtime-v4.1.10.json")
    parser.add_argument("--source", type=Path, help="optional checked-out upstream source")
    parser.add_argument("--editor-source", type=Path, help="optional checked-out OpenPLC Editor source")
    parser.add_argument("--strucpp-source", type=Path, help="optional checked-out STruC++ source")
    args = parser.parse_args()
    try:
        data = load_manifest(Path(args.manifest))
        if args.source:
            verify_source(data, args.source)
        verify_toolchain(data, args.editor_source, args.strucpp_source)
    except ValueError as exc:
        print(f"baseline verification failed: {exc}", file=sys.stderr)
        return 1
    print(f"baseline verified: {data['version']} {data['commit']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
