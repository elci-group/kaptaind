% KAPTAIND(1) kaptaind 10.3.3
% Elci Group
% September 2026

# NAME

**kaptaind** — automated semantic versioning daemon and command-line companion

# SYNOPSIS

**kaptaind** [**OPTIONS**]

**kaptaind** **--daemon**

**kaptaind** **--dock** | **--radar** | **--lanes**

**kaptaind** [**--repo**=*PATH*] *COMMAND* [*ARGS*]

# DESCRIPTION

**kaptaind** is a self-governing release companion that watches a repository for filesystem changes, clusters related events, analyzes the change set across structural, API, dependency, runtime, and bundle dimensions, computes a semantic-version bump, writes the **VERSION** file, persists analysis artifacts, creates a git commit, and optionally pushes.

Run **kaptaind** with no arguments (or **--daemon**) to run the watcher — in the foreground to see logs directly, or detached under the project's **.kaptaind/** directory. Run **kaptaind** with a subcommand (*COMMAND*) for one-off visibility into the daemon's state, dry-run analyses, Aim-of-Change session management, release qualification and shipping, and storage/high-availability tasks — all without requiring the daemon to be running. Both surfaces are one binary; there is no separate `kaptaind-cli` executable.

Most subcommands read **./kaptaind.toml** to determine repository paths and settings. Use **--repo** to override the repository path for a single invocation.

# GLOBAL OPTIONS

**-r**, **--repo**=*PATH*
:   Operate on *PATH* instead of the repository configured in **kaptaind.toml**. Applies to subcommands.

**-V**, **--version**
:   Print the version and exit.

**-h**, **--help**
:   Print help information and exit. With no further arguments, prints a colorized, grouped overview of every subcommand.

# DAEMON OPTIONS

These apply to bare **kaptaind** invocations (no subcommand) — running the watcher itself.

**-c**, **--config**=*PATH*
:   Path to the configuration file. Defaults to **./kaptaind.toml** in the current working directory. See also **KAPTAIND_CONFIG** below.

**-d**, **--daemon**
:   Run as a background daemon. Detaches from the terminal, redirects stdout/stderr to **.kaptaind/daemon.out** and **.kaptaind/daemon.err**, and writes the process ID to **.kaptaind/daemon.pid**.

**--dock**
:   Print the static list of watched projects (Dock view) and exit. Useful for confirming which repository is being monitored.

**--force**
:   Start even when the worktree has uncommitted changes. Overrides **[daemon] startup_guard = true** in *kaptaind.toml*, which otherwise refuses to start on a dirty tree — a protection for release trees where daemon runs are exceptional and an accidental start must not catch-up-commit in-flight work.

**--radar**
:   Print active project activity and event rates (Radar view) and exit.

**--lanes**
:   Print the service/model load breakdown (Lanes view) and exit. Useful for quick operational checks.

**--shark-mode**=*MODE*
:   Override the Shark Stating HA mode for this instance. *MODE* may be **auto**, **leader**, **standby**, or **observer**.

**--shark-arbiter**=*PATH*
:   Override the shared directory used for leadership leases when running multiple instances against the same repository.

**--health-port**=*PORT*
:   Override the health/metrics server port. Useful when running multiple instances on the same host, for example during a zero-downtime upgrade.

**-w**, **--web**
:   Start the embedded WebUI dashboard alongside the daemon runtime. The WebUI is served on the port configured by **--web-port** (default 8080).

**--web-port**=*PORT*
:   Override the WebUI server port. Must be different from **--health-port**.

**--dry-run**
:   Show the decision the daemon would make for pending changes without staging or committing.

# COMMANDS

## status

**kaptaind status**

Show current daemon health, version, and recent errors. Reports the daemon state (Idle, Clustering, Testing, Committing, Failed), installed binary locations, and current version.

Example:

    kaptaind status

## validate

**kaptaind validate**

Validate **kaptaind.toml** and report cross-field configuration errors, such as timeout constraints and Shark TTL consistency. Exits non-zero if validation fails.

Example:

    kaptaind validate

## log

**kaptaind log** [**-l** *N* | **--limit** *N*]

View recent automated commits and analysis decisions.

**-l**, **--limit**=*N*
:   Number of commits to display. Default: 10.

Example:

    kaptaind log --limit 20

## analyze

**kaptaind analyze**

Dry-run semantic diff analysis on the current working tree without committing. Shows score breakdown, detected API/dependency/runtime changes, and projected version bump.

