# Autonomous Commit Safety & Monorepo Hardening Plan

**Date:** 2026-07-11
**Author:** Engineering review (live-fire audit on the scotia monorepo)
**Scope:** kaptaind daemon — version engine, staging, watcher, repo model, operability, commit workflow
**Trigger:** 20 technical findings from a production deployment against a monorepo-subdirectory project (`/home/sal` git root, `scotia/` project). A commit cascade (v0.1.6 → v0.1.9 in three self-commits) was observed live and required SIGKILL to stop.
**Verdict:** The architecture is sound; the defaults are fail-open and the version/staging machinery has genuine correctness bugs. This plan converts the findings into a staged, test-gated remediation program with a hotfix release inside one week.

---

## 1. Executive Summary

The 20 findings reduce to five defect families:

| Family | Findings | Root cause |
|--------|----------|------------|
| **F1. Fail-open defaults** | #1, #20 | `mode = "all"` runs `git add -A`; hooks auto-install on startup |
| **F2. Version engine correctness** | #2, #4, #8, #17, #18 | VERSION-file-only baseline; `repo.root()` vs `repo_path` confusion; no monotonicity guard; lockfile decoupled |
| **F3. Self-interaction loops** | #3, #5 | Daemon does not recognize its own writes (version writeback, hook install) |
| **F4. Operability gaps** | #9, #10, #11, #12, #13, #14 | No startup rescan, no SIGTERM, no hot reload, stale state, blocking test gate, no dry-run |
| **F5. Workflow integrity** | #6, #7, #15, #16, #19 | Repo-wide diff pollution, silent skips, clock-based commit boundaries, template messages, opaque thresholds |

**Program goals (measurable):**

1. Zero data-safety findings reachable by default configuration (fail-closed defaults).
2. Version numbers in git are always monotonic and consistent across VERSION / Cargo.toml / Cargo.lock.
3. The daemon cannot commit more than once per genuine change cluster (cascade impossible by construction).
4. Full monorepo support: a project in a subdirectory of a larger repo behaves identically to a standalone repo.
5. Every daemon decision (commit *or skip*) is observable, explainable, and reproducible in dry-run.

**Non-goals:** redesigning the cluster→diff→score→bump→commit pipeline; the web UI; the qualification/release subsystem; adapter work.

**Release train:** `v9.7.17` (hotfix, Workstream A) → `v9.8.0` (correctness + operability, Workstreams B–D) → `v10.0.0` (breaking default flips + workflow, Workstream E).

---

## 2. Severity Model

| Priority | Definition | Findings |
|----------|------------|----------|
| **P0** | Can corrupt user data, git history, or version state with default config | #1, #2, #3, #5 |
| **P1** | Wrong behavior in supported (monorepo) configurations | #4, #6, #8, #9, #18 |
| **P2** | Operability: recovery, observability, control | #7, #10, #11, #12, #13, #14 |
| **P3** | Workflow quality / philosophy | #15, #16, #17, #19, #20 |

Findings #17 and #19 appear twice across families; the traceability matrix (§10) assigns each finding exactly one owning workstream.

---

## 3. Workstream A — Containment & Safe Defaults (P0 hotfix → v9.7.17)

**Objective:** eliminate every finding that can fire with default configuration. Small diffs, shipped fast, no design churn.

### A1. Default staging → `cluster` (#1, #20)
- Flip `StagingConfig::default()` to `StagingMode::Cluster` (`src/config/loader.rs`).
- On load, if the resolved mode is `All`, emit a **startup warning** naming the risk (`git add -A` sweeps the whole worktree) and the remediation. Do not silently override — warn loudly.
- Harden the secret denylist into a **fail-closed** gate: in `All` mode, if any staged path matches the denylist, abort the commit (today it unstages and continues).
- *Acceptance:* a temp repo with 50 untracked files and a watched subdir: default config commits only watched paths; `All` mode prints the warning.

