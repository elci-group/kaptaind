#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR="${1:-target/release}"
TARGET_DIR="${2:-/usr/local/bin}"

if [ ! -f "$SOURCE_DIR/kaptaind" ]; then
    echo "Error: kaptaind binary not found in $SOURCE_DIR"
    exit 1
fi

if [ ! -f "$SOURCE_DIR/kaptaind-cli" ]; then
    echo "Error: kaptaind-cli binary not found in $SOURCE_DIR"
    exit 1
fi

echo "Installing kaptaind binaries to $TARGET_DIR..."

if [ -w "$TARGET_DIR" ]; then
    install -m 755 "$SOURCE_DIR/kaptaind" "$TARGET_DIR/kaptaind"
    install -m 755 "$SOURCE_DIR/kaptaind-cli" "$TARGET_DIR/kaptaind-cli"
else
    sudo install -m 755 "$SOURCE_DIR/kaptaind" "$TARGET_DIR/kaptaind"
    sudo install -m 755 "$SOURCE_DIR/kaptaind-cli" "$TARGET_DIR/kaptaind-cli"
fi

echo "Daemon binaries installed successfully."
