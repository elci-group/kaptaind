use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use colored::*;
mod analyze;
mod autostart;
mod monitor;
mod table;
use analyze::handle_analyze;
use autostart::{handle_disable_autostart, handle_enable_autostart};
use kaptaind::config::loader::{self, Config};
use kaptaind::daemon::scheduler::AnalysisArtifact;
use kaptaind::daemon::shark::Arbiter;
use std::fs;
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

USAGE:
    kaptaind-cli [OPTIONS] <COMMAND>

COMMANDS:
    status            View current daemon health and version
    log               View recent automated commits and analysis decisions
    analyze           Dry-run: analyze working tree without committing
    dashboard         Live dashboard: stability, releases, recent analyses
    ci-hint           Release/hold recommendation for CI/CD pipelines
    aoc               Manage Aim of Change sessions (multi-commit grouping)
    ship              Build release binaries, installers, and distribute
    init              Initialize kaptaind config for a project
    trawl             Discover and initialize codebases in a directory tree
    trace             View and manage AoC traces
    vacs              Manage Visual Asset Channel Saturation
    storage           Workspace storage management
    shark             Shark Stating: high availability leader election

OPTIONS:
    -r, --repo <PATH>    Operate on the specified repository
    -h, --help           Print help (see --help for the long version)
    -V, --version        Print version

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

DOCUMENTATION:
    https://github.com/elci-group/kaptaind
    https://github.com/elci-group/kaptaind/blob/main/README.md
    https://github.com/elci-group/kaptaind/blob/main/INSTALL.md"#
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Repository path to operate on (overrides kaptaind.toml).
    #[arg(short, long, value_name = "PATH")]
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
    Status,

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
    intercept  Wrap a command and capture its trace
    log        List completed sessions

Examples:
    kaptaind-cli aoc start "feature: auth flow"
    kaptaind-cli aoc status
    kaptaind-cli aoc ship
    kaptaind-cli aoc intercept -- npm test

Notes:
    Session state is stored in .kaptaind/aoc/active.json and archived to
    .kaptaind/aoc/manifests/<id>.json when shipped."#
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

Examples:
    kaptaind-cli trawl
    kaptaind-cli trawl --path ~/projects
    kaptaind-cli trawl --max-depth 3
    kaptaind-cli trawl --type rust,go --dry-run

Notes:
    By default, projects with an existing kaptaind.toml are skipped."#,
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
    The github format emits workflow annotations and set-output commands for
    GitHub Actions."#)]
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
    show    Display a detailed trace breakdown
    prune   Remove traces older than N days

Examples:
    kaptaind-cli trace log
    kaptaind-cli trace log --limit 20
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
    plan      Preview the ship plan without building or publishing
    run       Execute the ship pipeline
    stable    Ship a stable release from the current VERSION
    nightly   Ship a nightly prerelease with an auto-generated version
    status    Show the last ship run and scheduled auto-releases

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
    autostart fallback that runs `kaptaind-cli monitor resume` on login.
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load optional `.env` file so provider API keys and other secrets can live
    // outside of `kaptaind.toml`.
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // Init and Trawl commands work without a valid config
    match &cli.command {
        Commands::Init => {
            let rbac_config = loader::load().map(|c| c.rbac).unwrap_or_default();
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
        } => {
            let rbac_config = loader::load().map(|c| c.rbac).unwrap_or_default();
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
            };
            handle_trawl(&options, format, *dry_run)?;
            return Ok(());
        }
        _ => {}
    }

    let mut config = loader::load()?;

    if let Some(repo_override) = cli.repo {
        config.repo_path = repo_override.canonicalize().unwrap_or(repo_override);
    }

    match &cli.command {
        Commands::Status => {
            handle_status(&config)?;
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
            if !matches!(ship_cmd, ShipCommand::Status { .. }) {
                kaptaind::rbac::check_permission(&config.rbac, "ship.run")?;
            }
            handle_ship(&config, ship_cmd).await?;
        }
        Commands::Trawl { .. } => {
            // Already handled above - this should not be reached
        }
    }

    Ok(())
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

fn handle_trawl(
    options: &kaptaind::trawler::TrawlOptions,
    format: &str,
    dry_run: bool,
) -> anyhow::Result<()> {
    use colored::*;

    println!(
        "{} {}",
        "🎣".cyan(),
        "Trawling for codebases...".bold().cyan()
    );
    println!("   Root: {}", options.root.display().to_string().blue());
    if let Some(depth) = options.max_depth {
        println!("   Max depth: {}", depth.to_string().yellow());
    }
    if !options.filter_types.is_empty() {
        let types: Vec<String> = options.filter_types.iter().map(|t| t.to_string()).collect();
        println!("   Filter: {}", types.join(", ").yellow());
    }
    if dry_run {
        println!("   Mode: {}", "dry-run (no changes)".magenta());
    }
    println!();

    let start_time = std::time::Instant::now();
    let result = kaptaind::trawler::trawl(options)?;
    let elapsed = start_time.elapsed();

    if format == "json" {
        let json_output = serde_json::json!({
            "projects": result.projects.iter().map(|p| serde_json::json!({
                "path": p.path.display().to_string(),
                "type": p.project_type.to_string(),
                "is_git": p.is_git_repo,
                "is_initialized": p.is_initialized,
            })).collect::<Vec<_>>(),
            "summary": {
                "discovered": result.projects.len(),
                "initialized": result.initialized_count,
                "registered": result.registered_count,
                "skipped": result.skipped_count,
                "errors": result.errors.len(),
            },
            "elapsed_ms": elapsed.as_millis(),
        });
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        // Text format
        println!("{}", "━".repeat(60).bright_black());

        if result.projects.is_empty() {
            println!("{} {}", "ℹ️".blue(), "No projects found.".blue());
        } else {
            println!(
                "{} {}",
                "📁".cyan(),
                format!("Discovered {} project(s):", result.projects.len()).bold()
            );
            println!();

            for project in &result.projects {
                let icon = match project.project_type {
                    kaptaind::trawler::ProjectType::Rust => "🦀",
                    kaptaind::trawler::ProjectType::Node => "📦",
                    kaptaind::trawler::ProjectType::Python => "🐍",
                    kaptaind::trawler::ProjectType::Go => "🐹",
                    kaptaind::trawler::ProjectType::Swift => "🦉",
                    kaptaind::trawler::ProjectType::Kotlin => "🅺",
                    kaptaind::trawler::ProjectType::Java => "☕",
                    kaptaind::trawler::ProjectType::Ruby => "💎",
                    kaptaind::trawler::ProjectType::Elixir => "💧",
                    kaptaind::trawler::ProjectType::Php => "🐘",
                    kaptaind::trawler::ProjectType::Dotnet => "🔷",
                    kaptaind::trawler::ProjectType::Cpp => "⚙️ ",
                    kaptaind::trawler::ProjectType::Lua => "🌙",
                    kaptaind::trawler::ProjectType::Scala => "🎯",
                    kaptaind::trawler::ProjectType::Clojure => "🍃",
                    kaptaind::trawler::ProjectType::Haskell => "λ",
                    kaptaind::trawler::ProjectType::Julia => "🎨",
                    kaptaind::trawler::ProjectType::R => "📊",
                    kaptaind::trawler::ProjectType::Perl => "🐪",
                    kaptaind::trawler::ProjectType::Unknown => "❓",
                };

                let status = if project.is_initialized {
                    "✅ initialized".dimmed()
                } else {
                    "🆕 new".green()
                };

                let git_indicator = if project.is_git_repo { "🌿" } else { "  " };

                println!(
                    "  {} {} {} {} {} {}",
                    icon,
                    project.project_type.to_string().cyan(),
                    project.path.display().to_string().blue(),
                    git_indicator,
                    status,
                    if dry_run && !project.is_initialized {
                        "[would init]".yellow()
                    } else {
                        "".normal()
                    }
                );
            }

            println!();
        }

        println!("{}", "━".repeat(60).bright_black());
        println!("{} {}", "📊".cyan(), "Summary:".bold());
        println!(
            "   Discovered: {}",
            result.projects.len().to_string().yellow()
        );

        if !dry_run {
            println!(
                "   Initialized: {}",
                result.initialized_count.to_string().green()
            );
            println!(
                "   Registered: {}",
                result.registered_count.to_string().green()
            );
            println!("   Skipped: {}", result.skipped_count.to_string().dimmed());
        } else {
            let would_init = result.projects.iter().filter(|p| !p.is_initialized).count();
            println!("   Would initialize: {}", would_init.to_string().green());
        }

        if !result.errors.is_empty() {
            println!("   Errors: {}", result.errors.len().to_string().red());
        }

        println!("   Time: {:.2}s", elapsed.as_secs_f64());
        println!("{}", "━".repeat(60).bright_black());

        if !result.errors.is_empty() {
            println!();
            println!("{} {}", "⚠️".yellow(), "Errors:".yellow().bold());
            for error in &result.errors {
                println!("   - {}", error.red());
            }
        }
    }

    Ok(())
}

