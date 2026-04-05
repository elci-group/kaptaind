# kaptaind

`kaptaind` is an automated, daemon-based semantic versioning tool. It actively watches a repository for changes, batches those changes into logical clusters based on time windows, analyzes the impact of those changes, computes a semantic-version bump, writes the new `VERSION`, persists analysis artifacts, and automatically produces rich `git` commits on your behalf.

It eliminates manual version bumping and subjective commit messages by replacing them with deterministic, rule-based Git operations.

## Features

- **Filesystem watcher:** Native, OS-level filesystem event watching using `notify`.
- **Change clustering:** Automatically batches grouped sequences of fast file changes (default window: 5 seconds).
- **Multi-language diff analysis** with dedicated adapters for 12 languages/frameworks:
  - **Rust** — `pub fn`, `pub struct`, `pub enum`, `pub trait`
  - **Go** — exported functions and types (uppercase identifiers)
  - **Swift** — `public`/`open` funcs, classes, structs, enums, protocols, `@objc` exports
  - **Kotlin** — `fun`, `class`, `data class`, `sealed class`, `object`, `interface`, `@Composable`, `@JvmStatic`
  - **TypeScript** — all export kinds, React hooks (`useX`), Next.js route exports, middleware detection
  - **JavaScript** — ESM exports, `module.exports`, React hooks
  - **Vue** — `defineProps`, `defineEmits`, `defineExpose` (removing props/emits is breaking)
  - **Svelte** — `export let` props, Svelte 5 `$props()` runes
  - **Astro** — frontmatter exports, `Astro.props`
  - **SCSS/Sass/Less** — `$variables`, `@variables`, `@mixin`, `@forward`, CSS custom properties
  - **HTML/CSS** — CSS custom properties, class selectors
  - **Python** — `def`, `class`
- **Intelligent Diff Scoring** across five dimensions:
  - *Structural:* Scores the amount of code churn and file spread.
  - *API Analysis:* Detects new, modified, and removed API surface via language adapters and fallback line scanning. Recognizes framework route files (Next.js `app/`, `pages/`, SvelteKit `routes/`), design token files (`tailwind.config`, `theme`, `tokens`), and CSS custom properties as API surface.
  - *Dependency Tracking:* Parses `Cargo.toml`, `package.json`, `requirements.txt`. Recognizes `yarn.lock`, `bun.lockb`, `pnpm-lock.yaml`, `Podfile`, `build.gradle(.kts)`, and `gradle.lockfile`.
  - *Runtime Impact:* Triggers high severity when deployment configs (`docker`, `k8s`, `.service`), web configs (`next.config.*`, `vite.config.*`, `vercel.json`, `tsconfig.*`), or mobile configs (`Info.plist`, `AndroidManifest.xml`, `*.xcconfig`) are modified.
  - *Bundle Size (opt-in):* Runs a configurable build command, measures output directory size, and scores based on delta from previous build.
- **Semantic Auto-versioning:**
  - **Major:** Automatically bumped on breaking API removals.
  - **Minor:** Automatically bumped when new APIs are added, or diff scores reach the `> 0.6` threshold.
  - **Patch:** Bumped for standard structural churn and minor improvements.
- **Automated Commit Formatting:** Git commits are generated for each bump, summarizing what changed semantically (e.g., `kaptaind: Minor -> v0.2.0 [api-added; paths=4; api_touches=2; deps=0; runtime=0; score=0.62]`).
- **Test Hook Gating:** Automatically runs a configurable test hook (like `cargo test`) before committing; fails block the commit entirely.
- **Configurable staging:** Choose between staging all files (default), only cluster-touched files, or pattern-matched files. Exclude patterns prevent sensitive files from being committed.
- **Aim of Change (AoC) sessions:** Group related changes into named sessions with full trace history, agent interception, and shipped manifests.
- **Multi-Provider Inference Routing:** Intelligently routes commit message generation to the best available inference provider. Automatically detects and prioritizes: **Anthropic Claude** → **OpenAI GPT-4o** → **Local Ollama** fallback. No API keys needed; works offline with Ollama.

## Getting Started

### Installation

Clone the repository and build using Cargo:

