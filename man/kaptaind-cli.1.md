% KAPTAIND-CLI(1) kaptaind 10.3.3
% Elci Group
% September 2026

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

## branch

**kaptaind-cli branch** *SUBCOMMAND*

Govern the repository's typed branch lifecycle.

### branch status

**kaptaind-cli branch status** [**--json**] [**--platform** *desktop*|*mobile*]

Report branch topology, versions, revisions, divergence, and promotion readiness.

### branch init

**kaptaind-cli branch init** [**--dry-run**] [**--json**]

Create missing mandatory lifecycle branches without overwriting existing refs.

### branch sync

**kaptaind-cli branch sync** [**--json**]

Diagnose missing or unexpectedly divergent lifecycle branches.

### branch promote

**kaptaind-cli branch promote** *SOURCE* *TARGET* [**--dry-run**]

Perform a permitted, clean, fast-forward lifecycle transition from *SOURCE* to *TARGET*.

Examples:

    kaptaind-cli branch status --json
    kaptaind-cli branch init --dry-run
    kaptaind-cli branch promote integration release/1.0

## integrate

**kaptaind-cli integrate analyze** *TARGET* *SOURCE* [**--json**] [**--no-persist**]

Analyse a proposed branch integration with Hybreed and Emulsify. Runs both tools and persists an advisory, machine-readable report.

*TARGET*
:   Host/target branch or ref.

*SOURCE*
:   Proposed source/fork branch or ref.

**--json**
:   Emit JSON instead of the concise summary.

**--no-persist**
:   Do not write the report or audit event.

Example:

    kaptaind-cli integrate analyze main feature/api

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

## explain

**kaptaind-cli explain** [**--last** *N*]

Show the last *N* cluster decisions recorded in *.kaptaind/decisions.jsonl* — commits and skips alike. Skip decisions name the exact threshold that was not met and the achieved score.

**--last**=*N*
:   Number of decisions to display. Default: 10.

Examples:

    kaptaind-cli explain
    kaptaind-cli explain --last 25

## rollback

**kaptaind-cli rollback** [*COMMIT*] [**-n** | **--dry-run**] [**-y** | **--yes**]

Safely undo the most recent automated commit (or a specific one) by creating a **git revert** commit. Never rewrites history. Targets commits whose subject starts with the daemon's **kaptaind:** prefix.

*COMMIT*
:   Specific commit to revert. Default: the latest kaptaind commit.

**-n**, **--dry-run**
:   Print the target and the equivalent git command only.

**-y**, **--yes**
:   Execute the revert. Omit to preview.

Examples:

    kaptaind-cli rollback
    kaptaind-cli rollback --yes
    kaptaind-cli rollback abc1234 --yes

If the revert conflicts, resolve it or run **git revert --abort** and retry.

## pull

**kaptaind-cli pull** [**--remote** *REMOTE*] [**--branch** *BRANCH*]
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

    kaptaind-cli pull --check
    kaptaind-cli pull --dry-run --json
    kaptaind-cli pull --strategy rebase
    kaptaind-cli pull --abort

## release

**kaptaind-cli release** *SUBCOMMAND*

Prepare, validate, issue, or roll back governed releases.

### release prepare

**kaptaind-cli release prepare** *VERSION* [**--source** *BRANCH*] [**--dry-run**] [**--json**]

Create an immutable-identity release candidate from the integration branch.

**--source**=*BRANCH*
:   Source branch or ref. Default: **integration**.

### release validate

**kaptaind-cli release validate** *VERSION* [**--json**]

Run the configured build/test and consistency gates for a candidate.

### release issue

**kaptaind-cli release issue** *VERSION* [**--platform** *desktop*|*mobile*] [**--dry-run**] [**--json**]

Atomically advance production and create the **v*VERSION*** tag after validation.

### release rollback

**kaptaind-cli release rollback** *VERSION* **--as** *NEW_VERSION* [**--platform** *desktop*|*mobile*] [**--dry-run**] [**--json**]

Issue a new release whose tree restores an older released version.

Examples:

    kaptaind-cli release prepare 10.4.0 --dry-run
    kaptaind-cli release validate 10.4.0
    kaptaind-cli release issue 10.4.0
    kaptaind-cli release rollback 10.2.0 --as 10.3.4

## checkout

**kaptaind-cli checkout** (**stable** | **bleeding**) [**--platform** *desktop*|*mobile*] [**--dry-run**]

Resolve and check out a consumer channel.

*CHANNEL*
:   **stable** or **bleeding**.

**--platform**=*PLATFORM*
:   **desktop** (default) or **mobile**.

**--dry-run**
:   Print the resolution without checking out.

Example:

    kaptaind-cli checkout stable --platform desktop

## suspend

**kaptaind-cli suspend** [**--reason** *TEXT*]

