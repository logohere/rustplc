#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="RustPLC Editor"
BUNDLE_ID="io.github.logohere.rustplc"
VERSION="$(python3 - "$ROOT" <<'PYVER'
import re, sys
from pathlib import Path
text = (Path(sys.argv[1]) / "Cargo.toml").read_text()
match = re.search(r'(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"$', text)
if not match:
    raise SystemExit("workspace version missing")
print(match.group(1))
PYVER
)"
TARGET_DIR="$ROOT/target/macos"
APP="$TARGET_DIR/$APP_NAME.app"
CANDIDATE="$TARGET_DIR/.$APP_NAME.app.candidate.$$"
CONTENTS="$CANDIDATE/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

cd "$ROOT"
cargo build --release -p open-rust-plc-editor

python3 - "$TARGET_DIR" "$APP_NAME" <<'PYCLEAN'
import shutil, sys
from pathlib import Path
target, app_name = Path(sys.argv[1]), sys.argv[2]
for candidate in target.glob(f".{app_name}.app.candidate.*"):
    if candidate.is_dir():
        shutil.rmtree(candidate)
PYCLEAN

mkdir -p "$MACOS" "$RESOURCES/compiler-runtime/include" "$RESOURCES/bin" "$RESOURCES/sources" "$RESOURCES/legal/licenses"
cp "$ROOT/target/release/openplc-editor" "$MACOS/$APP_NAME"
cp "$ROOT/crates/compiler-core/runtime/include/"*.hpp "$RESOURCES/compiler-runtime/include/"
cp "$ROOT/crates/compiler-core/runtime/runtime_v4_entry.cpp" "$RESOURCES/compiler-runtime/runtime_v4_entry.cpp"
cp "$ROOT/crates/compat-core/native/strucpp_program_reset.cpp" "$RESOURCES/compiler-runtime/strucpp_program_reset.cpp"
"$ROOT/scripts/fetch_arduino_cli_macos.sh" "$RESOURCES/bin/arduino-cli" >/dev/null
cp -R "$ROOT/crates/openplc-editor-macos/resources/sources/." "$RESOURCES/sources/"
cp "$ROOT/THIRD_PARTY_NOTICES.md" "$RESOURCES/legal/THIRD_PARTY_NOTICES.md"
cp "$ROOT/LICENSES/"* "$RESOURCES/legal/licenses/"
chmod +x "$MACOS/$APP_NAME"

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleExecutable</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>RustPLC Editor contributors</string>
</dict>
</plist>
PLIST

printf 'APPL????' > "$CONTENTS/PkgInfo"

IDENTITY="${OPENPLC_CODESIGN_IDENTITY:--}"
codesign --force --deep --options runtime --entitlements "$ROOT/crates/openplc-editor-macos/resources/macos/entitlements.plist" --sign "$IDENTITY" "$CANDIDATE"
codesign --verify --deep --strict "$CANDIDATE"

python3 - "$APP" "$CANDIDATE" <<'PYAPP'
import shutil, sys
from pathlib import Path
app, candidate = map(Path, sys.argv[1:])
if app.exists():
    shutil.rmtree(app)
candidate.rename(app)
PYAPP

echo "$APP"
