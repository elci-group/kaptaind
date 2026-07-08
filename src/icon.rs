//! Kaptaind logo/icon helpers.
//!
//! The notification logo is embedded at compile time so the binary can extract
//! it to the user's cache or icon theme without requiring the original source
//! repository.

use std::path::PathBuf;

/// Resized 256x128 PNG version of the kaptaind logo, suitable for desktop
/// notifications and small UI elements.
pub const NOTIFICATION_LOGO_PNG: &[u8] =
    include_bytes!("../docs/assets/kaptaind-logo-notification.png");

/// Return the user-specific cache directory for kaptaind.
pub fn cache_dir() -> PathBuf {
    std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("kaptaind")
}

/// Path where the embedded notification logo is cached at runtime.
pub fn cached_notification_icon_path() -> PathBuf {
    cache_dir().join("kaptaind-logo-notification.png")
}

/// Ensure the embedded notification logo is written to the cache directory.
/// Returns the path to the extracted icon.
pub fn ensure_cached_notification_icon() -> anyhow::Result<PathBuf> {
    let path = cached_notification_icon_path();
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, NOTIFICATION_LOGO_PNG)?;
    }
    Ok(path)
}

/// Install the kaptaind logo into the Freedesktop icon theme so it can be
/// referenced by name ("kaptaind") by notification daemons and desktop launchers.
///
/// * `user`   — install under `$HOME/.local/share/icons/hicolor/256x256/apps`
/// * `system` — install under `/usr/share/icons/hicolor/256x256/apps` (requires root)
pub fn install_icon(user: bool, system: bool) -> anyhow::Result<PathBuf> {
    let target_dir = if system {
        PathBuf::from("/usr/share/icons/hicolor/256x256/apps")
    } else if user {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".local/share/icons/hicolor/256x256/apps"))
            .map_err(|_| anyhow::anyhow!("HOME not set"))?
    } else {
        anyhow::bail!("specify --user or --system");
    };

    std::fs::create_dir_all(&target_dir)?;
    let target = target_dir.join("kaptaind.png");
    std::fs::write(&target, NOTIFICATION_LOGO_PNG)?;

    // Best-effort icon cache refresh.
    let _ = refresh_icon_cache(system);

    Ok(target)
}

fn refresh_icon_cache(system: bool) -> anyhow::Result<()> {
    let theme_dir = if system {
        PathBuf::from("/usr/share/icons/hicolor")
    } else {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".local/share/icons/hicolor"))
            .unwrap_or_else(|_| PathBuf::from("/usr/share/icons/hicolor"))
    };
    let _ = std::process::Command::new("gtk-update-icon-cache")
        .arg(theme_dir)
        .output();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_logo_is_png() {
        assert!(!NOTIFICATION_LOGO_PNG.is_empty());
        assert_eq!(
            &NOTIFICATION_LOGO_PNG[..8],
            &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }

    #[test]
    fn ensure_cached_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", tmp.path().as_os_str());
        let path = ensure_cached_notification_icon().unwrap();
        assert!(path.exists());
        assert!(path.metadata().unwrap().len() > 0);
    }
}