Temporarily suspend the daemon so it will not automatically process clusters, run tests, or create commits. Writes *.kaptaind/suspend.json* and updates *.kaptaind/status.json* to Suspended; the daemon checks this gate at the start of every cluster.

**--reason**=*TEXT*
:   Optional human-readable reason.

Examples:

    kaptaind-cli suspend
    kaptaind-cli suspend --reason "manual hold"

## resume

**kaptaind-cli resume**

Resume a daemon suspended via **kaptaind-cli suspend** or an Aim-of-Change session. Removes *.kaptaind/suspend.json* and sets *.kaptaind/status.json* to Idle.

Example:

    kaptaind-cli resume

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

## doctor

**kaptaind-cli doctor** [**-f** *FORMAT*]

Capture the host's hardware/OS profile, check inotify watch limits against the repo-size tier table, verify tool availability, and recommend a tier (T0–T4). Writes a machine-readable artifact to *.kaptaind/doctor/*; the JSON artifact includes the git revision and dirty flag and feeds the **report** qualification bundle.

**-f**, **--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

Examples:

    kaptaind-cli doctor
    kaptaind-cli doctor --format json

## stress

**kaptaind-cli stress run** [OPTIONS]

Generate a reproducible synthetic repo into a temp dir and run the real cluster → diff → weight → version pipeline (no commit, no daemon) over *N* change batches, asserting the version never decreases. Writes *.kaptaind/stress/<run-id>.json* with per-stage latency and the bump distribution.

**--files**=*N*
:   Number of synthetic source files. Default: 50.

**--batches**=*N*
:   Number of change batches. Default: 5.

**--seed**=*N*
:   Deterministic RNG seed. Default: 1.

**--langs**=*LIST*
:   Comma-separated languages. Default: **rust,ts,py,go**.

**-f**, **--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

Examples:

    kaptaind-cli stress run --files 100 --batches 10
    kaptaind-cli stress run --files 20 --batches 3 --format json

## report

**kaptaind-cli report** [OPTIONS]

Aggregate the latest doctor/bench/stress artifacts plus optional external logs into a **kaptaind.qualification.v1** JSON and a human markdown report. A section is PASS only with real evidence; missing evidence is PASS-WITH-NOTES ("not run in-session"); any FAIL marker makes it FAIL.

**-v**, **--version**=*V*
:   Version to report. Default: read **VERSION**.

**-o**, **--out**=*DIR*
:   Output directory. Default: *.kaptaind/report*.

**--cargo-test**=*PATH*
:   Text log whose last line carries **TEST_EXIT=<n>**.

**--clippy**=*PATH*
:   Text log whose last line carries **CLIPPY_EXIT=<n>**.

**--deny**=*PATH*
:   Text log whose last line carries **DENY_EXIT=<n>**.

**--container**=*PATH*
:   Text log whose last line carries **CONTAINER_EXIT=<n>**.

**-f**, **--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

Examples:

    kaptaind-cli report --version 10.3.3 --format json
    kaptaind-cli report --cargo-test target/test.log --clippy target/clippy.log

## logs

**kaptaind-cli logs** *SUBCOMMAND*

Tail, filter errors, or grep the daemon's text logs (*.kaptaind/daemon.out*, *.kaptaind/daemon.err*).

### logs tail

**kaptaind-cli logs tail** [**-n** *N*] [**-f** *FORMAT*]

Show the last *N* log lines. Default: 50.

### logs errors

**kaptaind-cli logs errors** [**-f** *FORMAT*]

Show ERROR/WARN log lines.

### logs grep

**kaptaind-cli logs grep** *REGEX* [**-f** *FORMAT*]

Filter log lines by a regular expression.

Examples:

    kaptaind-cli logs tail -n 50
    kaptaind-cli logs errors
    kaptaind-cli logs grep "commit" --format json

## audit

**kaptaind-cli audit** *SUBCOMMAND*

Tail, summarize, or verify the append-only compliance audit trail (*.kaptaind/audit.jsonl*). **verify** checks timestamp ordering and (when present) the per-entry **prev_hash** chain.

### audit tail

**kaptaind-cli audit tail** [**-n** *N*] [**-f** *FORMAT*]

Show the last *N* audit entries. Default: 50.

### audit stats

**kaptaind-cli audit stats** [**-f** *FORMAT*]

Summarize counts by event_type/result and the failure rate.

### audit verify

**kaptaind-cli audit verify** [**-f** *FORMAT*]

Verify append-only ordering and the optional hash chain.

### audit export-verify

**kaptaind-cli audit export-verify** [**-f** *FORMAT*]

Verify the configured audit-export mirror against the local chain.

Examples:

    kaptaind-cli audit tail -n 20
    kaptaind-cli audit stats
    kaptaind-cli audit verify
    kaptaind-cli audit export-verify

## evidence

**kaptaind-cli evidence** *SUBCOMMAND*

