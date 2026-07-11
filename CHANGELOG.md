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

## [Unreleased]

Security hardening pass (audit remediation). No version bump yet.

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
