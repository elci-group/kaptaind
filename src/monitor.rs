use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_HEALTH_PORT_START: u16 = 3000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorEntry {
    pub path: PathBuf,
    pub config: PathBuf,
    pub enabled: bool,
    pub health_port: u16,
    pub last_active: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MonitorRegistry {
    pub projects: Vec<MonitorEntry>,
}

impl MonitorRegistry {
    pub fn find(&self, path: &Path) -> Option<&MonitorEntry> {
        self.projects.iter().find(|e| e.path == path)
    }

    pub fn find_mut(&mut self, path: &Path) -> Option<&mut MonitorEntry> {
        self.projects.iter_mut().find(|e| e.path == path)
    }

    pub fn remove(&mut self, path: &Path) -> bool {
        let len = self.projects.len();
        self.projects.retain(|e| e.path != path);
        self.projects.len() < len
    }
}

pub fn registry_path() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Could not determine home directory")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("kaptaind")
        .join("monitored.json"))
}

pub fn load_registry() -> Result<MonitorRegistry> {
    load_registry_at(&registry_path()?)
}

pub fn save_registry(registry: &MonitorRegistry) -> Result<()> {
    save_registry_at(&registry_path()?, registry)
}

pub fn load_registry_at(path: &Path) -> Result<MonitorRegistry> {
    if !path.exists() {
        return Ok(MonitorRegistry::default());
    }
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read registry at {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse registry at {}", path.display()))
}

pub fn save_registry_at(path: &Path, registry: &MonitorRegistry) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(registry).context("Failed to serialize registry")?;
    fs::write(path, content)
        .with_context(|| format!("Failed to write registry at {}", path.display()))
}

fn absolutize(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn resolve_path(path: &Path) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let abs = absolutize(&cwd, path);
    abs.canonicalize().unwrap_or(abs)
}

fn next_free_port(registry: &MonitorRegistry, start: u16) -> u16 {
    let used: HashSet<u16> = registry.projects.iter().map(|e| e.health_port).collect();
    let mut port = start;
    while used.contains(&port) {
        port = port.saturating_add(1);
        if port == 0 {
            port = start;
        }
    }
    port
}

/// Add or update a project in the monitor registry.
///
/// Paths are resolved to absolute form. If no config path is supplied, the
/// default `<project>/kaptaind.toml` is used. If no health port is supplied,
/// the next free port starting at 3000 is assigned.
pub fn add(
    path: &Path,
    config: Option<&Path>,
    health_port: Option<u16>,
    enabled: Option<bool>,
) -> Result<()> {
    let abs_path = resolve_path(path);

    let config_path = match config {
        Some(c) => resolve_path(c),
        None => abs_path.join("kaptaind.toml"),
    };

    let mut registry = load_registry()?;

    let port = match health_port {
        Some(p) => p,
        None => next_free_port(&registry, DEFAULT_HEALTH_PORT_START),
    };

    let now = Utc::now();

    if let Some(entry) = registry.find_mut(&abs_path) {
        entry.config = config_path;
        entry.health_port = port;
        entry.last_active = Some(now);
        if let Some(enabled) = enabled {
            entry.enabled = enabled;
        }
    } else {
        registry.projects.push(MonitorEntry {
            path: abs_path.clone(),
            config: config_path,
            enabled: enabled.unwrap_or(true),
            health_port: port,
            last_active: Some(now),
        });
    }

    save_registry(&registry)?;

    if !abs_path.join("kaptaind.toml").exists() {
        eprintln!(
            "⚠️  Warning: no kaptaind.toml found at {}. The daemon may fail to start.",
            abs_path.display()
        );
    }

    Ok(())
}

/// Remove a project from the monitor registry by path.
/// Returns `true` if a matching entry was removed.
pub fn remove(path: &Path) -> Result<bool> {
    let abs_path = resolve_path(path);
    let mut registry = load_registry()?;
    let removed = registry.remove(&abs_path);
    save_registry(&registry)?;
    Ok(removed)
}

/// Enable or disable a registered project.
pub fn set_enabled(path: &Path, enabled: bool) -> Result<()> {
    let abs_path = resolve_path(path);
    let mut registry = load_registry()?;
    if let Some(entry) = registry.find_mut(&abs_path) {
        entry.enabled = enabled;
        save_registry(&registry)?;
    } else {
        anyhow::bail!("Project not registered: {}", abs_path.display());
    }
    Ok(())
}

/// Update the `last_active` timestamp for a registered project.
/// Silently succeeds if the project is not registered.
pub fn touch_last_active(path: &Path) -> Result<()> {
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut registry = load_registry()?;
    if let Some(entry) = registry.find_mut(&abs_path) {
        entry.last_active = Some(Utc::now());
        save_registry(&registry)?;
    }
    Ok(())
}