Record hashed CI, scanner, ITSM, or domain evidence for a release.

### evidence record

**kaptaind-cli evidence record** **--version** *V* **--kind** *K* **--source** *S* **--file** *PATH*

Record a local exported artifact as release evidence.

### evidence attach-snapshot

**kaptaind-cli evidence attach-snapshot** **--version** *V* **--file** *PATH*

Validate and record a **bound-snapshot/v1** artifact as release evidence.

## governance

**kaptaind-cli governance** [**-f** *FORMAT*]

Assess enforced enterprise governance controls.

**-f**, **--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

## integrations

**kaptaind-cli integrations** [**-f** *FORMAT*]

List the governed enterprise connector catalogue and active configuration.

**-f**, **--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

## environment

**kaptaind-cli environment** *SUBCOMMAND*

Observe environment lifecycle evidence: rollout, health, rollback, and drift records. Never performs deployments — **record**, **promote**, and **rollback** only record externally performed events.

### environment status

**kaptaind-cli environment status** [**-f** *FORMAT*]

Show the latest known release fact for each environment.

### environment risk

**kaptaind-cli environment risk** [**-f** *FORMAT*]

Explain risk from recorded rollout, health, rollback, and drift evidence.

### environment history

**kaptaind-cli environment history** *ENVIRONMENT* [**-f** *FORMAT*]

Show immutable lifecycle records for one environment.

### environment diff

**kaptaind-cli environment diff** *FROM* *TO* [**-f** *FORMAT*]

Compare the latest recorded version and configuration digest between two environments.

### environment record

**kaptaind-cli environment record** *ENVIRONMENT* **--version** *V* [**--health** *H*] [**--rollout-percent** *N*] [**--config-sha256** *S*] [**--note** *TEXT*]

Record an externally performed deployment or health observation.

### environment promote

**kaptaind-cli environment promote** *FROM* *TO* **--version** *V* [**--adr** *A*]

Record a promotion request; deployment remains external.

### environment rollback

**kaptaind-cli environment rollback** *ENVIRONMENT* **--version** *V* [**--adr** *A*]

Record a rollback decision; deployment remains external.

Examples:

    kaptaind-cli environment status
    kaptaind-cli environment record staging --version 10.3.3 --health pass
    kaptaind-cli environment promote staging production --version 10.3.3

## probe

**kaptaind-cli probe** *SUBCOMMAND*

Scrape the daemon's HTTP endpoints without hand-curling: **/health**, **/metrics**, **/metrics/prometheus**, and **/events** (SSE). Uses a minimal HTTP/1.1 client; if the daemon is not running, prints a clear message. Reads the health port from config (default 9090).

### probe health

**kaptaind-cli probe health** [**-f** *FORMAT*]

GET **/health**.

### probe metrics

**kaptaind-cli probe metrics** [**--prometheus**] [**-f** *FORMAT*]

GET **/metrics** (JSON) or, with **--prometheus**, the Prometheus text exposition endpoint.

### probe events

**kaptaind-cli probe events** [**--follow**] [**-f** *FORMAT*]

GET **/events**, optionally following the SSE stream until interrupted.

Examples:

    kaptaind-cli probe health
    kaptaind-cli probe metrics --prometheus
    kaptaind-cli probe events --follow

## migrate

**kaptaind-cli migrate** [OPTIONS]

Deterministically migrate the repository's *.kaptaind/state.toml* semantic-state document to a newer (or older) schema version, one discrete step at a time. Normal analysis never rewrites the document — **migrate** is the only mutation path, and every run is recorded in *.kaptaind/migrations/*.

**--check**
:   Report whether migration is needed (no changes).

**--strict**
:   With **--check**: exit non-zero when the document is outdated (CI).

**--to**=*VERSION*
:   Target schema version. Default: latest supported.

**--allow-lossy**
:   Permit migrations that discard information.

**-f**, **--format**=*FORMAT*
:   Output format: **text** (default) or **json**.

Examples:

    kaptaind-cli migrate
    kaptaind-cli migrate --check --strict
    kaptaind-cli migrate --to 2.0 --allow-lossy
    kaptaind-cli migrate --check --format json

## schema

**kaptaind-cli schema** *SUBCOMMAND*

Show which *.kaptaind* schema versions this kaptaind knows about.

### schema list

**kaptaind-cli schema list**

List installed schema versions.

### schema explain

**kaptaind-cli schema explain** *VERSION*

Describe a schema version (e.g. **2.1**).

Examples:

    kaptaind-cli schema list
    kaptaind-cli schema explain 2.1

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

Pull-specific stable exit statuses are **2** invalid invocation, **3** unsafe
repository state, **4** remote unavailable, **5** authentication/authorization
failure, **6** conflicts require intervention, **7** verification failure,
**8** rollback failure, **9** operation already in progress, and **10** policy
denied operation.

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
