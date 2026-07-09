# Kaptaind — Roadmap to the First Stable Release (Validation & Qualification)

**Date:** 2026-07-09
**Companion to:** `STABLE_RELEASE_ROADMAP.md` (release engineering — executed). This document covers the **validation and qualification** work that *proves* the first stable is fit to ship: benchmarking, stress/soak testing, trace/log/audit inspection, hardware requirement definitions, containerised testing, and an enterprise-grade qualification report.
**Status:** Executed for v9.7.16 (2026-07-09). Workstreams A1 (divan micro-benches), B2 (`stress` smoke), C (`trace` extension), D1 (`logs`), D2 (`audit`), D3 (`probe`), E3 (`doctor`), F1 (Docker container proof), and G (`report` generator) are implemented and run in-session; the long soak (B3), full distro matrix (F1 beyond the release image), macro-bench `bench` command (A2), Sigstore/keyless signing, and the 24 h CI harness are explicitly flagged as harness/CI-provided in the qualification report. See `docs/releases/qualification/9.7.16.md` for measured evidence.

---

## 0. Ground truth (verified)

| Surface | Reality today |
|---|---|
| Benchmarks | `tests/benches/` exists but is **empty**; no `[[bench]]`, no `criterion`/`divan`/`iai` dependency. No micro or macro bench harness. |
| Stress/soak | No synthetic-fixture generator, no event-storm harness, no soak runner, no fault injection. |
| Trace inspection | `kaptaind-cli trace {log,show,prune}` reads `.kaptaind/traces.db` (SQLite: `traces(cluster_id, aoc_id, started_at, ended_at, duration_ms, data JSON)`). Functional but minimal — no filtering, export, histograms, or comparison. |
| Audit log | `.kaptaind/audit.jsonl` (append-only JSONL; `AuditEntry{timestamp,event_type,actor,result,details}`). **No CLI inspector.** |
| Daemon logs | `.kaptaind/daemon.out` / `daemon.err` (plain `tracing` text). **No CLI inspector** beyond `tail`. |
| Health/metrics | `/health`, `/metrics` (JSON), `/metrics/prometheus`, `/events` (SSE) on `health_port` (default `9090`). Scrapeable but no bundled probe tool. |
| Analysis artifact | `.kaptaind/analysis/<cluster>.json` = `AnalysisArtifact{cluster_id, version, bump, event_count, started_at, ended_at, diff, weight, air_gapped}`. |
| Containers | Docker 27.0.3 + Compose v2.27.1 present. No podman. **Verified 2026-07-09:** the release `Dockerfile` builds cleanly into `kaptaind:stable-proof` (clean `deckhand` git-dep build), `kaptaind-cli` runs inside, `init`→`analyze` smoke works, the daemon starts, and the image `HEALTHCHECK` reports `healthy`. |
| `~/vico-vee` | **Not a directory** (re-confirmed absent 2026-07-09). `~/.local/share/vico-vee` exists and holds vico's execution/artifact SQLite stores (`vee_artifacts.db` ~98MB, `vee_executions.db`, `vee_patterns.db`, `vee_capabilities.db`, `vee_revocations.db`). The `vico` binary (`~/.local/bin/vico`) does **not** expose `--help` or a `vee` subcommand, so its run/exec interface is unknown. → Treat vico-vee as an **optional artifact/execution recorder**, not as a container runtime. Containerised testing uses Docker/Compose; vico-vee integration is best-effort and feature-flagged (Workstream F). |
| Tooling (supply chain) | **Verified 2026-07-09:** `cargo-deny 0.19.9` and `cargo-audit` are installed and run against this repo (see Workstream G evidence). `cross`, `cosign`, `cargo-cyclonedx`, and `syft` are **not** installed locally — those run in CI (`.github/workflows/release.yml`, `security-audit.yml`), not in-session. |

**Implication:** the engine is feature-complete (per the release roadmap) but **unmeasured and unproven under load**. The first stable cannot be declared on feature breadth; it must be declared on measured evidence. That evidence is the deliverable here.

