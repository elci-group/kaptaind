//! Mutation-readiness evaluation (`kaptaind ready`).
//!
//! kaptaind guards repository mutation behind several independent gates
//! (operation mode, execution trust, push switches, protected branches, the
//! startup guard, the version baseline, ...). Discovering them one refusal
//! at a time costs a round-trip each, so [`evaluate`] checks every gate at
//! once and reports each one's state together with its exact fix.
//!
//! Evaluation is strictly read-only: it never creates `.kaptaind/`, never
//! removes stale pid files, and never writes `VERSION`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Serialize;

use crate::config::loader::{Config, OperationMode, VersionBootstrap, WorkspacePolicy};

/// Hint appended to gate refusals elsewhere in kaptaind so a single refusal
/// points at the command that lists every blocker.
pub const READY_HINT: &str = "run 'kaptaind ready' for every blocker";

/// What the caller intends to do; decides which gates are required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Score changes and record a decision (no mutation).
    Analyze,
    /// Stage, write VERSION, and commit.
    Commit,
    /// Commit and push to a remote.
    Push,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Analyze => "analyze",
            Purpose::Commit => "commit",
            Purpose::Push => "push",
        }
    }

    fn mutates(self) -> bool {
        !matches!(self, Purpose::Analyze)
    }
}

impl fmt::Display for Purpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Purpose {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "analyze" | "analyse" => Ok(Purpose::Analyze),
            "commit" => Ok(Purpose::Commit),
            "push" => Ok(Purpose::Push),
            other => anyhow::bail!("unknown purpose {other:?}: expected analyze, commit, or push"),
        }
    }
}

/// State of one gate for the evaluated purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GateState {
    /// The gate is open.
    #[serde(rename = "pass")]
    Pass,
    /// The gate is closed and required for this purpose.
    #[serde(rename = "block")]
    Block,
    /// Not required, but likely to cause surprises.
    #[serde(rename = "warn")]
    Warn,
    /// Informational only.
    #[serde(rename = "info")]
    Info,
    /// Does not apply to this purpose.
    #[serde(rename = "n/a")]
    NotApplicable,
}

impl GateState {
    pub fn as_str(self) -> &'static str {
        match self {
            GateState::Pass => "pass",
            GateState::Block => "block",
            GateState::Warn => "warn",
            GateState::Info => "info",
            GateState::NotApplicable => "n/a",
        }
    }
}

impl fmt::Display for GateState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One evaluated gate. `id` values are stable snake_case identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gate {
    pub id: &'static str,
    pub state: GateState,
    pub detail: String,
    /// The precise kaptaind.toml key/value or command that opens the gate.
    pub fix: Option<String>,
}

impl Gate {
    fn new(id: &'static str, state: GateState, detail: impl Into<String>) -> Self {
        Self {
            id,
            state,
            detail: detail.into(),
            fix: None,
        }
    }

    fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
}

/// Stable gate identifiers, in evaluation order.
pub const GATE_IDS: &[&str] = &[
    "operation_mode",
    "execution_trust",
    "test_hook",
    "push_enabled",
    "network_push",
    "protected_branch",
    "startup_guard",
    "version_baseline",
    "kaptaind_dir_writable",
    "nested_git_root",
    "suspended",
    "daemon_running",
];

/// `true` when no gate blocks — i.e. every gate required by the purpose
/// passed (or does not apply).
pub fn is_ready(gates: &[Gate]) -> bool {
    gates.iter().all(|g| g.state != GateState::Block)
}

/// Evaluate every mutation gate for `purpose`. Read-only.
pub fn evaluate(config: &Config, purpose: Purpose) -> Vec<Gate> {
    let repo = config.repo_path.as_path();
    vec![
        operation_mode(config, purpose),
        execution_trust(config, purpose),
        test_hook(config, purpose),
        push_enabled(config, purpose),
        network_push(config, purpose),
        protected_branch(config, purpose),
        startup_guard(config, purpose),
        version_baseline(config, purpose),
        kaptaind_dir_writable(repo, purpose),
        nested_git_root(repo),
        suspended(repo, purpose),
        daemon_running(repo, purpose),
    ]
}

/// Block when the purpose requires the gate, otherwise warn.
fn required_or_warn(required: bool) -> GateState {
    if required {
        GateState::Block
    } else {
        GateState::Warn
    }
}

