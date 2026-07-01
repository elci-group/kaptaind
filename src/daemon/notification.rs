//! Desktop and system notifications for kaptaind.
//!
//! Supports:
//! - Native desktop notifications via notify-rust (when feature enabled)
//! - Shell command hooks
//! - Webhook notifications (Discord, Slack, generic)
//! - Status bar integration via status.json

use crate::config::loader::NotifyConfig;

/// Desktop notification priority
#[derive(Debug, Clone, Copy)]
pub enum Priority {
    Low,
    Normal,
    High,
}

/// Send a commit notification through all configured channels.
pub fn notify_commit(
    config: &NotifyConfig,
    version: &str,
    score: f32,
    msg: &str,
    files_changed: usize,
    webhook_enabled: bool,
) {
    // Shell command hook
    if let Some(cmd) = &config.on_commit {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .env("KAPTAIND_VERSION", version)
            .env("KAPTAIND_SCORE", score.to_string())
            .env("KAPTAIND_MSG", msg)
            .env("KAPTAIND_FILES", files_changed.to_string())
            .spawn();
    }

    // Desktop notification
    #[cfg(feature = "notifications")]
    {
        let title = format!("🚀 Kaptaind v{}", version);
        let body = format!(
            "Score: {:.3}\n{} file(s) changed\n{}",
            score,
            files_changed,
            truncate(msg, 100)
        );
        let _ = send_desktop_notification(&title, &body, Priority::Normal);
    }

    // Webhook
    if webhook_enabled {
        if let Some(webhook_url) = &config.webhook_url {
            let url = webhook_url.clone();
            let message = truncate(msg, 3500);
            let content = format!(
                "🚀 **Kaptaind** shipped `v{}`\n**Score:** {:.3}\n**Files:** {}\n**Message:**\n```\n{}\n```",
                version, score, files_changed, message
            );

            tokio::spawn(async move {
                let payload = if url.contains("discord.com") {
                    serde_json::json!({ "content": content })
                } else {
                    serde_json::json!({ "text": content })
                };

                let client = reqwest::Client::new();
                if let Err(err) = client.post(&url).json(&payload).send().await {
                    tracing::warn!(error = %err, "failed to send commit webhook");
                }
            });
        }
    }
}

/// Send an error notification through all configured channels.
pub fn notify_error(
    config: &NotifyConfig,
    error: &str,
    context: Option<&str>,
    webhook_enabled: bool,
) {
    // Shell command hook
    if let Some(cmd) = &config.on_error {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .env("KAPTAIND_ERROR", error)
            .env("KAPTAIND_CONTEXT", context.unwrap_or(""))
            .spawn();
    }

    // Desktop notification
    #[cfg(feature = "notifications")]
    {
        let title = "🚨 Kaptaind Error";
        let body = if let Some(ctx) = context {
            format!("{}\n{}", ctx, truncate(error, 200))
        } else {
            truncate(error, 200)
        };
        let _ = send_desktop_notification(title, &body, Priority::High);
    }

    // Webhook
    if webhook_enabled {
        if let Some(webhook_url) = &config.webhook_url {
            let url = webhook_url.clone();
            let error_text = if let Some(ctx) = context {
                format!("{}: {}", ctx, error)
            } else {
                error.to_string()
            };
            let content = format!(
                "🚨 **Kaptaind Error**\n```\n{}\n```",
                truncate(&error_text, 3500)
            );

            tokio::spawn(async move {
                let payload = if url.contains("discord.com") {
                    serde_json::json!({ "content": content })
                } else {
                    serde_json::json!({ "text": content })
                };

                let client = reqwest::Client::new();
                if let Err(err) = client.post(&url).json(&payload).send().await {
                    tracing::warn!(error = %err, "failed to send error webhook");
                }
            });
        }
    }
}

/// Send a status notification (e.g., daemon started, shut down).
pub fn notify_status(title: &str, message: &str) {
    let _ = send_desktop_notification(title, message, Priority::Low);
}

/// Send a native desktop notification.
#[cfg(feature = "notifications")]
fn send_desktop_notification(title: &str, body: &str, priority: Priority) -> anyhow::Result<()> {
    use notify_rust::{Notification, Timeout, Urgency};

    let urgency = match priority {
        Priority::Low => Urgency::Low,
        Priority::Normal => Urgency::Normal,
        Priority::High => Urgency::Critical,
    };

    Notification::new()
        .summary(title)
        .body(body)
        .icon("kaptaind") // May need to be installed system-wide
        .timeout(Timeout::Milliseconds(10000))
        .urgency(urgency)
        .show()?;

    Ok(())
}

/// Fallback when notifications feature is disabled
#[cfg(not(feature = "notifications"))]
fn send_desktop_notification(_title: &str, _body: &str, _priority: Priority) -> anyhow::Result<()> {
    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }
}
