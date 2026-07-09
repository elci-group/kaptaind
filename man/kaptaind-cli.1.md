% KAPTAIND-CLI(1) kaptaind 9.6.3
% Elci Group
% July 2026

# NAME

**kaptaind-cli** — command-line companion to the kaptaind daemon

# SYNOPSIS

**kaptaind-cli** [**--repo**=*PATH*] *COMMAND* [*ARGS*]

# DESCRIPTION

**kaptaind-cli** is the shore-side companion to **kaptaind**(1). It provides visibility into the daemon's state, runs dry-run analyses, manages Aim-of-Change sessions, handles release qualification and shipping, and performs storage and high-availability tasks — all without requiring the daemon to be running.

Most commands read **./kaptaind.toml** to determine repository paths and settings. Use **--repo** to override the repository path for a single invocation.

# GLOBAL OPTIONS

**-r**, **--repo**=*PATH*
:   Operate on *PATH* instead of the repository configured in **kaptaind.toml**.

**-V**, **--version**
:   Print the version and exit.

**-h**, **--help**
:   Print help information and exit.

# COMMANDS

## status

**kaptaind-cli status**

Show current daemon health, version, and recent errors. Reports the daemon state (Idle, Clustering, Testing, Committing, Failed), installed binary locations, and current version.

Example:

    kaptaind-cli status

## validate

**kaptaind-cli validate**

Validate **kaptaind.toml** and report cross-field configuration errors, such as timeout constraints and Shark TTL consistency. Exits non-zero if validation fails.

Example:

    kaptaind-cli validate

## log

**kaptaind-cli log** [**-l** *N* | **--limit** *N*]

View recent automated commits and analysis decisions.

**-l**, **--limit**=*N*
:   Number of commits to display. Default: 10.

Example:

    kaptaind-cli log --limit 20

## analyze

**kaptaind-cli analyze**

Dry-run semantic diff analysis on the current working tree without committing. Shows score breakdown, detected API/dependency/runtime changes, and projected version bump.

Example:

    kaptaind-cli analyze

## dashboard

**kaptaind-cli dashboard**

Launch a live terminal dashboard showing daemon status, stability score, release history, recent analyses, and telemetry.

Example:

    kaptaind-cli dashboard

## ci-hint

**kaptaind-cli ci-hint** [**--format** *FORMAT*]

Emit a release/hold recommendation for CI/CD pipelines based on stability, pass streak, diff-spike guard, and cooldown.

**--format**=*FORMAT*
:   Output format: **text** (default), **json**, or **github**.

Example:

    kaptaind-cli ci-hint --format json

## aoc

Manage Aim-of-Change sessions that group related commits under a named intent.

### aoc start

**kaptaind-cli aoc start** *LABEL*

Start a new AoC session. All subsequent commits are tagged with the session until shipped or ended.

Example:

    kaptaind-cli aoc start "feature: authentication flow"

### aoc status

**kaptaind-cli aoc status**

Show the active session name, commit count, and timeline.

Example:

    kaptaind-cli aoc status

### aoc ship

**kaptaind-cli aoc ship**

Finalize and archive the active AoC session, producing a manifest with commits, version progression, and test summary.

Example:

    kaptaind-cli aoc ship

### aoc intercept

**kaptaind-cli aoc intercept** [**--model** *MODEL*] [**--intent** *INTENT*] **--** *COMMAND* [*ARGS*...]

Wrap a command and capture its output, exit code, and timing, attaching the trace to the active AoC session.

**-m**, **--model**=*MODEL*
:   Agent or LLM model name.

**-i**, **--intent**=*INTENT*
:   Intent or task description for the trace.

Example:

    kaptaind-cli aoc intercept --model claude-3-5-sonnet --intent "refactor auth" -- cargo test

### aoc log

**kaptaind-cli aoc log** [**-l** *N* | **--limit** *N*]

List completed and shipped AoC sessions.

**-l**, **--limit**=*N*
:   Number of sessions to display. Default: 10.

Example:

    kaptaind-cli aoc log --limit 50

## init

**kaptaind-cli init**

Initialize **kaptaind.toml** and **.kaptainignore** for the current project. Auto-detects project type (Rust, Node.js, Python, Go, etc.) and sets sensible test/build hooks.

Example:

    kaptaind-cli init

## trawl

**kaptaind-cli trawl** [**--path** *PATH*] [**--max-depth** *N*] [**--include-existing**] [**--require-git**] [**--type** *TYPES*] [**--format** *FORMAT*] [**--dry-run**] [**--blacklist** *GLOBS*] [**--no-ignore**] [**--follow-links**] [**--expand-workspaces**]

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

    kaptaind-cli trawl --path ~/projects --type rust,go --dry-run

## trace

View and manage per-cluster traces.

### trace log

**kaptaind-cli trace log** [**--aoc-id** *ID*] [**-l** *N* | **--limit** *N*]

