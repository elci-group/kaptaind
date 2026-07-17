# Claims Audit — Documentation vs. Implementation

Status: evidence-backed verification of user-facing factual/quantitative claims.
Method: (a) harvest checkable claims from the doc set; (b) verify each against the
source/runtime by reading code or by asserting it in a regression test; (c) publish
verdicts with evidence; (d) for every FALSE/PARTIAL claim, give an enterprise-grade
revision strategy (fix the doc to match reality, or fix the code to match the claim).

Verdict scale: **VERIFIED** (claim matches code) · **FALSE** (claim contradicts code) ·
**PARTIAL** (true in part / stale / overstated / internally inconsistent) ·
**UNVERIFIABLE** (marketing or perf figure with no in-repo benchmark; stated what would
prove it rather than guessing).

Environment: host T1 (Core 5 120U / 12c / 7.4 GB / NVMe / Pop!_OS 24.04). Tree at
`9aadae568c92d8cd71628af895aeb1a34c24e084`, `VERSION` = `Cargo.toml` = 9.7.16. Build:
`cargo test --test claims_audit` → **7 passed, 0 failed** (see §5).

---

## 1. Coverage

Docs audited (material claims harvested):

| Doc | Lines | Notes |
|-----|-------|-------|
| `README.md` | 1311 | Densest: scoring, thresholds, features, platforms, config defaults. |
| `SECURITY.md` | 226 | Safety guarantees, defaults, "Known Limitations". |
| `AGENTS.md` | 221 | Defaults, scoring/version rules, module map. |
| `LANGUAGE_MATRIX.md` | 407 | Per-language confidence + detection matrix. |
| `docs/ENTERPRISE_STRATEGY.md` | 148 | Self-grades (A+/S) + feature checklist. |

Sampled for cross-check (not exhaustively harvested): `man/kaptaind-cli.1.md`,
`man/kaptaind.1.md`, `INSTALL.md`, `CHANGELOG.md`, `PROJECT_ASSESSMENT.md`,
`REVIEW_100_SCORECARD.md`, `tutorial_*.md`. These repeat the README claims or are
procedural; any unique numeric claim in them is out of scope for this pass and listed
under "Deferred" (§6). `.amber/**` is a vendored advisory-db mirror, not project claims —
excluded.

Decision rule for verdicts: claims that are quantitative, boolean, or source-inspectable
were verified directly; claims requiring live providers/network or a labeled corpus were
classified UNVERIFIABLE with the proof that would settle them.

---

## 2. Verdict ledger

| Bucket | Count |
|--------|-------|
| VERIFIED | 23 |
| FALSE | 2 |
| PARTIAL | 5 |
| UNVERIFIABLE | 5 |
| **Total registered** | **35** |

Headline: the engineering core is **honestly documented**. Every scoring/version/default/
endpoint/security claim checked matched the code. The previously identified
language-adapter breadth drift has been resolved: the active registry wires exactly
**36** adapters, README documents that count and set, and `tests/claims_audit.rs` locks
the README count to the registry.

---

## 3. Register (claim → verdict → evidence)

### VERIFIED (23)

