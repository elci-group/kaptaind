# Changelog

All notable changes to kaptaind are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project uses
[Semantic Versioning](https://semver.org/).

> **Version-line note (stable reset).** Early development auto-bumped the
> `VERSION` file on every daemon commit, which is why the file advanced far
> ahead of this changelog (the daemon previously dogfood-versioned its own
> repository without cutting git tags). From the stable line onward, releases
> are created explicitly via CI (annotated, signed tags on `VERSION` changes —
> see `.github/workflows/release.yml`), and each release gets a changelog entry
> here. Per-commit detail for the `v0.1.44 → v9.x` range lives in `git log`;
> the consolidated capability set is summarized under `[9.7.16]` below.

## [10.2.0] — 2026-08-01

Minor release: an explicit observe/actuate gate around every repository
mutation.

### Added
- **`[operation] mode`**: controls whether the daemon may perform repository
  mutations at all. `"observe"` (the default) runs the full analysis and
  scoring pipeline and records the decision to `.kaptaind/decisions.jsonl`,
  but never stages, commits, writes `VERSION`, pushes, or ships. `"actuate"`
  is required before any of those happen. `kaptaind-cli analyze` and
  `kaptaind-cli validate` do not currently flag a repo left in observe mode —
  check `kaptaind-cli explain` or `decisions.jsonl` for `"outcome":"observed"`
  if commits unexpectedly stop appearing after upgrading.
- This is additive to the `[trust]` and `[capabilities]` gates from
  [10.1.4](#1014--2026-07-18): a repository profile now needs
  `[trust] execution = "trusted"`, `[operation] mode = "actuate"`, and (for
  pushing) `[capabilities] network_push = true` together before the daemon
  will touch the working tree or the remote.

## [10.3.2] — 2026-08-25

### Added
- A transactional pull engine that separates fetch, topology inspection,
  strategy planning, integration, verification, and ref mutation instead of
  delegating repository safety to `git pull`.
- `kaptaind pull` and `kaptaind-cli pull` support for check-only and dry-run
  assessment, JSON reports, explicit merge/rebase/Hybreed/Emulsify strategies,
  autostash, abort/continue/recover flows, recovery refs, repository locks,
  persistent journals, structured conflicts, risk scoring, and stable exit
  codes.
- Remote/upstream resolution, ahead/behind classification, protected-branch
  policy, conflict prediction, and configurable post-integration build/test
  verification.

### Fixed
- Autostash no longer passes an all-files pathspec that makes Git reject an
  otherwise valid pull when `.kaptaind` transaction state is ignored.
- Updated `h2` to 0.4.16 or newer to address RUSTSEC-2026-0258 (unbounded
  empty DATA frames).

## [10.1.4] — 2026-07-18

Patch release hardening automated commit and release boundaries identified by
the code, security, dependency, architecture, and enterprise-readiness audit.

### Security
- Repository-supplied configuration now defaults to untrusted execution and
  must explicitly opt into local hooks, plugins, builds, and bundle commands.
- Outbound HTTP clients enforce the SSRF address policy during connection-time
  DNS resolution, closing the validation-to-connect rebinding window.
- `.kaptaind` runtime directories and files are created and maintained with
  owner-only permissions, including daemon logs, PID files, and audit chains.

### Fixed
- Stale filesystem clusters can no longer borrow unrelated dirty worktree state
  to create version-only commits.
- Version, manifest, and lockfile writeback is transactional, verified, and
  rolled back on failure so a commit cannot persist a divergent version tuple.
- Release build commands rejected by shell validation now fail closed without
  execution.

## [10.1.0] — 2026-07-15

Minor release: workspace-aware version bumping, opt-in through v10.x
(`docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md`; default flip planned for
v11). Also a startup guard against accidental daemon runs.

### Added
- **Workspace-aware version writeback** (`[versioning].workspace`):
  `root_only` (default; pre-v10 behavior), `touched` (bump only the members
  the cluster touched, plus the root crate for paths outside every member
  subtree), or `lockstep` (bump every member together). Discovery handles
  root-crate, virtual, and single-crate layouts, `version.workspace = true`
  inheritance, and glob/`exclude` member resolution. Writeback keeps
  per-member baselines, raises inter-member path-dependency floors (never
  widens), and does a single lockfile pass. Commit subjects carry the
  dominant member scope; `decisions.jsonl` records `members_bumped`, rendered
  by `kaptaind-cli explain`. Covered by 8 daemon regression tests
  (`tests/workspace_regressions.rs`) and a soak test with member waves.
- **`kaptaind-cli doctor` workspace checks**: `workspace_lock_drift`,
  `workspace_requirement_unsatisfiable`, `workspace_root_only_deflation`.
- **`[daemon] startup_guard`**: refuse to start while the worktree has
  uncommitted changes, unless `--force` is passed — an accidental start can
  no longer catch-up-commit in-flight release work.

### Changed
- The kaptaind repo itself now runs `[versioning].workspace = "touched"`,
  and release CI additionally cuts member tags (`kaptaind-diff-vX.Y.Z`) when
  a member manifest version has no tag yet. Root `vX.Y.Z` tags, the build
  matrix, and the GitHub release flow are unchanged and fire only on root
  version bumps.

### Fixed
- Restored the phantom-cluster regression test's version expectation to the
  designed api-added ⇒ Minor outcome (test drift only; no runtime change).

## [10.0.1] — 2026-07-12

Patch release: a cascade fix found by the new nightly chaos soak.

### Fixed
- **Phantom commits from test-hook cargo tempdirs.** Cargo creates its
  target dir as a root-level `targetXXXXXX` tempdir atomically renamed to
  `target`. That path escaped the version-file self-write guard and is not
  matched by the default `target` ignore pattern, so the daemon's own
  `cargo test` hook clustered a phantom change — observed skipping a patch
  version and logging `ERROR` on empty chore commits. The self-write guard
  now carries a hook-artifact window: while a cargo-based test/bundle hook
  runs (plus the existing 60s TTL), root-level `target`/tempdir events are
  suppressed. Path- and time-bounded; non-cargo hooks unaffected.

### Added
- **Nightly chaos soak** (`tests/soak.rs`, `.github/workflows/soak.yml`):
  a deterministic wave-based workload generator runs the real daemon for
  30 minutes in CI, asserting ≤1 commit per genuine cluster, version-triple
  consistency at every commit, lockfile consistency, and zero daemon
  errors. Run locally with
  `KAPTAIND_SOAK_SECS=600 cargo test --test soak -- --ignored --nocapture`.

## [10.0.0] — 2026-07-11

Workstream D (workflow integrity) of the autonomous-commit safety plan — the
breaking window. Two defaults flip, deterministic commit subjects become
conventional commits, and partial configs now parse. Below-threshold work is
never silently lost again, and version only moves on threshold-crossing
clusters. Run `kaptaind-cli doctor` against your repo for a per-config
migration report; operator docs: `docs/RUNBOOK.md`.

### ⚠️ MIGRATION GUIDE

- **`[commit] require_bump` flipped `true` → `false`.** Below-threshold
  clusters no longer sit uncommitted forever: they are captured with a
  non-bumping `chore:` commit (VERSION, Cargo.toml and Cargo.lock untouched),
  while version only moves on threshold-crossing clusters. This ends silent
  work loss (#7) and version inflation (#17). **To keep the pre-v10 skip
  behavior**, set `[commit] require_bump = true` — skips are logged as
  `no_bump` in `.kaptaind/decisions.jsonl` and visible via `kaptaind-cli
  explain`.
- **`[staging] mode` flipped `all` → `cluster` (already in v9.7.17 — recap
  for anyone upgrading from <9.7.17).** The daemon no longer runs
  `git add -A` by default; only clustered paths plus version metadata are
  staged. `mode = "all"` still works but sweeps the whole worktree (untracked
  files, including secrets) and logs a loud startup warning; commits abort
  fail-closed if a changed path matches the secret denylist. Prefer
  `mode = "cluster"`.
- **All config sections now have serde defaults.** A partial `kaptaind.toml`
  that previously failed to parse (missing `[watch]`, `[weights]`,
  `[cluster]`, `[push]`, `[test]` or `[ratelimit]`) now loads with defaults
  sourced from the init template. **Caveat:** a config that was previously
  *invalid* may now load silently with defaults — re-check your file with
  `kaptaind-cli validate`, which still rejects explicitly-invalid values.
- **Obsolete `.kaptainignore` workarounds now harm you.** Entries for
  `VERSION`, `Cargo.toml` or `Cargo.lock` were pre-v9.7.17 workarounds for
  the daemon's own writeback churn. The self-write guard (v9.7.17) makes them
  unnecessary, and keeping them means a lone dependency edit (e.g.
  `cargo update` touching `Cargo.toml`/`Cargo.lock`) never clusters and never
  commits. Delete those lines.
- **Commit messages are now conventional commits.** Deterministic subjects
  name the change class and primary paths, e.g. `build(deps): update
  dependencies (Cargo.toml)`, `docs: update documentation (README.md)`,
  `fix!(src): change public API (lib.rs)`, with a hard 72-char cap. The
  scorecard body block is unchanged. Anything scraping the retired
  `kaptaind:` subject prefix must switch to the scorecard body line.
- **Multiple daemons on one host need distinct health ports.** Run each
  instance with its own `health_port` (`--health-port` or config). Bind
  failures previously vanished silently — a second daemon on the same port
  ran with no health endpoint and no log line; they now log at ERROR.
- **Migration checker:** `kaptaind-cli doctor` flags every legacy pattern
  this release retires (explicit `staging.mode = "all"`, unset
  `staging.mode`/`require_bump`, obsolete `.kaptainignore` entries) with a
  concrete fix for each, in both text and `--format json` output. Operator
  runbook: `docs/RUNBOOK.md`.

### Added
- **`[commit] require_bump` option (D1).** When `false`, below-threshold
  clusters commit with a non-bumping `chore:` message using the exact
  staging/commit machinery of the bumping path (RepoContext scoping,
  fail-closed denylist). New `chore_commit` outcome in the decisions log,
  rendered by `kaptaind-cli explain`; `--dry-run` predicts the chore commit.
- **`kaptaind-cli doctor` config-migration checks (D3).** New MIGRATION
  section inspecting the loaded config, the raw `kaptaind.toml`, and
  `.kaptainignore` for retired legacy patterns; findings (severity, message,
  concrete fix) are included in the JSON artifact under a new `migration`
  field (additive — older readers ignore unknown fields).
- **Regression tests:** `chore_commit_captures_docs`,
  `docs_edit_does_not_bump_when_require_bump_off` (default flip guarded),
  and `commit_message_lint` (10-case matrix × both message builders asserting
  conventional-commit parse, ≤72-char subject, non-empty description).

### Changed
- **BREAKING: `[commit] require_bump` defaults to `false`.** Below-threshold
  clusters chore-commit instead of being silently dropped (see migration
  guide).
- **Deterministic commit subjects are conventional commits (D2).** Subjects
  name the change class and primary paths with a hard 72-char cap;
  truncation is confined to the trailing path list so parsing never breaks.
  The regression harness now detects daemon commits via the scorecard body
  line instead of the retired `kaptaind:` subject prefix.
- **Every config section has serde defaults sourced from the init template
  values.** Partial `kaptaind.toml` files parse; validation still rejects
  explicitly-invalid values. Default push branch is `main` (inert while
  `push.enabled = false`).

### Fixed
- **Health/web server bind failures now log at ERROR.** `tokio::spawn`
  dropped the `JoinHandle`, silently discarding bind errors: a second daemon
  on the same `health_port` ran with no health endpoint and no log line
  (found dogfooding v9.8.0 on scotia + fract, both defaulting to port 9090).

## [9.8.0] — 2026-07-11

Workstreams B (repo model & version consistency) and C (operability) of the
autonomous-commit safety plan. Monorepo-subdirectory projects are now
first-class, the version state is a single coherent triple, and every
decision the daemon makes is observable. Operator docs: `docs/RUNBOOK.md`.

### Fixed — repo model & version consistency
- **`RepoContext` everywhere.** The daemon distinguishes the git root (where
  git commands anchor) from the project root (where `kaptaind.toml`/`VERSION`
  live). Previously commits ran against the git root, so in a monorepo the
  meta files were resolved against the wrong directory and silently never
  staged — the commit claimed a bump its tree didn't contain. All staging is
  now scoped to the project subtree; `kaptaind analyze` reports only
  in-project paths.
- **Version triple consistency.** `save_version` syncs `VERSION`,
  `Cargo.toml`, **and** `Cargo.lock`'s own-package entry, and cluster
  meta-staging includes all three — after any auto-commit the triple agrees
  and no drift is left uncommitted (validated across a 20-commit synthetic
  workload).
- **Startup reconciliation.** `[watch] rescan_on_start` (default `true`):
  changes made while the daemon was down form a single catch-up cluster
  through the normal scored/tested/gated pipeline.
- **Manifest scoring calibration.** Metadata-only manifest edits (a
  repository-URL or version-key change) no longer score the entire
  dependency graph; dependency sections are compared against HEAD and only
  real dependency changes feed the `deps` score.

### Added — operability
- **Graceful shutdown.** `[daemon] shutdown_grace_secs` (default 10) bounds
  the task drain on SIGTERM/SIGINT; the runtime waits grace + 5s before
  forcing exit; the pid file is removed on clean shutdown.
- **Crash-safe state.** Stale `daemon.pid` files are detected and removed at
  startup; `status.json` writes are atomic (tmp + rename) and the startup
  `Idle` write lands before any cluster processing, so a crashed run never
  shows a frozen mid-state.
- **Decision transparency.** Every cluster decision — commit or skip —
  appends one JSON line to `.kaptaind/decisions.jsonl` (scores, thresholds,
  bump, reason, paths). `kaptaind-cli explain [--last N]` renders skips with
  the exact unmet threshold; `kaptaind --dry-run` prints the full decision
  (bump, next version, exact commit message) without writing anything.
  `kaptaind init` now emits `[version_thresholds]` explicitly.
- **Test-gate backpressure.** `[test] command_on = "always" | "code_only"`
  lets docs-only clusters skip the suite; ≥3 consecutive required-test
  failures log and broadcast a "commits blocked" warning.
- **Config hot reload.** `kaptaind.toml` and the ignore file are watched as
  configuration (never clustered): edits reload thresholds, weights, rate
  limits, and the ignore matcher within one cluster window; invalid TOML
  keeps the previous config and warns.

### Added — regression suite
- Five daemon-level regression tests on a monorepo fixture (parallel, one
  health port per test): cascade suppression + version-triple consistency +
  no fake `.git`; startup catch-up; decisions log records commit and skip;
  ignore-file hot reload; invalid-config survival.

## [9.7.17] — 2026-07-11

Security hardening pass (audit remediation) plus the M0 milestone of the
autonomous-commit safety plan (`docs/planning/AUTONOMOUS_COMMIT_SAFETY_PLAN.md`).

### Security — behavior changes
- **WebUI now requires authentication.** Every route except `GET /` requires a
  bearer token (`Authorization: Bearer <token>`, or `?token=` for the SSE
  stream). When `[web] auth_token` is unset, the daemon generates a random
  32-byte token at startup and prints the full `http://127.0.0.1:<port>/?token=...`
  URL to stderr. Token comparison is constant-time, `POST` requests must carry a
  loopback `Origin` (CSRF guard), and the previously permissive CORS headers are
  gone. Config writes through the UI are now disabled unless
  `[web] allow_config_write = true`, and the config endpoint redacts
  secret-shaped keys and never echoes raw TOML. Commit-detail `:id` is restricted
  to `^[A-Za-z0-9_-]{1,64}$`.
- **`.env` loading is non-overriding and allowlisted.** Variables already present
  in the environment always win (`.env` no longer overrides them), and only keys
  matching an allowlist of prefixes (`KAPTAIND_`, `ELEVENLABS_`, `OPENAI_`,
  `AZURE_SPEECH_`, `GOOGLE_`, `CARTESIA_`, `MOONSHOT_`, `KIMI_`, `ANTHROPIC_`,
  `OLLAMA_`, `AWS_`, `S3_`, `GITHUB_`) are imported. Everything else in `.env` is
  ignored.
- **Staging matcher is recursive and deny-by-default for secrets.** Basename
  exclude patterns now also match at any depth (`pat` → `pat` and `**/pat`), glob
  compilation is fail-closed (a bad pattern errors instead of silently matching
  nothing), and a built-in secret denylist (e.g. `*.pem`, `*.key`, `id_rsa`,
  `.env`, credentials) is always enforced and cannot be overridden by includes.

### Security — other fixes
- Added a central hardened HTTP client (`util::http`) with connect/total timeouts,
  `redirect::Policy::none()`, no environment proxy, and rustls, plus an SSRF guard
  (`validate_outbound_url` / `validate_inference_url`) that resolves and blocks
  loopback, link-local, RFC1918, CGNAT, multicast, and `169.254.169.254`. Wired
  into webhooks, notifications, all TTS providers, every inference backend, S3
  release, and GitHub push. Google TTS key moved out of the query string into the
  `X-Goog-Api-Key` header; Azure region is validated.
- Constant-time secret comparison now uses `subtle::ConstantTimeEq` (replaces the
  hand-rolled XOR loop). Added a replay-aware webhook verifier
  `verify_signature_with_timestamp` that rejects stale skew and binds the
  timestamp under the HMAC.
- Strict hook validators: test-hook and bait (binary + shell) validators now
  refuse unsafe commands instead of warn-and-allow. Docker and crane registry
  logins use `--password-stdin` (no secrets on argv). Windows TTS no longer
  interpolates text into PowerShell (passed via `KAPTAIND_TTS_TEXT`).
- Log-injection / size hygiene: outbound error bodies and webhook responses are
  truncated and control-stripped before logging, and never logged above debug.
  LLM-generated commit narratives are sanitized to a single control-free
  72-char subject before formatting (prevents forged commit headers).
- Supply chain: `install.sh` now downloads signed release archives, verifies
  `SHA256SUMS.txt`, and verifies the cosign keyless bundle when `cosign` is
  present; `--ref` pins a release and `--build-from-source` is gated behind a
  warning. Every `uses:` in CI is pinned to a commit SHA. Docker images are
  pinned by digest, `cargo fetch || true` is gone, and the compose stack runs
  with `no-new-privileges`, `cap_drop: ALL`, and read-only root filesystems.
  nginx now redirects 80→443, terminates TLS, drops `preload`, and sets
  `client_max_body_size`. The systemd unit and the autostart unit add sandboxing
  directives. The two committed group-writable ELFs in `deploy/daemon/` were
  removed and the directory is gitignored; `Cargo.lock` is no longer ignored.
- Removed the false "signed/notarized desktop app" claims from the download and
  security pages (source and the shipped static export); CLI release artifacts
  remain cosign keyless-signed, the desktop app is unsigned preview.

### Fixed — autonomous-commit safety (M0)
- **Staging defaults to cluster mode.** `StagingMode` no longer defaults to
  `All`; `All` and empty-`Pattern` staging now go through a fail-closed denylist
  guard that aborts before `git add -A` when a changed path matches the secret
  denylist, and the daemon logs a startup warning when `All` mode is configured.
- **Self-change cascades suppressed.** The scheduler records the paths it writes
  during version writeback (`VERSION`, `Cargo.toml`, `Cargo.lock`) in a 60s TTL
  guard and filters them out of incoming watch events, so the daemon's own
  writeback can no longer re-cluster into an endless commit cascade.
- **Version baseline is never guessed.** `version::resolve_baseline` resolves the
  baseline from the `VERSION` file, then `Cargo.toml` `[package].version`, and
  errors otherwise (previously a missing/unparseable source silently became
  `0.1.0`, fabricating downgrades). `save_version` refuses to write a version
  below the current baseline, leaving both files untouched on rejection.
- **Git hooks resolve the real gitdir.** `GitHookManager` now resolves the hooks
  directory via `git rev-parse --git-path hooks` (handles monorepo subprojects,
  worktrees, custom `GIT_DIR`) instead of assuming `<repo>/.git/hooks`. When the
  watched path is a subdirectory of a larger repo and no explicit `hooks_dir` is
  configured, hook installation skips with a warning instead of fabricating a
  `.git` directory inside the subproject.

### Added — regression suite
- `tests/regressions.rs`: a monorepo fixture (outer git repo with an in-repo
  `proj/` subproject) driving the real daemon binary, asserting exactly one
  auto-commit for a genuine change, no cascade after version writeback, and no
  fake `.git` in the subproject. Runs under the existing `cargo test --tests`
  CI step.

## [9.7.16] — stable candidate

Consolidated summary of the capability set present at the stable line. See
`README.md` and `AGENTS.md` for full details; the sections below de-duplicate
the accumulated `Unreleased` notes.

### Added
- Multi-language semantic diff (Rust, Go, Swift, Kotlin, Java, TypeScript,
  JavaScript, Python, Ruby, Elixir, PHP, .NET/C#, C++, plus Lua, Scala,
  Clojure, Haskell, Julia, R, Perl) with version-aware parsing (LV-SCL) and
  parser confidence scoring.
- Five-dimension scoring (structural, API, dependencies, runtime, opt-in bundle)
  with configurable weights and thresholds.
- Deterministic semantic auto-versioning (Major/Minor/Patch/None) writing
  `VERSION` (and `Cargo.toml` when present).
- Aim of Change (AoC) sessions, traces, and agent interception.
- Post-commit qualification, stability scoring (confidence- and flaky-test
  aware), and an opt-in release pipeline (stable/nightly, daemon cron).
- `kaptaind-cli` companion: `status`, `log`, `analyze`, `dashboard`, `ci-hint`,
  `aoc`, `ship`, `trawl`, `monitor`, `service`, `shark`, `trace`, `vacs`,
  `storage`, and `rollback`.
- Angler hook & selective capture system (git hooks, webhooks with HMAC,
  selective capture, bait plugins).
- Supply-chain features: GPG-signed commits (`[commit] sign`), required-CI
  branch protection (`[push.protection]`), SPDX SBOMs, SLSA provenance, and
  signed release artifacts (behind `[ship]` config).
- Observability: health/metrics server with Prometheus exposition, SSE events,
  status/telemetry/stability artifacts, and an embedded `--web` dashboard.
- HA leadership (`shark`), RBAC, storage hygiene (`deckhand`), and intelligent
  project discovery (`trawl`).

### Changed
- Release engineering is now CI-driven: multi-arch artifacts, SHA256 checksums,
  SBOMs, and keyless (Sigstore) signatures are produced by the GitHub release
  workflow rather than by the daemon dogfooding itself.

## [Unreleased]

### Added
- **🎣 Angler Hook & Selective Capture System:** A comprehensive four-part automation system:
  - *Git Hooks Integration:* Manage client-side git hooks (pre-commit, post-commit, pre-push, etc.) with configurable commands, timeouts, and file pattern matching.
  - *Enhanced Webhooks:* HTTP webhooks with HMAC signature verification, exponential backoff retries, rate limiting, and event filtering.
  - *Selective Change Capture:* Pattern-based filtering with actions (Pass, Block, Quarantine, Tag, Webhook, Execute). Includes security-sensitive file detection and predefined templates.
  - *Bait Plugin System:* External plugins responding to lifecycle events. Auto-discovers from `.kaptaind/baits/`.
- **Adaptive Clustering:** `ClusterEngine` linearly interpolates the merge window from `window` toward `max_window_secs` as the current cluster grows toward `burst_threshold`; opt-in via `[cluster] adaptive = true`
- **LV-SCL (Language Version Syntax Contextualization Layer):** All 12 language adapters are now version-aware. Language versions are detected from project manifests (`Cargo.toml` edition, `go.mod`, `.python-version`, `tsconfig.json` target, `package.json` engines, etc.) and cached at `.kaptaind/version_cache.json` with a 1-hour TTL. Version-specific syntax recognized: Python 3.10+ `match`/`case` + 3.12 `type_alias`; Go 1.18+ generics; TypeScript 3.8+ `export type` / 5.0+ `type_alias`; Svelte 5 `$state`/`$derived` runes. Per-file parse metadata (language, version, parser kind, fallback flag) is emitted into every analysis artifact.
- **Parser Confidence Scoring:** Every parse produces a confidence metric (0–1) adjustable by parser type (AST → 0.95, fallback → 0.65) and version source certainty (Runtime > Manifest > Inferred > Unknown). Confidence scores are tracked in `FileParseMetadata` for audit trail. Mean confidence per commit is threaded through to stability scoring.
- **Dual-Source Version Detection:** `VersionSource` enum tracks whether version came from Runtime (future), Manifest (declared), or Inferred (guessed). All version detectors now report source. Foundation for future runtime checks (`node --version`, etc.) with higher confidence.
- **Confidence-Aware Stability Scoring:** Modified stability formula to apply penalty for low parser confidence: `Sₙ = clamp(Sₙ₋₁ + w₁·T + w₂·B − w₃·Δ − w₄·R − w₅·(1−C) − λ·Δt, 0, 1)` where C is mean parse confidence. Prevents false stability inflation from unreliable parses; system is now self-aware of parsing uncertainty.
- **Intelligent Thresholds:** `decide(weight, &VersionThresholdConfig)` reads `[version_thresholds].minor` and `.patch` from config (defaults: `0.6` / `0.1`). `decide_default()` preserves legacy behaviour.
- **Incremental LLM Gate:** `[inference] min_score_for_inference` — inference is skipped when `weight.score` is below this value, saving API quota on trivial changes.
- **Plugin Architecture:** `PluginAdapter` executes any external script/binary as a language adapter using a JSON stdio protocol (`stdin: {"file":"<path>"}` → `stdout: {"symbols":[...]}`). Configure under `[[plugins.adapters]]`. `Language::Plugin` variant added; plugins participate in the full cache, version-detection, and scoring pipeline.
- **Post-Commit Qualification & Release Pipeline:** When `[qualification] enabled = true`, the daemon runs a build, updates a continuous stability score (with confidence penalty), evaluates qualification (score threshold, pass streak, diff-spike guard, cooldown, test gate, build gate), packages a `.tar.gz` artifact with a SHA-256 `manifest.json`, and distributes it. Idempotent via `.kaptaind/releases/index.json`.
- **`kaptaind-cli dashboard`:** Live terminal dashboard showing version, daemon state, stability bar, LLM cost, release history, and the 5 most recent analysis artifacts.
- **`kaptaind-cli ci-hint`:** Emits release/hold recommendation in `text`, `json`, or `github` (GitHub Actions annotations + `set-output`) format, driven by qualification policy thresholds.
- New artifacts: `.kaptaind/stability.json`, `.kaptaind/version_cache.json`, `.kaptaind/releases/index.json`, `.kaptaind/release_version`, `.kaptaind/releases/<version>.tar.gz`

### Changed
- `diff::analyze` gains sibling `analyze_with_plugins(cluster, repo_root, &PluginsConfig)` — scheduler now calls this to pass plugin adapters through the full pipeline
- `api_score_with_cache` refactored into `api_score_inner(registry)` so both the default and plugin-extended registries share one implementation
- Telemetry now tracks `stability`, `releases`, and `failed_releases` counters
- `AocSession` gains optional `intent` and `target_stability` fields for stability-aware session tracking

## [v0.1.44]

### Added
- Furnace integration: SHA256 file-hash caching for AST parsing (70-90% cache hit ratio on large repos)
- Furnace integration: `syn`-based Rust AST parser replacing line-based heuristics
- Bundle size scoring dimension (5th analysis dimension, opt-in)
- Comprehensive documentation: Performance Tuning, Bundle Size, AoC Sessions, Migration Guide, Troubleshooting sections in README
- SECURITY.md: Safety guarantees, audit trail, secret protection, best practices

### Changed
- Rust adapter now uses `syn::parse_file()` for precise multi-line function signatures, struct fields, trait methods
- API surface detection now includes: route files (`app/`, `pages/`, `routes/`), design tokens (`tailwind.config`, `theme`), CSS custom properties
- Cache module created at `src/diff/cache.rs` with persistent `.kaptaind/ast_cache.json`
- Analysis JSON now includes `cache_hits` metric for observability

### Fixed
- Multi-line function signatures now detected correctly in Rust (previously only first line was scanned)
- Struct public fields now extracted as symbols (e.g., `MyStruct.field`)
- Trait methods and associated types now detected (e.g., `MyTrait::method`)
- Enum variants now detected as API surface (e.g., `MyEnum::Variant`)

## [v0.1.43]

### Added
- Support for web framework configs: `next.config.*`, `vite.config.*`, `vercel.json`, `tsconfig.*`, etc.
- Lock file detection for Yarn (`yarn.lock`), Bun (`bun.lockb`)
- CSS custom property detection (`--variable: value`)

### Changed
- Improved TypeScript/JavaScript export detection: `export default`, `export const`
- Route file detection now covers Next.js `app/`, `pages/`, SvelteKit `routes/`

## [v0.1.42]

### Added
- Aim of Change (AoC) sessions for intent-driven change grouping
- Agent interception via `kaptaind-cli aoc intercept`
- `.kaptaind/aoc/manifests/` for shipped session summaries

### Changed
- Commit message format now includes cluster UUID

## [v0.1.41]

### Added
- Configurable staging modes: `all` (default), `cluster`, `pattern`
- Exclude patterns to prevent sensitive files from being committed
- `.kaptainignore` initialization in `kaptaind-cli init`

### Changed
- Staging now respects `[staging]` config section

## [v0.1.40]

### Added
- `kaptaind-cli init` command for quick project setup
- Auto-detection of project type (Rust, Node, Python, Go, Swift, Kotlin)
- Per-language weight recommendations in generated `kaptaind.toml`

### Changed
- Default test command now per-language (e.g., `npm test` for Node, `cargo test` for Rust)

## [v0.1.39]

### Added
- API surface detection for 12 languages/frameworks
- Dependency manifests: `Cargo.toml`, `package.json`, `requirements.txt`, `build.gradle(.kts)`
- Lock file support: `Cargo.lock`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `poetry.lock`, `Podfile`, `gradle.lockfile`, `Package.resolved`, `bun.lockb`
- Runtime config detection: Docker, k8s, Helm, web frameworks, mobile configs

### Changed
- Scoring dimensions now structured as: structural, API, dependencies, runtime, bundle (5 total)
- Weight calculation supports arbitrary weights per dimension

## [v0.1.38]

### Added
- Language adapters for Go, Swift, Kotlin, TypeScript, JavaScript, Vue, Svelte, Astro, Python, SCSS, HTML/CSS
- Fallback line-based signature scanning for unrecognized file types
- Language confidence scoring (Rust/Go/Swift/Kotlin=1.0, TypeScript=0.9, etc.)

## [v0.1.37]

### Added
- Test hook gating: configurable test command runs before every commit
- `[test].required` flag to block commits on test failure
- Test failure reporting to `.kaptaind/status.json`

## [v0.1.36]

### Added
- Telemetry tracking: `.kaptaind/telemetry.json` with token usage and cost metrics
- Daemon status reporting: `.kaptaind/status.json` with real-time state (`Idle`, `Clustering`, `Testing`, `Committing`, `Failed`)

## [v0.1.35]

### Added
- Rate limiting: configurable `[ratelimit].min_commit_interval` (default 10 seconds)
- Prevents commit spam on rapid file saves

## [v0.1.34]

### Added
- Optional push support: `[push].enabled` and `[push].branch`
- Pushes are disabled by default
- Full git orchestration via `git2` crate

## [v0.1.33]

### Added
- Configurable notifications: shell hooks and webhook support (Discord/Slack)
- `[notify]` section with `on_commit` and `on_error` hooks
- Environment variables: `$KAPTAIND_VERSION`, `$KAPTAIND_SCORE`, `$KAPTAIND_MSG`, `$KAPTAIND_ERROR`

## [v0.1.32]

### Added
- Cluster-based event batching with configurable time window
- `[cluster].window` setting (default 5 seconds)
- Prevents rapid saves from triggering multiple commits

## [v0.1.31]

### Added
- Semantic versioning with automatic bump decision logic
- Major: breaking API removals
- Minor: new API additions or score > 0.6
- Patch: structural churn (score > 0.1)
- `VERSION` file management and `Cargo.toml` version syncing

## [v0.1.30]

### Added
- Structural diff scoring: event density, path spread, code churn
- Weighted score calculation: `s*structural + a*api + d*deps + r*runtime`

## [v0.1.29]

### Added
- Filesystem watcher using `notify` crate
- Cross-platform file event detection (Linux inotify, macOS FSEvents, Windows ReadDirectoryChangesW)
- `.kaptainignore` file support for path filtering

## [v0.1.28]

### Added
- Daemon mode: `kaptaind --daemon` for background operation
- Daemonization via `daemonize` crate
- `.kaptaind/daemon.pid`, `.kaptaind/daemon.out`, `.kaptaind/daemon.err` files

## [v0.1.27]

### Added
- Configuration file support: `kaptaind.toml` with sensible defaults
- Config sections: `[watch]`, `[cluster]`, `[weights]`, `[test]`, `[notify]`
- Path normalization and relative path resolution

## [v0.1.26]

### Added
- CLI binary: `kaptaind-cli` with subcommands
- `kaptaind-cli status`: daemon health and current version
- `kaptaind-cli log`: recent commits and versions
- `kaptaind-cli analyze`: dry-run analysis without committing

## [v0.1.25]

### Added
- Core daemon architecture with async Tokio runtime
- Event clustering engine
- Multi-language diff analysis pipeline
- Git commit orchestration with configurable staging

## [v0.1.0]

### Added
- Initial release
- Basic semantic versioning automation
- Single-language (Rust) API detection
- Filesystem watching
- Git commit creation

---

## Breaking Changes

### Between v0.1.38 and v0.1.39
- `.kaptaind/analysis/` JSON structure extended with `dependency_manifests`, `dependency_nodes`, `dependency_edges`, `runtime_paths` fields

### Between v0.1.30 and v0.1.31
- No breaking changes; scoring weights expanded to include runtime dimension

### Between v0.1.26 and v0.1.27
- Configuration format introduced; old hardcoded defaults replaced with `kaptaind.toml`

## Deprecations

None currently. All APIs and config formats are stable.

## Migration Guides

### From v0.1.27 to v0.1.28+ (daemon mode)
No changes needed. `kaptaind` continues to work in foreground; use `--daemon` for background operation.

### From v0.1.30 to v0.1.31+ (weights format)
Old format (no weights section) defaults to: `s=0.35, a=0.3, d=0.2, r=0.15`. Explicitly set in `[weights]` to customize.

### From v0.1.38 to v0.1.39+ (5-dimension scoring)
`b` (bundle) weight defaults to `0.0` (disabled). Set `b=0.05` or higher in `[weights]` to enable bundle scoring.

---

**Note**: From the stable line onward this changelog is curated per release and
each entry corresponds to an annotated, signed git tag produced by the CI
release workflow (see `.github/workflows/release.yml`). Earlier auto-bump noise
is preserved in `git log` but is not reproduced verbatim here.
