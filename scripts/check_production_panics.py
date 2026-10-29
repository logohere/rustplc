#!/usr/bin/env python3
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates/openplc-editor-macos/src"

# These are compile-time/static invariants, not values originating from project,
# network, filesystem, or user input. New panic sites are rejected by default.
ALLOWED_SNIPPETS = (
    'expect("static GUID regex")',
    'expect("static constant regex")',
    'expect("static POU regex")',
    'expect("static TYPE regex")',
    'expect("static variable regex")',
    'expect("whole declaration match")',
    'expect("pinned POU declaration regex")',
    'expect("pinned POU documentation regex")',
    'expect("pinned VAR start regex")',
    'expect("pinned IEC keyword regex")',
    'expect("pinned variable declaration regex")',
    'expect("pinned alternate variable declaration regex")',
    'expect("pinned array type regex")',
    'expect("static ladder JSON must serialize")',
    'expect("static FBD JSON must serialize")',
    'expect("RFC3339 formatting must succeed")',
    'expect("language id serializes")',
    'expect("asset path serializes")',
)
PATTERN = re.compile(r'\.(?:unwrap|expect)\s*\(|\bpanic!\s*\(|\btodo!\s*\(|\bunimplemented!\s*\(')


def production_lines(path: Path):
    lines = path.read_text(encoding="utf-8").splitlines()
    depth = 0
    test_module_depth = None
    pending_cfg_test = False
    cfg_test_function_depth = None
    for line_no, line in enumerate(lines, 1):
        stripped = line.strip()
        if stripped == "#[cfg(test)]":
            pending_cfg_test = True
        opens = line.count("{")
        closes = line.count("}")
        if pending_cfg_test and re.match(r"mod\s+\w+\s*\{", stripped):
            test_module_depth = depth + opens - closes
            pending_cfg_test = False
        elif pending_cfg_test and re.search(r"\bfn\s+\w+\s*\(", stripped):
            cfg_test_function_depth = depth + opens - closes
            pending_cfg_test = False
        is_test = test_module_depth is not None or cfg_test_function_depth is not None
        if not is_test:
            yield line_no, line
        depth += opens - closes
        if test_module_depth is not None and depth < test_module_depth:
            test_module_depth = None
        if cfg_test_function_depth is not None and depth < cfg_test_function_depth:
            cfg_test_function_depth = None


violations = []
for path in SRC.rglob("*.rs"):
    if path.name == "workflow_parity.rs":
        continue
    for line_no, line in production_lines(path):
        if not PATTERN.search(line):
            continue
        if any(snippet in line for snippet in ALLOWED_SNIPPETS):
            continue
        violations.append(f"{path.relative_to(ROOT)}:{line_no}: {line.strip()}")

if violations:
    print("Unexpected production panic/unwrap sites:")
    print("\n".join(violations))
    raise SystemExit(1)
print(f"Production panic gate: only {len(ALLOWED_SNIPPETS)} approved static invariants remain")