Example:

    kaptaind analyze

## pull

**kaptaind pull** [**--remote** *REMOTE*] [**--branch** *BRANCH*]
[**--strategy** *STRATEGY*] [**--check**] [**--dry-run**] [**--force**]
[**--autostash**] [**--verbose**] [**--json**]

Fetch, inspect, plan, and transactionally integrate an upstream branch. The
engine never invokes **git pull**. Strategies are **auto**, **fast-forward**,
**merge**, **rebase**, **hybreed**, **emulsify**, and **manual**.

**--check** and **--dry-run** may update the selected remote-tracking ref but
do not modify the local branch, index, worktree, or commit history. Use
**--status**, **--continue**, **--abort**, or **--recover** to inspect or resume
a journaled transaction. **--autostash** explicitly permits Kaptaind to save
and restore a dirty worktree; this is never the default.

Examples:

    kaptaind pull --check
    kaptaind pull --dry-run --json
    kaptaind pull --strategy rebase
    kaptaind pull --abort

## push

**kaptaind push** [**--remote** *NAME*] [**--branch** *NAME*] [**--dry-run**]
[**--force**] [**--verbose**] [**--json**]

Push the local branch to a configured remote on demand, using the same
safety machinery (protected-branch checks, pre-push hooks, retry/backoff) as
the daemon's automatic post-commit push.

**--remote**=*NAME*
:   Push to this remote instead of the configured default.

**--branch**=*NAME*
:   Push this branch instead of the current one.

**--dry-run**
:   Pass **--dry-run** to git; nothing is actually pushed.

**--force**
:   Bypass the configured **protect_branches** list for this invocation only.

**-v**, **--verbose**
:   Print additional detail.

**--json**
:   Emit a machine-readable JSON summary.

Requires **[push] enabled = true** and **[capabilities] network_push = true**
in *kaptaind.toml* — without both, this command refuses to run rather than
silently doing nothing.

Examples:

    kaptaind push
    kaptaind push --remote upstream --branch main
    kaptaind push --dry-run

## dashboard

**kaptaind dashboard**

Launch a live terminal dashboard showing daemon status, stability score, release history, recent analyses, and telemetry.

Example:

    kaptaind dashboard

## ci-hint

**kaptaind ci-hint** [**--format** *FORMAT*]

Emit a release/hold recommendation for CI/CD pipelines based on stability, pass streak, diff-spike guard, and cooldown.

**--format**=*FORMAT*
:   Output format: **text** (default), **json**, or **github**.

Example:

    kaptaind ci-hint --format json

## aoc

Manage Aim-of-Change sessions that group related commits under a named intent.

### aoc start

**kaptaind aoc start** *LABEL*

Start a new AoC session. All subsequent commits are tagged with the session until shipped or ended.

Example:

    kaptaind aoc start "feature: authentication flow"

### aoc status

**kaptaind aoc status**

Show the active session name, commit count, and timeline.

Example:

    kaptaind aoc status

### aoc ship

**kaptaind aoc ship**

Finalize and archive the active AoC session, producing a manifest with commits, version progression, and test summary.

Example:

    kaptaind aoc ship

### aoc intercept

**kaptaind aoc intercept** [**--model** *MODEL*] [**--intent** *INTENT*] **--** *COMMAND* [*ARGS*...]

Wrap a command and capture its output, exit code, and timing, attaching the trace to the active AoC session.

**-m**, **--model**=*MODEL*
:   Agent or LLM model name.

**-i**, **--intent**=*INTENT*
:   Intent or task description for the trace.

Example:

    kaptaind aoc intercept --model claude-3-5-sonnet --intent "refactor auth" -- cargo test

### aoc log

**kaptaind aoc log** [**-l** *N* | **--limit** *N*]

List completed and shipped AoC sessions.

**-l**, **--limit**=*N*
:   Number of sessions to display. Default: 10.

Example:

    kaptaind aoc log --limit 50

## init

**kaptaind init**

Initialize **kaptaind.toml** and **.kaptainignore** for the current project. Auto-detects project type (Rust, Node.js, Python, Go, etc.) and sets sensible test/build hooks.

Example:

    kaptaind init

## trawl

**kaptaind trawl** [**--path** *PATH*] [**--max-depth** *N*] [**--include-existing**] [**--require-git**] [**--type** *TYPES*] [**--format** *FORMAT*] [**--dry-run**] [**--blacklist** *GLOBS*] [**--no-ignore**] [**--follow-links**] [**--expand-workspaces**]

