% KAPTAIND(1) kaptaind 9.6.3
% Elci Group
% July 2026

# NAME

**kaptaind** — automated semantic versioning daemon

# SYNOPSIS

**kaptaind** [**OPTIONS**]

**kaptaind** **--daemon**

**kaptaind** **--dock** | **--radar** | **--lanes**

# DESCRIPTION

**kaptaind** is a self-governing release companion that watches a repository for filesystem changes, clusters related events, analyzes the change set across structural, API, dependency, runtime, and bundle dimensions, computes a semantic-version bump, writes the **VERSION** file, persists analysis artifacts, creates a git commit, and optionally pushes.

Run **kaptaind** in the foreground to see logs directly, or pass **--daemon** to detach and run under the project's **.kaptaind/** directory.

The daemon's companion tool is **kaptaind-cli**(1).

# OPTIONS

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

**-V**, **--version**
:   Print the version and exit.

**-h**, **--help**
:   Print help information and exit.

# FILES

*./kaptaind.toml*
:   Main configuration file for the watched repository. Created automatically by **kaptaind-cli init**.

*.kaptainignore*
:   Per-repository ignore rules. Blank lines and **#** comments are ignored. Entries containing glob metacharacters are treated as glob patterns; otherwise they are treated as exact relative paths or prefixes.

*.kaptaind/*
:   Runtime directory for analysis artifacts, status, telemetry, traces, Aim-of-Change manifests, bundle metadata, and daemon logs/PID files.

*VERSION*
:   Authoritative semantic version for the repository. Created automatically when missing, starting from **0.1.0**.

# ENVIRONMENT

**KAPTAIND_CONFIG**
:   Path to the configuration file. Equivalent to **--config**. If unset, **kaptaind** looks for **./kaptaind.toml**.

**RUST_LOG**
:   Set the tracing log level, e.g. **debug**, **info**, **warn**, or **error**. Default is **info**.

# EXIT STATUS

**0**
:   Success.

**1**
:   General error, such as a missing configuration file, invalid config, or startup failure.

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

    kaptaind --web

Use a custom WebUI port:

    kaptaind --web --web-port 8080 --health-port 9090

# SEE ALSO

**kaptaind-cli**(1)
