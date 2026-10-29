#!/usr/bin/env python3
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates"
violations = []
for path in SRC.rglob("*.rs"):
    rel = path.relative_to(ROOT)
    if "tests" in rel.parts or "examples" in rel.parts or path.name == "tests.rs":
        continue
    lines = path.read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(lines):
        if re.search(r"\bunsafe\s*\{", line) or "unsafe impl " in line:
            context = " ".join(lines[max(0, index - 4): index])
            if "SAFETY:" not in context:
                violations.append(f"{rel}:{index + 1}: {line.strip()}")
if violations:
    print("Unsafe boundary comments missing:")
    print("\n".join(violations))
    raise SystemExit(1)
print("Unsafe boundary audit: all production unsafe blocks/impls have nearby SAFETY rationale")
