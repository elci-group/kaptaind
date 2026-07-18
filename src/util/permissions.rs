//! Private permissions for Kaptaind's local runtime state.

use std::io;
use std::path::Path;

pub fn create_private_file(path: &Path) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    set_private_file(path)?;
    Ok(file)
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)?;
    set_mode(path, 0o700)
}

pub fn set_private_file(path: &Path) -> io::Result<()> {
    if path.exists() {
        set_mode(path, 0o600)?;
    }
    Ok(())
}

/// Harden the complete local runtime tree without following symlinks.
/// Existing executable files retain owner execute permission for plugins.
pub fn harden_runtime_tree(repo_path: &Path) -> io::Result<()> {
    let root = repo_path.join(".kaptaind");
    ensure_private_dir(&root)?;
    harden_dir_contents(&root)
}

fn harden_dir_contents(dir: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            set_mode(&path, 0o700)?;
            harden_dir_contents(&path)?;
        } else if metadata.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let executable = metadata.permissions().mode() & 0o111 != 0;
                set_mode(&path, if executable { 0o700 } else { 0o600 })?;
            }
            #[cfg(not(unix))]
            set_mode(&path, 0o600)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn hardens_tree_without_following_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join(".kaptaind");
        let nested = runtime.join("traces");
        std::fs::create_dir_all(&nested).unwrap();
        let secret = nested.join("trace.json");
        let executable = runtime.join("plugin");
        std::fs::write(&secret, "secret").unwrap();
        std::fs::write(&executable, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o775)).unwrap();
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o664)).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let outside = dir.path().join("outside");
        std::fs::write(&outside, "outside").unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o644)).unwrap();
        symlink(&outside, runtime.join("link")).unwrap();

        harden_runtime_tree(dir.path()).unwrap();

        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&runtime), 0o700);
        assert_eq!(mode(&nested), 0o700);
        assert_eq!(mode(&secret), 0o600);
        assert_eq!(mode(&executable), 0o700);
        assert_eq!(mode(&outside), 0o644);
    }
}
