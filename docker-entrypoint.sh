#!/usr/bin/env bash
# Starts as root to ensure the kaptaind data directory is writable, then drops
# to the unprivileged `kaptaind` user before exec'ing the real command.
set -euo pipefail

DATA_DIR="${KAPTAIND_DATA_DIR:-/opt/kaptaind/.kaptaind}"
mkdir -p "$DATA_DIR"

if [ "$(id -u)" = "0" ]; then
    chown -R kaptaind:kaptaind "$DATA_DIR" /opt/kaptaind 2>/dev/null || true
    exec runuser -u kaptaind -- "$@"
else
    exec "$@"
fi