fn operation_mode(config: &Config, purpose: Purpose) -> Gate {
    match config.operation.mode {
        OperationMode::Actuate => Gate::new(
            "operation_mode",
            GateState::Pass,
            "[operation] mode = \"actuate\": mutations allowed",
        ),
        OperationMode::Observe if purpose.mutates() => Gate::new(
            "operation_mode",
            GateState::Block,
            "[operation] mode = \"observe\": changes are scored and recorded as \"observed\", \
             nothing is staged, committed, VERSION-written, or pushed",
        )
        .fix("set [operation] mode = \"actuate\" in kaptaind.toml"),
        OperationMode::Observe => Gate::new(
            "operation_mode",
            GateState::Pass,
            "[operation] mode = \"observe\": analysis is allowed in observe mode",
        ),
    }
}

fn execution_trust(config: &Config, purpose: Purpose) -> Gate {
    // Reuse the loader's own check so the gate and the refusal never drift.
    match config.validate_execution_trust() {
        Ok(()) => {
            let detail = if config.execution_trust_violations().is_empty()
                && config.trust.execution != crate::config::loader::ExecutionTrust::Trusted
            {
                "[trust] execution = \"untrusted\", but nothing in the config executes a program"
            } else {
                "[trust] execution = \"trusted\": configured commands may run"
            };
            Gate::new("execution_trust", GateState::Pass, detail)
        }
        Err(err) => Gate::new(
            "execution_trust",
            required_or_warn(purpose.mutates()),
            err.to_string(),
        )
        .fix("review the configured commands, then set [trust] execution = \"trusted\" in kaptaind.toml"),
    }
}

/// Shell builtins/keywords that are always "resolvable" under `sh -lc`.
const SHELL_BUILTINS: &[&str] = &[
    ":", "true", "false", "exit", "cd", "echo", "test", "[", "set", "export", "printf", "exec",
    "command", "if", "for", "while", "{", "(",
];

/// The program a `sh -lc <command>` invocation would execute first, skipping
/// leading `VAR=value` assignments. `None` for an empty command.
pub(crate) fn first_program(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .find(|tok| !is_env_assignment(tok))
        .map(|tok| tok.trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|tok| !tok.is_empty())
}

fn is_env_assignment(tok: &str) -> bool {
    match tok.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        None => false,
    }
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Resolve `program` the way `sh` would: a path containing `/` is taken
/// relative to `repo` (the hook's working directory); a bare name is looked
/// up on `PATH`. Builtins resolve to `None` inside `Ok`.
pub(crate) fn resolve_program(program: &str, repo: &Path) -> Result<Option<PathBuf>, String> {
    if SHELL_BUILTINS.contains(&program) {
        return Ok(None);
    }
    if program.contains('/') {
        let path = if Path::new(program).is_absolute() {
            PathBuf::from(program)
        } else {
            repo.join(program)
        };
        return if is_executable(&path) {
            Ok(Some(path))
        } else if path.exists() {
            Err(format!(
                "{} exists but is not an executable file",
                path.display()
            ))
        } else {
            Err(format!("{} not found (ENOENT)", path.display()))
        };
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(program);
        if is_executable(&candidate) {
            return Ok(Some(candidate));
        }
    }
    Err(format!("'{program}' not found on PATH (ENOENT)"))
}

fn test_hook(config: &Config, purpose: Purpose) -> Gate {
    let Some(command) = config
        .test
        .command
        .as_deref()
        .filter(|c| !c.trim().is_empty())
    else {
        return Gate::new(
            "test_hook",
            GateState::NotApplicable,
            "no [test] command configured",
        );
    };
    // The test hook only runs ahead of a commit.
    let required = purpose.mutates() && config.test.required;
    let Some(program) = first_program(command) else {
        return Gate::new(
            "test_hook",
            GateState::NotApplicable,
            "empty [test] command",
        );
    };
    match resolve_program(&program, &config.repo_path) {
        Ok(resolved) => {
            let location = resolved
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "shell builtin".to_string());
            Gate::new(
                "test_hook",
                GateState::Pass,
                format!("[test] command = {command:?}: '{program}' resolves to {location}"),
            )
        }
        Err(why) => {
            let state = if purpose.mutates() {
                required_or_warn(required)
            } else {
                GateState::Info
            };
            let required_note = if config.test.required {
                "required hook: every commit would be refused"
            } else {
                "optional hook: commits proceed but tests never run"
            };
            Gate::new(
                "test_hook",
                state,
                format!("[test] command = {command:?}: {why}; {required_note}"),
            )
            .fix(format!(
                "install '{program}' on PATH, or set [test] command = \"<resolvable command>\" \
                 (or [test] required = false) in kaptaind.toml"
            ))
        }
    }
}

