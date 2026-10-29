#!/usr/bin/env bash
set -euo pipefail

VERSION="1.4.1"
DEST="${1:-target/openplc-editor-tools/arduino-cli}"
ARCH="${OPENPLC_ARDUINO_ARCH:-$(uname -m)}"

case "$ARCH" in
  arm64|aarch64)
    ASSET="arduino-cli_${VERSION}_macOS_ARM64.tar.gz"
    ARCHIVE_SHA256="d9d19a3cc8e6e28d138c435e1055a0388c984827e93fccbd352fe5dac685a02b"
    BINARY_SHA256="fef5c424de81c9f9503f6cf35a6cae3269c8c21b497cca023ee409b2ff53bb84"
    ;;
  x86_64|amd64)
    ASSET="arduino-cli_${VERSION}_macOS_64bit.tar.gz"
    ARCHIVE_SHA256="3f2de15a37e580301eb8618fb6fd931ed0b7a8b044f0809a0ac6d20879400a7c"
    BINARY_SHA256="c5255dd9af245bbbfaded016001a941823632139755df1f67352780dfeb73044"
    ;;
  *)
    echo "Unsupported macOS architecture: $ARCH" >&2
    exit 2
    ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
case "$DEST" in
  /*) ;;
  *) DEST="$ROOT/$DEST" ;;
esac

verify_binary() {
  local file="$1"
  [[ -f "$file" ]] || return 1
  local actual
  actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  [[ "$actual" == "$BINARY_SHA256" ]]
}

if verify_binary "$DEST"; then
  chmod +x "$DEST"
  echo "$DEST"
  exit 0
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/openplc-arduino-cli.XXXXXX")"
cleanup() {
  find "$TMP" -depth -mindepth 1 -delete 2>/dev/null || true
  rmdir "$TMP" 2>/dev/null || true
}
trap cleanup EXIT

URL="https://github.com/arduino/arduino-cli/releases/download/v${VERSION}/${ASSET}"
curl --fail --location --silent --show-error "$URL" -o "$TMP/arduino-cli.tgz"
ACTUAL_ARCHIVE_SHA256="$(shasum -a 256 "$TMP/arduino-cli.tgz" | awk '{print $1}')"
if [[ "$ACTUAL_ARCHIVE_SHA256" != "$ARCHIVE_SHA256" ]]; then
  echo "Arduino CLI archive checksum mismatch: expected $ARCHIVE_SHA256, got $ACTUAL_ARCHIVE_SHA256" >&2
  exit 3
fi

tar -xzf "$TMP/arduino-cli.tgz" -C "$TMP"
if ! verify_binary "$TMP/arduino-cli"; then
  ACTUAL_BINARY_SHA256="$(shasum -a 256 "$TMP/arduino-cli" | awk '{print $1}')"
  echo "Arduino CLI binary checksum mismatch: expected $BINARY_SHA256, got $ACTUAL_BINARY_SHA256" >&2
  exit 4
fi

mkdir -p "$(dirname "$DEST")"
cp "$TMP/arduino-cli" "$DEST"
chmod +x "$DEST"
echo "$DEST"
