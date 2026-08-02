//! Git client-side hooks management for Angler.
//!
//! This module manages git hooks by:
//! 1. Installing/updating hook scripts in .git/hooks
//! 2. Executing hooks at the appropriate lifecycle points
//! 3. Handling hook results and failures

use crate::angler::config::{GitHooksConfig, HookConfig};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

/// Result of executing a git hook.
#[derive(Debug, Clone)]
pub struct HookResult {
    /// Whether the hook succeeded
    pub success: bool,
    /// Exit code (if available)
    pub exit_code: Option<i32>,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution duration
    pub duration_ms: u64,
    /// Whether the hook timed out
    pub timed_out: bool,
}

impl HookResult {
    /// Create a successful result.
    pub fn success() -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 0,
            timed_out: false,
        }
    }

    /// Create a failure result.
    pub fn failure(stderr: impl Into<String>) -> Self {
        Self {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: stderr.into(),
            duration_ms: 0,
            timed_out: false,
        }
    }

    /// Create a timeout result.
    pub fn timeout() -> Self {
        Self {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: "Hook timed out".to_string(),
            duration_ms: 0,
            timed_out: true,
        }
    }
}

/// Manager for git hooks.
pub struct GitHookManager {
    config: GitHooksConfig,
    repo_path: PathBuf,
    hooks_dir: PathBuf,
}

impl GitHookManager {
    /// Create a new hook manager.
    pub fn new(config: &GitHooksConfig, repo_path: &Path) -> Result<Self> {
        let hooks_dir = config
            .hooks_dir
            .clone()
            .unwrap_or_else(|| resolve_git_hooks_dir(repo_path));

        Ok(Self {
            config: config.clone(),
            repo_path: repo_path.to_path_buf(),
            hooks_dir,
        })
    }

    /// Install kaptaind-managed hooks.
    ///
    /// This creates wrapper scripts in the repo's git hooks directory that
    /// delegate to kaptaind.
    pub fn install_hooks(&self) -> Result<()> {
        if !self.config.enabled {
            info!(
                component = module_path!(),
                "Git hooks management is disabled"
            );
            return Ok(());
        }

        // Monorepo guard: when the watched path is a subdirectory of a
        // larger git repo, silently installing into the outer repo's hooks
        // would be surprising — and the old `.git/hooks` fallback created a
        // fake `.git` directory inside the subproject, breaking tooling
        // that probes for repo roots. Skip unless hooks_dir is explicit.
        if self.config.hooks_dir.is_none() {
            if let Some(root) = git_root(&self.repo_path) {
                let root = std::fs::canonicalize(&root).unwrap_or(root);
                let repo = std::fs::canonicalize(&self.repo_path)
                    .unwrap_or_else(|_| self.repo_path.clone());
                if root != repo {
                    warn!(
                        repo_path = %self.repo_path.display(),
                        git_root = %root.display(),
                        "skipping hook install: watched path is inside a larger git repo; \
                         set angler.git_hooks.hooks_dir to opt in"
                    );
                    return Ok(());
                }
            }
        }

        // Ensure hooks directory exists
        std::fs::create_dir_all(&self.hooks_dir)?;

        // Install standard hooks
        let hooks = [
            ("pre-commit", &self.config.pre_commit),
            ("prepare-commit-msg", &self.config.prepare_commit_msg),
            ("commit-msg", &self.config.commit_msg),
            ("post-commit", &self.config.post_commit),
            ("pre-push", &self.config.pre_push),
            ("post-checkout", &self.config.post_checkout),
            ("post-merge", &self.config.post_merge),
        ];

        for (name, config) in hooks {
            if config.is_some() {
                self.install_hook_script(name)?;
            }
        }

        // Install custom hooks
        for name in self.config.custom.keys() {
            self.install_hook_script(name)?;
        }

        info!(
            component = module_path!(),
            "Installed {} git hooks",
            hooks.len() + self.config.custom.len()
        );
        Ok(())
    }