fn push_enabled(config: &Config, purpose: Purpose) -> Gate {
    if purpose != Purpose::Push {
        return Gate::new(
            "push_enabled",
            GateState::NotApplicable,
            format!(
                "[push] enabled = {} (only needed for push)",
                config.push.enabled
            ),
        );
    }
    if config.push.enabled {
        Gate::new("push_enabled", GateState::Pass, "[push] enabled = true")
    } else {
        Gate::new(
            "push_enabled",
            GateState::Block,
            "[push] enabled = false: commits are never pushed",
        )
        .fix("set [push] enabled = true in kaptaind.toml")
    }
}

fn network_push(config: &Config, purpose: Purpose) -> Gate {
    if purpose != Purpose::Push {
        return Gate::new(
            "network_push",
            GateState::NotApplicable,
            format!(
                "[capabilities] network_push = {} (only needed for push)",
                config.capabilities.network_push
            ),
        );
    }
    if config.capabilities.network_push {
        Gate::new(
            "network_push",
            GateState::Pass,
            "[capabilities] network_push = true",
        )
    } else {
        Gate::new(
            "network_push",
            GateState::Block,
            "[capabilities] network_push = false: network pushes are disabled",
        )
        .fix("set [capabilities] network_push = true in kaptaind.toml (incompatible with air_gapped = true)")
    }
}

fn protected_branch(config: &Config, purpose: Purpose) -> Gate {
    let protected = &config.push.safety.protect_branches;
    if purpose != Purpose::Push {
        return Gate::new(
            "protected_branch",
            GateState::NotApplicable,
            "only checked for push",
        );
    }
    let branch = match crate::integration::current_branch(&config.repo_path) {
        Ok(b) if !b.trim().is_empty() => b.trim().to_string(),
        Ok(_) => {
            return Gate::new(
                "protected_branch",
                GateState::Warn,
                format!(
                    "detached HEAD: the push falls back to [push] branch = {:?}",
                    config.push.branch
                ),
            )
            .fix("git switch <branch>")
        }
        Err(err) => {
            return Gate::new(
                "protected_branch",
                GateState::Warn,
                format!("could not determine the current branch: {err}"),
            )
        }
    };
    if protected.iter().any(|p| p == &branch) {
        Gate::new(
            "protected_branch",
            GateState::Block,
            format!(
                "current branch '{branch}' is in [push.safety] protect_branches = {protected:?}: \
                 pushes to it are refused"
            ),
        )
        .fix(format!(
            "push from a feature branch (git switch -c <name>), remove \"{branch}\" from \
             [push.safety] protect_branches in kaptaind.toml, or `kaptaind push --force` for a one-off"
        ))
    } else {
        Gate::new(
            "protected_branch",
            GateState::Pass,
            format!("current branch '{branch}' is not protected ({protected:?})"),
        )
    }
}

fn startup_guard(config: &Config, purpose: Purpose) -> Gate {
    let guard = config.daemon.startup_guard;
    let dirty = match crate::git::repo::dirty_path_count(&config.repo_path) {
        Ok(n) => n,
        Err(err) => {
            return Gate::new(
                "startup_guard",
                GateState::Warn,
                format!("[daemon] startup_guard = {guard}; could not count dirty paths: {err}"),
            )
        }
    };
    let detail = format!("[daemon] startup_guard = {guard}; {dirty} uncommitted path(s)");
    if !guard || dirty == 0 {
        let state = if dirty > 0 {
            GateState::Info
        } else {
            GateState::Pass
        };
        return Gate::new("startup_guard", state, detail);
    }
    let consequence = "the daemon refuses to start on this dirty tree";
    let fix = "commit or stash first, start the daemon with `kaptaind --force`, or set \
               [daemon] startup_guard = false in kaptaind.toml";
    match purpose {
        Purpose::Analyze => Gate::new(
            "startup_guard",
            GateState::Info,
            format!("{detail}; {consequence}"),
        ),
        // A dirty tree is exactly what a commit picks up.
        Purpose::Commit => Gate::new(
            "startup_guard",
            GateState::Info,
            format!("{detail} (expected for a commit); {consequence}"),
        )
        .fix(fix),
        Purpose::Push => Gate::new(
            "startup_guard",
            GateState::Warn,
            format!("{detail}; {consequence}"),
        )
        .fix(fix),
    }
}

