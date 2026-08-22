//! Crash-safe pid file handling for `.kaptaind/daemon.pid`.
//!
//! The daemon writes its pid on startup (see `daemon::process::daemonize` for
//! `--daemon` mode). If the daemon is killed -9 the file is left behind; on the
//! next start `validate_and_clean` detects the stale entry via a liveness
//! check and removes it so operators and `kaptaind-cli monitor resume` don't
//! mistake a corpse for a running daemon.

use std::path::Path;

/// Result of validating an existing pid file at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidFileState {
    /// No pid file present.
    Missing,
    /// Pid file points at a live process (possibly a duplicate daemon).
    Live(u32),
    /// Pid file pointed at a dead process and was removed.
    StaleRemoved(u32),
}

/// Write the current process id to `path`.
pub fn write(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", std::process::id()))
}

/// Validate the pid file at `path`: a stale file (dead pid) is removed with a
/// log line; a live pid is reported so the caller can warn about a possible
/// duplicate daemon.
pub fn validate_and_clean(path: &Path) -> PidFileState {
    let Ok(content) = std::fs::read_to_string(path) else {
        return PidFileState::Missing;
    };
    let Ok(pid) = content.trim().parse::<u32>() else {
        tracing::warn!(path = %path.display(), "unparseable daemon pid file; removing");
        if let Err(error) = std::fs::remove_file(path) {
            tracing::warn!(
                ?error,
                operation = "validate_and_clean",
                source_line = line!(),
                "best-effort operation failed"
            );
        }
        return PidFileState::Missing;
    };

    if process_alive(pid) {
        if pid != std::process::id() {
            tracing::warn!(
                pid,
                path = %path.display(),
                "daemon pid file points at a live process; another daemon may be running"
            );
        }
        return PidFileState::Live(pid);
    }

    tracing::info!(
        pid,
        path = %path.display(),
        "removing stale daemon pid file (process no longer exists)"
    );
    if let Err(error) = std::fs::remove_file(path) {
        tracing::warn!(
            ?error,
            operation = "validate_and_clean",
            source_line = line!(),
            "best-effort operation failed"
        );
    }
    PidFileState::StaleRemoved(pid)
}

/// Best-effort process liveness check without new dependencies.
fn process_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        // Signal 0 performs error checking without sending a signal.
        (unsafe { libc::kill(pid as i32, 0) }) == 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_and_clean, write, PidFileState};
    use tempfile::tempdir;

    #[test]
    fn write_then_validate_reports_live_pid() {
        let dir = tempdir().expect("temp dir");
        let pid_path = dir.path().join("daemon.pid");

        write(&pid_path).expect("write pid");

        assert_eq!(
            validate_and_clean(&pid_path),
            PidFileState::Live(std::process::id())
        );
        assert!(pid_path.exists(), "live pid file must be kept");
    }

    #[test]
    fn stale_pid_file_is_removed() {
        let dir = tempdir().expect("temp dir");
        let pid_path = dir.path().join("daemon.pid");
        // 2^22 is the default Linux pid_max upper bound; this pid cannot exist.
        std::fs::write(&pid_path, "4194304\n").expect("write stale pid");

        assert_eq!(
            validate_and_clean(&pid_path),
            PidFileState::StaleRemoved(4194304)
        );
        assert!(!pid_path.exists(), "stale pid file must be removed");
    }

    #[test]
    fn missing_pid_file_is_fine() {
        let dir = tempdir().expect("temp dir");
        assert_eq!(
            validate_and_clean(&dir.path().join("daemon.pid")),
            PidFileState::Missing
        );
    }

    #[test]
    fn unparseable_pid_file_is_removed() {
        let dir = tempdir().expect("temp dir");
        let pid_path = dir.path().join("daemon.pid");
        std::fs::write(&pid_path, "not-a-pid").expect("write garbage");

        assert_eq!(validate_and_clean(&pid_path), PidFileState::Missing);
        assert!(!pid_path.exists());
    }
}
