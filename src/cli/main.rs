mod curly_expand;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use kaptaind::util::style::*;
mod analyze;
mod autostart;
mod commands;
mod monitor;
mod table;
use analyze::handle_analyze;
use autostart::{handle_disable_autostart, handle_enable_autostart};
use commands::*;
use kaptaind::config::loader::{self, Config};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "kaptaind-cli",
    version = env!("CARGO_PKG_VERSION"),
    author = "Elci Group <kaptaind@example.com>",
    about = "Kaptaind CLI companion for the automated versioning daemon",
    long_about = r#"Kaptaind CLI companion for the automated versioning daemon.

Kaptaind is an automated semantic-versioning daemon that watches a repository,
clusters filesystem events, analyzes the change set, computes a semantic-version
bump, and commits the result. This CLI provides visibility into the daemon's
state and offers one-off analysis, session management, and release operations
without starting the daemon.

EXAMPLES:
    kaptaind-cli status
    kaptaind-cli log --limit 20
    kaptaind-cli analyze
    kaptaind-cli dashboard
    kaptaind-cli ci-hint --format json
    kaptaind-cli aoc start "feature: auth"
    kaptaind-cli ship plan
    kaptaind-cli init

ENVIRONMENT:
    KAPTAIND_CONFIG      Path to kaptaind.toml (default: ./kaptaind.toml)

CONFIG FILE:
    Location: ./kaptaind.toml or ~/.kaptaind/config.toml
    Generate one with:
        kaptaind-cli init
    Then start the daemon with:
        kaptaind --daemon

REPOSITORY MUTATION:
    A generated profile defaults to observe-only: `kaptaind-cli analyze` and
    the daemon both score changes and record the decision, but nothing is
    staged, committed, VERSION-written, pushed, or shipped. Add to
    kaptaind.toml to allow real commits:
        [operation]
        mode = "actuate"
    Pushing additionally needs [push] enabled = true and
    [capabilities] network_push = true. `kaptaind-cli validate` does not
    currently flag observe-only repos; check `kaptaind-cli explain` or
    `.kaptaind/decisions.jsonl` for `"outcome":"observed"` if commits stop
    appearing. See CHANGELOG.md [10.2.0] and [10.1.4].

DOCUMENTATION:
    https://github.com/elci-group/kaptaind
    https://github.com/elci-group/kaptaind/blob/main/README.md
    https://github.com/elci-group/kaptaind/blob/main/INSTALL.md"#
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Repository path to operate on (overrides kaptaind.toml).
    #[arg(short, long, value_name = "PATH", global = true)]
    repo: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// 🟢 View current daemon health and version
    #[command(long_about = r#"Purpose:
    Show the daemon's current state, installed version, binary locations, and
    any recent error messages.

Usage:
    kaptaind-cli status [OPTIONS]

Options:
    -r, --repo <PATH>    Operate on the specified repository

Examples:
    kaptaind-cli status
    kaptaind-cli status --repo /path/to/project

Notes:
    Reads the daemon PID file and .kaptaind/status.json in the repository."#)]
    Status {
        /// Emit machine-readable lifecycle state.
        #[arg(long)]
        json: bool,
    },

    /// Govern the repository's typed branch lifecycle.
    #[command(subcommand)]
    Branch(BranchCommand),

    /// Analyse a proposed branch integration with Hybreed and Emulsify.
    #[command(subcommand)]
    Integrate(IntegrateCommand),

    /// Safely fetch, inspect, plan, and integrate an upstream branch.
    #[command(long_about = r#"Kaptaind's transactional pull engine.

Remote acquisition is separated from integration: this command never invokes
`git pull`. `--check` and `--dry-run` fetch and analyse remote state without
changing the local branch, index, worktree, or commit history."#)]
    Pull {
        #[arg(long)]
        remote: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long, default_value = "auto", value_parser = ["auto", "fast-forward", "merge", "rebase", "hybreed", "emulsify", "manual"])]
        strategy: String,
        #[arg(long)]
        check: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        autostash: bool,
        #[arg(long)]
        abort: bool,
        #[arg(long)]
        r#continue: bool,
        #[arg(long)]
        status: bool,
        #[arg(long)]
        recover: bool,
        #[arg(long)]
        verbose: bool,
        #[arg(long)]
        json: bool,
    },

    /// Prepare, validate, issue, or roll back governed releases.
    #[command(subcommand)]
    Release(LifecycleReleaseCommand),

    /// Resolve and check out a consumer channel (`stable` or `bleeding`).
    Checkout {
        #[arg(value_parser = ["stable", "bleeding"])]
        channel: String,
        #[arg(long, default_value = "desktop", value_parser = ["desktop", "mobile"])]
        platform: String,
        #[arg(long)]
        dry_run: bool,
    },

    /// ⏸️ Suspend automated daemon commits
    #[command(long_about = r#"Purpose:
    Temporarily suspend the daemon so it will not automatically process
    clusters, run tests, or create commits.

Usage:
    kaptaind-cli suspend [OPTIONS]

Options:
    -r, --repo <PATH>     Operate on the specified repository
        --reason <TEXT>   Optional human-readable reason

Examples:
    kaptaind-cli suspend
    kaptaind-cli suspend --reason "manual hold"

Notes:
    Writes .kaptaind/suspend.json and updates .kaptaind/status.json to
    Suspended. The daemon checks this gate at the start of every cluster."#)]
    Suspend {
        /// Optional reason for the suspension.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,
    },

    /// ▶️ Resume automated daemon commits
    #[command(long_about = r#"Purpose:
    Resume a daemon that was suspended via 'kaptaind-cli suspend' or an
    Aim-of-Change session.

Usage:
    kaptaind-cli resume [OPTIONS]

Options:
    -r, --repo <PATH>    Operate on the specified repository

Examples:
    kaptaind-cli resume

Notes:
    Removes .kaptaind/suspend.json and sets .kaptaind/status.json to Idle."#)]
    Resume,

    /// ✅ Validate kaptaind.toml and report configuration errors
    #[command(long_about = r#"Purpose:
    Perform a post-load validation pass on kaptaind.toml and report cross-field
    constraint violations.

Usage:
    kaptaind-cli config validate [OPTIONS]

Options:
    -r, --repo <PATH>    Operate on the specified repository

Examples:
    kaptaind-cli config validate

Notes:
    Exits with a non-zero status if validation fails."#)]
    Validate,

    /// 📜 View recent automated commits and analysis decisions
    #[command(long_about = r#"Purpose:
    List the most recent automated commits made by kaptaind, including version
    bumps, scores, and the reasons for each bump.

Usage:
    kaptaind-cli log [OPTIONS]

Options:
    -l, --limit <N>      Number of commits to display (default: 10).
    -r, --repo <PATH>    Operate on the specified repository

Examples:
    kaptaind-cli log
    kaptaind-cli log --limit 50
    kaptaind-cli log -l 5"#)]
    Log {
        /// Number of commits to display (default: 10).
        #[arg(short, long, value_name = "N", default_value_t = 10)]
        limit: usize,
    },

    /// 🔬 Analyze working tree without committing
    #[command(long_about = r#"Purpose:
    Run a full semantic diff analysis on the current uncommitted changes without
    actually committing. Shows the projected version bump and the score
    breakdown.

Usage:
    kaptaind-cli analyze [OPTIONS]

Options:
    -r, --repo <PATH>    Operate on the specified repository

Examples:
    kaptaind-cli analyze

Notes:
    Output includes the structural, API, dependency, runtime, and optional
    bundle scores, plus the projected version bump."#)]
    Analyze,

    /// 🧭 Explain recent cluster decisions (commits and skips)
    #[command(long_about = r#"Purpose:
    Show the last N cluster decisions recorded in .kaptaind/decisions.jsonl —
    commits and skips alike. Skip decisions name the exact threshold that was
    not met and the achieved score.

Usage:
    kaptaind-cli explain [OPTIONS]

Options:
    --last <N>           Number of decisions to display (default: 10).
    -r, --repo <PATH>    Operate on the specified repository

Examples:
    kaptaind-cli explain
    kaptaind-cli explain --last 25"#)]
    Explain {
        /// Number of decisions to display (default: 10).
        #[arg(long, value_name = "N", default_value_t = 10)]
        last: usize,
    },

    /// ↩️ Revert the most recent kaptaind-produced commit
    #[command(long_about = r#"Purpose:
    Safely undo the most recent automated commit (or a specific one) by creating
    a git revert commit. Never rewrites history. Targets commits whose subject
    starts with the daemon's `kaptaind:` prefix.

Usage:
    kaptaind-cli rollback [COMMIT] [OPTIONS]

Arguments:
    [COMMIT]    Specific commit to revert (default: latest kaptaind commit).

Options:
    -n, --dry-run    Print the target and the equivalent git command only.
    -y, --yes        Execute the revert (omit to preview).

Examples:
    kaptaind-cli rollback
    kaptaind-cli rollback --yes
    kaptaind-cli rollback abc1234 --yes

Notes:
    Defaults to preview mode unless --yes is passed. If the revert conflicts,
    resolve it or run `git revert --abort` and retry."#)]
    Rollback {
        /// Specific commit to revert (default: latest kaptaind commit).
        #[arg(value_name = "COMMIT")]
        commit: Option<String>,
        /// Preview only; do not modify the repository.
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Execute the revert.
        #[arg(short, long)]
        yes: bool,
    },

    /// 🎯 Manage Aim of Change sessions
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Group related changes into named, intent-driven sessions with full
    traceability. Useful for features, refactors, or coordinated multi-file
    changes.

Usage:
    kaptaind-cli aoc <SUBCOMMAND>

Subcommands:
    start      Start a new AoC session
    status     Show the active session
    ship       End and ship the current session
    cancel     Cancel the current session
    intercept  Wrap a command and capture its trace
    log        List completed sessions

Examples:
    kaptaind-cli aoc start "feature: auth flow"
    kaptaind-cli aoc status
    kaptaind-cli aoc ship
    kaptaind-cli aoc cancel
    kaptaind-cli aoc intercept -- npm test

Notes:
    Session state is stored in .kaptaind/aoc/active.json and archived to
    .kaptaind/aoc/manifests/<id>.json when shipped or cancelled."#
    )]
    Aoc(AocCommand),

    /// ⚙️ Initialize kaptaind config for the current project
    #[command(long_about = r#"Purpose:
    Auto-generate kaptaind.toml and .kaptainignore for the current project based
    on detected project type.