Recursively discover and auto-initialize codebases.

**-p**, **--path**=*PATH*
:   Root directory to scan. Default: current directory.

**-d**, **--max-depth**=*N*
:   Maximum recursion depth.

**-i**, **--include-existing**
:   Re-initialize projects that already have **kaptaind.toml**.

**-g**, **--require-git**
:   Only process git repositories.

**-t**, **--type**=*TYPES*
:   Comma-separated project-type filter, e.g. **rust,go,python**.

**-f**, **--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

**--dry-run**
:   Discover projects without initializing them.

**--blacklist**=*GLOBS*
:   Comma-separated directory names or globs to skip (e.g. **scratch,vendor/\***),
    layered on top of the built-in skip list and any **.gitignore**/**.ignore** files.

**--no-ignore**
:   Do not honor **.gitignore**/**.ignore** files; surface projects inside ignored dirs.

**--follow-links**
:   Follow symbolic links while walking (default: off).

**--expand-workspaces**
:   Also initialize Cargo workspace member crates with their own **kaptaind.toml**.
    Members are always *reported*; this only controls initialization.

Discovery is **root-down** and **ignore-aware**: **.gitignore**/**.ignore** files are
honored, the outermost valid project wins, and Cargo workspaces report their member
crates. A directory only counts as a Rust project when its **Cargo.toml** parses and
contains a **[package]** and/or **[workspace]** table, so stray or empty manifests are
ignored.

Example:

    kaptaind trawl --path ~/projects --type rust,go --dry-run

## trace

View and manage per-cluster traces.

### trace log

**kaptaind trace log** [**--aoc-id** *ID*] [**-l** *N* | **--limit** *N*]

List traces for the active or specified AoC session.

**--aoc-id**=*ID*
:   Filter by AoC session ID.

**-l**, **--limit**=*N*
:   Number of traces to display. Default: 10.

Example:

    kaptaind trace log --limit 20

### trace show

**kaptaind trace show** *CLUSTER_ID*

Show detailed breakdown of a specific trace/cluster.

Example:

    kaptaind trace show 018f3a42-...

### trace prune

**kaptaind trace prune** [**-d** *DAYS* | **--days** *DAYS*]

Remove traces older than *DAYS* days.

**-d**, **--days**=*DAYS*
:   Retention period in days. Default: 30.

Example:

    kaptaind trace prune --days 7

## vacs

Visual Asset Channel Saturation — inspect change-driven visual assets.

### vacs show

**kaptaind vacs show** [*COMMIT_OR_CONCEPT*]

Show generated visual assets, optionally filtered by commit or concept ID.

Example:

    kaptaind vacs show

### vacs generate

**kaptaind vacs generate** [**--asset-type** *TYPE*]

Manually trigger generation of a visual asset.

**--asset-type**=*TYPE*
:   Asset type to generate. Default: **diagram**.

Example:

    kaptaind vacs generate --asset-type diagram

## storage

Manage build artifacts and caches via the deckhand integration.

### storage clean

**kaptaind storage clean** [**--profile** *PROFILE*] [**--dry-run**] [**--older-than** *DAYS*]

Run a workspace clean.

**--profile**=*PROFILE*
:   Profile to clean: **debug**, **release**, or **all**. Default: **all**.

**--dry-run**
:   Only print what would be removed.

**--older-than**=*DAYS*
:   Only remove artifacts older than *DAYS* days.

Example:

    kaptaind storage clean --profile release --dry-run

### storage sweep

**kaptaind storage sweep** [**--keep-days** *DAYS*] [**--dry-run**]

Sweep stale caches and artifacts.

**--keep-days**=*DAYS*
:   Keep registry cache entries newer than *DAYS* days. Default: 30.

**--dry-run**
:   Only print what would be removed.

Example:

    kaptaind storage sweep --keep-days 14

### storage status

**kaptaind storage status** [**--json**] [**--limit** *N*]

Report workspace storage state and disk usage.

**--json**
:   Output JSON instead of text.

**-l**, **--limit**=*N*
:   Show only the top *N* largest artifacts.

Example:

    kaptaind storage status --limit 10

## shark

Shark Stating — high-availability leader election and zero-downtime upgrades.

### shark status

**kaptaind shark status** [**--json**]

Show current HA role and lease state.

**--json**
:   Output JSON instead of text.

Example:

    kaptaind shark status

### shark observe

**kaptaind shark observe** [**--interval-ms** *MS*]

Watch leadership changes in real time.

**--interval-ms**=*MS*
:   Poll interval in milliseconds. Default: 1000.

Example:

    kaptaind shark observe --interval-ms 500

### shark release

**kaptaind shark release**

Gracefully release leadership.

Example:

    kaptaind shark release

### shark upgrade

**kaptaind shark upgrade** [**--binary** *PATH*] [**--standby-health-port** *PORT*] [**--ready-timeout-ms** *MS*]

Perform a zero-downtime upgrade to a new kaptaind binary.

**-b**, **--binary**=*PATH*
:   Path to the new kaptaind binary.

**-s**, **--standby-health-port**=*PORT*
:   Temporary health port for the standby instance.

**-r**, **--ready-timeout-ms**=*MS*
:   How long to wait for the standby to become healthy before retiring. Default: 30000.

Example:

    kaptaind shark upgrade --binary target/release/kaptaind --standby-health-port 9090

## ship

Build release binaries, installers, and distribute to channels.

### ship plan

**kaptaind ship plan** [**--targets** *TARGETS*] [**--channels** *CHANNELS*] [**--format** *FORMAT*]

Preview the ship plan without building or publishing.

**-t**, **--targets**=*TARGETS*
:   Comma-separated target triples.

**-c**, **--channels**=*CHANNELS*
:   Comma-separated channels, e.g. **binaries,shell-installer,tauri,homebrew,github-releases**.

**--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

Example:

    kaptaind ship plan --format json

### ship run

**kaptaind ship run** [**--targets** *TARGETS*] [**--channels** *CHANNELS*] [**--force**] [**--format** *FORMAT*]

Execute the ship pipeline.

**-t**, **--targets**=*TARGETS*
:   Comma-separated target triples.

**-c**, **--channels**=*CHANNELS*
:   Comma-separated channels.

**-f**, **--force**
:   Skip qualification gates.

**--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

Example:

    kaptaind ship run --force

### ship stable

**kaptaind ship stable** [**--targets** *TARGETS*] [**--channels** *CHANNELS*] [**--dry-run**] [**--force**] [**--format** *FORMAT*]

Ship a stable release from the current **VERSION**.

Example:

    kaptaind ship stable --dry-run

### ship nightly

**kaptaind ship nightly** [**--targets** *TARGETS*] [**--channels** *CHANNELS*] [**--dry-run**] [**--no-force**] [**--format** *FORMAT*]

Ship a nightly prerelease with an auto-generated version.

**--no-force**
:   Enforce qualification gates (nightly skips them by default).

Example:

    kaptaind ship nightly --no-force

### ship status

**kaptaind ship status** [**--format** *FORMAT*] [**--auto**]

Show the last ship run and scheduled auto-releases.

**--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

**--auto**
:   Include next scheduled auto-nightly and auto-stable fire times.

Example:

    kaptaind ship status --auto

## enable-autostart

**kaptaind enable-autostart**

Deprecated. Use **kaptaind service install --user** instead.

## disable-autostart

**kaptaind disable-autostart**

Deprecated. Use **kaptaind service uninstall --user** instead.

## autostart

**kaptaind autostart**

Launch all enabled kaptaind daemons from the monitor registry. Used internally by the auto-start system; equivalent to **kaptaind monitor resume**.

Example:

    kaptaind autostart

## monitor

Manage the project monitor registry.

### monitor add

**kaptaind monitor add** [*PATH*] [**-c** *CONFIG*] [**-p** *PORT*] [**--enabled** *BOOL*]

Register a project for monitoring. *PATH* defaults to the current directory. If
no config is supplied, **kaptaind.toml** in the project root is assumed. If no
port is supplied, the next free health port starting at 3000 is assigned.

**-c**, **--config**=*CONFIG*
:   Absolute or relative path to **kaptaind.toml**.

**-p**, **--port**=*PORT*
:   Health server port for this project.

**--enabled**=*BOOL*
:   Enable or disable monitoring for this project. Default: **true**.

Example:

    kaptaind monitor add ~/projects/my-app --port 3001

### monitor remove

**kaptaind monitor remove** *PATH*

Remove a project from the monitor registry.

Example:

    kaptaind monitor remove ~/projects/my-app

### monitor list

**kaptaind monitor list**

List all registered projects with path, config, enabled state, health port, and
last active timestamp.

Example:

    kaptaind monitor list

### monitor enable / disable

**kaptaind monitor enable** *PATH*

**kaptaind monitor disable** *PATH*

Enable or disable a registered project. Disabled projects are skipped by
**monitor resume**.

Example:

    kaptaind monitor disable ~/projects/my-app

### monitor resume

**kaptaind monitor resume**

Start a daemon for every enabled project that is not already running. A project
is considered running when its **.kaptaind/daemon.pid** file points to a live
process. Each daemon is spawned with its stored config and health port.

Example:

    kaptaind monitor resume

## service

Install, uninstall, or inspect the user/system service that resumes monitored
projects on login or boot.

### service install

**kaptaind service install** (**--user** | **--system**)

Install a systemd user service (Linux), LaunchAgent (macOS), or shell autostart
fallback that runs **kaptaind monitor resume**. The system variant writes to
**/etc/systemd/system/kaptaind.service** and requires root.

