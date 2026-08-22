# AGENTS.md

> Operator guide for working on `kaptaind`. Read this before changing core behavior, adding a language adapter, or shipping a release.

## Project overview

`kaptaind` is a Rust application (`Cargo.toml`, edition 2021) that watches a repository for filesystem changes, clusters events, analyzes the change set, computes a semantic-version bump, writes `VERSION`, persists analysis artifacts, creates a git commit, and optionally pushes.

- Entry point: `src/main.rs` initializes tracing, loads config, then starts the daemon runtime.
- CLI entry point: `src/cli/main.rs` runs `kaptaind-cli` subcommands.

## Essential commands

- `cargo run` — run the daemon from the repository root.
- `cargo test` — run the unit and async tests embedded in module files.
- `cargo build` — build the binary.
- `cargo run --bin kaptaind-cli -- ship plan` — preview a manual release without side effects.
- `cargo fmt && cargo clippy --all-targets -- -D warnings` — required before committing.

## Repository layout

| Path | Responsibility |
|------|----------------|
| `src/main.rs` | Daemon startup wiring, `--config` handling, tracing init. |
| `src/cli/main.rs` | CLI binary (`kaptaind-cli`): argument parsing, `Commands` enum, and dispatch. |
| `src/cli/commands/` | Per-command handler modules (`status`, `log`, `analyze`, `init`, `aoc`, `ship`, `trawl`, etc.). |
| `src/config/` | Config loading, path normalization, defaults, structs for staging/bundle/notify/etc. |
| `src/watcher/` | Filesystem event types and notify-based watcher thread. |
| `src/daemon/` | Async runtime, scheduler loop, telemetry, health/metrics server, optional WebUI (`web.rs`, `web_ui.html`), storage hygiene (`deckhand`), HA leadership (`shark`), notifications, audit logging. |
| `src/cluster/` | Event clustering by time window. |
| `src/diff/` | Scoring across five dimensions: structural (`text.rs`), API surface (`ast.rs`), dependencies/runtime (`api.rs`), bundle size (`bundle.rs`), and the language adapter framework (`lang/`). |
| `src/diff/lang/` | `adapter.rs` (traits/representation), `registry.rs` (path→adapter resolution), `adapters/` (concrete adapters), `common.rs` (shared helpers). |
| `src/weight/` | Weighted score calculation: `s*structural + a*api + d*deps + r*runtime + b*bundle`. |
| `src/version/` | Semantic bump decision, `semver::Version` mutation, Cargo workspace discovery (`workspace.rs`), and version writeback (`writeback.rs`: `save_version` + `save_workspace_version` behind `[versioning].workspace`, default `root_only`). |
| `src/commit/` | Git commit orchestration with configurable staging and optional GPG-signed commits. |
| `src/push/` | Git push orchestration with retry, safety checks, and required-CI enforcement. |
| `src/git/` | Thin repository wrapper around `git2`. |
| `src/aoc/` | Aim of Change sessions, traces, agent interception, manifests. |
| `src/qualification/` | Release qualification gates (stability, streak, cooldown, diff spike). |
| `src/stability/` | Per-commit stability scoring and flaky-test detection. |
| `src/release/` | Post-commit release pipeline: build, package, distribute, ship stable/nightly, GPG-signed tags/artifacts, SPDX SBOMs, SLSA provenance. |
| `src/rbac/` | Role-based access control for CLI commands and daemon startup. |
| `src/schedule/` | Cron scheduling helpers for daemon-driven automated releases. |
| `src/trawler/` | Intelligent project discovery and bulk initialization. |
| `src/vacs/` | Visual Asset Channel Saturation (change-driven diagrams/charts). |
| `src/angler/` | Hook and selective capture system: git hooks, webhooks, selective capture, bait plugins. |
| `tests/cli_integration.rs` | Integration tests for CLI commands. |
| `web/` | Kaptaind Pro SaaS website (Next.js + Tailwind + NextAuth + Prisma). |

## Runtime flow

