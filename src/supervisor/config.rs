use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

fn default_listen_addr() -> SocketAddr {
    "127.0.0.1:3213"
        .parse()
        .expect("valid default listen address")
}

fn default_reconcile_interval_secs() -> u64 {
    30
}

fn default_max_starts() -> usize {
    4
}

fn default_worker_binary() -> PathBuf {
    PathBuf::from("kaptaind")
}

fn default_padagonia_endpoint() -> String {
    "http://127.0.0.1:7373".to_string()
}

fn default_padagonia_namespace() -> String {
    "kaptaind".to_string()
}

fn default_padagonia_token_env() -> String {
    "PADAGONIA_API_KEY".to_string()
}

fn default_timeout_secs() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PadagoniaConfig {
    pub enabled: bool,
    pub required: bool,
    pub endpoint: String,
    pub namespace: String,
    pub token_env: String,
    pub timeout_secs: u64,
}

impl Default for PadagoniaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            required: false,
            endpoint: default_padagonia_endpoint(),
            namespace: default_padagonia_namespace(),
            token_env: default_padagonia_token_env(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SupervisorConfig {
    pub state_path: PathBuf,
    pub legacy_registry_path: PathBuf,
    pub listen_addr: SocketAddr,
    pub reconcile_interval_secs: u64,
    pub max_starts_per_cycle: usize,
    pub worker_binary: PathBuf,
    pub api_token_env: Option<String>,
    pub padagonia: PadagoniaConfig,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        let root = config_root();
        Self {
            state_path: root.join("supervisor-state.json"),
            legacy_registry_path: root.join("monitored.json"),
            listen_addr: default_listen_addr(),
            reconcile_interval_secs: default_reconcile_interval_secs(),
            max_starts_per_cycle: default_max_starts(),
            worker_binary: default_worker_binary(),
            api_token_env: None,
            padagonia: PadagoniaConfig::default(),
        }
    }
}

impl SupervisorConfig {
    pub fn default_path() -> PathBuf {
        config_root().join("supervisor.toml")
    }

    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = path.map_or_else(Self::default_path, Path::to_path_buf);
        if !path.exists() {
            let config = Self::default();
            config.validate()?;
            return Ok(config);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read supervisor config at {}", path.display()))?;
        let mut config: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse supervisor config at {}", path.display()))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        config.state_path = absolutize(base, &config.state_path);
        config.legacy_registry_path = absolutize(base, &config.legacy_registry_path);
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.listen_addr.ip().is_loopback(),
            "supervisor listen address must be loopback"
        );
        anyhow::ensure!(
            self.reconcile_interval_secs > 0,
            "reconcile interval must be positive"
        );
        anyhow::ensure!(
            self.max_starts_per_cycle > 0,
            "max_starts_per_cycle must be positive"
        );
        anyhow::ensure!(
            !self.worker_binary.as_os_str().is_empty(),
            "worker_binary must not be empty"
        );
        anyhow::ensure!(
            !(self.padagonia.required && !self.padagonia.enabled),
            "required Padagonia control requires padagonia.enabled = true"
        );
        anyhow::ensure!(
            self.padagonia.timeout_secs > 0,
            "Padagonia timeout must be positive"
        );
        Ok(())
    }
}

fn config_root() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("kaptaind")
}

fn absolutize(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_loopback_listener() {
        let config = SupervisorConfig {
            listen_addr: "0.0.0.0:3213".parse().unwrap(),
            ..SupervisorConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn required_padagonia_must_be_enabled() {
        let mut config = SupervisorConfig::default();
        config.padagonia.required = true;
        assert!(config.validate().is_err());
    }
}