---

## 1. Validation principles & the stable gate

The first stable is gated on **evidence**, not assertions. Every claim in the qualification report (Workstream G) must be backed by an artifact produced by one of A–F.

Principles:
- **Reproducible:** every bench/stress/soak run is hermetic (fixed seed, pinned toolchain, containerised where possible) and emits a machine-readable result artifact.
- **Comparable:** each run records `git_rev`, toolchain, hardware profile, and config hash so results are diffable across commits (regression detection).
- **Inspectable:** every decision the daemon makes is reconstructable from `traces.db` + `audit.jsonl` + `analysis/*.json` via CLI tooling (C/D).
- **Bounded:** hardware requirements (E) are *measured*, not guessed; provisional targets below are ratified or replaced by Workstream A/B data.
- **Automated in CI:** a non-blocking qualification workflow runs A–F on demand and on `main`, publishing the report artifact; a blocking subset runs on every PR.

**Stable gate (all must hold):**
1. No correctness regression vs. the current claim-tests (`tests/claims_validation.rs`) across all synthetic fixtures.
2. Throughput/latency within the budgets in §2.2 on the reference hardware tier (§6).
3. Soak runs (§3.3) complete with **zero** data-loss, **zero** runaway version inflation, and bounded memory/FD growth.
4. Trace/audit/log inspectors (C/D) reconstruct any sampled decision end-to-end.
5. Container matrix (F) green on every supported distro.
6. Qualification report (G) generated, signed off, and attached to the release.

---

## 2. Workstream A — Benchmarking

**Goal:** turn "fast enough" into measured, regression-guarded numbers.

### A1. Micro-benchmarks (hot paths)
- Add `divan = "0.1"` to `[dev-dependencies]` (lightweight, stable-`rustc`, no nightly). Create `tests/benches/` modules:
  - `bench_cluster.rs` — `ClusterEngine` push/emit throughput at 1e3/1e5/1e6 events; adaptive vs. fixed window.
  - `bench_diff.rs` — `diff::analyze` over synthetic working trees (Rust/TS/Python/Go) at 10/100/1000 files; with and without AST cache warm.
  - `bench_adapters.rs` — per-language adapter parse time + cache hit rate (uses `tests/fixtures/adapters`).
  - `bench_weight_version.rs` — `weight::calculator` + `version::semver::decide` (should be ~constant; guard against accidental O(n)).
- `[[bench]]` entries in `Cargo.toml` with `harness = false` for divan.

### A2. Macro-benchmarks (end-to-end, realistic)
- A Rust binary `src/cli/commands/bench.rs` exposed as `kaptaind-cli bench` that:
  1. Generates a deterministic synthetic repo (seeded; configurable language mix, file count, size) into a temp dir via the fixture generator (§3.1).
  2. Runs the **real** pipeline: cluster → diff → weight → version → (no commit) over N change batches.
  3. Records per-stage latency (cluster wait, diff, parse, version), throughput (events/sec, files/sec), cache hit rate, peak RSS, and the bump distribution.
  4. Emits `.kaptaind/bench/<run-id>.json` (schema §8) and a human summary.
- Scenarios: `small-rust`, `mixed-monorepo`, `ts-nextjs`, `python-ml`, `all-languages` (one of each supported adapter), each at S/M/L sizes.

### A2.1 Budgets (provisional targets — ratify in Phase B)
| Metric | Target (reference tier §6) | Rationale |
|---|---|---|
| Diff latency p95, 100-file mixed change | < 250 ms | Interactive feel on save |
| Diff latency p95, 1000-file change | < 2.5 s | Acceptable batch commit |
| Adapter parse throughput (warm cache) | > 500 files/s | Large-repo keep-up |
| AST cache hit rate (incremental edits) | > 80 % | Documented claim |
| Cluster→commit decision latency p99 | < cluster.window + 1 s | Responsiveness bound |
| Peak RSS, 100k-file watched tree | < 512 MB | Home-user footprint |