1. **Startup**: `config::loader::load()` reads `kaptaind.toml` (or `--config` path), then `finalize_config()` normalizes all paths relative to the process working directory.
2. **Watcher spawn**: `daemon::runtime::start()` creates a Tokio MPSC channel and starts the OS watcher thread via `watcher::fs::start()`.
3. **Event ingestion**: `watcher::fs::start()` converts `notify` events into `FsEvent` values and sends them across the channel. The scheduler receives them on the async runtime.
4. **Clustering**: `daemon::scheduler::run()` batches events with `ClusterEngine`. Events are grouped while the time delta is strictly less than the configured window.
5. **Filtering & rate limits**: ignored paths are dropped, and commits are rate-limited by `min_commit_interval`.
6. **Suspend gate**: if `.kaptaind/suspend.json` marks the daemon as suspended, the cluster is skipped and a `SUSPENDED` decision is recorded. Manual `kaptaind-cli suspend`/`resume` or AoC start/ship/cancel read/write this file.
7. **Validation**: the configured test hook runs. A passing hook reduces runtime weight to `0.1`; a failing hook forces it to `1.0`.
7. **Diff analysis**: structural + API + dependency + runtime + optional bundle scoring are computed.
8. **Versioning**: `weight::calculator` combines scores, then `version::semver` decides `Major`/`Minor`/`Patch`/`None` and writes `VERSION` (+ updates `Cargo.toml` if present).
9. **Persistence**: analysis artifacts, telemetry, traces, and bundle state are written under `.kaptaind/`.
10. **Commit & push**: the scheduler stages files per `StagingConfig`, creates the commit, and pushes if enabled.
11. **Lifecycle hooks**: Angler hooks (pre-commit, post-commit, webhooks, selective capture, bait plugins) run at the appropriate points.
12. **Release automation**: qualification gates are evaluated, and—when `[ship.auto_nightly]`/`[ship.auto_stable]` are enabled—automated ship releases run on their cron schedules.
13. **WebUI (opt-in)**: when `--web` is passed, `daemon::runtime::start()` spawns the WebUI server on `--web-port` (default 8080), sharing the same event broadcast channel for live updates.
14. **Notifications**: nautical-themed commit/push/error/start/stop/release/qualification/pulse notifications are emitted via shell hooks, webhooks, and the health server's SSE endpoint.

## Configuration and on-disk files

- Main config file: `kaptaind.toml` in the current working directory, or the path passed via `--config`.
- If no config file exists, defaults come from `src/config/loader.rs`.
- Important observed defaults:
  - watch path defaults to the current directory.
  - watcher is recursive by default.
  - ignore file defaults to `.kaptainignore`.
  - cluster window defaults to 5 seconds.
  - minimum commit interval defaults to 10 seconds.
  - test hook defaults to `cargo test` and is required by default.
  - push is disabled by default and targets branch `main` when enabled.
  - WebUI is disabled by default; `--web` starts it on port 8080.
  - `[daemon].auto_suspend_on_aoc_start` and `[daemon].auto_resume_on_aoc_end` are `true` by default; starting an AoC session suspends the daemon, and shipping/cancelling it resumes the daemon.
- Paths are normalized in `finalize_config()`:
  - `repo_path` is resolved relative to the process working directory.
  - `watch.path` and `watch.ignore_file` are resolved relative to `repo_path`.
- Runtime artifacts:
  - `VERSION` in `repo_path` is read/written as the authoritative semantic version.
  - `.kaptaind/analysis/<cluster-id>.json` stores analysis artifacts for each processed cluster.
  - `.kaptaind/status.json` — daemon state for external integrations; includes `current_task` and `progress_percent` for live progress UI.
  - `.kaptaind/telemetry.json` — token usage and cost tracking.
  - `.kaptaind/bundle.json` — previous bundle size (when bundle scoring is enabled).
  - `.kaptaind/traces/<cluster-id>.json` — per-cluster trace records linked to AoC sessions.
  - `.kaptaind/suspend.json` — daemon suspension state (manual or AoC-driven).
  - `.kaptaind/aoc/active.json` — active Aim of Change session.
  - `.kaptaind/aoc/manifests/<id>.json` — shipped AoC session summaries.
- Environment variables:
  - kaptaind loads an optional `.env` file at startup (daemon and CLI) so provider keys and secrets can stay out of `kaptaind.toml`.
  - TTS provider keys are read from `.env`: `ELEVENLABS_API_KEY`, `OPENAI_API_KEY`, `AZURE_SPEECH_KEY`/`AZURE_SPEECH_REGION`, `GOOGLE_API_KEY`, `CARTESIA_API_KEY`. Local system TTS uses `say` (macOS), `espeak` (Linux), or PowerShell (Windows).

## Code patterns and conventions

- Module pattern is simple and explicit: each `mod.rs` re-exports the module’s public entry points.
- Error handling uses `anyhow` for application-level fallible boundaries and `git2::Error` where git operations are returned directly.
- Logging uses `tracing`; startup uses `tracing_subscriber::fmt::init()`.
- Most structs derive `Debug` and `Clone`; serde derives are used where data crosses config/artifact boundaries.
- Async work is confined to the daemon/scheduler path; filesystem watching is done on a dedicated OS thread and bridged into Tokio via `blocking_send`.
- Tests live inline in the same source files under `#[cfg(test)]`.

## Common tasks