Usage:
    kaptaind-cli init [OPTIONS]

Options:
    -r, --repo <PATH>    Operate on the specified repository

Examples:
    kaptaind-cli init
    kaptaind-cli init --repo /path/to/project

Detected project types:
    Rust       Cargo.toml
    Node       package.json
    Python     pyproject.toml or requirements.txt
    Go         go.mod
    Swift      Package.swift
    Kotlin     build.gradle(.kts)

Notes:
    Existing kaptaind.toml and .kaptainignore files are left untouched."#)]
    Init,

    /// 🎣 Discover and initialize codebases in a directory tree
    #[command(
        long_about = r#"Purpose:
    Recursively scan directories to discover codebases, automatically initialize
    kaptaind for each found project, and optionally register them for
    monitoring.

Usage:
    kaptaind-cli trawl [OPTIONS]

Options:
    -p, --path <PATH>            Root directory to start from (default: current directory).
    -m, --max-depth <DEPTH>      Maximum recursion depth (default: unlimited).
    -i, --include-existing       Re-initialize projects that already have kaptaind.toml.
    -g, --require-git            Only process git repositories.
        --no-register            Do not register discovered projects for autostart.
    -t, --type <TYPES>           Filter by project types (comma-separated).
    -f, --format <FORMAT>        Output format: text (default) or json.
        --dry-run                Discover but do not initialize anything.
        --blacklist <GLOBS>      Extra dir names or globs to skip (comma-separated),
                                 layered on the built-in skip list and ignore files.
        --no-ignore              Do not honor .gitignore/.ignore files.
        --follow-links           Follow symbolic links while walking.
        --expand-workspaces      Also initialize Cargo workspace member crates.

