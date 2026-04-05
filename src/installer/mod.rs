#[cfg(feature = "gui")]
pub mod gui;

pub const VERSION: &str = "0.1.0";
pub const REPO_URL: &str = "https://github.com/elci-group/kaptaind.git";

#[derive(Debug, Clone)]
pub struct InstallConfig {
    pub install_path: String,
    pub system_wide: bool,
    pub build_mode: String,
    pub run_init: bool,
}

impl Default for InstallConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        Self {
            install_path: format!("{}/.local/bin", home),
            system_wide: false,
            build_mode: "release".to_string(),
            run_init: true,
        }
    }
}