### A2. Self-change suppression (#3)
Replace ignore-file workarounds with a first-class mechanism:
- `process_cluster` records every path the daemon itself writes (save_version outputs, hook installs) into a `recent_self_writes: VecDeque<(PathBuf, Instant)>` (cap 64, TTL 60s).
- The event loop drops events whose paths are all in `recent_self_writes` before clustering. Events mixing self-writes and foreign paths are split, not dropped.
- *Acceptance:* a new regression test `daemon_does_not_cascade_on_version_writeback`: temp repo, daemon commits once, assert no second commit within 5× cluster window. Must run in CI (`rust.yml`).

### A3. Monotonic version guard (#2)
- `save_version` refuses to write a version **lower** than the current Cargo.toml/package.json version (parse manifest, compare semver, error + notification on violation).
- Version baseline resolution becomes: explicit `VERSION` file → package manifest → **error** (never silent `0.1.0`). Applies to `cli/analyze.rs:110` and `scheduler.rs:424`.
- *Acceptance:* property test (proptest): for arbitrary (manifest, VERSION) pairs, the resolved baseline ≥ both when they agree, and any write is monotonic.

### A4. Hooks dir resolves the real git dir (#5)
- `GitHookManager::new` (`angler/git_hooks.rs:80-91`): default `hooks_dir` = `git rev-parse --git-path hooks` (resolved from `repo_path`), never `repo_path/.git/hooks`.
- Refuse to create a `.git` directory: if the computed path's parent does not already exist as a valid gitdir, error instead of `create_dir_all`.
- Add `[angler.git_hooks] enabled` default-unchanged, but skip installation with a warning when the hooks dir would escape the project (monorepo detection: git root ≠ project root) unless `hooks_dir` is set explicitly.
- *Acceptance:* monorepo fixture test: no `scotia/.git` is created; with `hooks_dir` unset and monorepo detected, daemon logs one warning and continues.

**Exit criteria A:** all four regression tests green in CI; `cargo audit`/`deny` clean; CHANGELOG entry; tag `v9.7.17`.
**Effort:** ~3–4 days. **Owner:** core daemon. **No breaking changes** (warning-only for `All` mode).

---

## 4. Workstream B — Repo Model & Version Consistency (P1 → v9.8.0)

**Objective:** monorepo-subdirectory projects are first-class; version state is a single coherent triple.