**--user**
:   Install for the current user.

**--system**
:   Install system-wide (requires root).

Examples:

    kaptaind service install --user
    sudo kaptaind service install --system

### service uninstall

**kaptaind service uninstall** (**--user** | **--system**)

Remove the installed service.

Examples:

    kaptaind service uninstall --user
    sudo kaptaind service uninstall --system

### service install-icon

**kaptaind service install-icon** (**--user** | **--system**)

Install the kaptaind logo into the Freedesktop icon theme so notifications and
desktop launchers can display it by name. The user variant writes to
**~/.local/share/icons/hicolor/256x256/apps/kaptaind.png**; the system variant
writes to **/usr/share/icons/hicolor/256x256/apps/kaptaind.png** and requires
root.

The logo is also embedded in the binary and automatically extracted to the
user cache for native notifications, so this command is only needed if you
want the icon available system-wide.

**--user**
:   Install for the current user.

**--system**
:   Install system-wide (requires root).

Examples:

    kaptaind service install-icon --user
    sudo kaptaind service install-icon --system

### service status

**kaptaind service status** (**--user** | **--system**)

Report whether the service file is installed and enabled.

Examples:

    kaptaind service status --user

# FILES

*./kaptaind.toml*
:   Main configuration file for the watched repository. Created automatically by **kaptaind init**.