```bash
cargo build --release
cp target/release/kaptaind ~/.local/bin/
```

### Running

To run `kaptaind` against your current directory in the foreground:

```bash
kaptaind
```

To securely run it as a detached background daemon:

```bash
kaptaind --daemon
```

![Kaptaind Daemon Status](running_and_status.gif)

### Quick Setup

Generate a `kaptaind.toml` and `.kaptainignore` tuned to your project type:

```bash
kaptaind-cli init
```

Supported project types: Rust, Node, Python, Go, Swift, Kotlin. The command auto-detects by looking for `Cargo.toml`, `package.json`, `Package.swift`, `build.gradle.kts`, etc.

### CLI Inspection (`kaptaind-cli`)

Kaptaind comes with a secondary binary to inspect the daemon's state:

```bash
# View live daemon health and current version
kaptaind-cli status

# View recent automated commits, scores, and bump reasons
kaptaind-cli log

# Dry-run an analysis on the current uncommitted working tree
kaptaind-cli analyze

# Manage Aim of Change sessions
kaptaind-cli aoc start "feature: auth flow"
kaptaind-cli aoc status
kaptaind-cli aoc ship

# Intercept agent operations for contextual tracing
kaptaind-cli aoc intercept --model claude-3-5-sonnet --intent "refactor auth" -- npm test
```

![Kaptaind Analyze and Log Demo](analyze_and_log.gif)

You can also use special `kaptaind` flags to see system indices:
- `kaptaind --dock`: View watched static projects.
- `kaptaind --radar`: View active projects and their event rates.
- `kaptaind --lanes`: View the load states of internal analysis engines.

![Kaptaind System Indices](views.gif)

### Background Architecture & Daemon Lifecycle

Kaptaind operates entirely in the background, minimizing developer friction while maintaining deep contextual awareness of codebase changes. Here is how the internal architecture flows:

1. **Daemonization & Persistence (`src/main.rs`)**: 
   When executed with `--daemon`, the process forks and detaches from the current shell using the `daemonize` crate. It drops a `daemon.pid` alongside `daemon.out` and `daemon.err` files in the `.kaptaind/` directory.

2. **Filesystem Watcher (`src/watcher/`)**: 
   A dedicated OS thread runs the `notify` watcher. It translates low-level `inotify`/`FSEvents` into abstract `FsEvent` models and pushes them across a cross-thread `tokio::mpsc` channel. 

3. **Temporal Clustering (`src/cluster/`)**: 
   As file events stream in, the `ClusterEngine` groups them based on the `[cluster].window` (default 5s). This prevents rapid saves (e.g., from an IDE format-on-save) from triggering dozens of distinct commits.

4. **Analysis Pipeline (`src/diff/`)**: 
   Once a cluster window closes, the diff is scored across five engines:
   - *Structural (`text.rs`):* Counts raw path touches, path spread, and churn.
   - *AST/API (`ast.rs` + `lang/`):* Language-aware adapters extract exported symbols for Rust, Go, Swift, Kotlin, TypeScript, JavaScript, Vue, Svelte, Astro, SCSS, HTML/CSS, and Python. A fallback line scanner handles unrecognized files.
   - *Dependencies (`api.rs`):* Parses `Cargo.toml`, `package.json`, `requirements.txt`; recognizes lock files for npm, Yarn, pnpm, Bun, Cargo, Poetry, CocoaPods, and Gradle.
   - *Runtime (`api.rs`):* Detects changes to deployment orchestration files (Docker, k8s, Helm), web framework configs (Next.js, Vite, Nuxt, Svelte, Astro, Tailwind, PostCSS, webpack), and mobile platform configs (Xcode, Gradle, Android).
   - *Bundle Size (`bundle.rs`, opt-in):* Runs a build command, measures output size, and scores the delta against the previous build.

5. **Test Hook & Telemetry Gating (`src/daemon/scheduler.rs`)**: 
   Before any commit, the daemon updates `.kaptaind/status.json` and runs the pre-configured test hook (`cargo test` by default). If tests fail, the workflow aborts. It also tracks the "token cost" of the diff size and commit message size, writing to `.kaptaind/telemetry.json`.