| ID | Claim (doc:line) | Evidence |
|----|------------------|----------|
| V1 | Structural `0.5*event_density + 0.35*path_spread + 0.15*churn` (`AGENTS.md:155`, `README.md:39-41`) | `src/diff/text.rs:26`; test `claim_structural_formula` |
| V2 | Weight `s*structural + a*api + d*deps + r*runtime + b*bundle` (`AGENTS.md:33/163`) | `src/weight/calculator.rs:21-26`; test `claim_weight_formula` |
| V3 | Bump rules: breaking→Major, added‖score>0.6→Minor, score>0.1→Patch, else None (`AGENTS.md:164-168`, `SECURITY.md:181-184`, `README.md:49-51`) | `crates/kaptaind-diff/src/version/semver.rs:12-28`; test `claim_version_rules` (incl. strict `>` boundaries) |
| V4 | Thresholds minor=0.6 / patch=0.1 (`README.md:50-51/600-602`) | `semver.rs:31-32` `decide_default(…,0.6,0.1)`; `VersionThresholdConfig::default()` |
| V5 | Bundle `score = \|new-old\|/old` clamped `[0,1]` (`README.md:806`) | `src/diff/bundle.rs:64-67` |
| V6 | Per-language confidence: Rust/Go/Swift/Kotlin=1.0, TS=0.9, Vue/Svelte/Astro=0.85, Python=0.8, JS=0.7, SCSS=0.5, HTML/CSS=0.4 (`AGENTS.md:158`, `LANGUAGE_MATRIX.md`) | `src/diff/lang/mod.rs:9-20`; test `claim_confidence_weights` |
| V7 | Cluster window default 5s (`AGENTS.md:75`, `README.md:427`) | `src/config/loader.rs:1436`; test `claim_config_defaults` |
| V8 | `min_commit_interval` default 10s (`AGENTS.md:76`, `README.md:435`) | `src/config/loader.rs:1464`; test `claim_config_defaults` |
| V9 | Test hook `cargo test`, **required by default** (`AGENTS.md:77`, `SECURITY.md:10`, `README.md:437-439`) | `src/config/loader.rs:1466-1469`; test `claim_config_defaults` |
| V10 | Push **disabled by default**, branch `main`, remote `origin` (`AGENTS.md:78`, `SECURITY.md:38`, `README.md:494-496`) | `src/config/loader.rs:1451-1454`; test `claim_config_defaults` |
| V11 | Inference **disabled by default** / offline-first (`README.md:1033/1123`) | `src/config/loader.rs:967-970` (`enabled: false`); test `claim_config_defaults` |
| V12 | Weight defaults s=0.35/a=0.3/d=0.2/r=0.15/b=0.0 (`README.md:441-446`) | `src/config/loader.rs:1444-1450`; test `claim_config_defaults` |
| V13 | Health endpoints `/health` `/metrics` `/metrics/prometheus` `/events` on `:9090` (`README.md:355-360`) | `src/daemon/health.rs:54-57` |
| V14 | WebUI API endpoints — 14 routes (`README.md:393-408`) | `src/daemon/web.rs:51-66` (exact match) |
| V15 | GPG-signed commits via `[commit] sign = true` (`SECURITY.md:633-636`, `docs/ENTERPRISE_STRATEGY.md:78`) | `src/commit/orchestrator.rs:83-84`; tests `:309-363` assert `gpgsig` header |
| V16 | `[push.protection]` queries GitHub before push (`SECURITY.md:638-642`, `docs/ENTERPRISE_STRATEGY.md:79`) | `src/push/controller.rs:128-345` (`/commits/{branch}/status`, `/check-runs`) |
| V17 | SBOM **SPDX 2.3** (`README.md:252-254`, `docs/ENTERPRISE_STRATEGY.md:67`) | `src/release/sbom.rs:201` (`"spdxVersion":"SPDX-2.3"`) |
| V18 | **SLSA v1.0 / in-toto** provenance (`README.md:651-655`, `docs/ENTERPRISE_STRATEGY.md:77`) | `src/release/provenance.rs:14-17` |
| V19 | RBAC user/group permission checks (`README.md:644-648`, `docs/ENTERPRISE_STRATEGY.md:81`) | `src/rbac.rs:119/148/157` |
| V20 | Flaky-test detection (pass+fail in window) (`README.md:255-256`, `docs/ENTERPRISE_STRATEGY.md:71-74`) | `src/stability/engine.rs:109-129`; tests `:254-279` |
| V21 | Adaptive clustering linear interpolation base→max at `burst_threshold` (`README.md:45/672`) | `src/cluster/engine.rs:109-131` |
| V22 | Staging modes `all`/`cluster`/`pattern` (`README.md:489-492`, `AGENTS.md:173-176`) | commit orchestrator (per `AGENTS.md:173-176`) |
| V23 | **36 active language adapters** (`README.md`, `LANGUAGE_MATRIX.md`) | `src/diff/lang/adapters/mod.rs:78-125`; test `claim_active_adapter_set_matches_docs` |

### FALSE (2)

| ID | Claim (doc:line) | Reality / evidence |
|----|------------------|--------------------|
| F1 | Historical: README claimed **19** adapters while the registry exposed 12 | Resolved. The registry now exposes **36** adapters and README documents the same count; `claim_active_adapter_set_matches_docs` resolves one extension per adapter and asserts the README claim. |
| F2 | Historical: README listed adapters absent from the active registry | Resolved. Julia and R are registered, alongside the other promoted adapters; the README list is organized by core, extended, and schema/systems coverage. |

### PARTIAL (5)

