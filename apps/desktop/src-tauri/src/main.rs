#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub version: String,
    pub uptime_seconds: u64,
    pub watched_repos_count: usize,
    pub active_session_id: Option<String>,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionBump {
    pub version: String,
    pub bump_type: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    pub message: String,
}

impl From<anyhow::Error> for CommandError {
    fn from(err: anyhow::Error) -> Self {
        CommandError {
            message: err.to_string(),
        }
    }
}

#[tauri::command]
async fn get_daemon_status() -> Result<DaemonStatus, CommandError> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
    let path = PathBuf::from(home).join(".kaptaind/status.json");

    if path.exists() {
        let data = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read status: {}", e))?;
        let status: DaemonStatus = serde_json::from_str(&data)
            .map_err(|e| anyhow::anyhow!("Invalid status payload: {}", e))?;
        return Ok(status);
    }

    Ok(DaemonStatus {
        running: false,
        version: "unknown".to_string(),
        uptime_seconds: 0,
        watched_repos_count: 0,
        active_session_id: None,
        schema_version: "1.0.0".to_string(),
    })
}

#[tauri::command]
async fn get_recent_bumps() -> Result<Vec<VersionBump>, CommandError> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
    let analysis_dir = PathBuf::from(home).join(".kaptaind/analysis");

    let mut bumps = Vec::new();

    if analysis_dir.exists() {
        let mut entries: Vec<_> = std::fs::read_dir(&analysis_dir)
            .map_err(|e| anyhow::anyhow!("Failed to read analysis dir: {}", e))?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| {
            e.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        entries.reverse();

        for entry in entries.into_iter().take(5) {
            if let Ok(data) = std::fs::read_to_string(entry.path()) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                    let version = json
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?.?.?")
                        .to_string();
                    let bump = json
                        .get("bump")
                        .and_then(|v| v.as_str())
                        .unwrap_or("patch")
                        .to_string();
                    let ts = json
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    bumps.push(VersionBump {
                        version,
                        bump_type: bump,
                        timestamp: ts,
                    });
                }
            }
        }
    }

    if bumps.is_empty() {
        bumps.push(VersionBump {
            version: "0.1.0".to_string(),
            bump_type: "initial".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        });
    }

    Ok(bumps)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![get_daemon_status, get_recent_bumps])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
