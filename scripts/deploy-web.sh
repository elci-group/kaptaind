#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR="${1:-deploy/web}"
TARGET_DIR="${2:-/var/www/kaptaind/web}"

if [ ! -d "$SOURCE_DIR" ]; then
    echo "Error: Source directory '$SOURCE_DIR' not found."
    exit 1
fi

echo "Deploying web assets from $SOURCE_DIR to $TARGET_DIR..."

if command -v rsync >/dev/null 2>&1; then
    mkdir -p "$TARGET_DIR"
    rsync -a --delete "$SOURCE_DIR/" "$TARGET_DIR/"
else
    rm -rf "$TARGET_DIR"
    mkdir -p "$TARGET_DIR"
    cp -r "$SOURCE_DIR"/* "$TARGET_DIR/"
fi

if command -v systemctl >/dev/null 2>&1; then
    echo "Reloading nginx..."
    sudo systemctl reload nginx || true
elif command -v nginx >/dev/null 2>&1; then
    echo "Reloading nginx..."
    sudo nginx -s reload || true
fi

echo "Web deployment complete."