### A3. Regression policy
- Store last-known-good (LKG) bench JSON per scenario in `tests/bench-baselines/`.
- `kaptaind-cli bench --compare` diffs current vs. LKG; **>15 % latency regression or >10 % throughput drop fails** (configurable).
- CI job `bench` (`.github/workflows/bench.yml`): runs on `main` + `workflow_dispatch` (not every PR — too slow), uploads JSON artifacts, comments regressions.

---

## 3. Workstream B — Stress & soak testing

**Goal:** prove the daemon degrades gracefully and never corrupts/loses data under adversarial load.

### B1. Synthetic fixture generator
- New crate module `src/testutil/fixgen.rs` (+ `kaptaind-cli gen-fixtures`) producing reproducible repos:
  - Knobs: `--files`, `--dirs`, `--langs rust,ts,py,go`, `--size-bytes`, `--edit-batches`, `--burst-rate`, `--seed`.
  - Emits valid, parseable source per language (so adapters actually exercise API detection) and matching manifests (`Cargo.toml`/`package.json`/etc.).
- Stored corpora in `tests/fixtures/stress/{small,medium,large,xlarge}` generated by CI and cached; the xlarge corpus generated on demand (not committed).

### B2. Event-storm & adversarial cases
- `kaptaind-cli stress` drives the daemon against a fixture with:
  - **Save storms:** 1k–50k file events at configurable rates (test clustering/adaptive window under burst).
  - **Churn:** rapid modify→revert sequences (must not create empty commits; version must stay monotone).
  - **Rename/delete storms** and **binary/blob** changes (must be ignored/handled, not crash).
  - **Adversarial:** huge single file, deep dir nesting, symlinks, permission-denied files, non-UTF8 filenames, `.kaptainignore` boundaries, lock-file-only changes.
  - **Hook failures:** failing/slow `[test]` hook (must block/timeout correctly, no partial commits).
  - **Git pressure:** shallow clone, detached HEAD, protected branch push disabled, concurrent external commits.
- Assertions per run: no panic, no data loss (every committed change is reconstructable), version is monotone and matches the last analysis artifact, `.kaptaind/` stays within size bounds, no fd/thread leak (compare before/after).

### B3. Long soak
- `kaptaind-cli soak --duration 24h --rate 20/s --fixtures mixed` runs continuously, periodically sampling `/metrics`, RSS, fd count, and `audit.jsonl`.
- Pass criteria: stable RSS (no monotonic growth beyond a small plateau), stable fd count, no error-rate growth, qualification decisions remain consistent, daemon recovers from injected `SIGTERM`/restart without losing the in-flight cluster (idempotent resume).
- Run in container (F) for hermetic resource limits; emit `.kaptaind/soak/<run-id>.json`.

---

## 4. Workstream C — Trace inspection

**Goal:** any automated decision is explainable from artifacts, via CLI.

Extend `kaptaind-cli trace` (module `src/cli/commands/trace.rs`):
- `trace show <cluster>` — add **full payload** view: the diff breakdown (structural/api/deps/runtime/bundle), weight math, threshold decision, parse metadata (language/version/parser/confidence per file), and the exact `VERSION` mutation. Today it shows paths/agent only.
- `trace list` — replace `trace log` with filtering: `--aoc`, `--since/--until`, `--bump Major|Minor|Patch|None`, `--min-score`, `--result committed|skipped`, `--format text|json|csv`.
- `trace export <cluster> --format json|html` — portable, reviewable trace (HTML renders a one-page decision card for audits).
- `trace stats` — histograms: bump distribution, score distribution, parse confidence distribution, mean/p95 `duration_ms`, cache hit rate; correlates with `stability.json`.
- `trace verify <cluster>` — re-derive the decision from the stored diff + current thresholds and assert it matches the recorded `bump` (determinism check / tamper evidence).
- `trace diff <a> <b>` — compare two traces (regression review).

Cross-link: `trace show` surfaces the matching `audit.jsonl` entry (by `cluster_id`) and the `analysis/<cluster>.json` path.

---

