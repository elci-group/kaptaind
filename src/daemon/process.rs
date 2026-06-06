use anyhow::{anyhow, Context};
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::Path;

enum Fork {
    Parent,
    Child,
    Failed,
}

#[derive(Debug)]
enum DaemonizeOutcome {
    ParentExit,
    ChildContinues,
}

trait ProcessOps {
    fn fork(&mut self) -> Fork;
    fn setsid(&mut self) -> bool;
    fn chdir(&mut self, workdir: &CString) -> bool;
    fn dup2(&mut self, from: i32, to: i32) -> bool;
    fn pid(&self) -> libc::pid_t;
}

struct RealProcessOps;

impl ProcessOps for RealProcessOps {
    fn fork(&mut self) -> Fork {
        match unsafe { libc::fork() } {
            pid if pid < 0 => Fork::Failed,
            0 => Fork::Child,
            _pid => Fork::Parent,
        }
    }

    fn setsid(&mut self) -> bool {
        (unsafe { libc::setsid() }) >= 0
    }

    fn chdir(&mut self, workdir: &CString) -> bool {
        (unsafe { libc::chdir(workdir.as_ptr()) }) == 0
    }

    fn dup2(&mut self, from: i32, to: i32) -> bool {
        (unsafe { libc::dup2(from, to) }) >= 0
    }

    fn pid(&self) -> libc::pid_t {
        unsafe { libc::getpid() }
    }
}

/// Detach the current process into the background.
///
/// This intentionally implements only the daemon behavior Kaptaind needs:
/// fork, create a new session, switch working directory, redirect stdio,
/// and record the child pid. The parent process exits after the fork.
pub fn daemonize(
    workdir: &Path,
    pid_path: &Path,
    stdout: File,
    stderr: File,
) -> anyhow::Result<()> {
    let mut ops = RealProcessOps;
    match daemonize_inner(&mut ops, workdir, pid_path, stdout, stderr)? {
        DaemonizeOutcome::ParentExit => std::process::exit(0),
        DaemonizeOutcome::ChildContinues => Ok(()),
    }
}

fn daemonize_inner(
    ops: &mut dyn ProcessOps,
    workdir: &Path,
    pid_path: &Path,
    stdout: File,
    stderr: File,
) -> anyhow::Result<DaemonizeOutcome> {
    match ops.fork() {
        Fork::Parent => return Ok(DaemonizeOutcome::ParentExit),
        Fork::Child => {}
        Fork::Failed => return Err(anyhow!("fork failed")),
    }

    if !ops.setsid() {
        return Err(anyhow!("setsid failed"));
    }

    match ops.fork() {
        Fork::Parent => return Ok(DaemonizeOutcome::ParentExit),
        Fork::Child => {}
        Fork::Failed => return Err(anyhow!("second fork failed")),
    }

    let workdir = CString::new(workdir.to_string_lossy().as_bytes())
        .context("working directory contains null byte")?;
    if !ops.chdir(&workdir) {
        return Err(anyhow!("failed to change daemon working directory"));
    }

    redirect_fd(ops, stdout.as_raw_fd(), libc::STDOUT_FILENO)?;
    redirect_fd(ops, stderr.as_raw_fd(), libc::STDERR_FILENO)?;

    let mut pid_file = File::create(pid_path)?;
    writeln!(pid_file, "{}", ops.pid())?;

    Ok(DaemonizeOutcome::ChildContinues)
}

fn redirect_fd(ops: &mut dyn ProcessOps, from: i32, to: i32) -> anyhow::Result<()> {
    if !ops.dup2(from, to) {
        return Err(anyhow!("failed to redirect file descriptor"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parent_exits_after_first_fork_without_side_effects() {
        let dir = tempdir().unwrap();
        let mut ops = FakeOps::new(vec![Fork::Parent]);
        let outcome = daemonize_inner(
            &mut ops,
            dir.path(),
            &dir.path().join("daemon.pid"),
            File::create(dir.path().join("out")).unwrap(),
            File::create(dir.path().join("err")).unwrap(),
        )
        .unwrap();

        assert!(matches!(outcome, DaemonizeOutcome::ParentExit));
        assert!(!dir.path().join("daemon.pid").exists());
        assert_eq!(ops.setsid_calls, 0);
    }

    #[test]
    fn child_writes_pid_after_double_fork_and_redirects_stdio() {
        let dir = tempdir().unwrap();
        let mut ops = FakeOps::new(vec![Fork::Child, Fork::Child]);
        let outcome = daemonize_inner(
            &mut ops,
            dir.path(),
            &dir.path().join("daemon.pid"),
            File::create(dir.path().join("out")).unwrap(),
            File::create(dir.path().join("err")).unwrap(),
        )
        .unwrap();

        assert!(matches!(outcome, DaemonizeOutcome::ChildContinues));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("daemon.pid")).unwrap(),
            "4242\n"
        );
        assert_eq!(ops.setsid_calls, 1);
        assert_eq!(ops.chdir_calls, 1);
        assert_eq!(
            ops.dup2_targets,
            vec![libc::STDOUT_FILENO, libc::STDERR_FILENO]
        );
    }

    #[test]
    fn second_fork_failure_is_reported() {
        let dir = tempdir().unwrap();
        let mut ops = FakeOps::new(vec![Fork::Child, Fork::Failed]);
        let err = daemonize_inner(
            &mut ops,
            dir.path(),
            &dir.path().join("daemon.pid"),
            File::create(dir.path().join("out")).unwrap(),
            File::create(dir.path().join("err")).unwrap(),
        )
        .unwrap_err();

        assert!(err.to_string().contains("second fork failed"));
        assert!(!dir.path().join("daemon.pid").exists());
    }

    struct FakeOps {
        forks: std::collections::VecDeque<Fork>,
        setsid_calls: usize,
        chdir_calls: usize,
        dup2_targets: Vec<i32>,
    }

    impl FakeOps {
        fn new(forks: Vec<Fork>) -> Self {
            Self {
                forks: forks.into(),
                setsid_calls: 0,
                chdir_calls: 0,
                dup2_targets: Vec::new(),
            }
        }
    }

    impl ProcessOps for FakeOps {
        fn fork(&mut self) -> Fork {
            self.forks.pop_front().unwrap_or(Fork::Failed)
        }

        fn setsid(&mut self) -> bool {
            self.setsid_calls += 1;
            true
        }

        fn chdir(&mut self, _workdir: &CString) -> bool {
            self.chdir_calls += 1;
            true
        }

        fn dup2(&mut self, _from: i32, to: i32) -> bool {
            self.dup2_targets.push(to);
            true
        }

        fn pid(&self) -> libc::pid_t {
            4242
        }
    }
}