### B1. `RepoContext` everywhere (#4, #6)
- Introduce `RepoContext { git_root: PathBuf, project_root: PathBuf }` computed once at startup (`git rev-parse --show-toplevel` for git_root; config resolution for project_root).
- Audit and fix every consumer that conflates the two — known sites: `scheduler.rs:645` (`commit_with_staging(repo.root(), …)` → pass `project_root`, resolve meta paths against it), `unstage_excluded`/`changed_paths` (status paths are git-root-relative; normalize before glob matching), `analyze.rs` (scope `changed_paths` to `project_root` — fixes the 264-path pollution, #6).
- *Acceptance:* monorepo integration test asserts VERSION + Cargo.toml + Cargo.lock all appear in auto-commits; `analyze` reports only in-project paths.

### B2. Version triple consistency (#8)
- `save_version` writes VERSION **and** Cargo.toml **and** updates Cargo.lock's own-package entry (or shells `cargo update -p <name> --offline`-style targeted lock refresh; fallback: document that lock refresh is deferred to the test hook, which runs `cargo test` and rewrites the lock — then stage the lock too).
- Add Cargo.lock to cluster meta-staging (`commit/orchestrator.rs` meta_paths).
- *Acceptance:* after any auto-commit, `git status` shows no version drift among the triple; `cargo build --locked` passes at every commit.

### B3. Startup reconciliation (#9)
- On boot, diff working tree vs HEAD **scoped to project_root**. If non-empty: form a single catch-up cluster through the normal pipeline (so it is scored, tested, and gated like any other).
- Config: `[watch] rescan_on_start = true` (default true).
- *Acceptance:* edit a file with daemon down, start daemon → exactly one catch-up commit (or one skip decision, logged).

### B4. Scoring calibration for metadata-only manifest edits (#18)
- The diff analyzer currently counts any Cargo.toml change as full dependency-graph churn (a repository-URL edit scored 20 dependency nodes). Split manifest diffs into: dependency-section changes (`[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, workspace deps) vs. metadata (`version`, `repository`, `description`, …). Only the former feeds the `deps` score; metadata feeds a low structural signal.
- *Acceptance:* golden test: URL-only Cargo.toml edit scores `deps = 0`; adding a dependency scores `deps > 0`.

**Exit criteria B:** monorepo fixture suite green; version-triple invariant holds across a 20-commit synthetic workload; `v9.8.0` tagged.
**Effort:** ~2 weeks. **Breaking:** none (behavior corrections only).

---

## 5. Workstream C — Operability (P2 → v9.8.0)

### C1. Graceful shutdown (#10)
- SIGTERM (and SIGINT) wired to the existing `CancellationToken`: stop ingesting, finish or abort the in-flight cluster within a bounded grace period (default 10s, `[daemon] shutdown_grace_secs`), write final `status.json`, remove `daemon.pid`.
- *Acceptance:* `kill <pid>` during a test-hook run exits 0 within grace; pid file removed; status shows `Stopped`, not frozen mid-state.

### C2. Crash-safe state (#12)
- `daemon.pid` validated on start (stale-pid detection via `kill(pid, 0)` / flock); stale files removed with a log line.
- `status.json` written atomically (tmp + rename); on load, a state older than the process start is treated as historical, never resumed.
- *Acceptance:* kill -9 mid-commit, restart → clean recovery, no phantom "Testing" state, no duplicate daemon.

### C3. Config hot reload (#11)
- Watch `kaptaind.toml` and the ignore file **as config** (excluded from clustering); on change, reload thresholds, weights, ignore matcher, and rate limits without restart. Reload failures keep the old config and warn.
- *Acceptance:* edit `.kaptainignore`, daemon logs reload, new pattern effective within one cluster window; invalid TOML leaves daemon running on previous config.

### C4. Decision transparency & dry-run (#7, #14, #19)
- Every cluster decision — commit **or skip** — appends one JSON line to `.kaptaind/decisions.jsonl`: scores, threshold used, bump, reason (`no_bump`, `test_failed`, `blocked`, …), paths.
- `kaptaind-cli explain [--last N]` renders decisions in human form; skip decisions name the exact threshold that was not met and the achieved score.
- `kaptaind --dry-run`: full pipeline minus staging/commit; prints the decision it *would* make.
- Move default `version_thresholds` and `weights` **into the generated `kaptaind.toml`** so they are visible and editable.
- *Acceptance:* a below-threshold docs edit produces a decision record and `explain` output naming score vs threshold; `--dry-run` on a fixture predicts the exact commit message and bump.

### C5. Test-gate backpressure (#13)
- `[test] required = true` stays, but failures become **visible**: decision record + notification + `status.last_error`, and repeated failures (≥3) surface a warning that commits are blocked.
- Add `[test] command_on = "always" | "code_only"` — docs-only clusters may skip the suite (they cannot break the build), keeping the gate cheap.
- *Acceptance:* red suite → zero commits, explicit status; green suite → normal flow; docs-only cluster with `code_only` skips tests.

**Exit criteria C:** all operability acceptance tests green; operator docs updated (`docs/` runbook: start/stop/recover/explain).
**Effort:** ~1.5 weeks, parallelizable with B.

---

## 6. Workstream D — Workflow Integrity (P3 → v10.0.0, breaking window)

### D1. Capture without inflation (#7, #15, #17)
- New `[commit] require_bump` (default `true` today; default flips to `false` in v10): below-threshold clusters still commit — capturing work — with a non-bumping message (`chore: <deterministic summary>`), while version only moves on threshold-crossing clusters. This ends both silent work loss (#7) and version inflation (#17) without timer-tuning.
- Keep cluster window configurable; document the trade-off; no change to default window.

### D2. Commit message quality (#16)
- Deterministic template upgrade: subject names the change class + primary paths (`fix(cli): update repository URLs (Cargo.toml, install-scotia.sh)`), body keeps the scorecard block. LLM inference stays optional and additive.
- *Acceptance:* messages pass a lint test (conventional-commit parseable; ≤72-char subject).

### D3. Default flips (breaking, v10.0.0)
- `staging.mode` default `all` → `cluster` (hard flip; the v9.7.17 warning becomes the migration path).
- `commit.require_bump` default `true` → `false`.
- Migration guide in CHANGELOG + `kaptaind-cli doctor` check that flags legacy configs.

**Exit criteria D:** v10.0.0 with migration notes; dogfood reports (below) clean for 2 weeks.

---

## 7. Testing Strategy (cross-cutting)

1. **Characterization-first.** Before each fix lands, commit a failing regression test that reproduces the finding. 20 findings → 20 named regression tests (some merge, e.g. cascade + self-suppression). They live in `tests/regressions/` and run in `rust.yml` on every PR.
2. **Monorepo fixture harness.** New `tests/fixtures/monorepo.rs`: builds a tempdir with an outer git repo, an in-repo subproject with its own `kaptaind.toml`, and untracked noise files. All F1–F3 tests use it. This is the harness we never had — every monorepo finding was invisible without it.
3. **Property-based testing.** Extend proptest to: version resolution monotonicity (A3), threshold decision determinism (C4), ignore-matcher semantics (prefix vs glob).
4. **Fault injection.** Kill -9 mid-commit / mid-test / mid-writeback; corrupt `status.json`; stale pid; fsnotify storm (1k events/s); clock skew on the cluster window. Assert: no duplicate commits, no lost commits beyond documented semantics, clean recovery.
5. **Chaos soak.** A nightly CI job runs the daemon against a synthetic workload generator for 30 minutes asserting the invariants: ≤1 commit per genuine cluster, version triple consistency, `cargo build --locked` green at every commit.
6. **Quality gates (unchanged-plus):** `cargo fmt --check`, `clippy -D warnings` (all feature combos), full test suite, `cargo audit`, `cargo deny`, fuzz-smoke — plus the new regression and soak suites. Bench-regression guard for the watcher/diff hot paths (daemon overhead must stay <1% CPU idle; measure).

---

## 8. Rollout & Migration

1. **Dogfood fleet first.** The author's own monitored repos (scotia, fract, kaptaind itself) upgrade to each release candidate for one week before the tag; a short dogfood report (commits made, decisions skipped, anomalies) gates promotion.
2. **Config migration, not config breakage.** v9.7.17 warns; v9.8.0 corrects behavior non-breaking; v10.0.0 flips defaults with a `kaptaind-cli doctor` migration checker and a CHANGELOG migration section. The existing scotia workarounds (`.kaptainignore` version-file entries, disabled git hooks) are removed during dogfood of v9.8.0-RC1 to prove the fixes make them unnecessary.
3. **Rollback plan.** Every phase ships behind the existing config surface; rollback = downgrade binary + restore previous `kaptaind.toml` (versioned in each project's git). No state migrations are irreversible; `decisions.jsonl` is append-only and tolerated by older versions (ignored if unknown).
4. **Compatibility contract.** From v9.8.0, `status.json`, `traces.db`, and `decisions.jsonl` schemas carry a `schema_version` field; readers ignore unknown fields.

---

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Self-write suppression drops a genuine user edit that shares a path with a daemon write | Medium | Medium | TTL-bounded suppression (60s), path+generation matching, split mixed events; fault-injection test covers the race |
| Lockfile refresh (B2) slows the commit path on large workspaces | Medium | Low | Targeted lock update of own package only; benchmark gate; fallback to test-hook refresh |
| Monorepo path normalization breaks existing standalone repos | Low | High | RepoContext defaults to git_root == project_root for standalone; full existing test suite must pass unmodified |
| `require_bump = false` (D1) produces noisy chore commits | Medium | Low | Opt-out in v9.8, default flip only at v10 after dogfood; message lint |
| Hot reload (C3) applies a half-valid config | Low | Medium | Validate-then-swap; on any error keep previous config, warn |
| Regression suite adds CI time | Medium | Low | Fixtures are tempdir-local, no network; parallel test jobs; target <2 min added |
| Behavior corrections change bump outcomes for existing users | Medium | Low | Called out in CHANGELOG; `explain`/decisions log makes new behavior auditable |

---

## 10. Traceability Matrix (finding → workstream → key test)

| # | Finding (abbreviated) | WS | Key regression test |
|---|----------------------|----|---------------------|
| 1 | `git add -A` default sweep | A1 | `default_staging_never_touches_unwatched` |
| 2 | Version regression from stale VERSION | A3 | `version_never_moves_backwards` (proptest) |
| 3 | Commit cascade on writeback | A2 | `daemon_does_not_cascade_on_version_writeback` |
| 4 | Meta-staging joins git root | B1 | `monorepo_version_files_are_committed` |
| 5 | Fake `.git` created | A4 | `no_git_dir_created_in_project_root` |
| 6 | Repo-wide diff pollution | B1 | `analyze_scopes_to_project_root` |
| 7 | Below-threshold work never commits | C4/D1 | `skip_decision_is_logged`; `chore_commit_captures_docs` |
| 8 | Lockfile drift after bump | B2 | `version_triple_consistent_after_every_commit` |
| 9 | Events lost during downtime | B3 | `catchup_cluster_on_start` |
| 10 | SIGTERM ignored | C1 | `sigterm_graceful_shutdown` |
| 11 | No config hot reload | C3 | `ignore_reload_without_restart` |
| 12 | Stale pid/status after crash | C2 | `crash_recovery_cleans_state` |
| 13 | Test gate silently blocks all commits | C5 | `test_failure_visible_in_status` |
| 14 | No dry-run | C4 | `dry_run_predicts_decision` |
| 15 | Clock-based commit granularity | D1 | (documented trade-off; lint on commit contents) |
| 16 | Template commit messages | D2 | `commit_message_lint` |
| 17 | Version inflation | D1 | `docs_edit_does_not_bump_when_require_bump_off` |
| 18 | Metadata edit scores as dep churn | B4 | `manifest_metadata_edit_scores_zero_deps` |
| 19 | Opaque thresholds | C4 | `generated_config_contains_thresholds` |
| 20 | Repo mutation on startup | A1/A4 | `startup_side_effects_are_opt_in` |

---

## 11. Timeline & Milestones (single senior engineer)

| Milestone | Content | Duration | Gate |
|-----------|---------|----------|------|
| **M0** | Workstream A + regression harness skeleton | 1 week | v9.7.17 tagged; 4 P0 tests in CI |
| **M1** | Workstream B | 2 weeks | Monorepo fixture suite green; triple invariant holds |
| **M2** | Workstream C | 1.5 weeks (overlaps M1 by 0.5) | Operability suite green; runbook published |
| **M3** | v9.8.0 RC dogfood on 3 repos | 1 week | Dogfood report: zero anomalies; scotia workarounds removed |
| **M4** | Workstream D + v10.0.0 | 2 weeks | Default flips; migration checker; CHANGELOG complete |

**Total: ~7–8 weeks elapsed.** M0 ships independently and removes all P0 exposure; everything after M0 is correctness-and-polish, not emergency work.

---

## 12. Definition of Done (program level)

- All 20 regression tests present and green in CI, each naming its finding.
- Chaos soak runs nightly for two consecutive weeks with zero invariant violations.
- `kaptaind --dry-run` output matches actual daemon decisions on the dogfood fleet for one week (sampled).
- No monitored repo carries daemon-specific workarounds in `.kaptainignore`/`kaptaind.toml` — the defaults are safe.
- CHANGELOG documents every behavior change; `kaptaind-cli doctor` flags every legacy pattern this plan retires.