## 5. Workstream D — Log & audit inspection

**Goal:** first-class, queryable inspection of runtime logs and the compliance audit trail.

### D1. Daemon logs — `kaptaind-cli logs`
- New `src/cli/commands/logs.rs`. Sources: `.kaptaind/daemon.out`, `daemon.err`.
- `logs tail [-n N]`, `logs follow` (tail -f with rotation awareness), `logs errors` (filter `ERROR`/`WARN`), `logs grep <regex>`, `logs since <ts>`, `--format text|json`.
- Prefer structured: add an opt-in `tracing` JSON layer (`[logging] format = "json"`) writing `daemon.jsonl` so inspectors can filter by `level/target/span` without regex. Keep text as default for humans.

### D2. Audit trail — `kaptaind-cli audit`
- New `src/cli/commands/audit.rs` over `.kaptaind/audit.jsonl` (`AuditEntry`).
- `audit tail [-n N]`, `audit show <cluster|event-id>`, `audit stats` (counts by `event_type`/`result`, failure rate over window), `audit filter --event --actor --result --since --format json`.
- `audit verify` — integrity check: confirm append-only ordering by timestamp, detect gaps/truncation, and (when signing is enabled) verify per-entry hashes/signatures. Add a `prev_hash` chain to `AuditEntry` (additive, serde-default) so tampering is detectable; backfilled as `null` for legacy entries.

### D3. Health/metrics scraper — `kaptaind-cli probe`
- New `src/cli/commands/probe.rs`: `probe health`, `probe metrics [--prometheus]`, `probe events --follow` (SSE tail), `probe watch` (loop printing clusters/commits/stability). Wraps the existing endpoints so operators (and the soak harness) don't hand-curl.

---

## 6. Workstream E — Hardware requirement definitions

**Goal:** publish *measured* minimum/recommended specs per repo tier, with the method that produced them.

### E1. Repo-size tiers (input to benchmarks)
| Tier | Files | Working tree | Change rate | Example |
|---|---|---|---|---|
| T0 — tiny | ≤ 500 | ≤ 25 MB | low | single crate / small app |
| T1 — small | ≤ 5k | ≤ 250 MB | moderate | typical web app |
| T2 — medium | ≤ 50k | ≤ 2 GB | high | monorepo |
| T3 — large | ≤ 250k | ≤ 10 GB | burst | large monorepo |
| T4 — very large | > 250k | > 10 GB | burst | mega-monorepo (best-effort) |

### E2. Provisional hardware guidance (RATIFY via A/B — not final)
| Tier | CPU (min / rec) | RAM (min / rec) | Disk (min / rec) | Notes |
|---|---|---|---|---|
| T0 | 1 / 2 vCPU | 256 MB / 512 MB | 1× tree / 2× tree | inotify default limits fine |
| T1 | 1 / 2 vCPU | 512 MB / 1 GB | 2× tree / 4× tree | raise `fs.inotify.max_user_watches` to ~64k |
| T2 | 2 / 4 vCPU | 1 GB / 2 GB | 4× tree / SSD | `cluster`/`pattern` staging advised; raise watches to ~512k |
| T3 | 4 / 8 vCPU | 2 GB / 4 GB | SSD, 4× tree | bundle scoring off; adaptive clustering on |
| T4 | 8+ vCPU | 4 GB+ | NVMe | best-effort; expect to tune watcher/ignore |

> **Do not publish these as final.** Each cell is a hypothesis to be confirmed or replaced by Workstream A/B measurements on the reference host (§6.3). The qualification report records the *measured* values and the host they were measured on.

### E3. Measurement method & `kaptaind-cli doctor`
- Reference host profile captured into every bench/stress artifact: CPU model/cores, RAM, disk type, OS/kernel, `fs.inotify.*` limits, container limits (if any).
- New `src/cli/commands/doctor.rs` (`kaptaind-cli doctor`): reports the host profile, checks inotify/watch limits against the tier table, verifies `git`/Rust/DB availability, warns on low limits, and prints the recommended tier + config tweaks. Used by the report (G) and by users self-qualifying hardware.
- Platform notes captured: inotify (Linux) watch limits, FSEvents (macOS) coalescing, ReadDirectoryChangesW (Windows) large-tree lag — each mapped to config guidance.