fn handle_vacs(config: &Config, cmd: &VacsCommand) -> anyhow::Result<()> {
    match cmd {
        VacsCommand::Show { commit } => {
            let manager = kaptaind::vacs::asset::AssetManager::new(&config.repo_path);
            let assets = manager.get_all()?;

            let filtered: Vec<_> = if let Some(c) = commit {
                assets
                    .into_iter()
                    .filter(|a| a.source_commit == *c || a.concept_id == *c)
                    .collect()
            } else {
                assets
            };

            if filtered.is_empty() {
                println!("No VACS assets found.");
            } else {
                for a in filtered {
                    println!(
                        "Asset ID: {}\nType: {}\nCommit: {}\nConcept: {}\n",
                        a.asset_id, a.asset_type, a.source_commit, a.concept_id
                    );
                }
            }
        }
        VacsCommand::Generate { asset_type } => {
            println!(
                "Manually triggering generation for type: {} is not yet supported in MVP.",
                asset_type
            );
        }
    }
    Ok(())
}

fn handle_storage(config: &Config, cmd: &StorageCommand) -> anyhow::Result<()> {
    use colored::*;

    let dh_cfg = deckhand_config_from_kaptaind(config);

    match cmd {
        StorageCommand::Clean {
            profile,
            dry_run,
            older_than,
        } => {
            println!(
                "{} {} {}",
                "🧹".cyan(),
                "Storage clean:".bold().cyan(),
                profile.yellow()
            );
            deckhand::clean::run(&dh_cfg, profile, *dry_run, *older_than, None)?;
        }
        StorageCommand::Sweep { keep_days, dry_run } => {
            println!(
                "{} {} (keep {} days)",
                "🧹".cyan(),
                "Storage sweep".bold().cyan(),
                keep_days.to_string().yellow()
            );
            deckhand::sweep::run(&dh_cfg, &config.repo_path, *dry_run, *keep_days)?;
        }
        StorageCommand::Status { json, limit } => {
            deckhand::status::run(&dh_cfg, *json, *limit)?;
        }
    }

    Ok(())
}

fn deckhand_config_from_kaptaind(config: &Config) -> deckhand::config::Config {
    use deckhand::config::{
        AutoCleanConfig, CleanConfig, StatusConfig, SweepConfig, WorkspaceConfig,
    };

    deckhand::config::Config {
        workspace: WorkspaceConfig {
            path: config.repo_path.clone(),
            members: deckhand::config::MemberSpec::Auto,
        },
        clean: CleanConfig {
            profiles: config.deckhand.clean_profiles.clone(),
            keep_incremental: false,
            keep_days: config.deckhand.clean_older_than_days.unwrap_or(0),
            languages: vec!["cargo".to_string()],
            allow_native_commands: false,
            remove_node_modules: false,
            remove_venvs: false,
        },
        sweep: SweepConfig {
            registry_cache: true,
            git_checkouts: true,
            keep_registry_days: config.deckhand.sweep_keep_days,
            node_modules: false,
            python_bytecode: false,
            go_build_cache: false,
            swift_derived_data: false,
        },
        status: StatusConfig {
            warn_free_percent: config.deckhand.min_free_percent,
        },
        auto_clean: AutoCleanConfig::default(),
    }
}

fn parse_ship_format(format: &str) -> kaptaind::release::ship::OutputFormat {
    if format.eq_ignore_ascii_case("json") {
        kaptaind::release::ship::OutputFormat::Json
    } else {
        kaptaind::release::ship::OutputFormat::Text
    }
}

async fn handle_ship(config: &Config, cmd: &ShipCommand) -> anyhow::Result<()> {
    let empty_targets = Vec::new();
    let empty_channels = Vec::new();
    let (targets, channels, format) = match cmd {
        ShipCommand::Plan {
            targets,
            channels,
            format,
            ..
        }
        | ShipCommand::Run {
            targets,
            channels,
            format,
            ..
        }
        | ShipCommand::Stable {
            targets,
            channels,
            format,
            ..
        }
        | ShipCommand::Nightly {
            targets,
            channels,
            format,
            ..
        } => (targets, channels, parse_ship_format(format)),
        ShipCommand::Status { format, .. } => {
            (&empty_targets, &empty_channels, parse_ship_format(format))
        }
    };
    let targets = if targets.is_empty() {
        None
    } else {
        Some(targets.clone())
    };
    let channels = if channels.is_empty() {
        None
    } else {
        Some(channels.clone())
    };

    match cmd {
        ShipCommand::Plan { .. } => {
            let opts = kaptaind::release::ship::ShipOptions {
                dry_run: true,
                targets,
                channels,
                force: false,
                kind: kaptaind::release::ship::ShipKind::Manual,
                version_override: None,
                require_qualification: config.ship.require_qualification,
                format,
            };
            kaptaind::release::ship::run_ship(config, opts).await?;
        }
        ShipCommand::Run { force, .. } => {
            let opts = kaptaind::release::ship::ShipOptions {
                dry_run: false,
                targets,
                channels,
                force: *force,
                kind: kaptaind::release::ship::ShipKind::Manual,
                version_override: None,
                require_qualification: config.ship.require_qualification,
                format,
            };
            kaptaind::release::ship::run_ship(config, opts).await?;
        }
        ShipCommand::Stable { dry_run, force, .. } => {
            let require_qualification = config
                .ship
                .stable
                .require_qualification
                .unwrap_or(config.ship.require_qualification);
            let opts = kaptaind::release::ship::ShipOptions {
                dry_run: *dry_run,
                targets,
                channels,
                force: *force,
                kind: kaptaind::release::ship::ShipKind::Stable,
                version_override: None,
                require_qualification: if *force { false } else { require_qualification },
                format,
            };
            kaptaind::release::ship::run_stable(config, opts).await?;
        }
        ShipCommand::Nightly {
            dry_run, no_force, ..
        } => {
            let require_qualification = config.ship.nightly.require_qualification.unwrap_or(false);
            let opts = kaptaind::release::ship::ShipOptions {
                dry_run: *dry_run,
                targets,
                channels,
                force: false,
                kind: kaptaind::release::ship::ShipKind::Nightly,
                version_override: None,
                require_qualification: if *no_force {
                    true
                } else {
                    require_qualification
                },
                format,
            };
            kaptaind::release::ship::run_nightly(config, opts).await?;
        }
        ShipCommand::Status { auto, .. } => {
            if *auto {
                kaptaind::release::ship::print_auto_ship_status(config, format)?;
            }
            kaptaind::release::ship::print_ship_status(&config.repo_path, format)?;
        }
    }

    Ok(())
}