6. **Version Bump & Git Orchestration (`src/version/`, `src/commit/`)**: 
   The weights are aggregated. Breaking APIs trigger `Major` bumps; new APIs trigger `Minor`; standard churn triggers `Patch`. The new version is flushed to the `VERSION` file (and `Cargo.toml` if present), a rich commit message is generated, and a JSON artifact is stored in `.kaptaind/analysis/` before `git2` creates the commit. Staging is configurable: stage all files (default), only cluster-touched files, or pattern-matched files with optional excludes. Notifications are dispatched via shell hooks, Discord/Slack webhooks, or both.

## Configuration

`kaptaind` looks for an optional configuration file `kaptaind.toml` in the repository root.

If no file is found, it uses the following defaults:

```toml
repo_path = "."

[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5 # Events within 5 seconds belong to the same cluster

[weights]
s = 0.35 # Structural weight
a = 0.3  # API weight
d = 0.2  # Dependency weight
r = 0.15 # Runtime weight
b = 0.0  # Bundle size weight (opt-in, increase to enable)

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10 # Seconds

[test]
command = "cargo test"
required = true

[inference]
enabled = true
provider = "auto"          # "auto" (detect from env), "anthropic", "openai", or "ollama"
model = "auto"             # "auto" (provider default), or explicit model name
timeout_secs = 15
ollama_base_url = "http://localhost:11434"  # Only used when provider = "ollama"

[notify]
# Shell hooks — env vars: $KAPTAIND_VERSION, $KAPTAIND_SCORE, $KAPTAIND_MSG, $KAPTAIND_ERROR
# on_commit = 'notify-send "Kaptaind Bump" "Version $KAPTAIND_VERSION"'
# on_error = 'notify-send -u critical "Kaptaind Error" "$KAPTAIND_ERROR"'
# webhook_url = "https://discord.com/api/webhooks/..."  # Discord or Slack webhook

# [bundle]
# command = "npm run build"  # Build command to measure output size
# output_dir = "dist"        # Output directory (defaults to dist, build, .next, or out)

# [staging]
# mode = "all"               # "all" (default), "cluster" (only changed files), or "pattern"
# include = ["src/**"]       # Glob patterns to include (only used in "pattern" mode)
# exclude = ["*.log", ".env*"] # Glob patterns to always exclude from commits
```

### .kaptainignore

You can configure what files `kaptaind` ignores by placing a `.kaptainignore` file in the root directory.

It supports:
- Blank lines and `#` comments
- Glob patterns matching relative paths (e.g. `**/*.tmp`)
- Exact paths or directory prefixes (e.g. `target`)

## Performance Tuning

### Filesystem Watcher

The watcher performance varies by operating system:
- **Linux (inotify)**: Efficient for large repositories; scales well to thousands of files.
- **macOS (FSEvents)**: Coarse-grained events; may batch multiple file changes together.
- **Windows (ReadDirectoryChangesW)**: Works but can lag on very large directory trees.

To avoid excessive clustering on rapid saves (e.g., format-on-save), adjust the cluster window:

```toml
[cluster]
window = 5  # Default: groups events within 5 seconds
# Increase to 10 for slower feedback; decrease to 2 for snappier responsiveness
```

### Caching & AST Parsing

Kaptaind uses SHA256 file-hash caching to skip re-parsing unchanged files:
- **Cache hit**: File hash matches → reuse cached AST, skip `syn::parse_file()`
- **Cache miss**: File hash differs → parse with language adapter, update cache

Cache files live in `.kaptaind/ast_cache.json`. On large repositories, expect 70-90% cache hit ratios across commits.

If the cache becomes stale or corrupted, safely delete `.kaptaind/ast_cache.json`; it will be regenerated on the next analysis.

### Staging Mode Performance

Three staging modes trade off safety vs. speed:

- **`all` (default)**: Stages everything, then removes `exclude` patterns. Safest; minimal overhead.
- **`cluster`**: Stages only changed files + `VERSION` + `Cargo.toml`. Fastest for large repos; excludes unrelated changes.
- **`pattern`**: Stages files matching `include` globs, removes `exclude` patterns. Useful for monorepos with strict file boundaries.

For monorepos with 1000+ files, `cluster` staging can reduce staging time by 10-20x.