List traces for the active or specified AoC session.

**--aoc-id**=*ID*
:   Filter by AoC session ID.

**-l**, **--limit**=*N*
:   Number of traces to display. Default: 10.

Example:

    kaptaind-cli trace log --limit 20

### trace show

**kaptaind-cli trace show** *CLUSTER_ID*

Show detailed breakdown of a specific trace/cluster.

Example:

    kaptaind-cli trace show 018f3a42-...

### trace prune

**kaptaind-cli trace prune** [**-d** *DAYS* | **--days** *DAYS*]

Remove traces older than *DAYS* days.

**-d**, **--days**=*DAYS*
:   Retention period in days. Default: 30.

Example:

    kaptaind-cli trace prune --days 7

## vacs

Visual Asset Channel Saturation — inspect change-driven visual assets.

### vacs show

**kaptaind-cli vacs show** [*COMMIT_OR_CONCEPT*]

Show generated visual assets, optionally filtered by commit or concept ID.

Example:

    kaptaind-cli vacs show

### vacs generate

**kaptaind-cli vacs generate** [**--asset-type** *TYPE*]

Manually trigger generation of a visual asset.

**--asset-type**=*TYPE*
:   Asset type to generate. Default: **diagram**.

Example:

    kaptaind-cli vacs generate --asset-type diagram

## storage

Manage build artifacts and caches via the deckhand integration.

### storage clean

**kaptaind-cli storage clean** [**--profile** *PROFILE*] [**--dry-run**] [**--older-than** *DAYS*]

Run a workspace clean.

**--profile**=*PROFILE*
:   Profile to clean: **debug**, **release**, or **all**. Default: **all**.

**--dry-run**
:   Only print what would be removed.

**--older-than**=*DAYS*
:   Only remove artifacts older than *DAYS* days.

Example:

    kaptaind-cli storage clean --profile release --dry-run

### storage sweep

**kaptaind-cli storage sweep** [**--keep-days** *DAYS*] [**--dry-run**]

Sweep stale caches and artifacts.

**--keep-days**=*DAYS*
:   Keep registry cache entries newer than *DAYS* days. Default: 30.

**--dry-run**
:   Only print what would be removed.

Example:

    kaptaind-cli storage sweep --keep-days 14

### storage status

**kaptaind-cli storage status** [**--json**] [**--limit** *N*]

Report workspace storage state and disk usage.

**--json**
:   Output JSON instead of text.

**-l**, **--limit**=*N*
:   Show only the top *N* largest artifacts.

Example:

    kaptaind-cli storage status --limit 10

## shark

Shark Stating — high-availability leader election and zero-downtime upgrades.

### shark status

**kaptaind-cli shark status** [**--json**]

Show current HA role and lease state.

**--json**
:   Output JSON instead of text.

Example:

    kaptaind-cli shark status

### shark observe

**kaptaind-cli shark observe** [**--interval-ms** *MS*]

Watch leadership changes in real time.

**--interval-ms**=*MS*
:   Poll interval in milliseconds. Default: 1000.

Example:

    kaptaind-cli shark observe --interval-ms 500

### shark release

**kaptaind-cli shark release**

Gracefully release leadership.

Example:

    kaptaind-cli shark release

### shark upgrade

**kaptaind-cli shark upgrade** [**--binary** *PATH*] [**--standby-health-port** *PORT*] [**--ready-timeout-ms** *MS*]

Perform a zero-downtime upgrade to a new kaptaind binary.

**-b**, **--binary**=*PATH*
:   Path to the new kaptaind binary.

**-s**, **--standby-health-port**=*PORT*
:   Temporary health port for the standby instance.

**-r**, **--ready-timeout-ms**=*MS*
:   How long to wait for the standby to become healthy before retiring. Default: 30000.

Example:

    kaptaind-cli shark upgrade --binary target/release/kaptaind --standby-health-port 9090

## ship

Build release binaries, installers, and distribute to channels.

### ship plan

**kaptaind-cli ship plan** [**--targets** *TARGETS*] [**--channels** *CHANNELS*] [**--format** *FORMAT*]

Preview the ship plan without building or publishing.

**-t**, **--targets**=*TARGETS*
:   Comma-separated target triples.

**-c**, **--channels**=*CHANNELS*
:   Comma-separated channels, e.g. **binaries,shell-installer,tauri,homebrew,github-releases**.

**--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

Example:

    kaptaind-cli ship plan --format json

### ship run

**kaptaind-cli ship run** [**--targets** *TARGETS*] [**--channels** *CHANNELS*] [**--force**] [**--format** *FORMAT*]

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

    kaptaind-cli ship run --force

### ship stable

**kaptaind-cli ship stable** [**--targets** *TARGETS*] [**--channels** *CHANNELS*] [**--dry-run**] [**--force**] [**--format** *FORMAT*]

