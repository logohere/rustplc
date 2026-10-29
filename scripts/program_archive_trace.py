#!/usr/bin/env python3
"""Emit accepted OpenPLC project-archive observations."""

from __future__ import annotations

import argparse
import __future__
import hashlib
import json
import logging
import subprocess
import sys
import tempfile
import types
import zipfile
from pathlib import Path


COMPRESSION = {
    "stored": zipfile.ZIP_STORED,
    "deflated": zipfile.ZIP_DEFLATED,
    "bzip2": zipfile.ZIP_BZIP2,
    "lzma": zipfile.ZIP_LZMA,
}


def load_upstream(source: Path):
    runtime = types.ModuleType("webserver.runtimemanager")
    runtime.RuntimeManager = object
    logger = types.ModuleType("webserver.logger")
    logger.get_logger = lambda *_args, **_kwargs: (logging.getLogger("archive-trace"), None)
    logger.LogParser = object
    plugins = types.ModuleType("webserver.plugin_config_model")
    plugins.PluginsConfiguration = object
    plugins.PluginConfig = object
    plugins.PluginType = object
    sys.modules.update({
        "webserver.runtimemanager": runtime,
        "webserver.logger": logger,
        "webserver.plugin_config_model": plugins,
    })
    path = source / "webserver" / "plcapp_management.py"
    module = types.ModuleType("pinned_plcapp_management")
    module.__file__ = str(path)
    sys.modules[module.__name__] = module
    code = compile(
        path.read_text(encoding="utf-8"),
        str(path),
        "exec",
        flags=__future__.annotations.compiler_flag,
        dont_inherit=True,
    )
    exec(code, module.__dict__)
    return module


def write_archive(path: Path, case: dict) -> None:
    compression = COMPRESSION[case["compression"]]
    with zipfile.ZipFile(path, "w", compression=compression) as archive:
        for name, contents in case["entries"]:
            archive.writestr(name, contents.encode())


def observe(destination: Path) -> list[dict]:
    observations = []
    for path in sorted(item for item in destination.rglob("*") if item.is_file()):
        data = path.read_bytes()
        observations.append({
            "path": path.relative_to(destination).as_posix(),
            "bytes": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
        })
    return observations


def run_case(case: dict, root: Path, upstream, executable: Path | None) -> dict:
    archive = root / f"{case['name']}.zip"
    destination = root / f"{case['name']}-out"
    write_archive(archive, case)
    if upstream is not None:
        safe, entries = upstream.analyze_zip(archive)
        if not safe:
            raise ValueError(f"upstream rejected {case['name']}")
        destination.mkdir()
        upstream.safe_extract(archive, destination, entries)
    else:
        subprocess.run(
            [str(executable), str(archive), str(destination)],
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
    return {"case": case["name"], "accepted": True, "files": observe(destination)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", type=Path)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--upstream", type=Path)
    group.add_argument("--executable", type=Path)
    args = parser.parse_args()
    fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
    upstream = load_upstream(args.upstream) if args.upstream else None
    with tempfile.TemporaryDirectory(prefix="rustplc-archive-trace-") as temporary:
        root = Path(temporary)
        observations = [
            run_case(case, root, upstream, args.executable) for case in fixture["cases"]
        ]
    metadata = dict(fixture["metadata"])
    metadata["baseline_commit"] = fixture["baseline_commit"]
    metadata["runner"] = "pinned-openplc" if upstream else "rustplc"
    print(json.dumps({"metadata": metadata, "observations": observations}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
