# Kaptaind — Roadmap to the Best Automated Versioning Tool

**Date:** 2026-07-13
**Status:** Planning document (not yet committed)
**Baseline:** v10.0.6 (Cargo.toml), 35,000 lines, 562 tests, 177 source files, 44 test files
**Premise:** Kaptaind is already the *only* tool that does working-tree analysis with multi-language AST parsing and continuous checkpointing. This roadmap closes the gaps between "unique" and "best."

---

## Executive Summary

No tool in the world does what kaptaind does: continuously watch the working tree, cluster changes intelligently, parse 12+ language ASTs to detect API changes, score across five dimensions, and auto-commit with semantic meaning. This is the **right** architecture for the AI-agent era. The problem is that the implementation is incomplete, untrusted, and unproven.

This roadmap is organized around five objectives that make kaptaind the best:

1. **Reliable** — every commit works, every time
2. **Complete** — every language claimed works, every feature ships
3. **Trustworthy** — production-ready security, reproducibility, and auditability
4. **Fast** — proven performance against benchmarks, not marketing claims
5. **Leading** — the ecosystem standard for automated versioning

**Estimated timeline:** 12–16 weeks, 2–3 engineers full-time, parallelizable across tracks.

---

## 1. Objective: Reliable — Zero Bugs, Zero Flakiness

### 1.1 Fix the flaky regression test

**Problem:** `no_phantom_cluster_from_test_hook_target_dir` passes when run alone but fails under parallel execution (returns 0.2.0 instead of 0.1.1). This is a port collision in `MonorepoFixture` or shared filesystem state between test instances.

**Root cause:** `MonorepoFixture::new(port)` uses hardcoded ports that can collide when tests run in parallel. Each test instance spawns a real daemon on a port; if two tests use the same port, one fails to bind or they share state.

**Fix:**
- Replace fixed ports with ephemeral port allocation (bind to `0`, read the assigned port)
- Add `port` isolation via `tempfile` or a port-file lock in the fixture
- Ensure every `MonorepoFixture` instance uses a unique temp directory and port
- Run the full test suite in CI with `--test-threads=16` (or max) to verify no collisions

**Acceptance:** `cargo test --test regressions` passes with `--test-threads=16` every time, 100/100 runs.

### 1.2 Fix the build bloat

**Problem:** Debug builds reach 23GB. This is not normal for 35K lines of Rust. A typical Rust project of this size should have a 2–5GB debug target.

**Investigation:**
- Profile the build: `cargo bloat --release` and `cargo bloat --crates` to find the largest artifacts
- Check for `include_str!` / `include_bytes!` macros loading huge assets (logo, web UI bundles, etc.)
- Check for excessive monomorphization of generic functions (axum/tokio/serde can bloat)
- Check for debug info settings (`split-debuginfo`, `debug = 1` vs `2`)
- Check for duplicated dependencies in the lockfile

**Fix targets:**
- Move embedded assets to a `build.rs` that compresses them or uses `include_bytes!` with `zstd`
- Add `split-debuginfo = "packed"` to `Cargo.toml` profile
- Use `lto = "thin"` in release builds
- Audit the dependency tree for duplicates (`cargo tree --duplicates`)
- Add a CI gate that fails if `target/debug` exceeds 5GB after a clean build

**Acceptance:** Clean debug build ≤ 5GB. Release build ≤ 200MB binary.

### 1.3 Fix the version churn / tag discipline

**Problem:** The daemon auto-bumps `Cargo.toml` version on every commit, creating noisy diffs. There are no git tags for v9.7.16+. The `.kaptaind/status.json` shows `push failed: push to protected branch 'main' is disabled`.