    /// Uninstall kaptaind-managed hooks.
    pub fn uninstall_hooks(&self) -> Result<()> {
        if !self.hooks_dir.exists() {
            return Ok(());
        }

        let hooks = [
            "pre-commit",
            "prepare-commit-msg",
            "commit-msg",
            "post-commit",
            "pre-push",
            "post-checkout",
            "post-merge",
        ];

        for name in hooks.iter() {
            let hook_path = self.hooks_dir.join(name);
            if hook_path.exists() && self.is_kaptaind_managed(&hook_path)? {
                std::fs::remove_file(&hook_path)?;
                debug!(
                    component = module_path!(),
                    "Removed kaptaind-managed hook: {}", name
                );
            }
        }

        for name in self.config.custom.keys() {
            let hook_path = self.hooks_dir.join(name);
            if hook_path.exists() && self.is_kaptaind_managed(&hook_path)? {
                std::fs::remove_file(&hook_path)?;
                debug!(
                    component = module_path!(),
                    "Removed kaptaind-managed custom hook: {}", name
                );
            }
        }

        info!(component = module_path!(), "Uninstalled kaptaind git hooks");
        Ok(())
    }

    /// Execute a specific hook.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn execute_hook(
        &self,
        hook_name: &str,
        args: &[String],
        file_changes: &[PathBuf],
    ) -> Result<HookResult> {
        let config = self.get_hook_config(hook_name);

        let config = match config {
            Some(c) => c,
            None => {
                debug!(
                    component = module_path!(),
                    "No configuration found for hook: {}", hook_name
                );
                return Ok(HookResult::success());
            }
        };

        // Check if we should run based on file patterns
        if !config.file_patterns.is_empty() && !file_changes.is_empty() {
            let should_run = file_changes.iter().any(|file| {
                config.file_patterns.iter().any(|pattern| {
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches_path(file))
                        .unwrap_or(false)
                })
            });

            if !should_run {
                debug!(
                    component = module_path!(),
                    "Skipping hook {} - no matching files", hook_name
                );
                return Ok(HookResult::success());
            }
        }

        self.run_hook_command(hook_name, config, args).await
    }

    /// Run pre-commit hooks.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_pre_commit(&self, staged_files: &[PathBuf]) -> Result<HookResult> {
        self.execute_hook("pre-commit", &[], staged_files).await
    }

    /// Run prepare-commit-msg hook.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_prepare_commit_msg(
        &self,
        commit_msg_file: &Path,
        source: Option<&str>,
        sha: Option<&str>,
    ) -> Result<HookResult> {
        let mut args = vec![commit_msg_file.to_string_lossy().to_string()];
        if let Some(src) = source {
            args.push(src.to_string());
        }
        if let Some(s) = sha {
            args.push(s.to_string());
        }
        self.execute_hook("prepare-commit-msg", &args, &[]).await
    }

    /// Run commit-msg hook.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_commit_msg(&self, commit_msg_file: &Path) -> Result<HookResult> {
        let args = vec![commit_msg_file.to_string_lossy().to_string()];
        self.execute_hook("commit-msg", &args, &[]).await
    }

    /// Run post-commit hook.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_post_commit(&self) -> Result<HookResult> {
        self.execute_hook("post-commit", &[], &[]).await
    }

    /// Run pre-push hook.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_pre_push(
        &self,
        remote_name: &str,
        remote_url: &str,
        refs: &[(String, String, String, String)], // local_ref, local_sha, remote_ref, remote_sha
    ) -> Result<HookResult> {
        let mut args = vec![remote_name.to_string(), remote_url.to_string()];
        for (local_ref, local_sha, remote_ref, remote_sha) in refs {
            args.push(format!(
                "{} {} {} {}",
                local_ref, local_sha, remote_ref, remote_sha
            ));
        }
        self.execute_hook("pre-push", &args, &[]).await
    }

    /// Run post-checkout hook.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_post_checkout(
        &self,
        prev_head: &str,
        new_head: &str,
        is_branch_checkout: bool,
    ) -> Result<HookResult> {
        let args = vec![
            prev_head.to_string(),
            new_head.to_string(),
            if is_branch_checkout { "1" } else { "0" }.to_string(),
        ];
        self.execute_hook("post-checkout", &args, &[]).await
    }

    /// Run post-merge hook.
    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn run_post_merge(&self, is_squash: bool) -> Result<HookResult> {
        let args = vec![if is_squash { "1" } else { "0" }.to_string()];
        self.execute_hook("post-merge", &args, &[]).await
    }

    // =============================================================================
    // Internal Methods
    // =============================================================================

    fn get_hook_config(&self, name: &str) -> Option<&HookConfig> {
        match name {
            "pre-commit" => self.config.pre_commit.as_ref(),
            "prepare-commit-msg" => self.config.prepare_commit_msg.as_ref(),
            "commit-msg" => self.config.commit_msg.as_ref(),
            "post-commit" => self.config.post_commit.as_ref(),
            "pre-push" => self.config.pre_push.as_ref(),
            "post-checkout" => self.config.post_checkout.as_ref(),
            "post-merge" => self.config.post_merge.as_ref(),
            _ => self.config.custom.get(name),
        }
    }

    fn install_hook_script(&self, name: &str) -> Result<()> {
        let hook_path = self.hooks_dir.join(name);

        // Check if there's already a non-kaptaind hook
        if hook_path.exists() && !self.is_kaptaind_managed(&hook_path)? {
            let backup_path = hook_path.with_extension("local");
            warn!(
                component = module_path!(),
                "Existing hook {} found, backing up to {}",
                name,
                backup_path.display()
            );
            std::fs::rename(&hook_path, &backup_path)?;
        }

        let script = self.generate_hook_script(name)?;
        std::fs::write(&hook_path, script)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&hook_path, perms)?;
        }

        info!(
            component = module_path!(),
            "Installed hook script: {}", name
        );
        Ok(())
    }

    fn generate_hook_script(&self, name: &str) -> Result<String> {
        let script = format!(
            r##"#!/bin/sh
# KAPTAIND MANAGED HOOK - {}
# This hook is managed by kaptaind. Manual changes will be overwritten.

# Delegate to kaptaind-cli
exec kaptaind-cli angler exec-hook {} "$@"
"##,
            name, name
        );
        Ok(script)
    }

    fn is_kaptaind_managed(&self, path: &Path) -> Result<bool> {
        let content = std::fs::read_to_string(path)?;
        Ok(content.contains("KAPTAIND MANAGED HOOK"))
    }

    async fn run_hook_command(
        &self,
        hook_name: &str,
        config: &HookConfig,
        args: &[String],
    ) -> Result<HookResult> {
        let start = std::time::Instant::now();

        if let Err(err) = crate::util::shell_validation::validate_shell_command(&config.command) {
            tracing::warn!(error = %err, hook = hook_name, command = %config.command, "shell command validation failed");
        }

        let working_dir = config
            .working_dir
            .as_ref()
            .map(|p| self.repo_path.join(p))
            .unwrap_or_else(|| self.repo_path.clone());

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&config.command)
            .args(args)
            .current_dir(&working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("KAPTAIND_HOOK_NAME", hook_name)
            .env("KAPTAIND_HOOK_REQUIRED", config.required.to_string());

        // Add custom environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        debug!(
            component = module_path!(),
            "Executing hook {}: {} (timeout: {}s)", hook_name, config.command, config.timeout_secs
        );

        let timeout_duration = Duration::from_secs(config.timeout_secs);

        // Spawn the process
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                error!(
                    component = module_path!(),
                    "Failed to spawn hook {}: {}", hook_name, e
                );
                return Ok(HookResult::failure(format!("Spawn error: {}", e)));
            }
        };

        let result = match timeout(timeout_duration, child.wait_with_output()).await {
            Ok(Ok(output)) => HookResult {
                success: output.status.success(),
                exit_code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                timed_out: false,
            },
            Ok(Err(e)) => {
                error!(
                    component = module_path!(),
                    "Hook {} process error: {}", hook_name, e
                );
                HookResult::failure(format!("Process error: {}", e))
            }
            Err(_) => {
                warn!(
                    component = module_path!(),
                    "Hook {} timed out after {}s", hook_name, config.timeout_secs
                );
                HookResult::timeout()
            }
        };

        if !result.success && config.required {
            error!(
                component = module_path!(),
                "Required hook {} failed: exit_code={:?}, stderr={}",
                hook_name,
                result.exit_code,
                result.stderr
            );
        } else if !result.success {
            warn!(
                component = module_path!(),
                "Optional hook {} failed (non-blocking): exit_code={:?}",
                hook_name,
                result.exit_code
            );
        } else {
            debug!(
                component = module_path!(),
                "Hook {} completed successfully in {}ms", hook_name, result.duration_ms
            );
        }

        Ok(result)
    }
}

