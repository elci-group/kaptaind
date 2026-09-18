#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR="${1:-target/release}"
TARGET_DIR="${2:-/usr/local/bin}"

if [ ! -f "$SOURCE_DIR/kaptaind" ]; then
    echo "Error: kaptaind binary not found in $SOURCE_DIR"
    exit 1
fi

echo "Installing kaptaind binary to $TARGET_DIR..."

if [ -w "$TARGET_DIR" ]; then
    install -m 755 "$SOURCE_DIR/kaptaind" "$TARGET_DIR/kaptaind"
else
    sudo install -m 755 "$SOURCE_DIR/kaptaind" "$TARGET_DIR/kaptaind"
fi

echo "Daemon binary installed successfully."