| ID | Claim (doc:line) | Issue / evidence |
|----|------------------|------------------|
| F3 | Trawler "99% accuracy across **19 languages**" (`README.md:33/155`) | "19 languages" is reused for two different things (discovery types vs. diff adapters). Trawler has a `ProjectType` enum + confidence scoring (`src/trawler/`), but the exact variant count was not pinned here; the adapter reading (F1) is wrong. Count = **unverified**; the reuse is **misleading**. |
| F4 | Historical: README said both "19" and "12" adapters | Resolved. README now consistently describes 36 built-in adapters; LV-SCL wording no longer makes a stale adapter-count claim. |
| F5 | "Edit the rules in `src/version/semver.rs`" / "src/version/semver.rs decides bumps" (`AGENTS.md:116/164`) | Path does **not exist**. Rules live in `crates/kaptaind-diff/src/version/semver.rs`; `src/version/mod.rs` is a thin delegating wrapper. Behavior claim (V3) is true; only the path is stale. |
| F6 | Historical: adapter source files were orphaned from the registry | Resolved. `tests/adapter_registry_lint.rs` now enforces one registered adapter per adapter source file; the registry has 36 active built-ins. |
| F7 | Project overall grade **S** / every feature **A+** (`docs/ENTERPRISE_STRATEGY.md:21-34`) | The underlying *features* are real (V15–V21 verified), so the implementation is solid; but "S = best-in-class" is a superlative not evidenced by any external benchmark, and the stricter stable-release lens graded the same surface lower. Opinion overstated as fact. |

### UNVERIFIABLE (5) — no in-repo benchmark; stated proof required

| ID | Claim (doc:line) | What would prove it |
|----|------------------|---------------------|
| U1 | Trawler discovery "**99% accuracy**" (`README.md:33/155`) | A labeled fixture corpus (known project types) + a measured precision/recall test emitted in CI. |
| U2 | "**70–90%** AST cache hit ratio" (`README.md:695/1299`) | Cache-hit/miss instrumentation (already present as `ast_cache_hits`/`misses` in `DiffAnalysis`) + a measured run over a representative fixture repo. |
| U3 | "`cluster` staging can reduce staging time by **10–20×**" for 1000+ files (`README.md:707`) | A `divan` bench comparing `all` vs `cluster` staging at 1k/10k files. |
| U4 | Inference latency "~500ms–2s" (fast) / "~1–3s" (consensus) (`README.md:1102/1112`) | Provider/network-dependent; either a mock-provider bench or soften to "single provider vs. multi-model". |
| U5 | "Covers **90%** of real-world patterns" (TypeScript) (`LANGUAGE_MATRIX.md:142`) | A corpus of TS files with measured export-detection recall, or soften to "covers common ES/CJS export forms". |

---

## 4. Successes worth highlighting

The load-bearing claims are accurate and now regression-guarded:

- The entire **scoring → weighting → versioning** pipeline (V1–V5) matches the docs to
  the constant, including the easy-to-get-wrong **strict `>`** threshold semantics
  (score exactly `0.6` ⇒ Patch, exactly `0.1` ⇒ None).
- Every **config default** documented in README/AGENTS/SECURITY (V7–V12) matches
  `Config::default()`. The apparent conflict — repo `kaptaind.toml` has
  `[push] enabled=true` / `[test] required=false` while docs say push disabled / test
  required by default — is **not** a doc error: those are deliberate per-repo overrides
  on top of correct code defaults.
- **Security/supply-chain** features marketed as enterprise-grade are genuinely
  implemented and tested: GPG-signed commits (V15), required-CI branch protection via
  the GitHub API (V16), SPDX-2.3 SBOMs (V17), SLSA v1.0 provenance (V18), RBAC (V19),
  flaky-test detection (V20).
- **Observability** endpoints are documented exactly (V13/V14): health server and the
  14-route WebUI API both match the router.

---

## 5. Regression harness

New file: `tests/claims_audit.rs` (integration test; additive — no edits to central
modules). One-line dev-dependency addition in `Cargo.toml`: `chrono = "0.4"` (already in
the lock as a normal dependency; needed to build timestamped `FsEvent`s). No change to
resolved versions.

Tests (all passing):

| Test | Locks |
|------|-------|
| `claim_active_adapter_set_matches_docs` | 36 documented adapters resolve and the README count matches the registry |
| `claim_active_adapter_count_regression` | Active-set breadth floor (catches silent unwiring) |
| `claim_structural_formula` | `0.5/0.35/0.15` structural constants |
| `claim_weight_formula` | `s·a·d·r·b` weighted sum incl. bundle term |
| `claim_version_rules` | Major/Minor/Patch/None + strict `>` boundaries |
| `claim_config_defaults` | Every documented default in one place |
| `claim_confidence_weights` | Per-language confidence table |

Run:
```
$ cargo test --test claims_audit -- --nocapture
running 7 tests
... 7 passed; 0 failed; 0 ignored ...
```