fn version_baseline(config: &Config, purpose: Purpose) -> Gate {
    let repo = config.repo_path.as_path();
    if !crate::version::bootstrap::baseline_missing(repo) {
        return match crate::version::resolve_baseline(repo) {
            Ok(v) => Gate::new(
                "version_baseline",
                GateState::Pass,
                format!("baseline {v}"),
            ),
            Err(err) => Gate::new(
                "version_baseline",
                required_or_warn(purpose.mutates()),
                err.to_string(),
            )
            .fix("fix VERSION (or Cargo.toml [package].version) to hold a valid semver, e.g. `echo 0.1.0 > VERSION`"),
        };
    }
    if !matches!(config.versioning.workspace, WorkspacePolicy::RootOnly)
        && matches!(
            crate::version::workspace::WorkspaceLayout::discover(repo),
            Ok(crate::version::workspace::WorkspaceLayout::Virtual { .. })
        )
    {
        return Gate::new(
            "version_baseline",
            GateState::NotApplicable,
            "virtual workspace under a member-scoped [versioning] workspace policy: member baselines resolve during writeback",
        );
    }
    let missing = format!(
        "no VERSION file and no Cargo.toml [package].version in {}",
        repo.display()
    );
    if matches!(config.versioning.bootstrap, VersionBootstrap::Refuse) {
        return Gate::new(
            "version_baseline",
            required_or_warn(purpose.mutates()),
            format!("{missing}; [versioning] bootstrap = \"refuse\": the daemon will not start"),
        )
        .fix("echo 0.1.0 > VERSION && git add VERSION, or set [versioning] bootstrap = \"initialize\" in kaptaind.toml");
    }
    match crate::version::bootstrap::seed(repo, &config.versioning) {
        Ok((version, source)) => {
            let action = if matches!(config.operation.mode, OperationMode::Observe) {
                format!("observe mode leaves it unwritten; actuation would initialize VERSION = {version} (from {source})")
            } else {
                format!("daemon startup will initialize VERSION = {version} (from {source})")
            };
            Gate::new(
                "version_baseline",
                if purpose.mutates() {
                    GateState::Warn
                } else {
                    GateState::Info
                },
                format!("{missing}; [versioning] bootstrap = \"initialize\": {action}"),
            )
            .fix(format!(
                "echo {version} > VERSION && git add VERSION to make the baseline explicit"
            ))
        }
        Err(err) => Gate::new(
            "version_baseline",
            required_or_warn(purpose.mutates()),
            format!("{missing}; bootstrap cannot seed: {err}"),
        )
        .fix("set [versioning] initial_version = \"0.1.0\" in kaptaind.toml, or echo 0.1.0 > VERSION"),
    }
}

/// Read-only writability probe: `access(W_OK)` on Unix (which also reports
/// `EROFS` for read-only mounts), the read-only permission bit elsewhere.
fn writable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        // SAFETY: `c` is a valid NUL-terminated string that outlives the call.
        if unsafe { libc::access(c.as_ptr(), libc::W_OK) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(unix))]
    {
        let meta = std::fs::metadata(path)?;
        if meta.permissions().readonly() {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "read-only",
            ))
        } else {
            Ok(())
        }
    }
}

fn kaptaind_dir_writable(repo: &Path, purpose: Purpose) -> Gate {
    let dir = repo.join(".kaptaind");
    let required = purpose.mutates();
    let (probe, note) = if dir.is_dir() {
        (dir.clone(), "")
    } else if dir.exists() {
        return Gate::new(
            "kaptaind_dir_writable",
            required_or_warn(required),
            format!("{} exists but is not a directory", dir.display()),
        )
        .fix(format!(
            "move {} aside so kaptaind can create its state directory",
            dir.display()
        ));
    } else {
        (repo.to_path_buf(), " (not created yet; checked its parent)")
    };
    match writable(&probe) {
        Ok(()) => Gate::new(
            "kaptaind_dir_writable",
            GateState::Pass,
            format!("{} is writable{note}", dir.display()),
        ),
        Err(err) => Gate::new(
            "kaptaind_dir_writable",
            required_or_warn(required),
            format!("{} is not writable: {err}", probe.display()),
        )
        .fix(format!(
            "make {} writable (check mount options and ownership), or run kaptaind against a writable checkout via --repo",
            probe.display()
        )),
    }
}