Ship a stable release from the current **VERSION**.

Example:

    kaptaind-cli ship stable --dry-run

### ship nightly

**kaptaind-cli ship nightly** [**--targets** *TARGETS*] [**--channels** *CHANNELS*] [**--dry-run**] [**--no-force**] [**--format** *FORMAT*]

Ship a nightly prerelease with an auto-generated version.

**--no-force**
:   Enforce qualification gates (nightly skips them by default).

Example:

    kaptaind-cli ship nightly --no-force

### ship status

**kaptaind-cli ship status** [**--format** *FORMAT*] [**--auto**]

Show the last ship run and scheduled auto-releases.

**--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

**--auto**
:   Include next scheduled auto-nightly and auto-stable fire times.

Example:

    kaptaind-cli ship status --auto

## enable-autostart

**kaptaind-cli enable-autostart**

Deprecated. Use **kaptaind-cli service install --user** instead.

## disable-autostart

**kaptaind-cli disable-autostart**

Deprecated. Use **kaptaind-cli service uninstall --user** instead.

## autostart

**kaptaind-cli autostart**

Launch all enabled kaptaind daemons from the monitor registry. Used internally by the auto-start system; equivalent to **kaptaind-cli monitor resume**.

Example:

    kaptaind-cli autostart

## monitor

Manage the project monitor registry.

### monitor add

**kaptaind-cli monitor add** [*PATH*] [**-c** *CONFIG*] [**-p** *PORT*] [**--enabled** *BOOL*]

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

    kaptaind-cli monitor add ~/projects/my-app --port 3001

### monitor remove

**kaptaind-cli monitor remove** *PATH*

Remove a project from the monitor registry.

Example:

    kaptaind-cli monitor remove ~/projects/my-app

### monitor list

**kaptaind-cli monitor list**

List all registered projects with path, config, enabled state, health port, and
last active timestamp.

Example:

    kaptaind-cli monitor list

### monitor enable / disable

**kaptaind-cli monitor enable** *PATH*

**kaptaind-cli monitor disable** *PATH*

Enable or disable a registered project. Disabled projects are skipped by
**monitor resume**.

Example:

    kaptaind-cli monitor disable ~/projects/my-app

### monitor resume

**kaptaind-cli monitor resume**

Start a daemon for every enabled project that is not already running. A project
is considered running when its **.kaptaind/daemon.pid** file points to a live
process. Each daemon is spawned with its stored config and health port.

Example:

    kaptaind-cli monitor resume

## service

Install, uninstall, or inspect the user/system service that resumes monitored
projects on login or boot.

### service install

**kaptaind-cli service install** (**--user** | **--system**)

Install a systemd user service (Linux), LaunchAgent (macOS), or shell autostart
fallback that runs **kaptaind-cli monitor resume**. The system variant writes to
**/etc/systemd/system/kaptaind.service** and requires root.

**--user**
:   Install for the current user.

**--system**
:   Install system-wide (requires root).

Examples:

    kaptaind-cli service install --user
    sudo kaptaind-cli service install --system

### service uninstall

**kaptaind-cli service uninstall** (**--user** | **--system**)

Remove the installed service.

Examples:

    kaptaind-cli service uninstall --user
    sudo kaptaind-cli service uninstall --system

### service install-icon

**kaptaind-cli service install-icon** (**--user** | **--system**)

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

    kaptaind-cli service install-icon --user
    sudo kaptaind-cli service install-icon --system

### service status

**kaptaind-cli service status** (**--user** | **--system**)

Report whether the service file is installed and enabled.

Examples:

    kaptaind-cli service status --user

# FILES

*./kaptaind.toml*
:   Configuration file for the current project.

*.kaptainignore*
:   Per-repository ignore rules.

*.kaptaind/*
:   Runtime directory for analysis artifacts, status, telemetry, traces, and manifests.

*~/.config/kaptaind/monitored.json*
:   JSON registry of monitored projects for **monitor resume** and auto-start.

# ENVIRONMENT

**KAPTAIND_CONFIG**
:   Path to the configuration file. Defaults to **./kaptaind.toml**.

**RUST_LOG**
:   Set the tracing log level: **debug**, **info**, **warn**, or **error**.

# EXIT STATUS

**0**
:   Success.

**1**
:   General error, invalid configuration, permission failure, or command failure.

# EXAMPLES

Check daemon health:

    kaptaind-cli status

Run a dry-run analysis:

    kaptaind-cli analyze

Start a feature AoC session:

    kaptaind-cli aoc start "feature: payment webhooks"

Preview a release before shipping:

    kaptaind-cli ship plan

Clean release artifacts:

    kaptaind-cli storage clean --profile release

Operate on a different repository:

    kaptaind-cli --repo /path/to/repo status

# SEE ALSO

**kaptaind**(1)
