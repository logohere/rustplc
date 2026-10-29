#!/usr/bin/env python3
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
FILES = [
    ROOT / "crates/openplc-editor-macos/src/project.rs",
    ROOT / "crates/openplc-editor-macos/src/library_manager.rs",
    ROOT / "crates/openplc-editor-macos/src/vpp_package.rs",
]


def production_prefix(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    # These files keep unit fixtures in a final cfg(test) module; durable writes in
    # those fixtures are intentionally ordinary writes to tempdirs.
    match = re.search(r"(?m)^#\[cfg\(test\)\]\s*\nmod\s+\w+\s*\{", text)
    return text[: match.start()] if match else text

violations = []
for path in FILES:
    text = production_prefix(path)
    for line_no, line in enumerate(text.splitlines(), 1):
        if "fs::write(" in line:
            violations.append(f"{path.relative_to(ROOT)}:{line_no}: {line.strip()}")

if violations:
    print("Direct durable writes found; use atomic_file::write:")
    print("\n".join(violations))
    raise SystemExit(1)

required = {
    "project.rs": 6,
    "library_manager.rs": 2,
    "vpp_package.rs": 1,
}
for path in FILES:
    count = production_prefix(path).count("atomic_file::write")
    if count < required[path.name]:
        raise SystemExit(f"{path.name}: expected at least {required[path.name]} atomic durable writes, found {count}")
print("Durable-write gate: project/history/library/package registry persistence is atomic")
