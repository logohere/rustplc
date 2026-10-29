#!/usr/bin/env python3
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
workspace = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
editor = (ROOT / "crates/openplc-editor-macos/Cargo.toml").read_text(encoding="utf-8")
ledger = (ROOT / "docs/RUSTPLC_PARITY_LEDGER.md").read_text(encoding="utf-8")
policy = (ROOT / "docs/VERSIONING.md").read_text(encoding="utf-8")

match = re.search(r"(?m)^version = \"(\d+)\.(\d+)\.(\d+)\"$", workspace)
if not match:
    raise SystemExit("workspace SemVer is missing")
version = tuple(map(int, match.groups()))
if "version.workspace = true" not in editor:
    raise SystemExit("editor must inherit the RustPLC workspace version")
if "OpenPLC Editor `4.2.10`" not in policy:
    raise SystemExit("version policy must record the pinned OpenPLC Editor baseline")
if version[0] >= 1 and "Major product areas still incomplete" in ledger:
    raise SystemExit("RustPLC cannot be versioned >=1.0 while the parity ledger still declares incomplete product areas")
print("RustPLC SemVer gate: " + ".".join(map(str, version)))