### Add a new language adapter

1. Create `src/diff/lang/adapters/<lang>.rs` implementing `LanguageAdapter`.
2. Register it in `src/diff/lang/registry.rs`.
3. Add unit tests in the adapter module covering public symbols, route files, and design tokens.
4. Run `cargo test` and `cargo clippy --all-targets -- -D warnings`.

### Change version bump behavior

1. Edit the rules in `src/version/semver.rs`.
2. Update tests in the same file.
3. Update `README.md` and `man/kaptaind.1.md` if thresholds or semantics change.

### Add a notification event

1. Add the event variant in `src/daemon/notification.rs`.
2. Update the shell/webhook renderers and the SSE payload format.
3. If the event should trigger the storm/siren overlay, broadcast a `warning` SSE event from the scheduler.
4. Document environment variables in `README.md` under the Notifications section.

### Add a TTS provider

1. Add the variant to `TtsProvider` in `src/notify/audio.rs`.
2. Implement an async `*_speak` function and wire it in `speak_with_provider()`.
3. Update env-key detection in `resolve_provider()` and add the provider to the `[notify.tts]` config docs.
4. Add a unit test for provider parsing/env resolution.

### Change task-progress visuals

1. Update `StatusReport` fields in `src/daemon/status.rs` and the scheduler state transitions in `src/daemon/scheduler.rs`.
2. The embedded WebUI (`src/daemon/web_ui.html`) derives sky/siren visuals from `status.json` and `warning` SSE events.
3. The Next.js dashboard derives visuals from the same `status.json` via `DaemonStatusBadge.tsx` and `TaskProgress.tsx`.

### Ship a release manually

```bash
cargo run --bin kaptaind-cli -- ship plan   # dry run
cargo run --bin kaptaind-cli -- ship run    # execute
```

### Suspend and resume the daemon

```bash
kaptaind-cli suspend --reason "manual hold"
kaptaind-cli resume
```

- Starting an AoC session auto-suspends the daemon when `[daemon].auto_suspend_on_aoc_start = true` (default).
- Shipping or cancelling an AoC session auto-resumes it when `[daemon].auto_resume_on_aoc_end = true` (default).
- While suspended, `process_cluster` records a `SUSPENDED` decision and skips tests/analysis/commits.

### Debug the daemon

```bash
RUST_LOG=kaptaind=debug cargo run
```

## Scoring and versioning behavior

- `src/diff/text.rs` computes structural score: `0.5*event_density + 0.35*path_spread + 0.15*churn`.
- `src/diff/ast.rs` uses the `AdapterRegistry` to resolve language-specific adapters. If no adapter matches, falls back to line-based signature scanning.
- Language adapters (in `src/diff/lang/adapters/`) detect public API symbols for: Rust, Go, Swift, Kotlin, TypeScript, JavaScript, Vue, Svelte, Astro, SCSS/Sass/Less, HTML/CSS, Python.
- Scores are normalized by language confidence: Rust/Go/Swift/Kotlin=1.0, TypeScript=0.9, Vue/Svelte/Astro=0.85, Python=0.8, JavaScript=0.7, SCSS=0.5, HTML/CSS=0.4.
- API surface detection also covers: paths containing `/api/`, `/public/`, `.proto`, `.graphql`, `openapi.yaml/yml`; framework route files (`app/`, `pages/`, `routes/`); design token files (`tailwind.config`, `theme`, `tokens`); CSS custom properties.
- `src/diff/api.rs` parses dependency manifests from `Cargo.toml`, `package.json`, `requirements.txt`. Recognizes lock files: `Cargo.lock`, `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lockb`, `poetry.lock`, `Podfile`, `Podfile.lock`, `Package.resolved`, `build.gradle(.kts)`, `settings.gradle(.kts)`, `gradle.lockfile`.
- `src/diff/api.rs` runtime detection: `docker`, `deploy`, `k8s`, `helm`, `.sh`, `.service`, `.env`; web configs (`next.config.*`, `vite.config.*`, `nuxt.config.*`, `svelte.config.*`, `astro.config.*`, `tsconfig.*`, `jsconfig.*`, `webpack.config.*`, `postcss.config.*`, `tailwind.config.*`, `vercel.json`, `netlify.toml`, `.babelrc`); mobile configs (`Info.plist`, `project.pbxproj`, `AndroidManifest.xml`, `*.xcconfig`, `*.entitlements`, `proguard-rules.pro`, `gradle.properties`).
- `src/diff/bundle.rs` (opt-in): runs a build command, measures output dir size, scores `|new - old| / old`, clamped to [0, 1].
- `src/weight/calculator.rs` combines scores: `s*structural + a*api + d*deps + r*runtime + b*bundle`.
- `src/version/semver.rs` decides bumps with these rules:
  - breaking API => `Major`
  - added API or score `> 0.6` => `Minor`
  - score `> 0.1` => `Patch`
  - otherwise => `None`