### Test Hook Performance

Required test hooks block commits on failure, which can be expensive:

```toml
[test]
command = "cargo test"
required = false  # Set to false to make hook optional; failures don't block
```

If tests are slow, consider running a fast smoke test in `kaptaind` and a full suite in CI/CD:

```toml
[test]
command = "cargo test --lib"  # Skip integration tests for speed
```

## Bundle Size Scoring

Bundle size scoring measures the impact of changes on your output artifact (JavaScript bundles, compiled binaries, etc.). This is useful for teams shipping to bandwidth-constrained environments or tracking performance regressions.

### Setup

First, enable bundle weight in your config:

```toml
[weights]
s = 0.35  # Structural
a = 0.3   # API
d = 0.2   # Dependencies
r = 0.15  # Runtime
b = 0.05  # Bundle (opt-in; increase to prioritize bundle size)

[bundle]
command = "npm run build"    # Your build command
output_dir = "dist"          # Output directory (optional; auto-detects dist, build, .next, out)
```

### How It Works

1. On first analysis, kaptaind runs your build command and measures the total size of files in `output_dir`.
2. It stores the size in `.kaptaind/bundle.json`.
3. On subsequent analyses, it measures the new size and computes: `score = |new - old| / old`, clamped to `[0, 1]`.
4. This score is weighted by `b` (default `0.0`, meaning disabled) and included in the overall diff score.

### Example: Next.js Project

```toml
[bundle]
command = "npm run build"
output_dir = ".next"  # Next.js default output

[weights]
b = 0.1  # Bundle size contributes 10% to overall score; score > 0.6 triggers Minor version bump
```

After a change that increases the bundle by 5%, you might see:
```
kaptaind: Minor -> v0.2.0 [api-stable; paths=3; api_touches=0; deps=0; runtime=0; bundle=0.05; score=0.58]
```

### Troubleshooting Bundle Scoring

- **Build fails**: If `command` fails, bundle scoring is skipped (score `0.0`). Check `.kaptaind/status.json` for error details.
- **No output directory**: If `output_dir` doesn't exist after build, bundle score is `0.0`.
- **Stale size**: Delete `.kaptaind/bundle.json` to force a full re-baseline on the next analysis.

## Aim of Change (AoC) Sessions

Aim of Change sessions group related changes into named, intent-driven clusters with full traceability. This is useful for tracking feature work, refactoring, or coordinated multi-file changes.

### Starting a Session

```bash
kaptaind-cli aoc start "feature: authentication flow"
```

From this point forward, all commits will be tagged with this session and linked in `.kaptaind/aoc/active.json`.

### Checking Status

```bash
kaptaind-cli aoc status
```

Shows the active session name and commit count so far.

### Shipping the Session

```bash
kaptaind-cli aoc ship
```

Finalizes the session and moves the summary to `.kaptaind/aoc/manifests/<id>.json`. Useful for generating release notes or linking to deploy events.

### Agent Interception

For enhanced observability, pair AoC with agent-assisted change validation:

```bash
kaptaind-cli aoc intercept --model claude-3-5-sonnet --intent "refactor auth" -- npm test
```

This runs `npm test`, captures the output, and stores it alongside the AoC trace. Useful for audit trails in regulated environments.

## Migration Guide: Existing Projects

If your repo already has a version history (even irregular), you can safely adopt kaptaind:

### Step 1: Generate Config

```bash
cd /path/to/existing/repo
kaptaind-cli init
```

This creates `kaptaind.toml` and `.kaptainignore` based on your project type.

### Step 2: Verify Current VERSION

Check if a `VERSION` file exists:

```bash
cat VERSION    # If it exists, kaptaind will continue from here
# or
cat Cargo.toml | grep -A1 "\[package\]" | grep version  # Rust: falls back to Cargo.toml
```

If neither exists, kaptaind defaults to `0.1.0` on first commit.

### Step 3: Backfill Analysis Artifacts (Optional)

To preserve your version history, manually create a `.kaptaind/analysis/` directory:

```bash
mkdir -p .kaptaind/analysis
```

Existing commits won't be re-analyzed, but kaptaind will start producing analysis JSONs for new commits.

### Step 4: Dry-Run

