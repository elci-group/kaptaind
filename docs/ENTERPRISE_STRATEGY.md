# Enterprise Grade Strategy for kaptaind

This document captures the current maturity of each major feature, the gap analysis
against enterprise-grade (A+ / S-tier) expectations, and the concrete work executed
to close those gaps.

## Grading Rubric

| Grade | Meaning |
|-------|---------|
| **S** | Best-in-class: fully automated, observable, secure, resilient, well-tested, documented. |
| **A+** | Production-ready enterprise: strong defaults, auditability, automation, monitoring, and clear operational runbooks. |
| **A** | Solid and usable, with minor automation/observability/docs gaps. |
| **B+** | Good core implementation, but manual steps or missing telemetry limit enterprise adoption. |
| **C / D** | Functional but incomplete or unsafe for unsupervised production use. |

---

## Feature Grades

| Feature | Current | Target | Gap |
|---------|---------|--------|-----|
| Semantic diff & scoring | A | A+ | Add confidence-weighted rollups and deterministic benchmarks. |
| Git commit / push orchestration | A | A+ | Add signed commits and branch protection enforcement. |
| Qualification & stability | A | A+ | **Implemented**: per-test outcome tracking and flaky-test notifications. |
| Audit logging | A | A+ | Expand to structured OTel-style spans. |
| Config validation | A | A+ | Cross-field validation covers ship schedules; expand to plugin configs. |
| Manual `ship` (CLI) | A- | A+ | **Implemented**: GPG-signed tags, SHA256 checksums, detached signatures, SBOM generation. |
| Automated `ship` (daemon) | A+ | A+ / S | Add SLSA provenance attestation. |
| Notifications | A | A+ | **Implemented**: release, qualification, pulse, and flaky-test events. |
| Observability (status/telemetry) | A | A+ | **Implemented**: Prometheus `/metrics/prometheus` endpoint. |
| **Project overall** | **A** | **A+ / S** | Close remaining S-tier gaps (SLSA provenance, HA rollout, RBAC). |

---

## A+ / S Strategy

### 1. Automated Release Trains (Executed)
- Added `[ship.auto_nightly]` and `[ship.auto_stable]` cron configuration.
- Added `src/schedule/cron.rs` using the `cron` crate for robust 5-field expression parsing.
- Wired the daemon scheduler to evaluate ship schedules every 60 seconds, spawn `run_nightly` / `run_stable`, and prevent overlapping ship runs.
- Added `kaptaind-cli ship status --auto` to preview next fire times.

### 2. Nautical Real-Time Monitoring (Executed)
- Extended `src/daemon/notification.rs` with `ReleaseSuccess`, `ReleaseFailure`, `Qualification`, and `Pulse` events.
- Added maritime phrasing: "Fleet launched", "Fleet ran aground", "Clear skies", "Storm ahead", "Still on watch".
- Emitted qualification notifications from `src/release/orchestrator.rs`.
- Emitted pulse notifications every 15 minutes from the scheduler.
- Added shell hooks (`on_release`, `on_qualification`, `on_pulse`) and webhook support for all new events.

### 3. Operational Safety (Executed)
- Config validation rejects invalid cron schedules and unsupported timezones.
- Auto-ship uses existing idempotency guards (`find_existing_nightly`, release index dedup).
- Overlapping ship runs are serialized with an `AtomicBool` guard.
- Failures are audit-logged and notified.

### 4. Observability (Executed)
- Added Prometheus `/metrics/prometheus` endpoint with counters, stability score, release count, and version labels.
- Kept JSON `/metrics` endpoint for ad-hoc inspection and `/events` SSE stream.

### 5. Supply-Chain Hardening (Executed)
- Added `[ship] sign` and `gpg_key_id` config; per-kind `sign` override in `[ship.stable]` / `[ship.nightly]`.
- Ship pipeline now generates `{artifact}.sha256` checksums and `{artifact}.sha256.asc` detached GPG signatures when signing is enabled.
- Stable/nightly git tags are GPG-signed (`git tag -s`) when signing is enabled.
- Added `[ship.sbom]` config and `src/release/sbom.rs` generating SPDX 2.3 JSON from `Cargo.lock` / `package-lock.json`.
- SBOMs are attached to release artifacts and recorded in the ship index.

### 6. Flaky-Test Detection (Executed)
- Extended `StabilityRecord` with per-test `TestOutcomeRecord` history and `flaky_tests` cache.
- `StabilityEntry` carries `failed_tests` parsed from cargo test output.
- Detection flags tests with both pass and fail outcomes in the last 10 records.
- Added `NotificationEvent::FlakyTests` with nautical "🎣 Flaky tests spotted" rendering and `notify_flaky_tests()` helper.

### 7. Documentation & Tests (Executed)
- Added unit tests for cron parsing, config validation, notification rendering, Prometheus metrics, signing, SBOM generation, and flaky-test detection.
- Updated `README.md` with auto-ship examples, `ship status --auto`, monitoring section, signing, SBOM, and flaky-test notes.
- Updated `AGENTS.md` runtime flow and module list.
- Created this strategy document.

---

## Remaining S-Tier Work

| Item | Why | Estimated Effort |
|------|-----|------------------|
| SLSA provenance attestation | Formal supply-chain compliance | 2-3 days |
| HA / zero-downtime daemon upgrades (Shark) | 24/7 reliability | 3-5 days |
| Fine-grained RBAC for multi-user installs | Large-team adoption | 3-5 days |
| Signed git commits (not just tags) | Commit-level supply-chain assurance | 1-2 days |
| Branch protection / required-CI enforcement | Prevent bypassing qualification gates | 1-2 days |

---

## Acceptance Criteria (Met)

- [x] `cargo test` passes.
- [x] `cargo clippy --all-targets -- -D warnings` passes.
- [x] `cargo build --release` succeeds.
- [x] Binaries installed to `~/.local/bin/` with backups.
- [x] `[ship.auto_nightly]` / `[ship.auto_stable]` deserialize and validate.
- [x] Daemon scheduler emits pulse, qualification, and flaky-test notifications.
- [x] Automated ship task logs to audit and sends release notifications.
- [x] Prometheus `/metrics/prometheus` endpoint exposes counters, stability, releases, and version labels.
- [x] Ship pipeline generates SHA256 checksums, GPG signatures, and SPDX SBOMs when enabled.
- [x] Flaky-test detection tracks per-test outcomes and notifies operators.
- [x] Documentation updated and strategy published.

---

## How to Enable Automated Releases

```toml
[ship]
enabled = true

[ship.auto_nightly]
enabled = true
schedule = "0 2 * * *"      # 02:00 local time every day
cron_timezone = "local"
require_qualification = false

[ship.auto_stable]
enabled = true
schedule = "0 9 * * 1"      # 09:00 local time every Monday
cron_timezone = "local"
require_qualification = true
```

Preview the schedule:

```bash
kaptaind-cli ship status --auto
```
