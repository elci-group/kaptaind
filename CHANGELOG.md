# Changelog

All notable changes to kaptaind are documented here.

## [Unreleased]

### Added
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

**Note**: This changelog is maintained automatically by kaptaind itself. Each version entry reflects commits tagged in git history.
