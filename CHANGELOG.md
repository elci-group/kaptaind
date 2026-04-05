# Changelog

All notable changes to kaptaind are documented here.

## [Unreleased]

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