Examples:
    kaptaind-cli trawl
    kaptaind-cli trawl --path ~/projects
    kaptaind-cli trawl --max-depth 3
    kaptaind-cli trawl --type rust,go --dry-run
    kaptaind-cli trawl --blacklist scratch,vendor/* --type rust

Notes:
    Discovery is root-down and ignore-aware: .gitignore/.ignore files are honored, the
    outermost valid project wins, and Cargo workspaces report their member crates.
    Rust projects require a valid Cargo.toml ([package] and/or [workspace]); stray or
    empty manifests are ignored. Workspace members are reported but only initialized
    with --expand-workspaces. By default, projects with an existing kaptaind.toml are
    skipped."#,
        after_help = r#"See the kaptaind-cli(1) man page and kaptaind.toml(5) for details.
Relevant config section: [trawler] (if present)."#
    )]
    Trawl {
        /// Root directory to start trawling from (default: current directory).
        #[arg(short, long, value_name = "PATH")]
        path: Option<PathBuf>,
        /// Maximum recursion depth (default: unlimited).
        #[arg(short, long, value_name = "DEPTH")]
        max_depth: Option<usize>,
        /// Include projects that are already initialized.
        #[arg(short, long)]
        include_existing: bool,
        /// Only process git repositories.
        #[arg(short, long)]
        require_git: bool,
        /// Do not auto-register discovered projects for autostart.
        #[arg(long)]
        no_register: bool,
        /// Filter by project types (comma-separated).
        ///
        /// Supported values: rust, node, python, go, swift, kotlin, java, ruby,
        /// elixir, php, dotnet, cpp, lua, scala, clojure, haskell, julia, r, perl.
        #[arg(short, long, value_name = "TYPES", value_delimiter = ',')]
        r#type: Vec<String>,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
        /// Dry run: discover but do not initialize.
        #[arg(long)]
        dry_run: bool,
        /// Additional directory names or globs to skip (comma-separated), layered on top
        /// of the built-in skip list and any .gitignore/.ignore files.
        #[arg(long, value_name = "GLOBS", value_delimiter = ',')]
        blacklist: Vec<String>,
        /// Do not honor .gitignore/.ignore files (surface projects in ignored dirs).
        #[arg(long)]
        no_ignore: bool,
        /// Follow symbolic links while walking.
        #[arg(long)]
        follow_links: bool,
        /// Also initialize Cargo workspace member crates (always reported regardless).
        #[arg(long)]
        expand_workspaces: bool,
    },

    /// 📊 Live terminal dashboard
    #[command(long_about = r#"Purpose:
    Display a real-time view of kaptaind's state: version, daemon status,
    stability score, LLM costs, release history, and recent analysis artifacts.

Usage:
    kaptaind-cli dashboard [OPTIONS]

Options:
    -r, --repo <PATH>    Operate on the specified repository

Examples:
    kaptaind-cli dashboard

Notes:
    Updates by reading the latest .kaptaind/ state files."#)]
    Dashboard,

    /// 🚀 Emit release/hold recommendation for CI/CD pipelines
    #[command(long_about = r#"Purpose:
    Determine whether the current repository state qualifies for release based
    on stability score, pass streak, diff-spike guard, and cooldown.

Usage:
    kaptaind-cli ci-hint [OPTIONS]

Options:
    -f, --format <FORMAT>    Output format: text (default), json, or github.
    -r, --repo <PATH>        Operate on the specified repository

Examples:
    kaptaind-cli ci-hint
    kaptaind-cli ci-hint --format json
    kaptaind-cli ci-hint --format github

Notes:
    The github format emits workflow annotations and writes outputs through
    the GITHUB_OUTPUT environment file."#)]
    CiHint {
        /// Output format: text (default), json, or github.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },

    /// ✅ Enable auto-start for the kaptaind daemon
    #[command(long_about = r#"Purpose:
    Configure the system to automatically start kaptaind on boot or shell login.

Usage:
    kaptaind-cli enable-autostart

Examples:
    kaptaind-cli enable-autostart

Notes:
    Linux/systemd installs a user service, macOS adds a launchd plist, and the
    cross-platform fallback appends shell initialization to ~/.bashrc and
    ~/.zshrc."#)]
    EnableAutostart,

    /// ❌ Disable auto-start for the kaptaind daemon
    #[command(long_about = r#"Purpose:
    Remove auto-start configuration so kaptaind no longer starts automatically.

Usage:
    kaptaind-cli disable-autostart

Examples:
    kaptaind-cli disable-autostart

Notes:
    Disables the systemd user service, removes the launchd plist, or strips the
    init lines from ~/.bashrc and ~/.zshrc."#)]
    DisableAutostart,

    /// 🚀 Start all registered kaptaind daemons
    #[command(long_about = r#"Purpose:
    Read ~/.kaptaind/projects.txt and launch a kaptaind daemon for each
    registered project. Used internally by the auto-start system.

Usage:
    kaptaind-cli autostart

Examples:
    kaptaind-cli autostart"#)]
    Autostart,

    /// 📋 Monitor registered projects and resume daemons
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Register, list, enable, disable, and resume monitoring for kaptaind
    projects. The registry lives at ~/.config/kaptaind/monitored.json.

Usage:
    kaptaind-cli monitor <SUBCOMMAND>

Subcommands:
    add      Register a project for monitoring
    remove   Unregister a project
    list     Show all registered projects
    enable   Enable monitoring for a project
    disable  Disable monitoring for a project
    resume   Start daemons for all enabled, not-running projects

Examples:
    kaptaind-cli monitor add
    kaptaind-cli monitor add /path/to/repo --port 3001
    kaptaind-cli monitor list
    kaptaind-cli monitor resume"#
    )]
    Monitor(MonitorCommand),

    /// ⚙️ Install or manage the kaptaind system/user service
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Install, uninstall, check status, or install the notification icon for
    the systemd/LaunchAgent service that resumes monitored kaptaind projects
    on login or boot.

Usage:
    kaptaind-cli service <SUBCOMMAND>

Subcommands:
    install        --user | --system
    uninstall      --user | --system
    install-icon   --user | --system
    status         --user | --system

Examples:
    kaptaind-cli service install --user
    kaptaind-cli service install --system
    kaptaind-cli service install-icon --user
    kaptaind-cli service status --user"#
    )]
    Service(ServiceCommand),

    /// 🔍 View and manage AoC traces
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Inspect per-cluster trace records linked to Aim of Change sessions.

Usage:
    kaptaind-cli trace <SUBCOMMAND>

Subcommands:
    log     List traces for the current or a specified AoC session
    list    List traces with optional JSON output
    show    Display a detailed trace breakdown
    prune   Remove traces older than N days

Examples:
    kaptaind-cli trace log
    kaptaind-cli trace log --limit 20
    kaptaind-cli trace list --format json
    kaptaind-cli trace show <cluster-id>
    kaptaind-cli trace prune --days 7"#
    )]
    Trace(TraceCommand),

    /// 🎨 Manage Visual Asset Channel Saturation
    #[command(
        subcommand,
        long_about = r#"Purpose:
    View and manually trigger visual asset generation (diagrams, charts) linked
    to commits or concepts.

Usage:
    kaptaind-cli vacs <SUBCOMMAND>

Subcommands:
    show       List generated visual assets
    generate   Trigger asset generation

Examples:
    kaptaind-cli vacs show
    kaptaind-cli vacs show <commit-id>
    kaptaind-cli vacs generate --asset-type diagram"#
    )]
    Vacs(VacsCommand),

    /// 🧹 Workspace storage management
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Clean and sweep build artifacts, caches, and stale storage.

Usage:
    kaptaind-cli storage <SUBCOMMAND>

Subcommands:
    clean    Run cargo clean across the workspace
    sweep    Sweep stale artifacts and caches
    status   Report workspace storage state

Examples:
    kaptaind-cli storage clean
    kaptaind-cli storage clean --profile debug --dry-run
    kaptaind-cli storage sweep --keep-days 14
    kaptaind-cli storage status --json"#
    )]
    Storage(StorageCommand),

    /// 🦈 Shark Stating: high availability leader election
    #[command(
        subcommand,
        long_about = r#"Purpose:
    View and manage Shark Stating, the high-availability leader election system.

Usage:
    kaptaind-cli shark <SUBCOMMAND>

Subcommands:
    status     Show current role and lease state
    observe    Watch leadership changes in real time
    release    Gracefully release leadership
    upgrade    Perform a zero-downtime binary upgrade

Examples:
    kaptaind-cli shark status
    kaptaind-cli shark status --json
    kaptaind-cli shark observe
    kaptaind-cli shark upgrade --binary /usr/local/bin/kaptaind"#
    )]
    Shark(SharkCommand),

    /// 🚢 Build release binaries and distribute to channels
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Produce release binaries for configured targets, build installers, and
    publish to package managers and app stores.

Usage:
    kaptaind-cli ship <SUBCOMMAND>

Subcommands:
    plan             Preview the ship plan without building or publishing
    run              Execute the ship pipeline
    stable           Ship a stable release from the current VERSION
    nightly          Ship a nightly prerelease with an auto-generated version
    request-approval Request an approval-gated release for the current VERSION
    approve          Approve a previously requested release
    status           Show the last ship run and scheduled auto-releases

Options (common):
    -t, --targets <TARGETS>    Override target triples (comma-separated).
    -c, --channels <CHANNELS>  Override channels (comma-separated).
        --format <FORMAT>      Output format: text (default) or json.

Examples:
    kaptaind-cli ship plan
    kaptaind-cli ship run
    kaptaind-cli ship run --force
    kaptaind-cli ship stable --dry-run
    kaptaind-cli ship nightly --no-force
    kaptaind-cli ship status --auto

Notes:
    The run and stable subcommands skip qualification gates when --force is set.
    Nightly releases skip qualification gates by default; use --no-force to
    enforce them."#,
        after_help = r#"See the kaptaind-cli(1) man page and kaptaind.toml(5) for details.
Relevant config sections: [ship], [ship.stable], [ship.nightly], [ship.channels]."#
    )]
    Ship(ShipCommand),

    /// 🩺 Capture a host profile and recommend a repo-size tier
    #[command(long_about = r#"Purpose:
    Capture the host's hardware/OS profile, check inotify watch limits against
    the repo-size tier table, verify tool availability, and recommend a tier
    (T0–T4). Writes a machine-readable artifact to .kaptaind/doctor/.

Usage:
    kaptaind-cli doctor [OPTIONS]

Options:
    -f, --format <FORMAT>    Output format: text (default) or json.

Examples:
    kaptaind-cli doctor
    kaptaind-cli doctor --format json

Notes:
    The JSON artifact includes the git revision and dirty flag and feeds the
    `report` qualification bundle."#)]
    Doctor {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },

    /// 🌪️  Drive the real pipeline over a deterministic synthetic repo
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Generate a reproducible synthetic repo into a temp dir and run the real
    cluster → diff → weight → version pipeline (no commit, no daemon) over N
    change batches, asserting the version never decreases.

Usage:
    kaptaind-cli stress run [OPTIONS]

Options:
    --files <N>        Number of synthetic source files (default: 50).
    --batches <N>      Number of change batches (default: 5).
    --seed <N>         Deterministic RNG seed (default: 1).
    --langs <LIST>     Comma-separated languages (default: rust,ts,py,go).
    -f, --format <F>   Output format: text (default) or json.

Examples:
    kaptaind-cli stress run --files 100 --batches 10
    kaptaind-cli stress run --files 20 --batches 3 --format json

Notes:
    Writes .kaptaind/stress/<run-id>.json with per-stage latency and the bump
    distribution."#
    )]
    Stress(StressCommand),

    /// 📑 Aggregate qualification evidence into a report bundle
    #[command(long_about = r#"Purpose:
    Aggregate the latest doctor/bench/stress artifacts plus optional external
    logs (cargo-test, clippy, deny, container) into a
    `kaptaind.qualification.v1` JSON and a human markdown report.

Usage:
    kaptaind-cli report [OPTIONS]

Options:
    -v, --version <V>          Version to report (default: read VERSION).
    -o, --out <DIR>            Output directory (default: .kaptaind/report).
        --cargo-test <PATH>    Text log whose last line carries TEST_EXIT=<n>.
        --clippy <PATH>        Text log whose last line carries CLIPPY_EXIT=<n>.
        --deny <PATH>          Text log whose last line carries DENY_EXIT=<n>.
        --container <PATH>     Text log whose last line carries CONTAINER_EXIT=<n>.
    -f, --format <FORMAT>      Output format: text (default) or json.

Examples:
    kaptaind-cli report --version 9.7.16 --format json
    kaptaind-cli report --cargo-test target/test.log --clippy target/clippy.log

Notes:
    A section is PASS only with real evidence; missing evidence is
    PASS-WITH-NOTES ("not run in-session"); any FAIL marker makes it FAIL."#)]
    Report {
        /// Version to report (default: read VERSION).
        #[arg(short, long, value_name = "V")]
        version: Option<String>,
        /// Output directory (default: .kaptaind/report).
        #[arg(short, long, value_name = "DIR")]
        out: Option<PathBuf>,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
        /// cargo-test log with a TEST_EXIT=<n> marker on its last line.
        #[arg(long, value_name = "PATH")]
        cargo_test: Option<PathBuf>,
        /// clippy log with a CLIPPY_EXIT=<n> marker on its last line.
        #[arg(long, value_name = "PATH")]
        clippy: Option<PathBuf>,
        /// cargo-deny log with a DENY_EXIT=<n> marker on its last line.
        #[arg(long, value_name = "PATH")]
        deny: Option<PathBuf>,
        /// container log with a CONTAINER_EXIT=<n> marker on its last line.
        #[arg(long, value_name = "PATH")]
        container: Option<PathBuf>,
    },

    /// 📜 Inspect daemon logs (.kaptaind/daemon.out, daemon.err)
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Tail, filter errors, or grep the daemon's text logs.

Usage:
    kaptaind-cli logs <SUBCOMMAND>

Subcommands:
    tail     Show the last N lines
    errors   Show ERROR/WARN lines
    grep     Filter lines by a regex

Examples:
    kaptaind-cli logs tail -n 50
    kaptaind-cli logs errors
    kaptaind-cli logs grep "commit" --format json"#
    )]
    Logs(LogsCommand),

    /// 🔐 Inspect the compliance audit trail (.kaptaind/audit.jsonl)
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Tail, summarize, or verify the append-only audit log. `verify` checks
    timestamp ordering and (when present) the per-entry prev_hash chain.

Usage:
    kaptaind-cli audit <SUBCOMMAND>

Subcommands:
    tail     Show the last N entries
    stats    Counts by event_type/result and failure rate
    verify   Append-only ordering + optional hash-chain check
    export-verify  Verify integrity linkage for the configured collector mirror

Examples:
    kaptaind-cli audit tail -n 20
    kaptaind-cli audit stats
    kaptaind-cli audit verify
    kaptaind-cli audit export-verify"#
    )]
    Audit(AuditCommand),

    /// 🧾 Record hashed CI, scanner, ITSM, or domain evidence for a release
    #[command(subcommand)]
    Evidence(EvidenceCommand),

    /// 🏛️ Assess enforced enterprise governance controls
    Governance {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },

    /// 🔌 List the governed enterprise connector catalogue and active configuration
    Integrations {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },

    /// 🌍 Observe environment lifecycle evidence; never performs deployments
    #[command(subcommand)]
    Environment(EnvironmentCommand),

    /// 🛰️  Probe the daemon's health/metrics/events endpoints
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Scrape the daemon's HTTP endpoints without hand-curling: /health,
    /metrics, /metrics/prometheus, and /events (SSE). Uses a minimal HTTP/1.1
    client; if the daemon is not running, prints a clear message.

Usage:
    kaptaind-cli probe <SUBCOMMAND>

Subcommands:
    health    GET /health
    metrics   GET /metrics (--prometheus for text exposition)
    events    GET /events (--follow to stream SSE)

Examples:
    kaptaind-cli probe health
    kaptaind-cli probe metrics --prometheus
    kaptaind-cli probe events --follow

Notes:
    Reads the health port from config (default 9090)."#
    )]
    Probe(ProbeCommand),

    /// 🧬 Migrate the .kaptaind semantic-state document
    #[command(long_about = r#"Purpose:
    Deterministically migrate the repository's .kaptaind/state.toml semantic
    document to a newer (or older) schema version, one discrete step at a
    time. Normal analysis never rewrites the document — migrate is the only
    mutation path, and every run is recorded in .kaptaind/migrations/.

Usage:
    kaptaind-cli migrate [OPTIONS]

Options:
        --check               Report whether migration is needed (no changes).
        --strict              With --check: exit non-zero when outdated (CI).
        --to <VERSION>        Target schema version (default: latest supported).
        --allow-lossy         Permit migrations that discard information.
    -f, --format <FORMAT>     Output format: text (default) or json.

Examples:
    kaptaind-cli migrate
    kaptaind-cli migrate --check --strict
    kaptaind-cli migrate --to 2.0 --allow-lossy
    kaptaind-cli migrate --check --format json"#)]
    Migrate {
        /// Report whether migration is needed without changing anything.
        #[arg(long)]
        check: bool,
        /// With --check: exit non-zero when the document is outdated.
        #[arg(long)]
        strict: bool,
        /// Target schema version (default: latest supported).
        #[arg(long, value_name = "VERSION")]
        to: Option<String>,
        /// Permit migrations that discard information.
        #[arg(long)]
        allow_lossy: bool,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },

    /// 📚 Inspect installed .kaptaind schema versions
    #[command(
        subcommand,
        long_about = r#"Purpose:
    Show which .kaptaind schema versions this kaptaind knows about.

Usage:
    kaptaind-cli schema <SUBCOMMAND>

Subcommands:
    list               List installed schema versions
    explain <VERSION> Describe a schema version

Examples:
    kaptaind-cli schema list
    kaptaind-cli schema explain 2.1"#
    )]
    Schema(SchemaCommand),
}

#[derive(Subcommand)]
enum SchemaCommand {
    /// List installed schema versions
    List,
    /// Describe a schema version
    Explain {
        /// Schema version to explain (e.g. 2.1).
        #[arg(value_name = "VERSION")]
        version: String,
    },
}

#[derive(Subcommand)]
enum IntegrateCommand {
    /// Run both tools and persist an advisory, machine-readable report.
    Analyze {
        /// Host/target branch or ref.
        target: String,
        /// Proposed source/fork branch or ref.
        source: String,
        /// Emit JSON instead of the concise summary.
        #[arg(long)]
        json: bool,
        /// Do not write the report or audit event.
        #[arg(long)]
        no_persist: bool,
    },
}

#[derive(Subcommand)]
enum EnvironmentCommand {
    /// Show the latest known release fact for each environment
    Status {
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Explain risk from recorded rollout, health, rollback, and drift evidence
    Risk {
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Show immutable lifecycle records for one environment
    History {
        environment: String,
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Compare the latest recorded version and configuration digest
    Diff {
        from: String,
        to: String,
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Record an externally performed deployment or health observation
    Record {
        environment: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        health: Option<String>,
        #[arg(long)]
        rollout_percent: Option<u8>,
        #[arg(long)]
        config_sha256: Option<String>,
        #[arg(long)]
        note: Option<String>,
    },
    /// Record a promotion request; deployment remains external
    Promote {
        from: String,
        to: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        adr: Option<String>,
    },
    /// Record a rollback decision; deployment remains external
    Rollback {
        environment: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        adr: Option<String>,
    },
}

#[derive(Subcommand)]
enum StorageCommand {
    /// 🧹 Run cargo clean across the workspace
    #[command(long_about = r#"Purpose:
    Remove build artifacts for the specified cargo profile.

Usage:
    kaptaind-cli storage clean [OPTIONS]

Options:
    -p, --profile <PROFILE>    Profile to clean: debug, release, or all (default: all).
        --dry-run              Only print what would be removed.
    -o, --older-than <DAYS>    Only remove artifacts older than N days.

Examples:
    kaptaind-cli storage clean
    kaptaind-cli storage clean --profile debug
    kaptaind-cli storage clean --dry-run --older-than 7"#)]
    Clean {
        /// Profile to clean: debug, release, or all (default: all).
        #[arg(short, long, value_name = "PROFILE", default_value = "all")]
        profile: String,
        /// Only print what would be removed.
        #[arg(long)]
        dry_run: bool,
        /// Only remove artifacts older than N days.
        #[arg(short, long, value_name = "DAYS")]
        older_than: Option<u64>,
    },
    /// 🧹 Sweep stale artifacts and caches
    #[command(long_about = r#"Purpose:
    Remove stale registry cache entries, git checkouts, and other cached data.

Usage:
    kaptaind-cli storage sweep [OPTIONS]

Options:
    -k, --keep-days <DAYS>    Keep registry cache entries newer than N days (default: 30).
        --dry-run             Only print what would be removed.

Examples:
    kaptaind-cli storage sweep
    kaptaind-cli storage sweep --keep-days 14
    kaptaind-cli storage sweep --dry-run"#)]
    Sweep {
        /// Keep registry cache entries newer than N days (default: 30).
        #[arg(short, long, value_name = "DAYS", default_value_t = 30)]
        keep_days: u64,
        /// Only print what would be removed.
        #[arg(long)]
        dry_run: bool,
    },
    /// 📊 Report workspace storage state
    #[command(long_about = r#"Purpose:
    Report disk usage for workspace artifacts and caches.

Usage:
    kaptaind-cli storage status [OPTIONS]

Options:
    -j, --json          Output JSON instead of text.
    -l, --limit <N>     Show only the top N largest artifacts.

Examples:
    kaptaind-cli storage status
    kaptaind-cli storage status --json
    kaptaind-cli storage status --limit 10"#)]
    Status {
        /// Output JSON instead of text.
        #[arg(short, long)]
        json: bool,
        /// Show only the top N largest artifacts.
        #[arg(short, long, value_name = "N")]
        limit: Option<usize>,
    },
}

#[derive(Subcommand)]
enum SharkCommand {
    /// 🦈 Show current Shark Stating role and lease state
    #[command(long_about = r#"Purpose:
    Display the current instance's role (leader or standby) and the active lease
    state.

Usage:
    kaptaind-cli shark status [OPTIONS]

Options:
    -j, --json    Output JSON instead of text.

Examples:
    kaptaind-cli shark status
    kaptaind-cli shark status --json"#)]
    Status {
        /// Output JSON instead of text.
        #[arg(short, long)]
        json: bool,
    },
    /// 👀 Watch leadership changes in real time
    #[command(long_about = r#"Purpose:
    Poll the Shark arbiter and print leadership changes until interrupted.

Usage:
    kaptaind-cli shark observe [OPTIONS]

Options:
    -i, --interval-ms <MILLISECONDS>    Poll interval in milliseconds (default: 1000).

Examples:
    kaptaind-cli shark observe
    kaptaind-cli shark observe --interval-ms 500"#)]
    Observe {
        /// Poll interval in milliseconds (default: 1000).
        #[arg(short, long, value_name = "MILLISECONDS", default_value_t = 1000)]
        interval_ms: u64,
    },
    /// 🏳️ Gracefully release leadership
    #[command(long_about = r#"Purpose:
    Release the current instance's leadership lease, if held.

Usage:
    kaptaind-cli shark release

Examples:
    kaptaind-cli shark release

Notes:
    Requires the shark.release RBAC permission."#)]
    Release,
    /// ⬆️ Perform a zero-downtime upgrade to a new kaptaind binary
    #[command(
        long_about = r#"Purpose:
    Replace the running kaptaind binary with a new version without dropping the
    leader lease. Spawns a standby instance, waits for it to become healthy, and
    hands off leadership.

Usage:
    kaptaind-cli shark upgrade [OPTIONS]

Options:
    -b, --binary <BINARY>                  Path to the new kaptaind binary.
    -s, --standby-health-port <PORT>       Temporary health port for the standby instance.
    -r, --ready-timeout-ms <MILLISECONDS>  How long to wait for the standby to become
                                           healthy before retiring (default: 30000).

Examples:
    kaptaind-cli shark upgrade --binary /usr/local/bin/kaptaind
    kaptaind-cli shark upgrade --binary ./target/release/kaptaind --standby-health-port 9090

Notes:
    Must be run from the current leader. Requires the shark.upgrade RBAC
    permission."#,
        after_help = r#"See the kaptaind-cli(1) man page and kaptaind.toml(5) for details.
Relevant config section: [shark]."#
    )]
    Upgrade {
        /// Path to the new kaptaind binary.
        #[arg(short, long, value_name = "BINARY")]
        binary: PathBuf,
        /// Temporary health port for the standby instance.
        #[arg(short, long, value_name = "PORT")]
        standby_health_port: Option<u16>,
        /// How long to wait for the standby to become healthy before retiring (default: 30000).
        #[arg(short, long, value_name = "MILLISECONDS", default_value_t = 30000)]
        ready_timeout_ms: u64,
    },
}

#[derive(Subcommand)]
enum BranchCommand {
    /// Report topology, versions, revisions, divergence, and promotion readiness.
    Status {
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = "desktop", value_parser = ["desktop", "mobile"])]
        platform: String,
    },
    /// Create missing mandatory lifecycle branches without overwriting refs.
    Init {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Diagnose missing or unexpectedly divergent lifecycle branches.
    Sync {
        #[arg(long)]
        json: bool,
    },
    /// Perform a permitted, clean, fast-forward lifecycle transition.
    Promote {
        source: String,
        target: String,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum LifecycleReleaseCommand {
    /// Create an immutable-identity release candidate from integration.
    Prepare {
        version: String,
        #[arg(long, default_value = "integration")]
        source: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Run configured build/test and consistency gates for a candidate.
    Validate {
        version: String,
        #[arg(long)]
        json: bool,
    },
    /// Atomically advance production and create v<version> after validation.
    Issue {
        version: String,
        #[arg(long, default_value = "desktop", value_parser = ["desktop", "mobile"])]
        platform: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Issue a new release whose tree restores an older released version.
    Rollback {
        version: String,
        #[arg(long = "as", value_name = "NEW_VERSION")]
        new_version: String,
        #[arg(long, default_value = "desktop", value_parser = ["desktop", "mobile"])]
        platform: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ShipCommand {
    /// 📋 Preview the ship plan without building or publishing
    #[command(long_about = r#"Purpose:
    Show what the ship pipeline would build and publish without performing any
    destructive operations.

Usage:
    kaptaind-cli ship plan [OPTIONS]

Options:
    -t, --targets <TARGETS>    Override target triples (comma-separated).
    -c, --channels <CHANNELS>  Override channels (comma-separated).
        --format <FORMAT>      Output format: text (default) or json.

Examples:
    kaptaind-cli ship plan
    kaptaind-cli ship plan --targets x86_64-unknown-linux-gnu
    kaptaind-cli ship plan --format json"#)]
    Plan {
        /// Override target triples (comma-separated).
        #[arg(short, long, value_name = "TARGETS", value_delimiter = ',')]
        targets: Vec<String>,
        /// Override channels (comma-separated: binaries, shell-installer, tauri, homebrew, github-releases).
        #[arg(short, long, value_name = "CHANNELS", value_delimiter = ',')]
        channels: Vec<String>,
        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 🚢 Execute the ship pipeline
    #[command(long_about = r#"Purpose:
    Build release binaries, installers, and publish to configured channels.

Usage:
    kaptaind-cli ship run [OPTIONS]

Options:
    -t, --targets <TARGETS>    Override target triples (comma-separated).
    -c, --channels <CHANNELS>  Override channels (comma-separated).
    -f, --force                Skip qualification gates.
        --format <FORMAT>      Output format: text (default) or json.

Examples:
    kaptaind-cli ship run
    kaptaind-cli ship run --force
    kaptaind-cli ship run --channels binaries,homebrew"#)]
    Run {
        /// Override target triples (comma-separated).
        #[arg(short, long, value_name = "TARGETS", value_delimiter = ',')]
        targets: Vec<String>,
        /// Override channels (comma-separated).
        #[arg(short, long, value_name = "CHANNELS", value_delimiter = ',')]
        channels: Vec<String>,
        /// Skip qualification gates.
        #[arg(short, long)]
        force: bool,
        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 🏷️ Ship a stable release from the current VERSION
    #[command(long_about = r#"Purpose:
    Produce and publish a stable release using the current VERSION file.

Usage:
    kaptaind-cli ship stable [OPTIONS]

Options:
    -t, --targets <TARGETS>    Override target triples (comma-separated).
    -c, --channels <CHANNELS>  Override channels (comma-separated).
        --dry-run              Preview without building or publishing.
    -f, --force                Skip qualification gates.
        --format <FORMAT>      Output format: text (default) or json.

Examples:
    kaptaind-cli ship stable
    kaptaind-cli ship stable --dry-run
    kaptaind-cli ship stable --force"#)]
    Stable {
        /// Override target triples (comma-separated).
        #[arg(short, long, value_name = "TARGETS", value_delimiter = ',')]
        targets: Vec<String>,
        /// Override channels (comma-separated).
        #[arg(short, long, value_name = "CHANNELS", value_delimiter = ',')]
        channels: Vec<String>,
        /// Preview without building or publishing.
        #[arg(long)]
        dry_run: bool,
        /// Skip qualification gates.
        #[arg(short, long)]
        force: bool,
        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 🌙 Ship a nightly prerelease with an auto-generated version
    #[command(long_about = r#"Purpose:
    Produce and publish a nightly prerelease with an auto-generated version
    suffix.

Usage:
    kaptaind-cli ship nightly [OPTIONS]

Options:
    -t, --targets <TARGETS>    Override target triples (comma-separated).
    -c, --channels <CHANNELS>  Override channels (comma-separated).
        --dry-run              Preview without building or publishing.
        --no-force             Enforce qualification gates (nightly skips them by default).
        --format <FORMAT>      Output format: text (default) or json.

Examples:
    kaptaind-cli ship nightly
    kaptaind-cli ship nightly --dry-run
    kaptaind-cli ship nightly --no-force"#)]
    Nightly {
        /// Override target triples (comma-separated).
        #[arg(short, long, value_name = "TARGETS", value_delimiter = ',')]
        targets: Vec<String>,
        /// Override channels (comma-separated).
        #[arg(short, long, value_name = "CHANNELS", value_delimiter = ',')]
        channels: Vec<String>,
        /// Preview without building or publishing.
        #[arg(long)]
        dry_run: bool,
        /// Enforce qualification gates (nightly skips them by default).
        #[arg(long)]
        no_force: bool,
        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// Request an approval-gated release for the current VERSION.
    RequestApproval {
        /// Optional external change-ticket reference.
        #[arg(long, value_name = "TICKET")]
        ticket: Option<String>,
    },
    /// Approve a previously requested release.
    Approve {
        /// Version to approve (defaults to the current VERSION).
        #[arg(long, value_name = "VERSION")]
        version: Option<String>,
    },
    /// 📊 Show the last ship run and scheduled auto-releases
    #[command(long_about = r#"Purpose:
    Display the most recent ship run and, with --auto, the next scheduled
    auto-nightly and auto-stable fire times.

Usage:
    kaptaind-cli ship status [OPTIONS]

Options:
        --format <FORMAT>    Output format: text (default) or json.
        --auto               Include next scheduled auto-release fire times.

Examples:
    kaptaind-cli ship status
    kaptaind-cli ship status --auto
    kaptaind-cli ship status --format json"#)]
    Status {
        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: String,
        /// Include next scheduled auto-nightly and auto-stable fire times.
        #[arg(long)]
        auto: bool,
    },
}

#[derive(Subcommand)]
enum VacsCommand {
    /// 🖼️ Show generated visual assets
    #[command(long_about = r#"Purpose:
    List generated visual assets, optionally filtered by commit or concept ID.

Usage:
    kaptaind-cli vacs show [ID]

Arguments:
    [ID]    Optional commit or concept ID to filter by.

Examples:
    kaptaind-cli vacs show
    kaptaind-cli vacs show <commit-id>"#)]
    Show {
        /// Optional commit or concept ID to filter by.
        #[arg(value_name = "ID")]
        commit: Option<String>,
    },
    /// 🎨 Manually trigger generation of a visual asset
    #[command(long_about = r#"Purpose:
    Trigger generation of a visual asset of the specified type.

Usage:
    kaptaind-cli vacs generate [OPTIONS]

Options:
        --asset-type <TYPE>    Type of asset to generate (default: diagram).

Examples:
    kaptaind-cli vacs generate
    kaptaind-cli vacs generate --asset-type chart"#)]
    Generate {
        /// Type of asset to generate (default: diagram).
        #[arg(long, value_name = "TYPE", default_value = "diagram")]
        asset_type: String,
    },
}

#[derive(Subcommand)]
enum TraceCommand {
    /// 📜 List traces for the current or specified AoC session
    #[command(long_about = r#"Purpose:
    Display traces for the active Aim of Change session or a specified AoC ID.

Usage:
    kaptaind-cli trace log [OPTIONS]

Options:
    -a, --aoc-id <ID>    AoC ID to filter by (defaults to the active session).
    -l, --limit <N>      Number of traces to display (default: 10).

Examples:
    kaptaind-cli trace log
    kaptaind-cli trace log --limit 20
    kaptaind-cli trace log --aoc-id <id>"#)]
    Log {
        /// AoC ID to filter by (defaults to the active session).
        #[arg(short, long, value_name = "ID")]
        aoc_id: Option<String>,
        /// Number of traces to display (default: 10).
        #[arg(short, long, value_name = "N", default_value_t = 10)]
        limit: usize,
    },
    /// 📋 List traces (alias of log) with optional JSON output
    #[command(long_about = r#"Purpose:
    List traces for the active Aim of Change session. Equivalent to
    `trace log` but supports `--format json` for machine consumption.

Usage:
    kaptaind-cli trace list [OPTIONS]

Options:
    -f, --format <FORMAT>    Output format: text (default) or json.
    -l, --limit <N>          Number of traces to display (default: 10).

Examples:
    kaptaind-cli trace list --format json --limit 20"#)]
    List {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
        /// Number of traces to display (default: 10).
        #[arg(short, long, value_name = "N", default_value_t = 10)]
        limit: usize,
    },
    /// 🔍 Show detailed breakdown of a specific trace
    #[command(long_about = r#"Purpose:
    Display a detailed breakdown for a single trace by cluster ID.

Usage:
    kaptaind-cli trace show <CLUSTER_ID>

Arguments:
    <CLUSTER_ID>    Cluster or trace ID to display.

Examples:
    kaptaind-cli trace show <cluster-id>"#)]
    Show {
        /// Cluster or trace ID to display.
        #[arg(value_name = "ID")]
        cluster_id: String,
    },
    /// 🧹 Prune traces older than N days
    #[command(long_about = r#"Purpose:
    Remove trace records older than the specified retention period.

Usage:
    kaptaind-cli trace prune [OPTIONS]

Options:
    -d, --days <DAYS>    Retention period in days (default: 30).

Examples:
    kaptaind-cli trace prune
    kaptaind-cli trace prune --days 7"#)]
    Prune {
        /// Retention period in days (default: 30).
        #[arg(short, long, value_name = "DAYS", default_value_t = 30)]
        days: i64,
    },
}

#[derive(Subcommand)]
enum MonitorCommand {
    /// ➕ Register a project for monitoring
    #[command(long_about = r#"Purpose:
    Add a project to the monitor registry. Paths are resolved to absolute
    form. If no config is given, <project>/kaptaind.toml is assumed. If no
    port is given, the next free health port starting at 3000 is assigned.

Usage:
    kaptaind-cli monitor add [PATH] [OPTIONS]

Arguments:
    [PATH]    Project path (default: current directory).

Options:
    -c, --config <PATH>     Path to kaptaind.toml.
    -p, --port <PORT>       Health server port for this project.
        --enabled <BOOL>    Enable or disable monitoring (default: true).

Examples:
    kaptaind-cli monitor add
    kaptaind-cli monitor add ~/projects/my-app --port 3001
    kaptaind-cli monitor add /path/to/repo --config /path/to/repo/kaptaind.toml --enabled false"#)]
    Add {
        /// Project path (default: current directory).
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Path to kaptaind.toml.
        #[arg(short, long, value_name = "PATH")]
        config: Option<PathBuf>,

        /// Health server port for this project.
        #[arg(short, long, value_name = "PORT")]
        port: Option<u16>,

        /// Enable or disable monitoring.
        #[arg(long, value_name = "BOOL")]
        enabled: Option<bool>,
    },

    /// ➖ Unregister a project
    #[command(long_about = r#"Purpose:
    Remove a project from the monitor registry by path.

Usage:
    kaptaind-cli monitor remove <PATH>

Arguments:
    <PATH>    Project path.

Examples:
    kaptaind-cli monitor remove /path/to/repo
    kaptaind-cli monitor remove ."#)]
    Remove {
        /// Project path.
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },

    /// 📋 List registered projects
    #[command(long_about = r#"Purpose:
    Display all projects in the monitor registry, including their config
    path, enabled status, health port, and last active timestamp.

Usage:
    kaptaind-cli monitor list

Examples:
    kaptaind-cli monitor list"#)]
    List,

    /// ▶️ Enable monitoring for a project
    #[command(long_about = r#"Purpose:
    Mark a registered project as enabled so it is resumed on login.

Usage:
    kaptaind-cli monitor enable <PATH>

Arguments:
    <PATH>    Project path.

Examples:
    kaptaind-cli monitor enable /path/to/repo"#)]
    Enable {
        /// Project path.
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },

    /// ⏸️ Disable monitoring for a project
    #[command(long_about = r#"Purpose:
    Mark a registered project as disabled so it is skipped on resume.

Usage:
    kaptaind-cli monitor disable <PATH>

Arguments:
    <PATH>    Project path.

Examples:
    kaptaind-cli monitor disable /path/to/repo"#)]
    Disable {
        /// Project path.
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },

    /// 🚀 Start daemons for all enabled, not-running projects
    #[command(long_about = r#"Purpose:
    Iterate over enabled projects in the registry and start a kaptaind
    daemon for each one that is not already running. Already-running
    projects are detected via their .kaptaind/daemon.pid file.

Usage:
    kaptaind-cli monitor resume

Examples:
    kaptaind-cli monitor resume"#)]
    Resume,
}

#[derive(Subcommand)]
enum ServiceCommand {
    /// 🔧 Install the user or system service
    #[command(long_about = r#"Purpose:
    Install a systemd user service (Linux), LaunchAgent (macOS), or shell
    autostart fallback that runs `kaptaind-supervisor run` on login.
    The system variant writes to /etc/systemd/system and requires root.

Usage:
    kaptaind-cli service install --user
    kaptaind-cli service install --system

Options:
        --user      Install for the current user.
        --system    Install system-wide (requires root on Linux/macOS).

Examples:
    kaptaind-cli service install --user
    sudo kaptaind-cli service install --system"#)]
    Install {
        /// Install for the current user.
        #[arg(long)]
        user: bool,

        /// Install system-wide.
        #[arg(long)]
        system: bool,
    },

    /// 🗑️ Remove the user or system service
    #[command(long_about = r#"Purpose:
    Remove the installed systemd service, LaunchAgent, or shell autostart
    entry.

Usage:
    kaptaind-cli service uninstall --user
    kaptaind-cli service uninstall --system

Options:
        --user      Remove the user service.
        --system    Remove the system service (requires root).

Examples:
    kaptaind-cli service uninstall --user
    sudo kaptaind-cli service uninstall --system"#)]
    Uninstall {
        /// Remove the user service.
        #[arg(long)]
        user: bool,

        /// Remove the system service.
        #[arg(long)]
        system: bool,
    },

    /// 🎨 Install the kaptaind logo into the icon theme
    #[command(long_about = r#"Purpose:
    Install the kaptaind logo into the Freedesktop icon theme so notifications
    and desktop launchers can display it by name. The user variant installs to
    ~/.local/share/icons; the system variant installs to /usr/share/icons and
    requires root.

Usage:
    kaptaind-cli service install-icon --user
    kaptaind-cli service install-icon --system

Options:
        --user      Install for the current user.
        --system    Install system-wide (requires root on Linux).

Examples:
    kaptaind-cli service install-icon --user
    sudo kaptaind-cli service install-icon --system"#)]
    InstallIcon {
        /// Install for the current user.
        #[arg(long)]
        user: bool,

        /// Install system-wide.
        #[arg(long)]
        system: bool,
    },

    /// ℹ️ Check whether the service is installed
    #[command(long_about = r#"Purpose:
    Report whether the user or system service file is present and enabled.

Usage:
    kaptaind-cli service status --user
    kaptaind-cli service status --system

Options:
        --user      Check the user service.
        --system    Check the system service.

Examples:
    kaptaind-cli service status --user"#)]
    Status {
        /// Check the user service.
        #[arg(long)]
        user: bool,

        /// Check the system service.
        #[arg(long)]
        system: bool,
    },
}

#[derive(Subcommand)]
enum AocCommand {
    /// 🎯 Start a new Aim of Change session
    #[command(long_about = r#"Purpose:
    Begin a named session to group related commits under a single intent. All
    commits while the session is active will be tagged with this label and
    linked in the manifest.

Usage:
    kaptaind-cli aoc start <LABEL>

Arguments:
    <LABEL>    User-friendly name for this session.

Examples:
    kaptaind-cli aoc start "feature: authentication flow"
    kaptaind-cli aoc start "refactor: database layer"
    kaptaind-cli aoc start "fix: memory leaks"

Notes:
    Session state is stored in .kaptaind/aoc/active.json. Only one session can
    be active at a time."#)]
    Start {
        /// User-friendly name for this session.
        #[arg(value_name = "LABEL")]
        label: String,
    },

    /// 🚢 End and ship the current session
    #[command(long_about = r#"Purpose:
    Finalize the active Aim of Change session and create a manifest containing
    the session name, ID, timestamps, commits, version progression, and test
    results.

Usage:
    kaptaind-cli aoc ship

Examples:
    kaptaind-cli aoc ship

Notes:
    The manifest is archived to .kaptaind/aoc/manifests/<id>.json and the active
    session is removed."#)]
    Ship,

    /// 📋 Show status of the current session
    #[command(long_about = r#"Purpose:
    Display the active session name, start time, initial version, and number of
    traces collected so far.

Usage:
    kaptaind-cli aoc status

Examples:
    kaptaind-cli aoc status

Notes:
    Returns an error if no session is active."#)]
    Status,

    /// ❌ Cancel the current session
    #[command(long_about = r#"Purpose:
    Cancel the active Aim of Change session without creating a manifest.

Usage:
    kaptaind-cli aoc cancel

Examples:
    kaptaind-cli aoc cancel

Notes:
    Removes .kaptaind/aoc/active.json. When [daemon].auto_resume_on_aoc_end
    is true (the default), this also resumes a daemon suspended by the
    session."#)]
    Cancel,

    /// 🤖 Intercept agent operations for contextual tracing
    #[command(
        long_about = r#"Purpose:
    Wrap a command (test, build, script) and capture its output, exit code, and
    execution time. Optionally record the agent model name and intent
    description.

Usage:
    kaptaind-cli aoc intercept [OPTIONS] -- <COMMAND> [ARGS]...

Arguments:
    <COMMAND>    Command to wrap and execute.
    [ARGS]...    Arguments for the command.

Options:
    -m, --model <MODEL>          Agent or LLM model name.
    -i, --intent <DESCRIPTION>   High-level description of the agent's task.

Examples:
    kaptaind-cli aoc intercept -- npm test
    kaptaind-cli aoc intercept --model claude-3-5-sonnet -- cargo test
    kaptaind-cli aoc intercept --intent "refactor auth" -- npm test

Notes:
    If no AoC session is active, a temporary session named after --intent (or
    "agent-intercept") is created. The session remains active for the daemon to
    process."#,
        after_help = r#"See the kaptaind-cli(1) man page and kaptaind.toml(5) for details.
Relevant config section: [aoc] (if present)."#
    )]
    Intercept {
        /// Agent or LLM model name (e.g., claude-3-5-sonnet, gpt-4, local-llama).
        #[arg(short, long, value_name = "MODEL")]
        model: Option<String>,

        /// High-level description of the agent's task.
        #[arg(short, long, value_name = "DESCRIPTION")]
        intent: Option<String>,

        /// Command to wrap and execute (everything after --).
        #[arg(value_name = "COMMAND")]
        command: String,

        /// Arguments for the command.
        #[arg(value_name = "ARGS")]
        args: Vec<String>,
    },

    /// 📚 View completed Aim of Change sessions
    #[command(long_about = r#"Purpose:
    List shipped AoC sessions with their manifests, showing the session name,
    version change, commit count, and test results.

Usage:
    kaptaind-cli aoc log [OPTIONS]

Options:
    -l, --limit <N>    Number of sessions to display (default: 10).

Examples:
    kaptaind-cli aoc log
    kaptaind-cli aoc log --limit 50"#)]
    Log {
        /// Number of sessions to display (default: 10).
        #[arg(short, long, value_name = "N", default_value_t = 10)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum StressCommand {
    /// 🌪️  Run the deterministic stress pipeline
    #[command(long_about = r#"Purpose:
    Generate a synthetic repo and run the real cluster → diff → weight →
    version pipeline over N batches (no commit, no daemon), asserting the
    version is monotone.

Usage:
    kaptaind-cli stress run [OPTIONS]

Examples:
    kaptaind-cli stress run --files 50 --batches 5
    kaptaind-cli stress run --files 20 --batches 3 --format json"#)]
    Run {
        /// Number of synthetic source files (default: 50).
        #[arg(long, value_name = "N", default_value_t = 50)]
        files: usize,
        /// Number of change batches (default: 5).
        #[arg(long, value_name = "N", default_value_t = 5)]
        batches: usize,
        /// Deterministic RNG seed (default: 1).
        #[arg(long, value_name = "N", default_value_t = 1)]
        seed: u64,
        /// Languages to generate (comma-separated; default rust,ts,py,go).
        #[arg(long, value_name = "LIST", value_delimiter = ',')]
        langs: Vec<String>,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
}

#[derive(Subcommand)]
enum LogsCommand {
    /// 📜 Show the last N log lines
    Tail {
        /// Number of lines to show (default: 50).
        #[arg(short = 'n', long, value_name = "N", default_value_t = 50)]
        n: usize,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 🚨 Show ERROR/WARN log lines
    Errors {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 🔎 Filter log lines by a regex
    Grep {
        /// Regular expression to match.
        #[arg(value_name = "REGEX")]
        pattern: String,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
}

#[derive(Subcommand)]
enum AuditCommand {
    /// 📜 Show the last N audit entries
    Tail {
        /// Number of entries to show (default: 50).
        #[arg(short = 'n', long, value_name = "N", default_value_t = 50)]
        n: usize,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 📊 Summarize counts by event_type/result
    Stats {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 🔐 Verify append-only ordering and optional hash chain
    Verify {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 🔗 Verify the configured audit-export mirror against the local chain
    ExportVerify {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
}

#[derive(Subcommand)]
enum EvidenceCommand {
    /// Record a local exported artifact as release evidence.
    Record {
        #[arg(long)]
        version: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Validate and record a bound-snapshot/v1 artifact as release evidence.
    AttachSnapshot {
        #[arg(long)]
        version: String,
        #[arg(long)]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum ProbeCommand {
    /// 🛰️  GET /health
    Health {
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 📈 GET /metrics (JSON) or /metrics/prometheus
    Metrics {
        /// Use the Prometheus text exposition endpoint.
        #[arg(long)]
        prometheus: bool,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
    /// 📡 GET /events, optionally following the SSE stream
    Events {
        /// Stream server-sent events until interrupted.
        #[arg(long)]
        follow: bool,
        /// Output format: text (default) or json.
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        format: String,
    },
}

#[tokio::main]
async fn __curly_original_main() -> anyhow::Result<()> {
    // Load optional `.env` file so provider API keys and other secrets can live
    // outside of `kaptaind.toml`.
    if let Err(error) = kaptaind::util::dotenv::load() {
        tracing::warn!(
            ?error,
            operation = "main",
            source_line = line!(),
            "best-effort operation failed"
        );
    }

    let cli = Cli::parse();

    // Init and Trawl commands work without a valid config
    match &cli.command {
        Commands::Init => {
            let rbac_config = loader::load()
                .map(|config| {
                    kaptaind::audit::configure_export(config.audit.export.clone());
                    kaptaind::audit::configure_governance_context(
                        config.governance.organization_id.clone(),
                        config.governance.tenant_id.clone(),
                    );
                    kaptaind::compliance::configure(config.clone());
                    config.rbac
                })
                .unwrap_or_default();
            kaptaind::rbac::check_permission(&rbac_config, "config.edit")?;

            let repo_path = cli
                .repo
                .map(|p| p.canonicalize().unwrap_or(p))
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let config = Config {
                repo_path,
                ..Config::default()
            };
            handle_init(&config)?;
            return Ok(());
        }
        Commands::Trawl {
            path,
            max_depth,
            include_existing,
            require_git,
            no_register,
            r#type,
            format,
            dry_run,
            blacklist,
            no_ignore,
            follow_links,
            expand_workspaces,
        } => {
            let rbac_config = loader::load()
                .map(|config| {
                    kaptaind::audit::configure_export(config.audit.export.clone());
                    kaptaind::audit::configure_governance_context(
                        config.governance.organization_id.clone(),
                        config.governance.tenant_id.clone(),
                    );
                    kaptaind::compliance::configure(config.clone());
                    config.rbac
                })
                .unwrap_or_default();
            kaptaind::rbac::check_permission(&rbac_config, "config.edit")?;

            let options = kaptaind::trawler::TrawlOptions {
                root: path.clone().unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                }),
                max_depth: *max_depth,
                skip_initialized: !include_existing,
                require_git: *require_git,
                auto_register: !no_register && !dry_run,
                filter_types: parse_project_types(r#type),
                min_confidence: 0.55, // Medium confidence minimum
                blacklist: blacklist.clone(),
                respect_ignore_files: !no_ignore,
                follow_links: *follow_links,
                expand_workspaces: *expand_workspaces,
            };
            handle_trawl(&options, format, *dry_run)?;
            return Ok(());
        }
        _ => {}
    }

    let mut config = loader::load()?;
    kaptaind::audit::configure_export(config.audit.export.clone());
    kaptaind::audit::configure_governance_context(
        config.governance.organization_id.clone(),
        config.governance.tenant_id.clone(),
    );
    kaptaind::compliance::configure(config.clone());

    if let Some(repo_override) = cli.repo {
        config.repo_path = repo_override.canonicalize().unwrap_or(repo_override);
    }

    match &cli.command {
        Commands::Status { json } => {
            if *json {
                let report = kaptaind::lifecycle::status(
                    &config.repo_path,
                    kaptaind::lifecycle::Platform::Desktop,
                )?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                handle_status(&config)?;
            }
        }
        Commands::Branch(command) => match command {
            BranchCommand::Status { json, platform } => {
                let report =
                    kaptaind::lifecycle::status(&config.repo_path, lifecycle_platform(platform)?)?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!("Kaptaind branch status\n");
                    println!(
                        "Current: {} ({:?})",
                        report.current_branch, report.semantic_branch_type
                    );
                    println!(
                        "Version: {}",
                        report.version.as_deref().unwrap_or("unknown")
                    );
                    println!("Commit: {}", report.current_commit);
                    println!(
                        "Production: {}",
                        report.production_version.as_deref().unwrap_or("unreleased")
                    );
                    println!(
                        "Development: {}",
                        report.development_version.as_deref().unwrap_or("unknown")
                    );
                    println!("Pending changes: {}", report.changes_pending);
                    println!(
                        "Promotion: {}",
                        if report.promotion_available {
                            "AVAILABLE"
                        } else {
                            "BLOCKED"
                        }
                    );
                }
            }
            BranchCommand::Init { dry_run, json } => {
                let report = kaptaind::lifecycle::init(&config.repo_path, *dry_run)?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!(
                        "Created: {}\nExisting: {}",
                        report.created.join(", "),
                        report.existing.join(", ")
                    );
                }
            }
            BranchCommand::Sync { json } => {
                let report = kaptaind::lifecycle::sync(&config.repo_path)?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!(
                        "Missing: {}",
                        if report.missing.is_empty() {
                            "none".into()
                        } else {
                            report.missing.join(", ")
                        }
                    );
                    for divergence in &report.divergences {
                        println!(
                            "DIVERGED: {} -> {} (ahead {}, behind {})",
                            divergence.source,
                            divergence.target,
                            divergence.ahead,
                            divergence.behind
                        );
                    }
                }
                if !report.divergences.is_empty() {
                    anyhow::bail!("lifecycle branches have diverged; explicit resolution required");
                }
            }
            BranchCommand::Promote {
                source,
                target,
                dry_run,
            } => {
                kaptaind::lifecycle::validate_promotion(
                    &config.repo_path,
                    &kaptaind::lifecycle::ValidationConfig {
                        build_command: config.build.command.clone(),
                        test_command: config.test.command.clone(),
                    },
                )?;
                kaptaind::lifecycle::promote(&config.repo_path, source, target, *dry_run)?;
                println!(
                    "{} {} -> {}",
                    if *dry_run {
                        "Would promote"
                    } else {
                        "Promoted"
                    },
                    source,
                    target
                );
            }
        },
        Commands::Integrate(IntegrateCommand::Analyze {
            target,
            source,
            json,
            no_persist,
        }) => {
            let report = kaptaind::integration::analyse(
                &config.repo_path,
                target,
                source,
                &config.integrations,
                !no_persist,
            )?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Integration analysis: {} -> {}", source, target);
                println!("{}", report.recommendation);
                if let Some(path) = report.persisted {
                    println!("Report: {}", path.display());
                }
            }
        }
        Commands::Pull {
            remote,
            branch,
            strategy,
            check,
            dry_run,
            force,
            autostash,
            abort,
            r#continue,
            status,
            recover,
            verbose,
            json,
        } => {
            let control_count = [*abort, *r#continue, *status, *recover]
                .into_iter()
                .filter(|enabled| *enabled)
                .count();
            if control_count > 1 {
                eprintln!(
                    "pull --abort, --continue, --status, and --recover are mutually exclusive"
                );
                std::process::exit(kaptaind::pull::ExitCode::InvalidInvocation as i32);
            }
            let result = if *abort {
                kaptaind::pull::abort(&config.repo_path).map(|()| None)
            } else if *recover {
                kaptaind::pull::recover(&config.repo_path).map(|()| None)
            } else if *status {
                match kaptaind::pull::status(&config.repo_path) {
                    Ok(value) => {
                        if *json {
                            println!("{}", serde_json::to_string_pretty(&value)?);
                        } else if let Some(value) = value {
                            println!("{}", serde_json::to_string_pretty(&value)?);
                        } else {
                            println!("No pull transactions found.");
                        }
                        Ok(None)
                    }
                    Err(error) => Err(error),
                }
            } else if *r#continue {
                kaptaind::pull::continue_operation(&config.repo_path, &config.pull).map(Some)
            } else {
                let parsed: Result<kaptaind::pull::IntegrationStrategy, _> = strategy.parse();
                let parsed_strategy = match parsed {
                    Ok(strategy) => strategy,
                    Err(error) => {
                        eprintln!("ERROR: {error}");
                        std::process::exit(error.exit_code());
                    }
                };
                kaptaind::pull::run(
                    &config.repo_path,
                    &kaptaind::pull::PullOptions {
                        remote: remote.clone(),
                        branch: branch.clone(),
                        strategy: parsed_strategy,
                        check: *check,
                        dry_run: *dry_run,
                        force: *force,
                        autostash: *autostash,
                        verbose: *verbose,
                        emit_assessment: !*json,
                    },
                    &config.pull,
                    &config.integrations,
                )
                .map(Some)
            };
            match result {
                Ok(Some(report)) => {
                    if *json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        print!("{}", kaptaind::pull::render_text(&report, *verbose));
                    }
                }
                Ok(None) if *abort || *recover => {
                    println!("Kaptaind pull transaction restored to its recovery point.");
                }
                Ok(None) => {}
                Err(error) => {
                    if *json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "schema": kaptaind::pull::JSON_SCHEMA,
                                "operation": "pull",
                                "status": "error",
                                "exit_code": error.exit_code(),
                                "error": error.to_string(),
                            })
                        );
                    } else {
                        eprintln!("ERROR: {error}");
                    }
                    std::process::exit(error.exit_code());
                }
            }
        }
        Commands::Release(command) => match command {
            LifecycleReleaseCommand::Prepare {
                version,
                source,
                dry_run,
                json,
            } => {
                let candidate = kaptaind::lifecycle::prepare_release(
                    &config.repo_path,
                    version,
                    source,
                    *dry_run,
                )?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&candidate)?);
                } else {
                    println!(
                        "{} {} @ {}",
                        if *dry_run {
                            "Would prepare"
                        } else {
                            "Prepared"
                        },
                        candidate.branch,
                        candidate.source_commit
                    );
                }
            }
            LifecycleReleaseCommand::Validate { version, json } => {
                let validation = kaptaind::lifecycle::validate_release(
                    &config.repo_path,
                    version,
                    &kaptaind::lifecycle::ValidationConfig {
                        build_command: config.build.command.clone(),
                        test_command: config.test.command.clone(),
                    },
                )?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&validation)?);
                } else {
                    println!("Validated release {} @ {}", version, validation.commit);
                }
            }
            LifecycleReleaseCommand::Issue {
                version,
                platform,
                dry_run,
                json,
            } => {
                let event = kaptaind::lifecycle::issue_release(
                    &config.repo_path,
                    version,
                    lifecycle_platform(platform)?,
                    "kaptaind-cli",
                    *dry_run,
                )?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&event)?);
                } else {
                    println!(
                        "{} release {} -> {}",
                        if *dry_run { "Would issue" } else { "Issued" },
                        version,
                        event.production_branch
                    );
                }
            }
            LifecycleReleaseCommand::Rollback {
                version,
                new_version,
                platform,
                dry_run,
                json,
            } => {
                let event = kaptaind::lifecycle::rollback(
                    &config.repo_path,
                    version,
                    new_version,
                    lifecycle_platform(platform)?,
                    "kaptaind-cli",
                    *dry_run,
                )?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&event)?);
                } else {
                    println!(
                        "{} rollback release {} restoring {}",
                        if *dry_run { "Would issue" } else { "Issued" },
                        new_version,
                        version
                    );
                }
            }
        },
        Commands::Checkout {
            channel,
            platform,
            dry_run,
        } => {
            let branch = kaptaind::lifecycle::checkout_channel(
                &config.repo_path,
                channel,
                lifecycle_platform(platform)?,
                *dry_run,
            )?;
            println!(
                "{} {} -> {}",
                if *dry_run {
                    "Would resolve"
                } else {
                    "Checked out"
                },
                channel,
                branch
            );
        }
        Commands::Suspend { reason } => {
            handle_suspend(&config, reason.as_deref())?;
        }
        Commands::Resume => {
            handle_resume(&config)?;
        }
        Commands::Validate => match config.validate() {
            Ok(()) => {
                println!("{} Configuration is valid", "✅".green());
            }
            Err(err) => {
                eprintln!("{} {}", "❌".red(), err);
                std::process::exit(1);
            }
        },
        Commands::Log { limit } => {
            handle_log(&config, *limit)?;
        }
        Commands::Analyze => {
            handle_analyze(&config)?;
        }
        Commands::Explain { last } => {
            handle_explain(&config, *last)?;
        }
        Commands::Rollback {
            commit,
            dry_run,
            yes,
        } => {
            handle_rollback(&config, commit.as_deref(), *dry_run, *yes)?;
        }
        Commands::Aoc(aoc_cmd) => {
            handle_aoc(&config, aoc_cmd)?;
        }
        Commands::Init => {
            handle_init(&config)?;
        }
        Commands::Dashboard => {
            handle_dashboard(&config)?;
        }
        Commands::CiHint { format } => {
            handle_ci_hint(&config, format)?;
        }
        Commands::EnableAutostart => {
            handle_enable_autostart()?;
        }
        Commands::DisableAutostart => {
            handle_disable_autostart()?;
        }
        Commands::Autostart => {
            handle_autostart()?;
        }
        Commands::Monitor(monitor_cmd) => {
            handle_monitor(monitor_cmd)?;
        }
        Commands::Service(service_cmd) => {
            handle_service(service_cmd)?;
        }
        Commands::Trace(trace_cmd) => {
            handle_trace(&config, trace_cmd)?;
        }
        Commands::Doctor { format } => {
            handle_doctor(&config, format)?;
        }
        Commands::Stress(stress_cmd) => match stress_cmd {
            StressCommand::Run {
                files,
                batches,
                seed,
                langs,
                format,
            } => {
                handle_stress(&config, *files, *batches, *seed, langs.clone(), format)?;
            }
        },
        Commands::Report {
            version,
            out,
            format,
            cargo_test,
            clippy,
            deny,
            container,
        } => {
            let opts = commands::report::ReportOptions {
                version: version.as_deref(),
                out: out.as_deref(),
                cargo_test: cargo_test.as_deref(),
                clippy: clippy.as_deref(),
                deny: deny.as_deref(),
                container: container.as_deref(),
            };
            handle_report(&config, &opts, format)?;
        }
        Commands::Logs(logs_cmd) => {
            let (action, format) = match logs_cmd {
                LogsCommand::Tail { n, format } => {
                    (commands::logs::LogsAction::Tail { n: *n }, format)
                }
                LogsCommand::Errors { format } => (commands::logs::LogsAction::Errors, format),
                LogsCommand::Grep { pattern, format } => (
                    commands::logs::LogsAction::Grep {
                        pattern: pattern.clone(),
                    },
                    format,
                ),
            };
            handle_logs(&config, &action, format)?;
        }
        Commands::Audit(audit_cmd) => {
            let (action, format) = match audit_cmd {
                AuditCommand::Tail { n, format } => {
                    (commands::audit::AuditAction::Tail { n: *n }, format)
                }
                AuditCommand::Stats { format } => (commands::audit::AuditAction::Stats, format),
                AuditCommand::Verify { format } => (commands::audit::AuditAction::Verify, format),
                AuditCommand::ExportVerify { format } => {
                    (commands::audit::AuditAction::ExportVerify, format)
                }
            };
            handle_audit(&config, &action, format)?;
        }
        Commands::Evidence(EvidenceCommand::Record {
            version,
            kind,
            source,
            file,
        }) => {
            kaptaind::rbac::check_permission(&config.rbac, "ship.run")?;
            commands::evidence::record(&config, version, kind, source, file)?;
        }
        Commands::Evidence(EvidenceCommand::AttachSnapshot { version, file }) => {
            kaptaind::rbac::check_permission(&config.rbac, "ship.run")?;
            commands::evidence::record_snapshot(&config, version, file)?;
        }
        Commands::Governance { format } => {
            commands::governance::handle_governance_assess(&config, format)?;
        }
        Commands::Integrations { format } => {
            commands::integrations::handle_integrations(&config, format)?;
        }
        Commands::Environment(command) => match command {
            EnvironmentCommand::Status { format } => {
                commands::environment::status(&config.repo_path, format)?;
            }
            EnvironmentCommand::Risk { format } => {
                commands::environment::risk(&config.repo_path, format)?;
            }
            EnvironmentCommand::History {
                environment,
                format,
            } => {
                commands::environment::history(&config.repo_path, environment, format)?;
            }
            EnvironmentCommand::Diff { from, to, format } => {
                commands::environment::diff(&config.repo_path, from, to, format)?;
            }
            EnvironmentCommand::Record {
                environment,
                version,
                health,
                rollout_percent,
                config_sha256,
                note,
            } => {
                commands::environment::record(
                    &config.repo_path,
                    environment,
                    version,
                    health.clone(),
                    *rollout_percent,
                    config_sha256.clone(),
                    note.clone(),
                )?;
            }
            EnvironmentCommand::Promote {
                from,
                to,
                version,
                adr,
            } => {
                commands::environment::promote(&config.repo_path, from, to, version, adr.clone())?;
            }
            EnvironmentCommand::Rollback {
                environment,
                version,
                adr,
            } => {
                commands::environment::rollback(
                    &config.repo_path,
                    environment,
                    version,
                    adr.clone(),
                )?;
            }
        },
        Commands::Probe(probe_cmd) => {
            let (action, format) = match probe_cmd {
                ProbeCommand::Health { format } => (commands::probe::ProbeAction::Health, format),
                ProbeCommand::Metrics { prometheus, format } => (
                    commands::probe::ProbeAction::Metrics {
                        prometheus: *prometheus,
                    },
                    format,
                ),
                ProbeCommand::Events { follow, format } => (
                    commands::probe::ProbeAction::Events { follow: *follow },
                    format,
                ),
            };
            handle_probe(&config, &action, format)?;
        }
        Commands::Migrate {
            check,
            strict,
            to,
            allow_lossy,
            format,
        } => {
            let args = commands::MigrateArgs {
                check: *check,
                strict: *strict,
                to: to.clone(),
                allow_lossy: *allow_lossy,
                format: format.clone(),
            };
            if !*check {
                kaptaind::rbac::check_permission(&config.rbac, "config.edit")?;
            }
            commands::schema::handle_migrate(&config.repo_path, &args)?;
        }
        Commands::Schema(schema_cmd) => match schema_cmd {
            SchemaCommand::List => commands::schema::handle_schema_list()?,
            SchemaCommand::Explain { version } => commands::schema::handle_schema_explain(version)?,
        },
        Commands::Vacs(vacs_cmd) => {
            handle_vacs(&config, vacs_cmd)?;
        }
        Commands::Storage(storage_cmd) => {
            handle_storage(&config, storage_cmd)?;
        }
        Commands::Shark(shark_cmd) => {
            match shark_cmd {
                SharkCommand::Release => {
                    kaptaind::rbac::check_permission(&config.rbac, "shark.release")?;
                }
                SharkCommand::Upgrade { .. } => {
                    kaptaind::rbac::check_permission(&config.rbac, "shark.upgrade")?;
                }
                _ => {}
            }
            handle_shark(&config, shark_cmd).await?;
        }
        Commands::Ship(ship_cmd) => {
            match ship_cmd {
                ShipCommand::Approve { .. } => {
                    kaptaind::rbac::check_permission(&config.rbac, "ship.approve")?;
                }
                ShipCommand::Status { .. } => {}
                _ => {
                    kaptaind::rbac::check_permission(&config.rbac, "ship.run")?;
                }
            }
            handle_ship(&config, ship_cmd).await?;
        }
        Commands::Trawl { .. } => {
            // Already handled above - this should not be reached
        }
    }

    Ok(())
}

fn lifecycle_platform(value: &str) -> anyhow::Result<kaptaind::lifecycle::Platform> {
    match value {
        "desktop" => Ok(kaptaind::lifecycle::Platform::Desktop),
        "mobile" => Ok(kaptaind::lifecycle::Platform::Mobile),
        _ => anyhow::bail!("unknown lifecycle platform `{value}`"),
    }
}

fn parse_project_types(type_strings: &[String]) -> Vec<kaptaind::trawler::ProjectType> {
    type_strings
        .iter()
        .filter_map(|s| match s.to_lowercase().as_str() {
            "rust" => Some(kaptaind::trawler::ProjectType::Rust),
            "node" | "nodejs" | "node.js" | "js" | "ts" => {
                Some(kaptaind::trawler::ProjectType::Node)
            }
            "python" | "py" => Some(kaptaind::trawler::ProjectType::Python),
            "go" | "golang" => Some(kaptaind::trawler::ProjectType::Go),
            "swift" => Some(kaptaind::trawler::ProjectType::Swift),
            "kotlin" | "kt" => Some(kaptaind::trawler::ProjectType::Kotlin),
            "java" => Some(kaptaind::trawler::ProjectType::Java),
            "ruby" | "rb" => Some(kaptaind::trawler::ProjectType::Ruby),
            "elixir" | "ex" | "exs" => Some(kaptaind::trawler::ProjectType::Elixir),
            "php" => Some(kaptaind::trawler::ProjectType::Php),
            "dotnet" | "csharp" | "cs" | "fsharp" | "fs" => {
                Some(kaptaind::trawler::ProjectType::Dotnet)
            }
            "cpp" | "c++" | "cxx" | "cc" => Some(kaptaind::trawler::ProjectType::Cpp),
            "lua" => Some(kaptaind::trawler::ProjectType::Lua),
            "scala" => Some(kaptaind::trawler::ProjectType::Scala),
            "clojure" | "clj" => Some(kaptaind::trawler::ProjectType::Clojure),
            "haskell" | "hs" => Some(kaptaind::trawler::ProjectType::Haskell),
            "julia" | "jl" => Some(kaptaind::trawler::ProjectType::Julia),
            "r" | "r-project" => Some(kaptaind::trawler::ProjectType::R),
            "perl" | "pl" => Some(kaptaind::trawler::ProjectType::Perl),
            _ => None,
        })
        .collect()
}

fn format_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw_args: Vec<String> = std::env::args().collect();
    let mut positions: Vec<usize> = Vec::new();
    let mut fields: Vec<Vec<String>> = Vec::new();
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--repo" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--repo=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--repo={}", v))
                    .collect(),
            );
            break;
        }
    }
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--config" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--config=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--config={}", v))
                    .collect(),
            );
            break;
        }
    }

    if fields.is_empty() || fields.iter().all(|f| f.len() <= 1) {
        return Ok(__curly_original_main()?);
    }

    let combos = curly_expand::cartesian(&fields);
    let exe = std::env::current_exe().expect("resolve current exe");
    let mut had_failure = false;
    for combo in &combos {
        let mut new_args = raw_args.clone();
        for (slot, value) in positions.iter().zip(combo.iter()) {
            new_args[*slot] = value.clone();
        }
        let status = std::process::Command::new(&exe)
            .args(&new_args[1..])
            .status()
            .expect("failed to re-exec self");
        if !status.success() {
            had_failure = true;
        }
    }
    if had_failure {
        std::process::exit(1);
    }
    Ok(())
}