async fn handle_shark(config: &Config, cmd: &SharkCommand) -> anyhow::Result<()> {
    use colored::*;
    use std::time::Duration;

    let arbiter_path = config.shark_arbiter_path();
    let arbiter = kaptaind::daemon::shark::FileArbiter::new(&arbiter_path)?;
    let instance_id = config.shark_instance_id();

    match cmd {
        SharkCommand::Status { json } => {
            let lease = arbiter.current_lease()?;
            if *json {
                let output = serde_json::json!({
                    "instance_id": instance_id,
                    "role": if lease.as_ref().map(|l| l.instance_id == instance_id).unwrap_or(false) {
                        "leader"
                    } else {
                        "standby"
                    },
                    "leader_id": lease.as_ref().map(|l| l.instance_id.clone()),
                    "lease_acquired_at": lease.as_ref().map(|l| l.acquired_at.to_rfc3339()),
                    "lease_renewed_at": lease.as_ref().map(|l| l.renewed_at.to_rfc3339()),
                    "lease_ttl_ms": lease.as_ref().map(|l| l.ttl_ms),
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("{} {}", "🦈".cyan(), "Shark Stating".bold().cyan());
                println!("{} {}", "Instance:".bold(), instance_id.yellow());
                let role = if lease
                    .as_ref()
                    .map(|l| l.instance_id == instance_id)
                    .unwrap_or(false)
                {
                    "leader".green()
                } else {
                    "standby".blue()
                };
                println!("{} {}", "Role:".bold(), role);
                if let Some(lease) = lease {
                    println!("{} {}", "Leader:".bold(), lease.instance_id.magenta());
                    println!(
                        "{} {}",
                        "Renewed:".bold(),
                        lease.renewed_at.to_rfc3339().dimmed()
                    );
                    println!("{} {}ms", "TTL:".bold(), lease.ttl_ms.to_string().dimmed());
                } else {
                    println!("{}", "No active lease".dimmed());
                }
            }
        }
        SharkCommand::Observe { interval_ms } => {
            println!(
                "{} {}",
                "🦈".cyan(),
                "Observing Shark Stating (Ctrl-C to stop)".bold().cyan()
            );
            let interval = Duration::from_millis(*interval_ms);
            let mut last_leader: Option<String> = None;
            loop {
                let lease = arbiter.current_lease()?;
                let leader_id = lease.as_ref().map(|l| l.instance_id.clone());
                let role = if leader_id.as_ref() == Some(&instance_id) {
                    "leader".green()
                } else if leader_id.is_some() {
                    "standby".blue()
                } else {
                    "no leader".dimmed()
                };
                if leader_id != last_leader {
                    println!(
                        "{} role={} leader={} renewed={}",
                        Utc::now().to_rfc3339(),
                        role,
                        leader_id.as_deref().unwrap_or("none").magenta(),
                        lease
                            .as_ref()
                            .map(|l| l.renewed_at.to_rfc3339())
                            .unwrap_or_default()
                            .dimmed()
                    );
                    last_leader = leader_id;
                }
                tokio::time::sleep(interval).await;
            }
        }
        SharkCommand::Release => {
            arbiter.release(&instance_id)?;
            println!(
                "{} {}",
                "🦈".cyan(),
                "Leadership released (if held by this instance)".green()
            );
        }
        SharkCommand::Upgrade {
            binary,
            standby_health_port,
            ready_timeout_ms,
        } => {
            println!(
                "{} {} {}",
                "🦈".cyan(),
                "Shark upgrade:".bold().cyan(),
                binary.display().to_string().yellow()
            );

            let current_lease = arbiter.current_lease()?;
            let leader_id = current_lease
                .as_ref()
                .map(|l| l.instance_id.clone())
                .unwrap_or_else(|| instance_id.clone());

            if current_lease
                .as_ref()
                .map(|l| l.instance_id != instance_id)
                .unwrap_or(false)
            {
                println!(
                    "{} current leader is {}; this instance is standby. Upgrade must be run from the leader.",
                    "ℹ️".blue(),
                    leader_id.blue()
                );
                return Ok(());
            }

            // Pick a health port for the standby. If the user did not supply one,
            // choose an ephemeral port by binding to 127.0.0.1:0 and reading it back.
            let standby_port = match *standby_health_port {
                Some(port) => port,
                None => {
                    let listener = std::net::TcpListener::bind("127.0.0.1:0")
                        .context("failed to bind ephemeral health port")?;
                    listener.local_addr()?.port()
                }
            };

            // Spawn standby instance.
            let mut child = kaptaind::daemon::shark::spawn_standby(
                &config.repo_path,
                binary,
                &arbiter_path,
                Some(standby_port),
            )
            .await?;
            println!(
                "{} standby spawned with pid {} (health port {})",
                "✅".green(),
                child.id(),
                standby_port
            );

            // Wait for the standby to report healthy before asking the leader to retire.
            let ready_timeout = Duration::from_millis(*ready_timeout_ms);
            if let Err(err) =
                kaptaind::daemon::shark::wait_for_standby_ready(standby_port, ready_timeout).await
            {
                let _ = child.kill();
                anyhow::bail!("standby failed to become ready: {}", err);
            }
            println!("{} standby is healthy", "✅".green());

            // Request the current leader (us) to retire.
            kaptaind::daemon::shark::request_retire(
                &arbiter_path,
                &instance_id,
                Some(standby_port),
            )?;
            println!(
                "{} retire marker written for {} (standby health port {})",
                "✅".green(),
                instance_id.yellow(),
                standby_port.to_string().dimmed()
            );

            // Wait for the standby to acquire leadership.
            let timeout = Duration::from_millis(config.shark.upgrade_handoff_timeout_ms);
            let acquired = tokio::time::timeout(timeout, async {
                loop {
                    match arbiter.current_lease() {
                        Ok(Some(lease)) if lease.instance_id != instance_id => {
                            return Ok::<_, anyhow::Error>(lease)
                        }
                        _ => {}
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            })
            .await;

            match acquired {
                Ok(Ok(lease)) => {
                    println!(
                        "{} upgrade complete; new leader is {}",
                        "🚀".green(),
                        lease.instance_id.green()
                    );
                    kaptaind::audit::log_event(
                        &config.repo_path,
                        &instance_id,
                        "shark.upgrade",
                        true,
                        serde_json::json!({
                            "new_leader": lease.instance_id,
                            "standby_health_port": standby_port,
                            "binary": binary.display().to_string(),
                        }),
                    );
                }
                _ => {
                    // Attempt to clean up the child and cancel retirement.
                    let _ = child.kill();
                    kaptaind::daemon::shark::cancel_upgrade(&arbiter_path, &instance_id);
                    eprintln!(
                        "{} upgrade handoff timed out; old leader retains control",
                        "❌".red()
                    );
                    kaptaind::audit::log_event(
                        &config.repo_path,
                        &instance_id,
                        "shark.upgrade",
                        false,
                        serde_json::json!({
                            "standby_health_port": standby_port,
                            "binary": binary.display().to_string(),
                            "error": "upgrade handoff timed out",
                        }),
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

fn handle_aoc(config: &Config, cmd: &AocCommand) -> anyhow::Result<()> {
    match cmd {
        AocCommand::Start { label } => {
            handle_aoc_start(config, label)?;
        }
        AocCommand::Ship => {
            handle_aoc_ship(config)?;
        }
        AocCommand::Status => {
            handle_aoc_status(config)?;
        }
        AocCommand::Intercept {
            model,
            intent,
            command,
            args,
        } => {
            handle_aoc_intercept(config, model.clone(), intent.clone(), command, args)?;
        }
        AocCommand::Log { limit } => {
            handle_aoc_log(config, *limit)?;
        }
    }
    Ok(())
}

fn handle_aoc_start(config: &Config, label: &str) -> anyhow::Result<()> {
    // Check if an active session already exists
    if let Ok(Some(_)) = kaptaind::aoc::session::load_active(&config.repo_path) {
        anyhow::bail!("An AoC session is already active. Run 'aoc ship' to end it.");
    }

    // Read current version
    let version_path = config.repo_path.join("VERSION");
    let initial_version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "0.1.0".to_string()
    };

    // Create new session
    let session = kaptaind::aoc::AocSession {
        id: uuid::Uuid::new_v4().to_string(),
        label: label.to_string(),
        created_at: Utc::now(),
        initial_version: initial_version.clone(),
        intent: None,
        target_stability: None,
    };

    // Save session
    kaptaind::aoc::session::save_active(&config.repo_path, &session)?;

    println!(
        "{} {} {} {}",
        "🎯".cyan(),
        "AoC started:".bold().cyan(),
        label.magenta(),
        format!("@ v{}", initial_version).blue()
    );

    Ok(())
}

fn handle_aoc_ship(config: &Config) -> anyhow::Result<()> {
    // Load active session
    let session = kaptaind::aoc::session::load_active(&config.repo_path)?
        .ok_or_else(|| anyhow::anyhow!("No active AoC session found"))?;

    // Read final version
    let version_path = config.repo_path.join("VERSION");
    let final_version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "0.1.0".to_string()
    };

    // Read traces
    let traces = kaptaind::aoc::tracer::read_traces_for_aoc(&config.repo_path, &session.id)?;

    // Count commits and test failures
    let commit_count = traces
        .iter()
        .filter(|t| matches!(t.result, kaptaind::aoc::TraceResult::Committed { .. }))
        .count();
    let test_failures = traces.iter().filter(|t| t.test.outcome == "failed").count();

    // Create manifest
    let manifest = kaptaind::aoc::AocManifest {
        id: session.id.clone(),
        label: session.label.clone(),
        created_at: session.created_at,
        shipped_at: Utc::now(),
        initial_version: session.initial_version.clone(),
        final_version: final_version.clone(),
        cluster_count: traces.len(),
        commit_count,
        test_failures,
        trace_ids: traces.iter().map(|t| t.cluster_id.clone()).collect(),
    };

    // Save manifest
    kaptaind::aoc::session::save_manifest(&config.repo_path, &manifest)?;

    // Remove active session
    kaptaind::aoc::session::remove_active(&config.repo_path)?;

    // Print summary
    println!(
        "{} {} {} {}",
        "🚢".green(),
        "AoC shipped:".bold().green(),
        session.label.magenta(),
        "✓".green()
    );
    println!("{}", "---".green());
    println!(
        "{} {} {}",
        "Version:".cyan(),
        format!("{} → {}", session.initial_version, final_version).magenta(),
        if session.initial_version != final_version {
            "✨"
        } else {
            ""
        }
        .yellow()
    );
    println!(
        "{} {}",
        "Clusters:".cyan(),
        traces.len().to_string().yellow()
    );
    println!(
        "{} {}",
        "Commits:".cyan(),
        commit_count.to_string().yellow()
    );
    println!(
        "{} {}",
        "Test Failures:".cyan(),
        format!("{}", test_failures).yellow()
    );

    Ok(())
}

fn handle_aoc_status(config: &Config) -> anyhow::Result<()> {
    match kaptaind::aoc::session::load_active(&config.repo_path)? {
        Some(session) => {
            // Count traces
            let traces =
                kaptaind::aoc::tracer::read_traces_for_aoc(&config.repo_path, &session.id)?;

            println!("{} {}", "🎯".cyan(), "Active AoC:".bold().cyan());
            println!("{}", "---".cyan());
            println!("{} {}", "Label:".cyan(), session.label.magenta());
            println!(
                "{} {}",
                "Started:".cyan(),
                format_datetime(session.created_at).blue()
            );
            println!(
                "{} {}",
                "Initial Version:".cyan(),
                session.initial_version.yellow()
            );
            println!("{} {}", "Traces:".cyan(), traces.len().to_string().yellow());
        }
        None => {
            println!("{} {}", "ℹ️".blue(), "No active AoC session.".blue());
        }
    }

    Ok(())
}

fn handle_aoc_intercept(
    config: &Config,
    model: Option<String>,
    intent: Option<String>,
    command: &str,
    args: &[String],
) -> anyhow::Result<()> {
    // Check if an active session already exists, start one if not
    let mut tmp_aoc = false;
    if kaptaind::aoc::session::load_active(&config.repo_path)?.is_none() {
        tmp_aoc = true;
        let label = intent
            .clone()
            .unwrap_or_else(|| "agent-intercept".to_string());
        handle_aoc_start(config, &label)?;
    }

    let start_time = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();

    println!(
        "{} {}",
        "🤖".cyan(),
        "Intercepting Agent execution...".bold().cyan()
    );

    // Spawn command
    let mut child = std::process::Command::new(command).args(args).spawn()?;

    let status = child.wait()?;

    let end_time = Utc::now();
    let duration = (end_time - start_time).num_milliseconds().max(0) as u64;

    // Build AgentEvent
    let agent_event = kaptaind::aoc::AgentEvent {
        id,
        timestamp: start_time,
        model: model.clone(),
        input: intent.map(serde_json::Value::String),
        output: Some(serde_json::Value::String(format!(
            "exit code: {:?}",
            status.code()
        ))),
        tools: vec![command.to_string()], // simple tool recording
        latency_ms: duration,
    };

    kaptaind::aoc::interceptor::log_event(&config.repo_path, &agent_event)?;

    println!(
        "{} {}",
        "✅".green(),
        "Agent event logged for context mapping.".bold().green()
    );

    if tmp_aoc {
        println!("{} {}", "ℹ️".blue(), "AoC session remains active for daemon to process clusters. Run 'kaptaind-cli aoc ship' later.".blue());
    }

    Ok(())
}

fn handle_aoc_log(config: &Config, limit: usize) -> anyhow::Result<()> {
    let manifests = kaptaind::aoc::session::list_manifests(&config.repo_path)?;

    if manifests.is_empty() {
        println!("No completed AoC sessions found.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = manifests
        .into_iter()
        .take(limit)
        .map(|m| {
            vec![
                m.label.magenta().to_string(),
                format!("{} → {}", m.initial_version, m.final_version)
                    .cyan()
                    .to_string(),
                m.cluster_count.to_string(),
                m.commit_count.to_string(),
                m.test_failures.to_string(),
                format_datetime(m.shipped_at).blue().to_string(),
            ]
        })
        .collect();

    table::print_table(
        &[
            "🏷️ Label",
            "📈 Version",
            "🗂️ Clusters",
            "🚀 Commits",
            "🧪 Failures",
            "🕒 Shipped",
        ],
        &rows,
    );

    Ok(())
}

fn handle_init(config: &Config) -> anyhow::Result<()> {
    let root = &config.repo_path;

    // Don't overwrite existing config
    let toml_path = root.join("kaptaind.toml");
    if toml_path.exists() {
        println!(
            "{} {}",
            "⚠️".yellow(),
            "kaptaind.toml already exists. Skipping.".yellow()
        );
        return Ok(());
    }

    let project = detect_project_type(root);

    // Generate kaptaind.toml
    let toml_content = generate_toml(&project);
    fs::write(&toml_path, &toml_content)?;
    println!(
        "{} {} {}",
        "✅".green(),
        "Created".green(),
        "kaptaind.toml".bold()
    );

    // Generate .kaptainignore
    let ignore_path = root.join(".kaptainignore");
    if !ignore_path.exists() {
        let ignore_content = generate_ignore(&project);
        fs::write(&ignore_path, &ignore_content)?;
        println!(
            "{} {} {}",
            "✅".green(),
            "Created".green(),
            ".kaptainignore".bold()
        );
    } else {
        println!(
            "{} {}",
            "⚠️".yellow(),
            ".kaptainignore already exists. Skipping.".yellow()
        );
    }

    println!(
        "\n{} {} {}",
        "🎯".cyan(),
        "Detected project type:".cyan(),
        format!("{:?}", project).bold().magenta()
    );

    // Register project for autostart/monitoring
    let _ = kaptaind::monitor::add(root, None, None, Some(true));

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Swift,
    Kotlin,
    Unknown,
}

fn detect_project_type(root: &std::path::Path) -> ProjectType {
    if root.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if root.join("Package.swift").exists() {
        ProjectType::Swift
    } else if root.join("build.gradle.kts").exists() || root.join("build.gradle").exists() {
        ProjectType::Kotlin
    } else if root.join("package.json").exists() {
        ProjectType::Node
    } else if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        ProjectType::Python
    } else if root.join("go.mod").exists() {
        ProjectType::Go
    } else {
        ProjectType::Unknown
    }
}

fn generate_toml(project: &ProjectType) -> String {
    let (test_cmd, weights) = match project {
        ProjectType::Rust => ("cargo test", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Node => ("npm test", "s = 0.30\na = 0.35\nd = 0.20\nr = 0.15"),
        ProjectType::Python => ("pytest", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Go => ("go test ./...", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Swift => ("swift test", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Kotlin => ("./gradlew test", "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15"),
        ProjectType::Unknown => (
            "echo 'no test command configured'",
            "s = 0.35\na = 0.30\nd = 0.20\nr = 0.15",
        ),
    };

    format!(
        r#"# kaptaind configuration — auto-generated by `kaptaind-cli init`

[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5
# max_paths = 0      # flush cluster after N events (0 = disabled)
# flush_after = 10   # idle timeout in seconds (defaults to window)

[weights]
{weights}

[push]
enabled = false
branch = "main"

[ratelimit]
min_commit_interval = 10

[test]
command = "{test_cmd}"
required = true

# [staging]
# mode = "all"        # "all" (default), "cluster" (only changed files), or "pattern"
# include = ["src/**"] # only used in "pattern" mode
# exclude = ["*.log", ".env*"]

# [deckhand]
# enabled = false              # enable automatic storage management
# interval_minutes = 360       # how often to run (default: 6 hours)
# sweep_keep_days = 30         # keep registry/git cache entries newer than N days
# clean_profiles = ["debug"]   # cargo profiles to clean
# clean_older_than_days = 14   # only clean artifacts older than N days (optional)
# dry_run = false              # only report what would be removed
# min_free_percent = 10        # skip pass when more than this % of disk is free

# [shark]
# enabled = false              # enable Shark Stating high availability
# arbiter_path = ".kaptaind/shark"  # shared directory for leadership leases
# heartbeat_interval_ms = 1000 # how often to renew/inspect lease
# heartbeat_timeout_ms = 5000  # how long to wait before considering leader dead
# lease_ttl_ms = 10000         # lease expiration time
# instance_id = "kaptaind-a"   # stable identifier for this instance
# upgrade_handoff_timeout_ms = 30000
# mode = "auto"                # "auto", "leader", "standby", or "observer"
"#
    )
}

fn generate_ignore(project: &ProjectType) -> String {
    let mut lines = vec![
        "# Common",
        ".git",
        ".kaptaind",
        ".DS_Store",
        "*.swp",
        "*.swo",
    ];

    match project {
        ProjectType::Rust => {
            lines.extend(["", "# Rust", "target"]);
        }
        ProjectType::Node => {
            lines.extend([
                "",
                "# Node",
                "node_modules",
                ".next",
                "dist",
                "build",
                ".turbo",
                ".vercel",
                ".output",
                "*.lock",
            ]);
        }
        ProjectType::Python => {
            lines.extend([
                "",
                "# Python",
                "__pycache__",
                ".venv",
                ".pytest_cache",
                "*.egg-info",
                "dist",
            ]);
        }
        ProjectType::Go => {
            lines.extend(["", "# Go", "vendor"]);
        }
        ProjectType::Swift => {
            lines.extend([
                "",
                "# Swift",
                ".build",
                "DerivedData",
                "*.xcodeproj/xcuserdata",
                "*.xcworkspace/xcuserdata",
                "Pods",
            ]);
        }
        ProjectType::Kotlin => {
            lines.extend([
                "",
                "# Kotlin/Gradle",
                "build",
                ".gradle",
                "*.iml",
                ".idea",
                "local.properties",
            ]);
        }
        ProjectType::Unknown => {}
    }

    lines.push(""); // trailing newline
    lines.join("\n")
}

fn handle_status(config: &Config) -> anyhow::Result<()> {
    let version_path = config.repo_path.join("VERSION");
    let version = if version_path.exists() {
        fs::read_to_string(&version_path)?.trim().to_string()
    } else {
        "None (no VERSION file)".to_string()
    };

    println!("{} {}", "🚢".blue(), "Kaptaind Status".bold().blue());
    println!("{}", "=================".blue());
    println!(
        "{} {}",
        "📂 Repository: ".bold().cyan(),
        config.repo_path.display().to_string().blue()
    );
    println!("{} {}", "🏷️  Version:    ".bold().cyan(), version.magenta());

    let pid_running = get_daemon_pid(config);
    if let Some(pid) = pid_running {
        let status_json = config.repo_path.join(".kaptaind").join("status.json");
        let mut state_display = "[🟢 Running]".green().to_string();

        if let Ok(content) = fs::read_to_string(&status_json) {
            if let Ok(report) =
                serde_json::from_str::<kaptaind::daemon::scheduler::StatusReport>(&content)
            {
                state_display = match report.status {
                    kaptaind::daemon::scheduler::State::Idle => "[💤 Idle]".blue().to_string(),
                    kaptaind::daemon::scheduler::State::Clustering => {
                        "[🔍 Clustering]".cyan().to_string()
                    }
                    kaptaind::daemon::scheduler::State::Testing => {
                        "[🧪 Testing]".yellow().to_string()
                    }
                    kaptaind::daemon::scheduler::State::Committing => {
                        "[🚢 Committing]".magenta().to_string()
                    }
                    kaptaind::daemon::scheduler::State::Failed => "[🛑 Failed]".red().to_string(),
                    kaptaind::daemon::scheduler::State::Stopping => {
                        "[⏹️  Stopping]".yellow().to_string()
                    }
                    kaptaind::daemon::scheduler::State::Stopped => {
                        "[⏹️  Stopped]".bright_black().to_string()
                    }
                };
            }
        }

        println!(
            "{} {} {}",
            "⚙️  Daemon:     ".bold().cyan(),
            state_display,
            format!("(PID: {})", pid).blue()
        );
    } else {
        println!(
            "{} {}",
            "⚙️  Daemon:     ".bold().cyan(),
            "🛑 Stopped".red()
        );
    }

    Ok(())
}

fn get_daemon_pid(config: &Config) -> Option<i32> {
    let pid_file = config.repo_path.join(".kaptaind").join("daemon.pid");
    if let Ok(pid_str) = fs::read_to_string(pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            // Signal 0 checks if the process is running and we have permissions to signal it.
            if unsafe { libc::kill(pid, 0) } == 0 {
                return Some(pid);
            }
        }
    }
    None
}

struct LogRow {
    version: String,
    bump: String,
    score: String,
    paths: usize,
    api_touches: usize,
    api_added: String,
    api_break: String,
    events: usize,
    date: String,
    id: String,
}

fn handle_log(config: &Config, limit: usize) -> anyhow::Result<()> {
    let analysis_dir = config.repo_path.join(".kaptaind").join("analysis");
    if !analysis_dir.exists() {
        println!("No analysis history found in {}", analysis_dir.display());
        return Ok(());
    }

    let mut artifacts = Vec::new();
    for entry in fs::read_dir(analysis_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(artifact) = serde_json::from_str::<AnalysisArtifact>(&content) {
                    artifacts.push(artifact);
                }
            }
        }
    }

    artifacts.sort_by_key(|a| std::cmp::Reverse(a.ended_at));
    artifacts.truncate(limit);

    if artifacts.is_empty() {
        println!("History is empty.");
        return Ok(());
    }

    let rows: Vec<LogRow> = artifacts
        .into_iter()
        .map(|a| {
            let bump_display = match a.bump.as_str() {
                "Major" => "🚀 Major".red().bold().to_string(),
                "Minor" => "✨ Minor".cyan().bold().to_string(),
                "Patch" => "🩹 Patch".green().to_string(),
                _ => "📌 Stable".blue().to_string(),
            };

            LogRow {
                version: a.version.magenta().to_string(),
                bump: bump_display,
                score: format!("{:.3}", a.weight.score).yellow().to_string(),
                paths: a.diff.touched_paths,
                api_touches: a.diff.api_touches,
                api_added: if a.diff.api_added {
                    "Yes".green().to_string()
                } else {
                    "No".to_string()
                },
                api_break: if a.diff.api_breaking {
                    "Yes".red().bold().to_string()
                } else {
                    "No".to_string()
                },
                events: a.event_count,
                date: format_datetime(a.ended_at),
                id: a.cluster_id.chars().take(8).collect(),
            }
        })
        .collect();

    let table_rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|row| {
            vec![
                row.version,
                row.bump,
                row.score,
                row.paths.to_string(),
                row.api_touches.to_string(),
                row.api_added,
                row.api_break,
                row.events.to_string(),
                row.date,
                row.id,
            ]
        })
        .collect();

    table::print_table(
        &[
            "🏷️ Version",
            "📈 Bump",
            "🎯 Score",
            "🗂️ Paths",
            "🔌 API Touches",
            "➕ API Added",
            "💥 API Break",
            "⚡ Events",
            "🕒 Date",
            "🆔 ID",
        ],
        &table_rows,
    );

    Ok(())
}

fn format_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn handle_dashboard(config: &Config) -> anyhow::Result<()> {
    let kd = config.repo_path.join(".kaptaind");

    // --- Version ---
    let version = fs::read_to_string(config.repo_path.join("VERSION"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // --- Daemon status ---
    let daemon_state = fs::read_to_string(kd.join("status.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::daemon::scheduler::StatusReport>(&s).ok());

    // --- Telemetry ---
    let telemetry = fs::read_to_string(kd.join("telemetry.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::daemon::telemetry::TokenMetrics>(&s).ok());

    // --- Stability ---
    let stability = fs::read_to_string(kd.join("stability.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::stability::model::StabilityRecord>(&s).ok());

    // --- Releases index ---
    let release_index = fs::read_to_string(kd.join("releases").join("index.json"))
        .ok()
        .and_then(|s| {
            serde_json::from_str::<kaptaind::release::orchestrator::ReleaseIndex>(&s).ok()
        });

    // --- Recent analyses ---
    let analysis_dir = kd.join("analysis");
    let mut recent_analyses: Vec<kaptaind::daemon::scheduler::AnalysisArtifact> = Vec::new();
    if analysis_dir.exists() {
        let mut entries: Vec<_> = fs::read_dir(&analysis_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        for entry in entries.iter().take(5) {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(a) =
                    serde_json::from_str::<kaptaind::daemon::scheduler::AnalysisArtifact>(&content)
                {
                    recent_analyses.push(a);
                }
            }
        }
    }

    // ======= Render =======
    println!();
    println!(
        "{}",
        "╔══════════════════════════════════════════════╗".cyan()
    );
    println!(
        "{}",
        "║          kaptaind  ·  Live Dashboard         ║"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════╝".cyan()
    );
    println!();

    // Version + daemon
    println!(
        "{}",
        "── Project ─────────────────────────────────────".bright_black()
    );
    println!("  {}  {}", "Version:".bold(), version.magenta().bold());
    println!(
        "  {}  {}",
        "Repo:   ".bold(),
        config.repo_path.display().to_string().blue()
    );
    if let Some(ref st) = daemon_state {
        let state_str = match st.status {
            kaptaind::daemon::scheduler::State::Idle => "Idle".green().to_string(),
            kaptaind::daemon::scheduler::State::Clustering => "Clustering".cyan().to_string(),
            kaptaind::daemon::scheduler::State::Testing => "Testing".yellow().to_string(),
            kaptaind::daemon::scheduler::State::Committing => "Committing".magenta().to_string(),
            kaptaind::daemon::scheduler::State::Failed => "Failed".red().bold().to_string(),
            kaptaind::daemon::scheduler::State::Stopping => "Stopping".yellow().to_string(),
            kaptaind::daemon::scheduler::State::Stopped => "Stopped".bright_black().to_string(),
        };
        println!("  {}  {}", "Daemon: ".bold(), state_str);
        if let Some(ref err) = st.last_error {
            println!("  {}  {}", "Error:  ".bold(), err.red());
        }
    } else {
        println!(
            "  {}  {}",
            "Daemon: ".bold(),
            "Not running / no status file".bright_black()
        );
    }
    println!();

    // Stability
    println!(
        "{}",
        "── Stability ───────────────────────────────────".bright_black()
    );
    if let Some(ref s) = stability {
        let bar = stability_bar(s.score);
        let score_colored = if s.score >= 0.85 {
            format!("{:.3}", s.score).green().bold().to_string()
        } else if s.score >= 0.6 {
            format!("{:.3}", s.score).yellow().to_string()
        } else {
            format!("{:.3}", s.score).red().to_string()
        };
        println!(
            "  Score:  {} {}  {}",
            bar,
            score_colored,
            format!("({} commits tracked)", s.history.len()).bright_black()
        );
        if let Some(reg_ts) = s.last_regression {
            let reg_dt = chrono::DateTime::<chrono::Utc>::from_timestamp(reg_ts, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("  Last regression: {}", reg_dt.yellow());
        }
    } else {
        println!("  {}", "No stability data yet.".bright_black());
    }
    println!();

    // Telemetry
    println!(
        "{}",
        "── Telemetry ───────────────────────────────────".bright_black()
    );
    if let Some(ref t) = telemetry {
        println!(
            "  {}  ${:.4}  (${:.6} this session)",
            "LLM cost:".bold(),
            t.aggregate_cost,
            t.marginal_cost
        );
        println!(
            "  {}  {}  failed: {}",
            "Releases:".bold(),
            t.releases.to_string().green(),
            t.failed_releases.to_string().red()
        );
    } else {
        println!("  {}", "No telemetry data.".bright_black());
    }
    println!();

    // Recent releases
    println!(
        "{}",
        "── Releases ────────────────────────────────────".bright_black()
    );
    if let Some(ref idx) = release_index {
        if idx.releases.is_empty() {
            println!("  {}", "No releases yet.".bright_black());
        } else {
            for entry in idx.releases.iter().rev().take(5) {
                let ts = chrono::DateTime::<chrono::Utc>::from_timestamp(entry.released_at, 0)
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                println!(
                    "  {} {}  {}  S={:.3}",
                    "▸".green(),
                    format!("v{}", entry.version).magenta().bold(),
                    ts.bright_black(),
                    entry.stability
                );
            }
        }
    } else {
        println!("  {}", "No release index found.".bright_black());
    }
    println!();

    // Recent analyses
    println!(
        "{}",
        "── Recent Analyses ─────────────────────────────".bright_black()
    );
    if recent_analyses.is_empty() {
        println!("  {}", "No analyses yet.".bright_black());
    } else {
        for a in &recent_analyses {
            let bump_sym = match a.bump.as_str() {
                "Major" => "🚀".to_string(),
                "Minor" => "✨".to_string(),
                "Patch" => "🩹".to_string(),
                _ => "─".to_string(),
            };
            println!(
                "  {} {}  score={:.3}  bump={}{}  paths={}",
                bump_sym,
                a.version.magenta(),
                a.weight.score,
                a.bump.cyan(),
                if a.weight.api_breaking {
                    " [BREAKING]".red().to_string()
                } else {
                    String::new()
                },
                a.diff.touched_paths
            );
        }
    }
    println!();

    Ok(())
}

fn stability_bar(score: f64) -> String {
    let filled = (score * 20.0).round() as usize;
    let empty = 20usize.saturating_sub(filled);
    let bar: String = std::iter::repeat_n('█', filled)
        .chain(std::iter::repeat_n('░', empty))
        .collect();
    format!("[{}]", bar)
}

fn handle_ci_hint(config: &Config, format: &str) -> anyhow::Result<()> {
    let kd = config.repo_path.join(".kaptaind");

    let stability = fs::read_to_string(kd.join("stability.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<kaptaind::stability::model::StabilityRecord>(&s).ok());

    let release_index = fs::read_to_string(kd.join("releases").join("index.json"))
        .ok()
        .and_then(|s| {
            serde_json::from_str::<kaptaind::release::orchestrator::ReleaseIndex>(&s).ok()
        });

    let current_score = stability.as_ref().map(|s| s.score).unwrap_or(0.0);
    let pass_streak = stability
        .as_ref()
        .map(kaptaind::stability::engine::pass_streak)
        .unwrap_or(0);
    let threshold = config.qualification.stability_threshold;
    let min_streak = config.qualification.min_pass_streak;

    let qualified = current_score >= threshold && pass_streak >= min_streak;
    let last_version = release_index
        .as_ref()
        .and_then(|idx| idx.releases.last())
        .map(|e| e.version.clone())
        .unwrap_or_else(|| "none".to_string());
    let current_version = fs::read_to_string(config.repo_path.join("VERSION"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    match format {
        "json" => {
            let out = serde_json::json!({
                "qualified": qualified,
                "stability_score": current_score,
                "pass_streak": pass_streak,
                "threshold": threshold,
                "min_streak": min_streak,
                "current_version": current_version,
                "last_released_version": last_version,
                "recommendation": if qualified { "release" } else { "hold" }
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        "github" => {
            // GitHub Actions workflow command format
            if qualified {
                println!("::notice title=kaptaind::Release qualified — v{current_version} (stability={current_score:.3}, streak={pass_streak})");
                println!("::set-output name=qualified::true");
                println!("::set-output name=version::{current_version}");
            } else {
                println!("::warning title=kaptaind::Hold — stability={current_score:.3} (need {threshold:.3}), streak={pass_streak} (need {min_streak})");
                println!("::set-output name=qualified::false");
                println!("::set-output name=version::{current_version}");
            }
        }
        _ => {
            // Plain text
            let status_str = if qualified {
                "RELEASE".green().bold().to_string()
            } else {
                "HOLD".yellow().bold().to_string()
            };
            println!("{} {}", "CI Hint:".bold(), status_str);
            println!(
                "  Stability score : {:.3}  (threshold: {:.3})",
                current_score, threshold
            );
            println!(
                "  Pass streak     : {}  (required: {})",
                pass_streak, min_streak
            );
            println!("  Current version : {}", current_version.magenta());
            println!("  Last release    : {}", last_version.blue());
            if qualified {
                println!(
                    "  → Recommendation: {}",
                    "ship v".green().to_string() + &current_version
                );
            } else {
                let missing_score = (threshold - current_score).max(0.0);
                let missing_streak = min_streak.saturating_sub(pass_streak);
                if missing_score > 0.001 {
                    println!("  → Need +{:.3} stability score to qualify", missing_score);
                }
                if missing_streak > 0 {
                    println!(
                        "  → Need {} more passing commit(s) in streak",
                        missing_streak
                    );
                }
            }
        }
    }

    Ok(())
}

fn handle_autostart() -> anyhow::Result<()> {
    monitor::resume()
}

fn handle_monitor(cmd: &MonitorCommand) -> anyhow::Result<()> {
    match cmd {
        MonitorCommand::Add {
            path,
            config,
            port,
            enabled,
        } => {
            let project_path = path
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            monitor::add(&project_path, config.as_deref(), *port, *enabled)?;
            println!(
                "{} {} {}",
                "✅".green(),
                "Registered".green(),
                project_path.display().to_string().blue()
            );
        }
        MonitorCommand::Remove { path } => {
            if monitor::remove(path)? {
                println!(
                    "{} {} {}",
                    "✅".green(),
                    "Removed".green(),
                    path.display().to_string().blue()
                );
            } else {
                anyhow::bail!("Project not registered: {}", path.display());
            }
        }
        MonitorCommand::List => {
            monitor::list()?;
        }
        MonitorCommand::Enable { path } => {
            monitor::set_enabled(path, true)?;
            println!(
                "{} {} {}",
                "✅".green(),
                "Enabled".green(),
                path.display().to_string().blue()
            );
        }
        MonitorCommand::Disable { path } => {
            monitor::set_enabled(path, false)?;
            println!(
                "{} {} {}",
                "✅".green(),
                "Disabled".green(),
                path.display().to_string().blue()
            );
        }
        MonitorCommand::Resume => {
            monitor::resume()?;
        }
    }
    Ok(())
}

fn handle_service(cmd: &ServiceCommand) -> anyhow::Result<()> {
    match cmd {
        ServiceCommand::Install { user, system } => {
            monitor::install_service(*user, *system)?;
        }
        ServiceCommand::Uninstall { user, system } => {
            monitor::uninstall_service(*user, *system)?;
        }
        ServiceCommand::InstallIcon { user, system } => {
            let target = kaptaind::icon::install_icon(*user, *system)?;
            println!(
                "{} {} {}",
                "✓".green(),
                "Logo installed to".green(),
                target.display().to_string().blue()
            );
            println!("  Notifications and launchers can now reference the icon by name: kaptaind");
        }
        ServiceCommand::Status { user, system } => {
            monitor::service_status(*user, *system)?;
        }
    }
    Ok(())
}

fn handle_trace(config: &Config, cmd: &TraceCommand) -> anyhow::Result<()> {
    match cmd {
        TraceCommand::Log { aoc_id, limit } => {
            handle_trace_log(config, aoc_id.as_deref(), *limit)?;
        }
        TraceCommand::Show { cluster_id } => {
            handle_trace_show(config, cluster_id)?;
        }
        TraceCommand::Prune { days } => {
            handle_trace_prune(config, *days)?;
        }
    }
    Ok(())
}

fn handle_trace_log(config: &Config, aoc_id: Option<&str>, limit: usize) -> anyhow::Result<()> {
    let target_aoc_id = match aoc_id {
        Some(id) => id.to_string(),
        None => {
            let session =
                kaptaind::aoc::session::load_active(&config.repo_path)?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "No active AoC session found. Provide --aoc-id or start a session."
                    )
                })?;
            session.id
        }
    };

    let traces = kaptaind::aoc::db::get_traces_for_aoc(&config.repo_path, &target_aoc_id)?;

    println!(
        "{} {} {}",
        "📜".cyan(),
        "Traces for AoC:".bold(),
        target_aoc_id.magenta()
    );
    println!("{}", "-".repeat(80).cyan());

    let rows: Vec<Vec<String>> = traces
        .iter()
        .rev()
        .take(limit)
        .map(|t| {
            let result = match &t.result {
                kaptaind::aoc::TraceResult::Committed { bump, version } => {
                    format!("✅ {} ({})", bump, version).green().to_string()
                }
                kaptaind::aoc::TraceResult::Skipped { reason } => {
                    format!("⏭️  Skipped ({})", reason).yellow().to_string()
                }
            };

            vec![
                t.cluster_id[..8].to_string(),
                t.started_at.format("%H:%M:%S").to_string(),
                format!("{}ms", t.duration_ms),
                result,
            ]
        })
        .collect();

    if rows.is_empty() {
        println!("No traces found for this session.");
    } else {
        table::print_table(&["ID", "Time", "Duration", "Result"], &rows);
    }

    Ok(())
}

fn handle_trace_show(config: &Config, cluster_id: &str) -> anyhow::Result<()> {
    let db_path = config.repo_path.join(".kaptaind").join("traces.db");
    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt =
        conn.prepare("SELECT data FROM traces WHERE cluster_id = ?1 OR cluster_id LIKE ?2")?;

    let pattern = format!("{}%", cluster_id);
    let mut rows = stmt.query([cluster_id, &pattern])?;

    if let Some(row) = rows.next()? {
        let data: String = row.get(0)?;
        let trace: kaptaind::aoc::TraceRecord = serde_json::from_str(&data)?;

        println!(
            "{} {} {}",
            "🔬".cyan(),
            "Trace:".bold(),
            trace.cluster_id.magenta()
        );
        println!("{} {}", "AoC ID:".bold(), trace.aoc_id);
        println!("{} {}", "Started:".bold(), trace.started_at);
        println!("{} {}ms", "Duration:".bold(), trace.duration_ms);
        println!("{} {}", "Test:".bold(), trace.test.outcome);

        match &trace.result {
            kaptaind::aoc::TraceResult::Committed { bump, version } => {
                println!("{} {} ({})", "Result:".bold(), bump.green(), version.blue());
            }
            kaptaind::aoc::TraceResult::Skipped { reason } => {
                println!(
                    "{} {}",
                    "Result:".bold(),
                    format!("Skipped ({})", reason).yellow()
                );
            }
        }

        println!("\n{}", "📂 Touched Paths:".bold());
        for event in &trace.events {
            for path in &event.paths {
                println!(
                    "  {} {}",
                    match event.kind.as_str() {
                        "create" => "+".green(),
                        "modify" => "M".yellow(),
                        "remove" => "-".red(),
                        _ => "?".blue(),
                    },
                    path
                );
            }
        }

        if let Some(agent) = &trace.agent_event {
            println!("\n{}", "🤖 Agent Event:".bold());
            println!("  Model:   {}", agent.model.as_deref().unwrap_or("unknown"));
            println!("  Latency: {}ms", agent.latency_ms);
            println!("  Tools:   {}", agent.tools.join(", "));
        }
    } else {
        anyhow::bail!("Trace not found: {}", cluster_id);
    }

    Ok(())
}

fn handle_trace_prune(config: &Config, days: i64) -> anyhow::Result<()> {
    let deleted = kaptaind::aoc::db::prune_old_traces(&config.repo_path, days)?;
    println!(
        "{} {} traces older than {} days.",
        "🧹".green(),
        deleted,
        days
    );
    Ok(())
}
