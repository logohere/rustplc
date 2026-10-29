#!/usr/bin/env python3
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
LOCK = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
EDITOR_TOML = (ROOT / "crates/openplc-editor-macos/Cargo.toml").read_text(encoding="utf-8")


def version(name: str) -> tuple[int, int, int]:
    match = re.search(
        rf'\[\[package\]\]\nname = "{re.escape(name)}"\nversion = "([0-9]+)\.([0-9]+)\.([0-9]+)"',
        LOCK,
    )
    if not match:
        raise SystemExit(f"required dependency {name!r} is missing from Cargo.lock")
    return tuple(map(int, match.groups()))


def require_at_least(name: str, minimum: tuple[int, int, int]) -> None:
    current = version(name)
    if current < minimum:
        raise SystemExit(f"{name} {current} is below security floor {minimum}")
    print(f"{name}: {current[0]}.{current[1]}.{current[2]} >= {minimum[0]}.{minimum[1]}.{minimum[2]}")


require_at_least("bytes", (1, 11, 1))
require_at_least("webbrowser", (1, 2, 2))
require_at_least("zip", (9, 0, 5))
require_at_least("rustls", (0, 23, 46))
require_at_least("rustls-webpki", (0, 103, 16))
require_at_least("webpki-roots", (1, 0, 10))
require_at_least("tokio", (1, 53, 3))

line = next(
    (line for line in EDITOR_TOML.splitlines() if line.startswith("dioxus-desktop = ")),
    "",
)
if '"devtools"' not in line:
    raise SystemExit("Dioxus Desktop 0.7.3 requires its devtools feature to compile the macOS menu/WebView integration")

# Dioxus 0.7.3 carries the devtools crate when this compile-time feature is on.
# Hot-reload code is additionally gated by debug_assertions upstream; the security
# floor checks above cover the transitive webbrowser/bytes advisories relevant here.

block = version("block")
print(f"block: {block[0]}.{block[1]}.{block[2]} (known upstream Cocoa future-Rust compatibility risk)")
print("Dependency-floor gate: patched security floors present; Dioxus 0.7.3 devtools compatibility feature is explicitly accounted for")