fn nested_git_root(repo: &Path) -> Gate {
    match crate::git::repo::RepoContext::discover(repo) {
        Ok(ctx) => {
            let prefix = ctx.project_prefix();
            if prefix.as_os_str().is_empty() {
                Gate::new(
                    "nested_git_root",
                    GateState::Pass,
                    format!("git root is repo_path ({})", ctx.git_root.display()),
                )
            } else {
                Gate::new(
                    "nested_git_root",
                    GateState::Warn,
                    format!(
                        "repo_path {} is not its own git root: commits land in the enclosing checkout {} (project prefix {})",
                        ctx.project_root.display(),
                        ctx.git_root.display(),
                        prefix.display()
                    ),
                )
                .fix(format!(
                    "intended monorepo subproject: no action; otherwise run `git init` in {} or pass --repo {}",
                    ctx.project_root.display(),
                    ctx.git_root.display()
                ))
            }
        }
        Err(err) => Gate::new(
            "nested_git_root",
            GateState::Block,
            format!("{} is not inside a git work tree: {err}", repo.display()),
        )
        .fix(format!(
            "run `git init` in {} or pass --repo <git checkout>",
            repo.display()
        )),
    }
}

fn suspended(repo: &Path, purpose: Purpose) -> Gate {
    if !crate::daemon::suspend::is_suspended(repo) {
        return Gate::new("suspended", GateState::Pass, "daemon is not suspended");
    }
    let mut detail =
        "daemon is suspended (.kaptaind/suspend.json): clusters are skipped".to_string();
    if let Ok(Some(state)) = crate::daemon::suspend::load(repo) {
        detail.push_str(&format!(" — source: {}", state.source));
        if let Some(reason) = state.reason {
            detail.push_str(&format!(", reason: {reason}"));
        }
    }
    let state = if purpose.mutates() {
        GateState::Block
    } else {
        GateState::Info
    };
    Gate::new("suspended", state, detail).fix("kaptaind resume")
}

