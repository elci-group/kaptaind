#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

echo "== Rust advisory audit =="
if ! command -v cargo-audit >/dev/null 2>&1; then
  echo "cargo-audit is not installed. Install it with: cargo install cargo-audit" >&2
  exit 127
fi
cargo audit

echo "== Web dependency audit =="
cd "$ROOT/web"
npm audit