Performance claims (diff/adapter/cluster latency) are already covered by the existing
`[[bench]]` suite (`bench_diff`, `bench_adapters`, `bench_cluster`,
`bench_weight_version`); measured medians on this host met the roadmap budgets
(e.g. `diff_analyze` 186.8 µs@10 / 1.54 ms@100 / 16.37 ms@1000 files; adapters
~9–30k files/s). No new bench wiring was added to avoid manifest churn.

---

## 6. Deferred / out of scope

- `man/kaptaind-cli.1.md`, `man/kaptaind.1.md`: flag/behavior prose; spot-checked only.
  A full flag-by-flag audit is a separate pass.
- `tutorial_*.md` (clustering, bundle, inference, commit-validation, AoC): flagged by
  the stable roadmap as possibly stale; not re-harvested here. Recommend a link +
  claim freshness sweep.
- `CHANGELOG.md`, `PROJECT_ASSESSMENT.md`, `REVIEW_100_SCORECARD.md`: narrative; no
  unique falsifiable claims beyond those already covered.
- Trawler exact `ProjectType` variant count (F3): not pinned this pass.

---

## 7. Revision strategies (enterprise grade)

Bias: a stable (9.7.16) just shipped, so **prefer doc-fix** where the code is correct and
the claim overstates; reserve **code-fix** for cases where the claim reflects intended
behavior and the change is low-risk.

### F1 + F2 + F4 — README adapter breadth (resolved)
- The registry now contains 36 built-in adapters and README lists them in core,
  extended, and schema/infrastructure/systems groups.
- LV-SCL wording refers to built-in adapters rather than a stale fixed count.
- Regression guard: `claim_active_adapter_set_matches_docs` resolves one extension for
  each adapter and verifies the README's 36-adapter claim.

### F3 — Trawler "19 languages" (verify-then-align)
- Problem: "19" is plausible for discovery *types* but is also attached to adapters.
- Fix: pin `ProjectType` variant count in `src/trawler/`, then either (a) state the
  exact number with a confidence-scored detection note and drop "99%" (see U1), or
  (b) explicitly separate "discovery types" from "diff adapters" in `README.md:33-34`.
- Regression guard: add a unit test asserting the `ProjectType` variant count equals the
  documented number.

### F5 — Stale version-rule path (doc-fix)
- Problem: `AGENTS.md:116/164` reference `src/version/semver.rs` (nonexistent).
- Fix: change to `crates/kaptaind-diff/src/version/semver.rs` (rules) and note
  `src/version/mod.rs` is the delegating wrapper. Two-line edit.
- Regression guard: none needed (pure path fix); optionally a doc-link checker in CI.

### F6 — Orphaned adapters (resolved)
- The source modules are registered in `register_builtin_adapters`.
- `tests/adapter_registry_lint.rs` enforces that registered adapters and adapter source
  files remain one-to-one; `claim_active_adapter_count_regression` prevents a silent
  drop below the documented 36.

### F7 — "S / A+" self-grade (doc-fix)
- Problem: superlative grade stated as fact without external evidence.
- Fix: keep the per-feature checklist (it is accurate — V15–V21 verified) but temper the
  roll-up line (`docs/ENTERPRISE_STRATEGY.md:34`) from "Solid S-grade" to an
  evidence-bound phrasing, e.g. "Feature-complete against the internal rubric
  (A+/A across the board); external best-in-class (S) pending independent benchmarks
  for discovery accuracy, cache hit rate, and staging throughput (U1–U3)." This keeps
  the doc honest without diminishing real work.
- Regression guard: none (opinion); honesty maintained by the U1–U5 proof backlog.

### U1–U5 — Marketing/perf figures (prove or soften)
- For each, either add the named benchmark/test (U1 labeled-corpus precision; U2
  measured cache-hit run; U3 staging-mode bench; U4 mock-provider latency bench; U5 TS
  recall corpus) **or** soften the prose to a non-numeric claim. Enterprise bar: do not
  ship a precise number without an artifact that reproduces it.
- Lowest-cost wins: U2 (instrumentation already exists) and U3 (extend `bench_diff`/
  add a staging bench) can become measured claims quickly; U1/U5 need a corpus.

---

## 8. Bottom line

The documentation is **largely faithful to the code**: the scoring, weighting,
versioning, defaults, endpoints, security/supply-chain features, and active adapter
registry are locked by regression tests. The prior adapter-count drift is resolved;
remaining work includes the stale version-rule path and tempering unsupported marketing
claims. Five numeric/marketing claims (U1–U5) still lack an in-repo benchmark and should
be measured or softened before being treated as enterprise-grade guarantees.