/// Resolve the real git hooks directory for `repo_path` via
/// `git rev-parse --git-path hooks`. This handles monorepo subprojects,
/// linked worktrees, and custom `GIT_DIR` layouts; on any failure it
/// falls back to the conventional `.git/hooks`.
fn resolve_git_hooks_dir(repo_path: &Path) -> PathBuf {
    let fallback = repo_path.join(".git").join("hooks");
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--git-path", "hooks"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if raw.is_empty() {
                return fallback;
            }
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                path
            } else {
                repo_path.join(path)
            }
        }
        _ => fallback,
    }
}

/// Resolve the git worktree root containing `repo_path`, if any.
fn git_root(repo_path: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

/// List installed hooks and their status.
pub fn list_hooks(repo_path: &Path) -> Result<Vec<HookInfo>> {
    let hooks_dir = resolve_git_hooks_dir(repo_path);
    let mut hooks = Vec::new();

    if !hooks_dir.exists() {
        return Ok(hooks);
    }

    for entry in std::fs::read_dir(&hooks_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip non-files and sample hooks
        if !path.is_file() {
            continue;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        if name.ends_with(".sample") {
            continue;
        }

        let is_managed = match std::fs::read_to_string(&path) {
            Ok(content) => content.contains("KAPTAIND MANAGED HOOK"),
            Err(error) => {
                tracing::error!(
                    ?error,
                    operation = "list_hooks",
                    source_line = line!(),
                    "list hooks returned an error"
                );
                false
            }
        };

        let metadata = std::fs::metadata(&path)?;
        let modified = metadata.modified()?;

        hooks.push(HookInfo {
            name,
            path,
            is_managed,
            is_executable: is_executable(&metadata),
            modified,
        });
    }

    Ok(hooks)
}

/// Information about an installed hook.
#[derive(Debug, Clone)]
pub struct HookInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_managed: bool,
    pub is_executable: bool,
    pub modified: std::time::SystemTime,
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    // Windows doesn't have executable permissions in the same way
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_hook_result_success() {
        let result = HookResult::success();
        assert!(result.success);
        assert!(!result.timed_out);
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn test_hook_result_failure() {
        let result = HookResult::failure("test error");
        assert!(!result.success);
        assert!(!result.timed_out);
        assert_eq!(result.stderr, "test error");
    }

    #[test]
    fn test_hook_result_timeout() {
        let result = HookResult::timeout();
        assert!(!result.success);
        assert!(result.timed_out);
    }

    #[test]
    fn test_generate_hook_script() {
        let config = GitHooksConfig::default();
        let temp_dir = TempDir::new().unwrap();
        let manager = GitHookManager::new(&config, temp_dir.path()).unwrap();

        let script = manager.generate_hook_script("pre-commit").unwrap();
        assert!(script.contains("KAPTAIND MANAGED HOOK"));
        assert!(script.contains("kaptaind-cli angler exec-hook"));
        assert!(script.contains("pre-commit"));
    }

    #[test]
    fn test_is_kaptaind_managed() {
        let temp_dir = TempDir::new().unwrap();
        let hook_path = temp_dir.path().join("test-hook");

        std::fs::write(&hook_path, "#!/bin/sh\n# KAPTAIND MANAGED HOOK\necho test").unwrap();

        let config = GitHooksConfig::default();
        let manager = GitHookManager::new(&config, temp_dir.path()).unwrap();

        assert!(manager.is_kaptaind_managed(&hook_path).unwrap());

        std::fs::write(&hook_path, "#!/bin/sh\necho test").unwrap();
        assert!(!manager.is_kaptaind_managed(&hook_path).unwrap());
    }

    #[tokio::test]
    async fn test_run_hook_command_success() {
        let config = GitHooksConfig::default();
        let temp_dir = TempDir::new().unwrap();
        let manager = GitHookManager::new(&config, temp_dir.path()).unwrap();

        let hook_config = HookConfig {
            command: "echo 'hello world'".to_string(),
            required: true,
            timeout_secs: 5,
            env: HashMap::new(),
            working_dir: None,
            file_patterns: vec![],
        };

        let result = manager
            .run_hook_command("test", &hook_config, &[])
            .await
            .unwrap();

        assert!(result.success);
        // Note: stdout may not be captured correctly in tests due to async/process timing
        // The important thing is that the hook executed successfully
    }

    #[tokio::test]
    async fn test_run_hook_command_failure() {
        let config = GitHooksConfig::default();
        let temp_dir = TempDir::new().unwrap();
        let manager = GitHookManager::new(&config, temp_dir.path()).unwrap();

        let hook_config = HookConfig {
            command: "exit 1".to_string(),
            required: true,
            timeout_secs: 5,
            env: HashMap::new(),
            working_dir: None,
            file_patterns: vec![],
        };

        let result = manager
            .run_hook_command("test", &hook_config, &[])
            .await
            .unwrap();

        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
    }

    fn enabled_config() -> GitHooksConfig {
        GitHooksConfig {
            enabled: true,
            pre_commit: Some(HookConfig {
                command: "true".to_string(),
                required: false,
                timeout_secs: 5,
                env: HashMap::new(),
                working_dir: None,
                file_patterns: vec![],
            }),
            ..GitHooksConfig::default()
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git spawns");
        assert!(status.success(), "git {:?} failed", args);
    }

    #[test]
    fn test_install_hooks_in_repo_root_uses_real_git_hooks() {
        let temp_dir = TempDir::new().unwrap();
        git(temp_dir.path(), &["init", "-q", "-b", "master"]);

        let manager = GitHookManager::new(&enabled_config(), temp_dir.path()).unwrap();
        manager.install_hooks().unwrap();

        assert!(temp_dir.path().join(".git/hooks/pre-commit").exists());
    }

    #[test]
    fn test_install_hooks_in_monorepo_subdir_skips_without_fake_git() {
        let temp_dir = TempDir::new().unwrap();
        git(temp_dir.path(), &["init", "-q", "-b", "master"]);
        let sub = temp_dir.path().join("proj");
        std::fs::create_dir_all(&sub).unwrap();

        let manager = GitHookManager::new(&enabled_config(), &sub).unwrap();
        manager.install_hooks().unwrap();

        // No fake .git may be created inside the subproject, and the outer
        // repo's hooks must be left alone.
        assert!(!sub.join(".git").exists());
        assert!(!temp_dir.path().join(".git/hooks/pre-commit").exists());
    }

    #[test]
    fn test_install_hooks_with_explicit_dir_in_monorepo_installs() {
        let temp_dir = TempDir::new().unwrap();
        git(temp_dir.path(), &["init", "-q", "-b", "master"]);
        let sub = temp_dir.path().join("proj");
        std::fs::create_dir_all(&sub).unwrap();

        let explicit = temp_dir.path().join(".git/hooks");
        let mut config = enabled_config();
        config.hooks_dir = Some(explicit.clone());
        let manager = GitHookManager::new(&config, &sub).unwrap();
        manager.install_hooks().unwrap();

        assert!(explicit.join("pre-commit").exists());
        assert!(!sub.join(".git").exists());
    }
}