## Git behavior

- Repo access goes through `git2`.
- `commit::orchestrator` supports three staging modes via `StagingConfig`:
  - `all` (default): `index.add_all(["*"])` stages everything, then removes `exclude` patterns.
  - `cluster`: only stages files from the detected cluster + `VERSION` + `Cargo.toml`.
  - `pattern`: stages files matching `include` globs, removes `exclude` patterns.
- The scheduler skips work when `Repo::is_clean()` reports no changes.
- Commit message format is generated in `src/daemon/scheduler.rs` and includes bump, version, API summary, touched path count, dependency/runtime stats, score, cluster UUID, and agent model (if AoC intercepted).
- `save_version()` writes `VERSION` and also updates `version` in `Cargo.toml` if present.
- Pushes only happen when `config.push.enabled` is true; the code pushes `refs/heads/<branch>` to `origin`.

## Ignore and watcher behavior

- Ignore rules are loaded from `.kaptainignore` relative to `repo_path` unless overridden in config.
- Ignore file behavior is custom, not gitignore-compatible:
  - blank lines and `#` comments are ignored.
  - entries containing glob metacharacters (`*`, `?`, `[`, `{`) are treated as glob patterns via `globset`.
  - other entries are treated as exact relative paths/prefixes.
- Paths are matched relative to `repo_path` when possible.
- Watcher startup is synchronized with a readiness channel; startup failures are surfaced before returning from `watcher::fs::start()`.

## Testing approach

- Unit tests are colocated in modules:
  - `src/cluster/engine.rs`
  - `src/config/loader.rs` — path normalization, staging config deserialization
  - `src/diff/api.rs` — dependency/runtime detection, web/mobile configs
  - `src/diff/ast.rs` — signature detection, route files, design tokens
  - `src/diff/bundle.rs` — bundle scoring, backward compat
  - `src/diff/lang/adapters/` — all 12 language adapters
  - `src/version/semver.rs`
  - `src/daemon/scheduler.rs`
- Integration tests in `tests/cli_integration.rs` cover: `status`, `log`, `analyze`, `init` commands.
- Tests use `tempfile` heavily for filesystem-dependent behavior.
- Async behavior is tested with `#[tokio::test]` in `src/daemon/scheduler.rs`.
- The scheduler’s test hook runs commands with `sh -lc <command>` and sets the working directory to `repo_path`.

## Agent gotchas

- Run commands from the repository root if you want config discovery to find `kaptaind.toml`.
- `Config::default()` uses the process current directory at runtime; tests or tools that change cwd can affect defaults.
- Required test hooks block commits on failure; optional hooks do not.
- A passing test hook reduces runtime weight to `0.1`; a failing hook forces runtime weight to `1.0`.
- When no `VERSION` file exists, the scheduler starts from `0.1.0`.
- `ClusterEngine` groups events only while the time delta is strictly less than the configured window.
- Ignore matching checks whether any path in an event matches; one ignored path suppresses the whole event.
- Pre-commit gate is `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings` (enforced in `.github/workflows/rust.yml`).
- Supply-chain checks run in `.github/workflows/security-audit.yml`: `cargo audit`, `cargo deny check` (config in `deny.toml`), and `npm audit` for `web/`.
- The Rust toolchain is pinned via `rust-toolchain.toml` (stable, clippy + rustfmt). Keep code building on stable.
- Do not run the kaptaind daemon against this repository during release work — it dogfood-versions `VERSION` and creates noisy auto-commits. Keep `VERSION`, `Cargo.toml`, `CHANGELOG.md`, and git tags in agreement; tags are cut by CI, not by the daemon.
- The repo dogfoods `[versioning].workspace = "touched"` (see `docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md`): a cluster touching only `crates/kaptaind-diff/**` bumps the member manifest, not the root `VERSION`. CI cuts two tag shapes — `vX.Y.Z` (root, drives the release matrix) and `kaptaind-diff-vX.Y.Z` (member, tag only) — each created only when missing.
- The repo's `kaptaind.toml` sets `[daemon] startup_guard = true`: the daemon refuses to start while the worktree is dirty (accidental starts must not catch-up-commit release work). A deliberate run needs `--force`.
- `deckhand` is a pinned git dependency (`Cargo.toml`). To hack on it against a sibling checkout, create a local, gitignored `.cargo/config.toml` with `paths = ["../deckhand"]`; bump the `rev` in `Cargo.toml` to ship a newer deckhand.
