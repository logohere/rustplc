#!/usr/bin/env python3
"""Fail when a static Rust rsx! class token has no compiled CSS selector."""

from pathlib import Path
import re
import sys
from typing import Optional

ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = ROOT / "crates" / "openplc-editor-macos" / "src"
CSS_PATH = ROOT / "crates" / "openplc-editor-macos" / "assets" / "openplc.css"

# Semantic/custom selectors supplied by pinned global CSS or by SVG/component behavior.
CUSTOM_CLASSES = {
    "group",
    "peer",
    "box",
    "sidebar-scroll",
    "oplc-titlebar-container",
    "oplc-titlebar-content",
    "oplc-titlebar-drag-region",
    "oplc-monaco-wrapper",
    "debug-panel",
    "react-flow",
    "retro-icon",
    "spin-refresh",
    "ai-chat-typing-dot",
    "ai-diff-added",
    "ai-diff-added-gutter",
    "ai-diff-removed-zone",
    "scroll-area",
}

CLASS_LITERAL = re.compile(r'class\s*:\s*"([^"]*)"')

FONT_SIZES = {
    "text-xs", "text-sm", "text-base", "text-lg", "text-xl", "text-2xl",
    "text-3xl", "text-4xl", "text-cp-xs", "text-cp-sm", "text-cp-base",
}
FONT_WEIGHTS = {
    "font-thin", "font-extralight", "font-light", "font-normal", "font-medium",
    "font-semibold", "font-bold", "font-extrabold", "font-black",
}


def class_group(token: str) -> Optional[tuple[str, str]]:
    parts = token.split(":")
    utility = parts[-1]
    variant = ":".join(parts[:-1])
    if utility.startswith("h-") and not utility.startswith(("h-fit", "h-auto")):
        return variant, "height"
    if utility.startswith("w-"):
        return variant, "width"
    if utility.startswith("px-"):
        return variant, "padding-x"
    if utility.startswith("py-"):
        return variant, "padding-y"
    if utility in FONT_WEIGHTS:
        return variant, "font-weight"
    if utility in FONT_SIZES or utility.startswith("text-["):
        return variant, "font-size"
    if utility.startswith("bg-"):
        return variant, "background"
    if utility.startswith("opacity-"):
        return variant, "opacity"
    if utility.startswith("border-"):
        suffix = utility.removeprefix("border-")
        if suffix in {"0", "2", "4", "8", "t", "b", "l", "r", "x", "y", "dashed", "solid", "double", "none"} or suffix.startswith(("t-", "b-", "l-", "r-", "x-", "y-", "[")):
            return None
        return variant, "border-color"
    return None


def static_class_conflicts() -> list[tuple[str, int, str, list[str]]]:
    conflicts = []
    for path in SOURCE_ROOT.rglob("*.rs"):
        source = path.read_text(encoding="utf-8")
        for match in CLASS_LITERAL.finditer(source):
            groups: dict[tuple[str, str], list[str]] = {}
            for token in match.group(1).split():
                group = class_group(token)
                if group is not None:
                    groups.setdefault(group, []).append(token)
            for (variant, group), tokens in groups.items():
                unique = list(dict.fromkeys(tokens))
                if len(unique) > 1:
                    line = source.count("\n", 0, match.start()) + 1
                    conflicts.append((str(path.relative_to(ROOT)), line, f"{variant}:{group}" if variant else group, unique))
    return conflicts


def tailwind_selector_escape(token: str) -> str:
    """Match Tailwind/PostCSS selector escaping closely enough for utility classes."""
    result: list[str] = []
    for index, char in enumerate(token):
        if index == 0 and char.isdigit():
            # Tailwind emits a compact hex escape when the next character cannot
            # be parsed as another hex digit (e.g. `2xl:` -> `\\32xl\\:`).
            next_char = token[1] if len(token) > 1 else ""
            separator = " " if next_char and next_char.lower() in "0123456789abcdef" else ""
            result.append(f"\\{ord(char):x}{separator}")
        elif char == ",":
            # postcss-selector-parser uses the hexadecimal comma escape in arbitrary values.
            result.append("\\2c ")
        elif char.isascii() and (char.isalnum() or char in "_-"):
            result.append(char)
        else:
            result.append("\\" + char)
    return "." + "".join(result)


def static_class_tokens() -> set[str]:
    tokens: set[str] = set()
    for path in SOURCE_ROOT.rglob("*.rs"):
        source = path.read_text(encoding="utf-8")
        for match in CLASS_LITERAL.finditer(source):
            for token in match.group(1).split():
                if token and "{" not in token and "}" not in token:
                    tokens.add(token)
    return tokens


def main() -> int:
    css = CSS_PATH.read_text(encoding="utf-8")
    tokens = static_class_tokens()
    missing = []
    for token in sorted(tokens):
        if token in CUSTOM_CLASSES:
            continue
        selector = tailwind_selector_escape(token)
        if selector not in css:
            missing.append((token, selector))

    conflicts = static_class_conflicts()
    print(f"Static Rust class tokens: {len(tokens)}")
    print(f"Missing compiled selectors: {len(missing)}")
    for token, selector in missing:
        print(f"  {token} -> {selector}")
    print(f"Conflicting static class groups: {len(conflicts)}")
    for path, line, group, values in conflicts:
        print(f"  {path}:{line}: {group}: {values}")
    return 1 if missing or conflicts else 0


if __name__ == "__main__":
    sys.exit(main())