---

## 7. Workstream F — Containerised testing

**Goal:** prove the stable builds and runs across supported distros, hermetically, with resource limits.

### F1. Test matrix (Docker/Compose)
- `tests/containers/` with per-distro Dockerfiles and a `compose.test.yml`:
  - `ubuntu:24.04`, `debian:bookworm-slim`, `fedora:40`, `alpine:3.20` (musl check), plus the release `Dockerfile`.
  - Each: install pinned toolchain (via `rust-toolchain.toml`), build `kaptaind` + `kaptaind-cli`, run `cargo test`, then a smoke scenario: `init` → synthetic fixture → daemon foreground for N seconds → assert commits/version/artifacts → `rollback` → assert clean.
- Resource limits per tier (E2) applied via Compose `deploy.resources` to validate min-spec claims (e.g., run T1 scenario capped at 512 MB and confirm it works or document the floor).
- CI job `containers` (`.github/workflows/containers.yml`): matrix over distros, runs on `main` + `workflow_dispatch`, publishes logs as artifacts.

### F2. vico-vee integration (optional, best-effort)
- Reality: `~/.local/share/vico-vee` is vico's SQLite-backed execution/artifact store; the `vico` CLI exposes no documented run interface. **Do not block the release on it.**
- Provide a thin, feature-flagged hook: `kaptaind-cli report --vico-vee` that, *if* a `vico` invocation to record an execution/artifact is discovered, posts the run artifact (bench/soak/report JSON) into `vee_executions.db`/`vee_artifacts.db`; otherwise it no-ops with a clear message. Discover the real interface via `vico --help`/`vico list` during execution; until then the harness records artifacts to the local filesystem and (optionally) copies them into `~/.local/share/vico-vee/artifacts/` as plain files for vico to index.
- Containerised runs remain runnable with **only** Docker; vico-vee is an accelerator/recorder, never a requirement.

---

## 8. Workstream G — Enterprise-grade qualification report

**Goal:** a signed-off, reproducible *Stable Release Qualification Report* attached to the first stable release.

### G1. Generator — `kaptaind-cli report`
- New `src/cli/commands/report.rs`: aggregates artifacts from A–F into `report-<version>-<date>.{md,json}`:
  - Inputs: `bench/*.json`, `soak/*.json`, `stress/*.json`, `doctor` host profile, `cargo-deny`/`cargo-audit`/`npm-audit` outputs, clippy/test results, container matrix results, git rev + dirty flag, config hash.
  - Verdict per section: `PASS` / `PASS-WITH-NOTES` / `FAIL`, each linked to the evidence artifact.
  - Output paths: `docs/releases/qualification/<version>.md` (human) + `.kaptaind/report/<run-id>.json` (machine). Markdown is committed for the release; JSON is the evidence bundle.

### G2. Report schema (JSON, illustrative)
```json
{
  "schema": "kaptaind.qualification.v1",
  "version": "9.7.16",
  "git": { "rev": "<sha>", "dirty": false },
  "generated_at": "<rfc3339>",
  "host": { "cpu": "…", "cores": 8, "ram_gb": 16, "disk": "nvme", "os": "…", "container": null },
  "toolchain": { "rustc": "stable …", "cargo_deny": "…" },
  "sections": {
    "correctness":   { "verdict": "PASS", "evidence": ["cargo-test.json", "claims.json"] },
    "benchmarks":    { "verdict": "PASS", "evidence": ["bench/<id>.json"], "vs_baseline": "within 15%" },
    "stress":        { "verdict": "PASS", "evidence": ["stress/<id>.json"] },
    "soak":          { "verdict": "PASS-WITH-NOTES", "evidence": ["soak/<id>.json"], "notes": "RSS plateau 312MB" },
    "inspection":    { "verdict": "PASS", "evidence": ["trace-verify.json", "audit-verify.json"] },
    "containers":    { "verdict": "PASS", "evidence": ["containers/<distro>.log"] },
    "security":      { "verdict": "PASS", "evidence": ["cargo-deny.json", "cargo-audit.json", "npm-audit.json"] },
    "hardware":      { "verdict": "PASS", "evidence": ["doctor.json"], "tiers": "E2-measured" }
  },
  "overall": "PASS",
  "sign_off": { "prepared_by": "<actor>", "approved_by": null, "approved_at": null }
}
```