*.kaptainignore*
:   Per-repository ignore rules. Blank lines and **#** comments are ignored. Entries containing glob metacharacters are treated as glob patterns; otherwise they are treated as exact relative paths or prefixes.

*.kaptaind/*
:   Runtime directory for analysis artifacts, status, telemetry, traces, Aim-of-Change manifests, bundle metadata, and daemon logs/PID files.

*VERSION*
:   Authoritative semantic version for the repository. Created automatically when missing, starting from **0.1.0**.

*~/.config/kaptaind/monitored.json*
:   JSON registry of monitored projects for **monitor resume** and auto-start.

# ENVIRONMENT

**KAPTAIND_CONFIG**
:   Path to the configuration file. Equivalent to **--config**. If unset, **kaptaind** looks for **./kaptaind.toml**.

**RUST_LOG**
:   Set the tracing log level, e.g. **debug**, **info**, **warn**, or **error**. Default is **info**.

# EXIT STATUS

**0**
:   Success.

**1**
:   General error, such as a missing configuration file, invalid config, permission failure, or command/startup failure.

Pull-specific stable exit statuses are **2** invalid invocation, **3** unsafe
repository state, **4** remote unavailable, **5** authentication/authorization
failure, **6** conflicts require intervention, **7** verification failure,
**8** rollback failure, **9** operation already in progress, and **10** policy
denied operation.

# EXAMPLES

Run the daemon in the foreground for interactive development:

    kaptaind

Run the daemon in the background:

    kaptaind --daemon

Use a specific configuration file:

    kaptaind --config /path/to/kaptaind.toml

Inspect the currently watched project:

    kaptaind --dock

Check operational load before an upgrade:

    kaptaind --lanes

Start the daemon with the WebUI dashboard:

    kaptaind --web --web-port 8080 --health-port 9090

Check daemon health:

    kaptaind status

Run a dry-run analysis:

    kaptaind analyze

Start a feature AoC session:

    kaptaind aoc start "feature: payment webhooks"

Push the current branch on demand:

    kaptaind push --dry-run

Preview a release before shipping:

    kaptaind ship plan

Clean release artifacts:

    kaptaind storage clean --profile release

Operate on a different repository:

    kaptaind --repo /path/to/repo status

# SEE ALSO

**git**(1)
