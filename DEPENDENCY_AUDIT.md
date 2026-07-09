# Dependency Audit and Replacement Plan

Date: 2026-06-04

## Audit Summary

### Rust

Audit method:
- `cargo tree -e normal`
- Source usage checks with `rg`
- `scripts/security-audit.sh` now enforces `cargo audit` when `cargo-audit` is installed.
- A scheduled CI workflow installs `cargo-audit` and runs both Rust and npm advisory checks.

Removed direct dependencies:
- `hex`: replaced with `src/util/hex.rs`, a tiny lowercase byte-to-hex encoder.
- `md5`: replaced with SHA-256 using the existing `sha2` dependency for VACS asset hashes.
- `thiserror`: no in-repo usage.
- `opentelemetry`, `opentelemetry_sdk`, `tracing-opentelemetry`: no in-repo usage.
- `colored`: replaced with `src/util/style.rs`, a minimal ANSI styling shim.
- `dotenvy`: replaced with `src/util/dotenv.rs`, a tiny `.env` file loader.
- `base64`: replaced with `src/util/base64.rs`, a small RFC 4648 encoder/decoder.
- `fs2`: replaced with `src/util/file_lock.rs` and `src/util/disk.rs`, cross-platform wrappers around `flock`/`LockFileEx` and `statvfs`/`GetDiskFreeSpaceExW`.
- `subtle`: replaced with `src/util/constant_time.rs`, a `black_box`-guarded constant-time comparison for webhook signature verification.

Kept dependencies and rationale:
- `tokio`, `notify`, `git2`, `rusqlite`, `reqwest`, `serde`, `toml`, `clap`, `syn`, `sha2`, `hmac`: correctness or security-sensitive infrastructure where an in-house implementation would be higher risk.
- `glob`/`globset`: pattern semantics are subtle and used by safety filters.
- `tar`/`flate2`: archive generation is not worth reimplementing in-house.
- `tabled`: replaceable in principle, but mostly CLI presentation. Replacing it would be a larger mechanical pass with low security value.

### Web

Audit method:
- `npm audit --json`
- `npm outdated --json`
- Source usage checks with `rg`

Changes made:
- Upgraded `next` and `eslint-config-next` to `16.2.7`, removing the high-severity Next.js advisories from the previous audit.
- Removed unused direct dependencies: `react-markdown`, `recharts`, and direct `zod`.
- Removed `@anthropic-ai/sdk`; the app already uses a fetch-based Anthropic Messages API client in `web/lib/inference.ts`.

Remaining `npm audit` findings:
- None. Targeted `overrides` pin transitive `postcss` and `uuid` to patched versions while keeping current `next` and `next-auth`.

## In-House Replacement Plan

Implemented:
1. Local hex encoder:
   - Small, deterministic, tested.
   - Replaces all `hex::encode` calls.
2. VACS SHA-256 asset hash:
   - Removes MD5 dependency.
   - Improves hash strength while using existing `sha2`.
3. Fetch-based Anthropic integration:
   - Removes SDK vulnerability surface and package weight.
   - Keeps provider behavior centralized in `web/lib/inference.ts`.
4. Local ANSI styling module:
   - `src/util/style.rs` covers the existing `colored` API surface.
   - Respects `NO_COLOR` and terminal detection.
5. Local `.env` loader:
   - `src/util/dotenv.rs` replaces the `dotenvy` crate.
   - Supports comments, blank lines, and simple quoted values.
6. Local base64 codec:
   - `src/util/base64.rs` replaces the `base64` crate.
   - Covers standard encoding/decoding with padding and whitespace tolerance.
7. Local file locking and disk-space wrappers:
   - `src/util/file_lock.rs` and `src/util/disk.rs` replace `fs2`.
   - Cross-platform Unix/Windows support.
8. Local constant-time comparison:
   - `src/util/constant_time.rs` replaces `subtle` for webhook signature verification.
   - Uses XOR accumulation guarded by `std::hint::black_box`.
9. Manual test-output parser:
   - Replaced the single `regex` usage in `src/daemon/scheduler.rs` with string slicing.
   - The `regex` crate is retained for `src/angler/selective.rs`, which exposes user-configurable regex patterns.

Deferred:
1. CLI table rendering:
   - `tabled` is replaceable, but the CLI has a large display surface.
   - Recommended next step: introduce a small `cli::view` layer and migrate table views one command family at a time.
2. Glob matching:
   - Reimplementing glob semantics would risk security regressions in selective capture and ignore handling.
3. Git, watcher, HTTP, database, archive, and crypto crates:
   - These are not suitable in-house replacement candidates for this project.