### G3. Sign-off checklist (embedded in the .md)
- [ ] All sections `PASS` or `PASS-WITH-NOTES` (no `FAIL`).
- [ ] Hardware tiers (§6.2) replaced with measured values and the host recorded.
- [ ] No open high/critical advisories (`cargo-deny`/`cargo-audit`/`npm-audit`).
- [ ] Container matrix green on every supported distro.
- [ ] Soak ≥ 24 h with bounded memory/fd and zero data loss.
- [ ] Trace/audit determinism checks (`trace verify`, `audit verify`) pass on a sampled set.
- [ ] Prepared-by and approved-by filled; approved-by is a named maintainer (CODEOWNERS).
- [ ] Report `.md` committed under `docs/releases/qualification/` and linked from the GitHub Release.

---

## 9. Sequencing to the first stable

| Phase | Workstreams | Output | ~Effort |
|---|---|---|---|
| **V1 — Instrument** | A1 micro-benches; D3 probe; E3 doctor (host profile) | Measurable baselines; host capture in every artifact | 2–3 d |
| **V2 — Generate & inspect** | B1 fixture generator; C trace extensions; D1 logs; D2 audit (incl. hash chain) | Reproducible fixtures; full decision reconstruction | 3–4 d |
| **V3 — Stress & soak** | A2 macro-bench/`bench`; B2 `stress`; B3 `soak`; A3 baselines | Budgets ratified; regression policy live | 3–4 d |
| **V4 — Containers** | F1 distro matrix + resource caps; F2 vico-vee hook (best-effort) | Cross-distro proof; min-spec validated | 2–3 d |
| **V5 — Qualify** | E2 measured tiers; G report generator + sign-off | Qualification report v1 attached to RC | 2 d |
| **V6 — RC → GA** | Run full suite on RC; sign off; attach report to release | First stable, evidence-backed | 1–2 d |

**Total: ~2–3 focused weeks** (parallelizable across Rust-tooling vs. container vs. docs tracks). Depends on the release-engineering roadmap (already executed) for the artifacts the report references.

---

## 10. Risks & notes

- **Don't invent hardware numbers.** §6.2 are hypotheses; the report must carry measured values or explicitly mark tiers "unmeasured." Publishing guessed specs would undermine the "enterprise-grade" claim.
- **Bench determinism.** Filesystem/notify timing is noisy — pin seeds, warm caches explicitly, run multiple iterations, and report p50/p95 with variance; gate regressions on relative %, not absolutes, and only on the reference host.
- **Soak cost.** 24 h soaks are expensive; run them containerised on `main`/schedule, not per-PR. Per-PR gets the smoke + micro-bench subset.
- **vico-vee coupling.** Keep it optional and behind a discoverable interface; never let an unknown internal tool block the public stable.
- **Additive audit schema.** Adding `prev_hash` to `AuditEntry` must be `#[serde(default)]` so legacy `audit.jsonl` entries still parse; `audit verify` treats `null` as "pre-chain".
- **Resource-floor honesty.** If a tier fails at its provisional min-RAM during F1, the correct outcome is to *raise the documented floor*, not to fudge the test.

---

## 11. Immediate next step

Start **V1 (Instrument)**: add `divan`, the first three `tests/benches/` modules, `kaptaind-cli doctor` (host profile), and `kaptaind-cli probe`. These unblock every later workstream by establishing the host-capture and measurement conventions the report will aggregate. Confirm and I'll execute V1.
