# kaptaind

`kaptaind` is an automated, daemon-based semantic versioning tool. It actively watches a repository for changes, batches those changes into logical clusters based on time windows, analyzes the impact of those changes, computes a semantic-version bump, writes the new `VERSION`, persists analysis artifacts, and automatically produces rich `git` commits on your behalf.

It eliminates manual version bumping and subjective commit messages by replacing them with deterministic, rule-based Git operations.

## Features

- **Filesystem watcher:** Native, OS-level filesystem event watching using `notify`.
- **Change clustering:** Automatically batches grouped sequences of fast file changes (default window: 5 seconds).
- **Intelligent Diff Scoring:**
  - *Structural:* Scores the amount of code churn and file spread.
  - *API Analysis:* Detects new, modified, and removed API surface by scanning exported signatures in Rust, Python, TS/JS.
  - *Dependency Tracking:* Parses `Cargo.toml`, `package.json`, and `requirements.txt` to calculate dependency risk.
  - *Runtime Impact:* Triggers high severity when deployment configs (`docker`, `k8s`, `.service`, etc.) are modified.
- **Semantic Auto-versioning:**
  - **Major:** Automatically bumped on breaking API removals.
  - **Minor:** Automatically bumped when new APIs are added, or diff scores reach the `> 0.6` threshold.
  - **Patch:** Bumped for standard structural churn and minor improvements.
- **Automated Commit Formatting:** Git commits are generated for each bump, summarizing what changed semantically (e.g., `kaptaind: Minor -> v0.2.0 [api-added; paths=4; api_touches=2; deps=0; runtime=0; score=0.62]`).
- **Test Hook Gating:** Automatically runs a configurable test hook (like `cargo test`) before committing; fails block the commit entirely.

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

### CLI Inspection (`kaptaind-cli`)

Kaptaind comes with a secondary binary to inspect the daemon's state:

```bash
# View live daemon health and current version
kaptaind-cli status

# View recent automated commits, scores, and bump reasons
kaptaind-cli log

# Dry-run an analysis on the current uncommitted working tree
kaptaind-cli analyze
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
   Once a cluster window closes, the diff is scored across four specific engines:
   - *Structural:* Counts raw path touches.
   - *AST/API:* Regex-scans code for exported signatures (`pub fn`, `export class`, `def`).
   - *Dependencies:* Parses `Cargo.toml`, `package.json`, etc.
   - *Runtime:* Detects changes to deployment orchestration files (Dockerfiles, Kubernetes configs).

5. **Test Hook & Telemetry Gating (`src/daemon/scheduler.rs`)**: 
   Before any commit, the daemon updates `.kaptaind/status.json` and runs the pre-configured test hook (`cargo test` by default). If tests fail, the workflow aborts. It also tracks the "token cost" of the diff size and commit message size, writing to `.kaptaind/telemetry.json`.

6. **Version Bump & Git Orchestration (`src/version/`, `src/commit/`)**: 
   The weights are aggregated. Breaking APIs trigger `Major` bumps; new APIs trigger `Minor`; standard churn triggers `Patch`. The new version is flushed to the `VERSION` file, a rich commit message is generated, and a JSON artifact is stored in `.kaptaind/analysis/` before `git2` creates the commit. Optional DBUS desktop notifications (via `notify-send`) are dispatched via the `[notify]` config block.

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

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10 # Seconds

[test]
command = "cargo test"
required = true

[notify]
# Optional shell hooks for desktop notifications (e.g. notify-send)
# Context variables available: $KAPTAIND_VERSION, $KAPTAIND_SCORE, $KAPTAIND_MSG, $KAPTAIND_ERROR
# on_commit = 'notify-send "Kaptaind Bump" "Version $KAPTAIND_VERSION"'
# on_error = 'notify-send -u critical "Kaptaind Error" "$KAPTAIND_ERROR"'
```

### .kaptainignore

You can configure what files `kaptaind` ignores by placing a `.kaptainignore` file in the root directory.

It supports:
- Blank lines and `#` comments
- Glob patterns matching relative paths (e.g. `**/*.tmp`)
- Exact paths or directory prefixes (e.g. `target`)

## Artifacts

As `kaptaind` runs, it drops critical artifacts:
- `VERSION`: Contains the authoritative, dynamically-managed semantic version (e.g. `0.1.2`).
- `.kaptaind/analysis/<uuid>.json`: Retains full structured evidence of exactly *why* a semantic bump occurred for every cluster that resulted in a commit.
- `.kaptaind/status.json`: Contains the real-time status of the daemon (`Idle`, `Clustering`, `Testing`, `Committing`, `Failed`), useful for integration with `i3status` or `polybar`.

## License

Standard open-source MIT License.
