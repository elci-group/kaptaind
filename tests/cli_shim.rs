//! Integration tests for the `kaptaind-cli` deprecation shim
//! (`scripts/kaptaind-cli`) and its embedded copy in `install.sh`.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Write a fake `kaptaind` that records its argv (one per line) to stdout.
fn fake_kaptaind(dir: &Path) {
    let fake = dir.join("kaptaind");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho \"fake-kaptaind argc=$#\"\nfor a in \"$@\"; do echo \"arg=$a\"; done\nexit 7\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn run_shim(args: &[&str]) -> std::process::Output {
    let bin_dir = tempfile::tempdir().unwrap();
    fake_kaptaind(bin_dir.path());
    let path = format!(
        "{}:{}",
        bin_dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new("sh")
        .arg(repo_file("scripts/kaptaind-cli"))
        .args(args)
        .env("PATH", path)
        .output()
        .expect("run shim")
}

#[test]
fn shim_passes_args_through_and_warns_on_stderr() {
    let out = run_shim(&["logs", "--lines", "80", "with space", ""]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(
        stdout,
        "fake-kaptaind argc=5\narg=logs\narg=--lines\narg=80\narg=with space\narg=\n"
    );
    assert_eq!(
        stderr.lines().collect::<Vec<_>>(),
        vec!["kaptaind-cli is deprecated; use kaptaind <cmd>"],
        "exactly one deprecation line on stderr"
    );
    // exec: the exit status is kaptaind's own.
    assert_eq!(out.status.code(), Some(7));
}

#[test]
fn shim_with_no_args_execs_kaptaind() {
    let out = run_shim(&[]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "fake-kaptaind argc=0\n"
    );
}

#[test]
fn shim_is_executable_in_repo() {
    let mode = std::fs::metadata(repo_file("scripts/kaptaind-cli"))
        .unwrap()
        .permissions()
        .mode();
    assert!(mode & 0o111 != 0, "scripts/kaptaind-cli must be executable");
}

#[test]
fn install_sh_embedded_shim_matches_repo_script() {
    let install = std::fs::read_to_string(repo_file("install.sh")).unwrap();
    let shim = std::fs::read_to_string(repo_file("scripts/kaptaind-cli")).unwrap();
    let start = install
        .find("    cat << 'SHIM'\n")
        .expect("install.sh embeds the shim heredoc")
        + "    cat << 'SHIM'\n".len();
    let end = start
        + install[start..]
            .find("\nSHIM\n")
            .expect("heredoc terminator")
        + 1;
    assert_eq!(
        &install[start..end],
        shim,
        "install.sh emit_cli_shim must stay byte-identical to scripts/kaptaind-cli"
    );
}
