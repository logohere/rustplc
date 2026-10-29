#!/usr/bin/env python3
from pathlib import Path
import plistlib
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "target/macos/RustPLC Editor.app"
PLIST = APP / "Contents/Info.plist"
EXECUTABLE = APP / "Contents/MacOS/RustPLC Editor"
ARDUINO = APP / "Contents/Resources/bin/arduino-cli"
LEGAL = APP / "Contents/Resources/legal"

workspace = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
match = re.search(r'(?m)^version = "(\d+\.\d+\.\d+)"$', workspace)
if not match:
    raise SystemExit("workspace version is missing")
version = match.group(1)

if not PLIST.is_file():
    raise SystemExit(f"missing packaged Info.plist: {PLIST}")
with PLIST.open("rb") as handle:
    info = plistlib.load(handle)

expected = {
    "CFBundleIdentifier": "io.github.logohere.rustplc",
    "CFBundleName": "RustPLC Editor",
    "CFBundleShortVersionString": version,
    "CFBundleVersion": version,
}
for key, value in expected.items():
    if info.get(key) != value:
        raise SystemExit(f"{key}: expected {value!r}, got {info.get(key)!r}")

for path in (EXECUTABLE, ARDUINO):
    if not path.is_file():
        raise SystemExit(f"missing packaged executable: {path}")
    if path.stat().st_mode & 0o111 == 0:
        raise SystemExit(f"packaged executable is not executable: {path}")

for relative in (
    "THIRD_PARTY_NOTICES.md",
    "licenses/GPL-3.0.txt",
    "licenses/APACHE-2.0.txt",
    "licenses/STRUCPP-RUNTIME-EXCEPTION.txt",
):
    path = LEGAL / relative
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"missing/empty legal resource: {path}")

required_runtime = [
    APP / "Contents/Resources/compiler-runtime/runtime_v4_entry.cpp",
    APP / "Contents/Resources/compiler-runtime/strucpp_program_reset.cpp",
    APP / "Contents/Resources/compiler-runtime/include/iec_types.hpp",
    APP / "Contents/Resources/compiler-runtime/include/debug_dispatch.hpp",
]
for path in required_runtime:
    if not path.is_file():
        raise SystemExit(f"missing packaged compiler/runtime resource: {path}")

verify = subprocess.run(
    ["codesign", "--verify", "--deep", "--strict", str(APP)],
    text=True,
    capture_output=True,
)
if verify.returncode != 0:
    raise SystemExit(verify.stderr or verify.stdout or "codesign verification failed")

print(f"macOS bundle gate: RustPLC Editor {version} resources/version/codesign verified")