fn daemon_running(repo: &Path, purpose: Purpose) -> Gate {
    let pid_file = repo.join(".kaptaind").join("daemon.pid");
    // Deliberately not `pidfile::validate_and_clean`: that removes stale
    // files, and readiness must not write.
    let Ok(content) = std::fs::read_to_string(&pid_file) else {
        return Gate::new("daemon_running", GateState::Pass, "no daemon pid file");
    };
    let Ok(pid) = content.trim().parse::<u32>() else {
        return Gate::new(
            "daemon_running",
            GateState::Pass,
            format!("unparseable pid file {} (ignored)", pid_file.display()),
        );
    };
    if pid == std::process::id() || !crate::daemon::pidfile::process_alive(pid) {
        return Gate::new(
            "daemon_running",
            GateState::Pass,
            format!(
                "stale pid file {} (pid {pid} not running)",
                pid_file.display()
            ),
        );
    }
    let detail = format!(
        "a daemon is running for this repo (pid {pid}, {})",
        pid_file.display()
    );
    match purpose {
        Purpose::Analyze => Gate::new("daemon_running", GateState::Info, detail),
        _ => Gate::new(
            "daemon_running",
            GateState::Block,
            format!("{detail}: `kaptaind commit` refuses to race it (two writers)"),
        )
        .fix(format!(
            "let the daemon (pid {pid}) make the commit, or pass `kaptaind commit --force`; \
             never kill or background the daemon to get a commit through"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::loader::ExecutionTrust;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        dir
    }

    fn config_for(dir: &Path) -> Config {
        Config {
            repo_path: dir.to_path_buf(),
            ..Config::default()
        }
    }

    fn gate<'a>(gates: &'a [Gate], id: &str) -> &'a Gate {
        gates.iter().find(|g| g.id == id).expect("gate present")
    }

    #[test]
    fn evaluates_every_gate_in_stable_order() {
        let dir = init_repo();
        let gates = evaluate(&config_for(dir.path()), Purpose::Push);
        let ids: Vec<&str> = gates.iter().map(|g| g.id).collect();
        assert_eq!(ids, GATE_IDS);
    }

    #[test]
    fn default_observe_config_blocks_commit_but_not_analyze() {
        let dir = init_repo();
        let mut config = config_for(dir.path());
        config.test.command = Some("true".to_string());
        std::fs::write(dir.path().join("VERSION"), "1.0.0\n").unwrap();

        let commit = evaluate(&config, Purpose::Commit);
        assert_eq!(gate(&commit, "operation_mode").state, GateState::Block);
        assert!(gate(&commit, "operation_mode")
            .fix
            .as_deref()
            .unwrap()
            .contains("mode = \"actuate\""));
        assert!(!is_ready(&commit));

        let analyze = evaluate(&config, Purpose::Analyze);
        assert!(is_ready(&analyze), "{analyze:#?}");
    }

    #[test]
    fn push_reports_every_push_blocker_at_once() {
        let dir = init_repo();
        let mut config = config_for(dir.path());
        config.operation.mode = OperationMode::Actuate;
        config.test.command = Some("true".to_string());
        config.push.enabled = false;
        config.capabilities.network_push = false;
        config.push.safety.protect_branches = vec!["main".to_string()];
        std::fs::write(dir.path().join("f"), "x").unwrap();
        git(dir.path(), &["add", "f"]);
        git(
            dir.path(),
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "init",
            ],
        );

        let gates = evaluate(&config, Purpose::Push);
        for id in ["push_enabled", "network_push", "protected_branch"] {
            assert_eq!(gate(&gates, id).state, GateState::Block, "{id}");
            assert!(gate(&gates, id).fix.is_some(), "{id}");
        }
        assert!(!is_ready(&gates));

        // The same config is irrelevant for commit.
        let commit = evaluate(&config, Purpose::Commit);
        for id in ["push_enabled", "network_push", "protected_branch"] {
            assert_eq!(gate(&commit, id).state, GateState::NotApplicable, "{id}");
        }
    }

    #[test]
    fn untrusted_test_command_blocks_commit_with_loader_message() {
        let dir = init_repo();
        let mut config = config_for(dir.path());
        config.trust.execution = ExecutionTrust::Untrusted;
        config.test.command = Some("cargo test".to_string());
        let gates = evaluate(&config, Purpose::Commit);
        let g = gate(&gates, "execution_trust");
        assert_eq!(g.state, GateState::Block);
        assert!(g.detail.contains("test.command"), "{}", g.detail);
        assert!(g.fix.as_deref().unwrap().contains("[trust] execution"));
        assert_eq!(
            gate(&evaluate(&config, Purpose::Analyze), "execution_trust").state,
            GateState::Warn
        );
    }

    #[test]
    fn trust_refusal_points_at_ready() {
        let mut config = Config::default();
        config.trust.execution = ExecutionTrust::Untrusted;
        config.test.command = Some("cargo test".to_string());
        let err = config.validate_execution_trust().unwrap_err().to_string();
        assert!(err.contains(READY_HINT), "{err}");
    }

    #[test]
    fn unresolvable_test_program_blocks_commit() {
        let dir = init_repo();
        let mut config = config_for(dir.path());
        config.test.command = Some("FOO=1 definitely-not-a-real-binary-xyz --all".to_string());
        let gates = evaluate(&config, Purpose::Commit);
        let g = gate(&gates, "test_hook");
        assert_eq!(g.state, GateState::Block);
        assert!(g.detail.contains("ENOENT"), "{}", g.detail);
        assert!(g.detail.contains("definitely-not-a-real-binary-xyz"));

        config.test.required = false;
        assert_eq!(
            gate(&evaluate(&config, Purpose::Commit), "test_hook").state,
            GateState::Warn
        );
    }

    #[test]
    fn test_program_resolution() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(first_program("A=1 B=2 ./run.sh x"), Some("./run.sh".into()));
        assert_eq!(first_program("   "), None);
        assert!(resolve_program("sh", dir.path()).is_ok());
        assert!(resolve_program("true", dir.path()).is_ok());
        assert!(resolve_program("./missing.sh", dir.path())
            .unwrap_err()
            .contains("ENOENT"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let script = dir.path().join("run.sh");
            std::fs::write(&script, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(
                resolve_program("./run.sh", dir.path()).unwrap(),
                Some(dir.path().join("./run.sh"))
            );
        }
    }

    #[test]
    fn dirty_tree_is_info_for_commit_with_startup_guard() {
        let dir = init_repo();
        let mut config = config_for(dir.path());
        config.daemon.startup_guard = true;
        std::fs::write(dir.path().join("dirty.txt"), "x").unwrap();
        let commit = evaluate(&config, Purpose::Commit);
        assert_eq!(gate(&commit, "startup_guard").state, GateState::Info);
        assert!(gate(&commit, "startup_guard")
            .detail
            .contains("1 uncommitted"));
        let push = evaluate(&config, Purpose::Push);
        assert_eq!(gate(&push, "startup_guard").state, GateState::Warn);
    }

    #[test]
    fn version_baseline_reports_bootstrap_seed() {
        let dir = init_repo();
        let mut config = config_for(dir.path());
        let g = version_baseline(&config, Purpose::Commit);
        assert_eq!(g.state, GateState::Warn);
        assert!(g.detail.contains("VERSION = 0.1.0"), "{}", g.detail);
        assert!(!dir.path().join("VERSION").exists(), "must not write");

        config.versioning.bootstrap = VersionBootstrap::Refuse;
        assert_eq!(
            version_baseline(&config, Purpose::Commit).state,
            GateState::Block
        );

        std::fs::write(dir.path().join("VERSION"), "2.3.4\n").unwrap();
        let g = version_baseline(&config, Purpose::Commit);
        assert_eq!(g.state, GateState::Pass);
        assert!(g.detail.contains("2.3.4"));
    }

    #[test]
    fn nested_repo_warns_and_non_repo_blocks() {
        let parent = init_repo();
        let child = parent.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let g = nested_git_root(&child);
        assert_eq!(g.state, GateState::Warn);
        assert!(g.detail.contains("enclosing checkout"), "{}", g.detail);

        let lone = tempfile::tempdir().unwrap();
        // A tempdir may itself live under a git checkout; only assert the
        // block when git agrees it is not a work tree.
        if crate::git::repo::Repo::open(lone.path()).is_err() {
            assert_eq!(nested_git_root(lone.path()).state, GateState::Block);
        }
    }

    #[cfg(unix)]
    #[test]
    fn suspended_and_live_daemon_gates() {
        let dir = init_repo();
        let state = crate::daemon::suspend::SuspendState::suspended("manual", Some("hold".into()));
        crate::daemon::suspend::save(dir.path(), &state).unwrap();
        let g = suspended(dir.path(), Purpose::Commit);
        assert_eq!(g.state, GateState::Block);
        assert!(g.detail.contains("hold"));
        assert_eq!(g.fix.as_deref(), Some("kaptaind resume"));
        assert_eq!(
            suspended(dir.path(), Purpose::Analyze).state,
            GateState::Info
        );

        // Our parent process is alive and is not us.
        let live = std::os::unix::process::parent_id();
        std::fs::write(dir.path().join(".kaptaind/daemon.pid"), format!("{live}\n")).unwrap();
        assert_eq!(
            daemon_running(dir.path(), Purpose::Analyze).state,
            GateState::Info
        );
        assert_eq!(
            daemon_running(dir.path(), Purpose::Commit).state,
            GateState::Block
        );

        std::fs::write(dir.path().join(".kaptaind/daemon.pid"), "4294967\n").unwrap();
        assert_eq!(
            daemon_running(dir.path(), Purpose::Commit).state,
            GateState::Pass
        );
        assert!(
            dir.path().join(".kaptaind/daemon.pid").exists(),
            "stale pid file must not be removed"
        );
    }

    #[test]
    fn unwritable_state_dir_names_the_path() {
        let dir = init_repo();
        let state = dir.path().join(".kaptaind");
        std::fs::write(&state, "not a dir").unwrap();
        let g = kaptaind_dir_writable(dir.path(), Purpose::Commit);
        assert_eq!(g.state, GateState::Block);
        assert!(g.detail.contains(&state.display().to_string()));
    }

    #[test]
    fn json_shape_uses_stable_strings() {
        let g = Gate::new("suspended", GateState::NotApplicable, "x");
        let v = serde_json::to_value(&g).unwrap();
        assert_eq!(v["id"], "suspended");
        assert_eq!(v["state"], "n/a");
        assert!(v["fix"].is_null());
        assert_eq!("push".parse::<Purpose>().unwrap(), Purpose::Push);
        assert!("deploy".parse::<Purpose>().is_err());
    }
}