Test the configuration without committing:

```bash
kaptaind-cli analyze
```

Review the output. If the score seems off, adjust weights in `kaptaind.toml`.

### Step 5: Enable Daemon

When confident:

```bash
kaptaind --daemon
```

The daemon will pick up on the next file change.

### Notes

- **Existing CI/CD**: Kaptaind doesn't interfere with GitHub Actions, GitLab CI, etc. It commits independently, and CI runs normally on those commits.
- **Push conflicts**: If you have `[push].enabled = true`, ensure your CI doesn't also push to the same branch, or use different branches.
- **Test hooks**: The configured `[test].command` will run before every kaptaind-triggered commit. If you want a lightweight check, use a fast smoke test here and full tests in CI.

## Troubleshooting

### "Kaptaind is committing too frequently"

**Symptom**: A new commit appears every few seconds.

**Cause**: Cluster window is too small, or your editor is saving files very rapidly.

**Fix**:
```toml
[cluster]
window = 10  # Increase from default 5 to 10 seconds
```

Or configure your editor to debounce saves (e.g., VS Code: `files.autoSaveDelay = 2000`).

### "Kaptaind is never committing"

**Symptom**: You make changes but no commits appear.

**Cause**: Either the watcher isn't active, or `[test].required = true` and tests are failing.

**Fix**:
```bash
# Check daemon is running
ps aux | grep kaptaind

# Check test command manually
cargo test  # or your configured test command

# Check status
kaptaind-cli status
```

### "Test hook is blocking every commit"

**Symptom**: Tests fail, and kaptaind refuses to commit.

**Cause**: `[test].required = true` (default).

**Fix**:
```toml
[test]
required = false  # Tests won't block, but will still be logged
```

Or fix the failing tests.

### "Daemon won't start on Linux"

**Symptom**: `kaptaind --daemon` hangs or exits immediately.

**Cause**: Daemonization requires write permissions to `.kaptaind/` and parent directories.

**Fix**:
```bash
mkdir -p /path/to/repo/.kaptaind
chmod 755 /path/to/repo/.kaptaind
kaptaind --daemon
```

Check logs:
```bash
tail -50 .kaptaind/daemon.err
```

### "Cache is stale or giving wrong results"

**Symptom**: AST detection seems off after a code change.

**Cause**: Cache file mismatch or corruption.

**Fix**:
```bash
rm .kaptaind/ast_cache.json
# Next analysis will regenerate
```

### "Git status is dirty after kaptaind commit"

**Symptom**: `git status` shows staged or unstaged changes after kaptaind commits.

**Cause**: Staging mode is `pattern` or `cluster`, and some files weren't staged.

**Fix**: Either change to `all` (default, stage everything) or intentionally stage the remaining files separately.

### "Version bumps seem wrong"

**Symptom**: Minor changes bump the version more than expected.

**Cause**: Weights are imbalanced, or a single dimension scores high.

**Fix**: Review the analysis artifact:
```bash
cat .kaptaind/analysis/*.json | jq '.api, .deps, .runtime, .structural'
```

Adjust weights in `kaptaind.toml` if needed:
```toml
[weights]
a = 0.5  # Increase API weight if API changes matter most to you
```

## Artifacts

As `kaptaind` runs, it drops critical artifacts:
- `VERSION`: Contains the authoritative, dynamically-managed semantic version (e.g. `0.1.2`).
- `.kaptaind/analysis/<uuid>.json`: Full structured evidence of *why* a semantic bump occurred for every cluster that resulted in a commit.
- `.kaptaind/status.json`: Real-time daemon state (`Idle`, `Clustering`, `Testing`, `Committing`, `Failed`), useful for integration with `i3status` or `polybar`.
- `.kaptaind/telemetry.json`: Token usage and cost tracking metrics.
- `.kaptaind/bundle.json`: Previous bundle size state (when bundle scoring is enabled).
- `.kaptaind/traces/<uuid>.json`: Per-cluster trace records linked to AoC sessions.
- `.kaptaind/aoc/active.json`: Currently active Aim of Change session.
- `.kaptaind/aoc/manifests/<id>.json`: Shipped AoC session summaries.

## License

Standard open-source MIT License.