**Fix:**
- Separate `VERSION` (the project version) from `Cargo.toml` (the daemon version). The daemon should only write `VERSION` for the *watched* project, not its own `Cargo.toml`.
- Add `kaptaind.toml` option: `[version] write_cargo_toml = false` (default for the daemon's own repo)
- Make `kaptaind-cli ship` create the git tag, not the daemon. The daemon should commit; the release tool should tag.
- Fix the push failure: add `kaptaind-cli` support for creating PRs (like release-please) instead of pushing directly to protected branches
- Add a CI workflow that tags on VERSION bump, not on every commit

**Acceptance:**
- The daemon's own repo has no Cargo.toml version churn between releases
- Every release has a matching annotated git tag
- Push to protected branches works via PR creation, not direct push

### 1.4 Fix the orphaned adapters

**Problem:** 13 adapter files exist but aren't compiled (`c`, `clojure`, `cpp`, `csharp`, `elixir`, `erlang`, `haskell`, `java`, `lua`, `ocaml`, `perl`, `php`, `scala`). 3 more are declared but unregistered (`dart`, `fsharp`, `ruby`).

**Fix:**
- Implement the no-orphan lint from `ADAPTER_200_ROADMAP.md` §4
- Wire the 13 orphaned adapters into `adapters/mod.rs` and `normalize()`
- Or delete them if they are non-functional and add a comment explaining why
- The no-orphan lint should be a CI gate: every `.rs` file in `adapters/` must be declared, registered, and confidence-tabled

**Acceptance:** `cargo test` passes; no-orphan lint passes; README says the exact number of active adapters.

---

## 2. Objective: Complete — Ship Every Feature, Support Every Language

### 2.1 Complete the adapter coverage (T1 + T2)

**Goal:** Ship the top 40 languages with first-class adapters, covering 99% of active open-source code.

**T1 (top 20, AST-grade):** The 12 active adapters + 8 new ones:
- Java (currently orphaned)
- C++ (currently orphaned)
- C# (currently orphaned)
- PHP (currently orphaned)
- Lua (currently orphaned)
- Ruby (declared but unregistered)
- Dart (declared but unregistered)
- F# (declared but unregistered)

**T2 (ranks 21–60, structured-regex):** 20 new adapters:
- Scala, Clojure, Haskell, Erlang, Elixir (currently orphaned)
- OCaml, Perl (currently orphaned)
- Julia, R (claimed but non-existent)
- Groovy, Kotlin (already supported, verify depth)
- Shell/Bash, PowerShell, SQL, HTML/CSS, SCSS/Sass, Vue, Svelte, Astro (already supported)
- Additional: Markdown, YAML, JSON, TOML, XML (config-as-code languages)

**Method:**
- Follow the `ADAPTER_200_ROADMAP.md` §3 stage gates: Research → Conceptualize → Design → Implement → Test → Benchmark → Edge-case → Evaluate → Refine → Integrate
- Each adapter must pass: ≥95% fixture recall, 0 false-public on negatives, bench budget met, F1 calibrated to confidence
- Use the scaffold generator: `kaptaind-cli adapter scaffold <lang>`

**Timeline:** 8–10 weeks for 28 new adapters (parallelizable; 2–3 engineers).

### 2.2 Complete the release pipeline

**Goal:** The `ship` command actually works end-to-end.

**Current gaps:**
- Multi-arch builds only produce Linux x86_64
- No signed artifacts, no SBOM attached to releases
- No Homebrew/.deb/winget packages
- Static-export hack breaks Next.js API routes

**Fix:**
- **Phase 1:** Multi-arch CI builds (linux x86_64/arm64, macOS x86_64/arm64, Windows x86_64)
- **Phase 2:** SHA256 checksums + GPG signing + Sigstore keyless cosign
- **Phase 3:** SPDX 2.3 SBOM + SLSA v1.0 provenance
- **Phase 4:** Package managers (Homebrew tap, `.deb` repo, `winget` manifest, signed macOS `.dmg`)
- **Phase 5:** Fix Next.js standalone mode (delete static-export hack, serve Node server behind nginx)

**Acceptance:** A tag push produces signed, multi-arch artifacts with SBOM and provenance, installable via package manager on every major OS.

### 2.3 Complete the web deployment

**Goal:** The WebUI and API actually work in production.

**Current gaps:**
- `docker-compose.yml` serves static files, so `/api/*` and `/dashboard/*` are broken
- No HSTS/CSP headers
- Default DB credentials in compose file
- `HEALTHCHECK` fails on fresh container

**Fix:**
- Switch `web/` to Next.js `output: 'standalone'`
- Rewrite `docker-compose.yml` to run the Node server behind nginx (reverse proxy)
- Add HSTS + CSP (Stripe allowlist) + Permissions-Policy headers
- Remove default DB credentials from shipped compose; generate on first run
- Fix `HEALTHCHECK` to hit the daemon's `/health` endpoint, not `kaptaind-cli status`
- Add smoke test in CI: `docker compose up` → `/api/health` reachable

### 2.4 Complete the rollback / undo system

**Goal:** `kaptaind-cli rollback` works.

**Current state:** Documented as `git revert` only; no first-class command.

**Implementation:**
- `kaptaind-cli rollback --last` — revert the most recent kaptaind commit
- `kaptaind-cli rollback --cluster <id>` — revert a specific cluster
- `kaptaind-cli rollback --dry-run` — preview without applying
- Restore `VERSION`, `Cargo.toml`, and `Cargo.lock` to pre-commit state
- Record rollback in `audit.jsonl`
- Refuse on dirty tree unless `--force`

---

## 3. Objective: Trustworthy — Production-Ready Security and Auditability

### 3.1 Fix the security documentation

**Problem:** `SECURITY.md` still says "No commit signing" and "No branch protection" but both are implemented. This actively misleads operators.

**Fix:**
- Update `SECURITY.md` to reflect implemented features: GPG signing, branch protection, SBOM, provenance, RBAC
- Add a verification section: how to verify a signed release artifact
- Document the security contact and response process
- Add a `SECURITY.md` round-trip test (send a test email, verify response path)

### 3.2 Fix the supply chain posture

**Problem:** `git2 0.19.0` has RUSTSEC advisories that are allow-listed. No `cargo-deny` license bans. No pinned toolchain (`rust-toolchain.toml`).

**Fix:**
- Upgrade `git2` → `0.20` / `libgit2` ≥ 1.9; fix API breaks in `src/git/**`, `src/commit/**`, `src/push/**`
- Drop the git2 advisory IDs from `deny.toml`/`.cargo/audit.toml`
- Add `cargo-deny` config with license bans and advisory checks
- Add `rust-toolchain.toml` with pinned toolchain and MSRV policy
- Add CI gate: `cargo audit` + `cargo deny` must be green on default features
- Add stale-allow-list watchdog: any allow-list entry older than 90 days fails CI

### 3.3 Fix the audit log integrity

**Problem:** `audit.jsonl` is append-only but has no integrity chain. Tampering is undetectable.

**Fix:**
- Add `prev_hash` to `AuditEntry` (SHA-256 of previous entry's JSON)
- Backfill legacy entries with `prev_hash: null` (serde default)
- Add `kaptaind-cli audit verify` — check the hash chain, detect gaps/truncation
- When signing is enabled, sign each entry with the GPG key
- Add `audit stats` — counts by event type, failure rate, time window
- Add `audit export --format json|html` — portable reviewable trace

### 3.4 Add deterministic diff verification

**Problem:** The daemon's version bump decisions are not reproducible. A user cannot verify that a given diff *should* have produced a given bump.

**Fix:**
- Add `kaptaind-cli trace verify <cluster>` — re-derive the decision from the stored diff + current thresholds, assert it matches the recorded bump
- Store the full weight math (s, a, d, r, b values and the final score) in the analysis artifact
- Store the exact threshold config used at the time of the decision
- This makes every bump decision auditable and contestable

---

## 4. Objective: Fast — Proven Performance with Benchmarks

### 4.1 Fix the unverifiable performance claims

**Problem:** The README claims "70–90% AST cache hit ratio", "10–20× staging reduction", "500ms–2s inference latency" with no in-repo benchmarks. These are marketing claims, not engineering facts.

**Fix:**
- Implement the `divan` micro-benchmarks from `FIRST_STABLE_VALIDATION_ROADMAP.md` §A1:
  - `bench_cluster.rs` — ClusterEngine throughput at 1e3/1e5/1e6 events
  - `bench_diff.rs` — diff::analyze over 10/100/1000 files, with and without AST cache
  - `bench_adapters.rs` — per-language adapter parse time + cache hit rate
  - `bench_weight_version.rs` — weight calculator + version decision latency
- Implement the macro-benchmark `kaptaind-cli bench` from §A2:
  - Generate deterministic synthetic repos (seeded, configurable language mix)
  - Run the real pipeline: cluster → diff → weight → version → (no commit)
  - Record per-stage latency, throughput, cache hit rate, peak RSS
  - Emit `.kaptaind/bench/<run-id>.json` with full metrics
- Store LKG (last-known-good) baselines in `tests/bench-baselines/`
- Add CI regression gate: >15% latency regression or >10% throughput drop fails

### 4.2 Prove the cache hit claim

**Problem:** "70–90% AST cache hit ratio" is unmeasured.

**Fix:**
- Add instrumentation to `DiffAnalysis` that records `ast_cache_hits` and `ast_cache_misses` per run
- The cache hit ratio is already present in `DiffAnalysis` but not validated
- Run a benchmark over a representative fixture repo with incremental edits
- Publish the measured ratio in the qualification report
- If the ratio is <70%, either fix the cache or soften the claim

### 4.3 Prove the staging reduction claim

**Problem:** "`cluster` staging can reduce staging time by 10–20× for 1000+ files" is unmeasured.

**Fix:**
- Add a `bench_staging_mode` comparison: `all` vs `cluster` at 1k/10k files
- Measure: time from event to commit decision, number of files staged, git index operations
- Run in CI and publish results
- If the claim holds, add it to the qualification report; if not, remove the claim

### 4.4 Prove the inference latency claim

**Problem:** "~500ms–2s (fast mode) / ~1–3s (consensus mode)" is provider-dependent and unmeasured.

**Fix:**
- Add a mock-provider benchmark that simulates inference without network calls
- Measure the overhead of the inference pipeline (prompt generation, provider routing, response parsing)
- Separate "network latency" from "kaptaind overhead"
- Either publish measured numbers with the mock provider or soften the claim to "single provider vs. multi-model"

### 4.5 Add the stress and soak harness

**Problem:** No synthetic fixture generator, no event-storm harness, no long-running soak test.

**Fix:**
- Implement `kaptaind-cli gen-fixtures` — reproducible synthetic repos with configurable language mix, file count, size, edit batches, burst rate, seed
- Implement `kaptaind-cli stress` — adversarial test cases:
  - Save storms: 1k–50k file events at configurable rates
  - Churn: rapid modify→revert sequences (must not create empty commits)
  - Rename/delete storms, binary/blob changes, symlinks, permission-denied files
  - Hook failures: failing/slow test hook must block/timeout correctly
  - Git pressure: shallow clone, detached HEAD, protected branch push disabled
- Implement `kaptaind-cli soak --duration 24h --rate 20/s --fixtures mixed`:
  - Run continuously, sample RSS, fd count, error rate
  - Assert: stable RSS (no monotonic growth), stable fd count, no error-rate growth
  - Run in container with resource limits
  - Emit `.kaptaind/soak/<run-id>.json`

**Acceptance:** Stress test passes with 0 panics, 0 data loss, monotone versions. Soak test passes 24h with bounded memory/fd.

---

## 5. Objective: Leading — Ecosystem Standard for Automated Versioning

### 5.1 Fix the README positioning

**Problem:** README claims "19 languages" but only 12 are active. Claims "enterprise-grade" and "S-grade" without evidence. Mixes "discovery types" with "diff adapters" confusingly.

**Fix:**
- Replace the language list with the exact active set: "12 language-aware adapters with AST parsing and confidence scoring: Rust, Go, Swift, Kotlin, TypeScript, JavaScript, Python, Vue, Svelte, Astro, SCSS/Sass, HTML/CSS"
- Add a separate section: "28 additional languages covered by structured regex and fallback scanning" with a table showing tier and confidence
- Remove "S-grade" / "A+" self-grades. Replace with: "Feature-complete against internal rubric; external benchmarks pending for discovery accuracy, cache hit rate, and staging throughput"
- Separate "project discovery types" (trawler) from "diff adapters" (analysis) — they are different numbers
- Add a "Compared to" section: semantic-release (post-hoc git analysis), release-please (Release PRs), changesets (human-written changesets), cargo-semver-checks (Rust-only static analysis). Position kaptaind as the *only* working-tree analyzer.

### 5.2 Add the comparison page

**Goal:** A user comparing tools should immediately understand why kaptaind is different.

**Table:**

| Feature | kaptaind | semantic-release | release-please | changesets | cargo-semver-checks |
|---------|----------|-------------------|----------------|------------|---------------------|
| Analyzes working tree | ✅ Yes | ❌ No (git history) | ❌ No (git history) | ❌ No (human files) | ❌ No (compiled API) |
| Multi-language | ✅ 12+ | ❌ JS only | ❌ N/A | ❌ N/A | ❌ Rust only |
| AST-aware diff | ✅ Yes | ❌ No | ❌ No | ❌ No | ✅ Yes |
| Continuous daemon | ✅ Yes | ❌ CI only | ❌ CI only | ❌ CI only | ❌ CI only |
| Intelligent clustering | ✅ Yes | ❌ N/A | ❌ N/A | ❌ N/A | ❌ N/A |
| Confidence scoring | ✅ Yes | ❌ No | ❌ No | ❌ No | ❌ No |
| Stability tracking | ✅ Yes | ❌ No | ❌ No | ❌ No | ❌ No |
| Auto-commit | ✅ Yes | ❌ No | ❌ No | ❌ No | ❌ No |
| Release publishing | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes | ❌ No |

**Positioning statement:** "kaptaind is the only tool that watches your working tree, understands your code's API surface across 12+ languages, and auto-commits meaningful checkpoints. Other tools analyze *what you already committed*; kaptaind analyzes *what you're about to commit*."

### 5.3 Add the plugin ecosystem

**Goal:** Community plugins extend kaptaind to any language without core changes.

**Current state:** Plugin protocol exists (`stdin {"file"} → stdout {"symbols":[]}`) but no plugin gallery, no scaffold generator, no conformance suite.

**Fix:**
- Add `kaptaind-cli plugin scaffold <name>` — generates a conformant plugin skeleton with the JSON protocol
- Add a plugin registry page in the WebUI
- Publish a "Plugin Author Guide" documenting the protocol, cache integration, version detection, scoring pipeline
- Add a conformance suite: a plugin must pass the same §5 test corpus (positive/negative/breaking/nonbreaking/version/edge) to be listed as "supported"
- Add telemetry: emit per-plugin detection counts and confidence in `status.json`

### 5.4 Add the enterprise integration guide

**Goal:** Large teams can adopt kaptaind with confidence.

**Content:**
- CI/CD integration: how to run kaptaind in Docker, Kubernetes, GitHub Actions
- RBAC configuration: user/group permission checks for shared machines
- Audit log forwarding: how to ship `audit.jsonl` to Splunk/ELK/Datadog
- Prometheus metrics: scrape config, Grafana dashboard template
- Backup/restore: `.kaptaind/` directory structure, how to migrate between hosts
- Multi-repo monitoring: `kaptaind-cli monitor` across a fleet of repos
- High availability: `kaptaind-cli shark` leader election for redundant daemon instances

### 5.5 Add the qualification report generator

**Goal:** Every release ships with a signed, machine-readable qualification report.

**Implementation:**
- `kaptaind-cli report generate` — aggregates all bench/stress/soak/doctor artifacts into `report-<version>.md` + `report-<version>.json`
- Sections: Correctness, Benchmarks, Stress, Soak, Inspection, Containers, Security, Hardware
- Each section: PASS / PASS-WITH-NOTES / FAIL, linked to evidence artifact
- Verdict: overall PASS only if every section is PASS or PASS-WITH-NOTES
- Signed by the CI key, attached to the GitHub release

**Acceptance:** The v11.0.0 release has a qualification report attached, with every claim backed by a benchmark or test artifact.

---

## 6. Sequencing

| Phase | Tracks | Duration | Exit Criteria |
|-------|--------|----------|---------------|
| **P0 — Stabilize** | Fix flaky test, fix build bloat, fix version churn, fix orphaned adapters, clean working tree | 2–3 weeks | All tests pass 100/100 parallel runs; clean build ≤ 5GB; no-orphan lint green; working tree stable |
| **P1 — Complete** | Ship 28 new adapters (T1+T2), fix release pipeline, fix web deployment, add rollback | 6–8 weeks | 40 adapters active; multi-arch signed release; web deploy works; rollback works; 100% fixture recall on all adapters |
| **P2 — Trustworthy** | Fix security docs, fix supply chain, add audit integrity, add deterministic verification | 2–3 weeks | SECURITY.md accurate; cargo audit/deny green; audit hash chain; trace verify works; SBOM + provenance on every release |
| **P3 — Fast** | Add benchmarks, prove cache/staging/inference claims, add stress/soak harness | 3–4 weeks | All README performance claims backed by benchmarks; stress 0 panics; soak 24h green; regression gates in CI |
| **P4 — Lead** | Fix README positioning, add comparison page, add plugin ecosystem, add enterprise guide, qualification report | 2–3 weeks | README honest; comparison page published; plugin gallery live; enterprise guide published; v11.0.0 with qualification report |

**Total: 15–21 weeks** (3.5–5 months), 2–3 engineers full-time, parallelizable across tracks.

---

## 7. Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| **Adapter explosion** | Tiered model (T1/T2/T3/T4) + plugin ecosystem; don't hand-write 200 adapters |
| **Build bloat recurs** | CI gate on target size; `cargo bloat` in CI; strict dependency audit |
| **Performance claims wrong** | Measure before claiming; soften claims if benchmarks don't match |
| **Web deployment complexity** | Hire a frontend engineer or scope web to metrics/dashboard only; don't build a full app |
| **Release pipeline never works** | Start with the simplest path: GitHub Actions → signed tarball; iterate to packages |
| **Community doesn't adopt** | Focus on the "AI checkpointing" story; be the only tool for that use case |
| **Rivals copy the features** | Keep the AST adapter depth as the moat; the 40-language coverage is hard to replicate |
| **Working tree corruption** | Extensive test coverage; dry-run mode; rollback; audit trail; never auto-push without test pass |

---

## 8. Success Criteria (KPIs)

A kaptaind release is "the best automated versioning tool" when all of these are true:

- [ ] 40 language adapters active, each with ≥95% fixture recall, 0 false-public, bench budget met
- [ ] Every README claim backed by a benchmark or test artifact; no unverifiable numbers
- [ ] Multi-arch signed release with SBOM + provenance, installable via package manager on every major OS
- [ ] `docker compose up` yields a working daemon + web + API on first run
- [ ] `cargo test` passes 100/100 runs with `--test-threads=16`; no flaky tests
- [ ] Clean debug build ≤ 5GB; release binary ≤ 200MB
- [ ] `cargo audit` + `cargo deny` green on default features; no stale allow-list entries
- [ ] `kaptaind-cli audit verify` detects tampering; every decision is reproducible via `trace verify`
- [ ] 24h soak test shows stable RSS/fd, zero data loss, zero panics
- [ ] Stress test passes with 50k file events, 0 panics, 0 empty commits, monotone versions
- [ ] Qualification report attached to every release; overall PASS with no FAIL sections
- [ ] README positions kaptaind as the only working-tree analyzer; comparison table published
- [ ] Plugin ecosystem with conformance suite; community plugins can be listed as "supported"
- [ ] Enterprise integration guide published; RBAC, audit forwarding, Prometheus, HA documented
- [ ] Home-user first-run succeeds offline; one-command install works on macOS/Linux/Windows

---

## 9. Immediate Next Steps (This Week)

1. **Fix the flaky test.** Add ephemeral port allocation to `MonorepoFixture`. Verify with 100 parallel runs.
2. **Investigate build bloat.** Run `cargo bloat` and find the top 10 largest artifacts. Target the biggest wins.
3. **Fix version churn.** Separate `VERSION` from `Cargo.toml` in the daemon's own repo. Stop auto-bumping Cargo.toml.
4. **Implement no-orphan lint.** A single CI test that asserts every adapter file is declared, registered, and confidence-tabled.
5. **Start the adapter scaffold generator.** `kaptaind-cli adapter scaffold <lang>` should emit the four integration points + fixture skeleton + bench stub.
6. **Create the comparison page.** A `docs/COMPARISON.md` with the table above, committed to the repo.

---

*End of roadmap.*