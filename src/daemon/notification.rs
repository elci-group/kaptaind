//! Desktop and system notifications for kaptaind.
//!
//! Supports:
//! - Native desktop notifications via notify-rust (when feature enabled)
//! - Shell command hooks (`on_commit`, `on_error`, `on_push`, `on_start`, `on_shutdown`)
//! - Webhook notifications (Discord, Slack, generic)
//! - Status bar integration via status.json
//! - A beautified nautical theme with emoji and maritime phrasing

use crate::config::loader::NotifyConfig;

/// Desktop notification priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Low,
    Normal,
    High,
}

/// The kind of event being announced.
pub enum NotificationEvent<'a> {
    Commit {
        version: &'a str,
        score: f32,
        msg: &'a str,
        files_changed: usize,
    },
    PushSuccess {
        version: &'a str,
        branch: &'a str,
        remote: &'a str,
    },
    PushFailure {
        error: &'a str,
        branch: &'a str,
        remote: &'a str,
    },
    Error {
        error: &'a str,
        context: Option<&'a str>,
    },
    MonitorStart {
        repo_path: &'a str,
    },
    MonitorStop {
        repo_path: &'a str,
    },
}

impl NotificationEvent<'_> {
    fn event_name(&self) -> &'static str {
        match self {
            NotificationEvent::Commit { .. } => "commit",
            NotificationEvent::PushSuccess { .. } => "push_success",
            NotificationEvent::PushFailure { .. } => "push_failure",
            NotificationEvent::Error { .. } => "error",
            NotificationEvent::MonitorStart { .. } => "start",
            NotificationEvent::MonitorStop { .. } => "stop",
        }
    }

    fn shell_hook<'a>(&self, config: &'a NotifyConfig) -> Option<&'a String> {
        match self {
            NotificationEvent::Commit { .. } => config.on_commit.as_ref(),
            NotificationEvent::PushSuccess { .. } | NotificationEvent::PushFailure { .. } => {
                config.on_push.as_ref()
            }
            NotificationEvent::Error { .. } => config.on_error.as_ref(),
            NotificationEvent::MonitorStart { .. } => config.on_start.as_ref(),
            NotificationEvent::MonitorStop { .. } => config.on_shutdown.as_ref(),
        }
    }
}

/// Rendered notification text.
struct RenderedNotification {
    title: String,
    body: String,
    webhook: String,
    priority: Priority,
}

/// Send a notification for any supported event through all configured channels.
pub fn notify(config: &NotifyConfig, event: NotificationEvent<'_>, webhook_enabled: bool) {
    let rendered = render(&event, config.nautical_theme);

    // Shell command hook.
    if let Some(cmd) = event.shell_hook(config) {
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg(cmd);
        inject_env(&mut command, &event);
        let _ = command.spawn();
    }

    // Desktop notification.
    let _ = send_desktop_notification(&rendered.title, &rendered.body, rendered.priority);

    // Webhook.
    if webhook_enabled {
        if let Some(webhook_url) = config.webhook_url.clone() {
            let content = rendered.webhook;
            tokio::spawn(async move {
                let payload = if webhook_url.contains("discord.com") {
                    serde_json::json!({ "content": content })
                } else {
                    serde_json::json!({ "text": content })
                };

                let client = reqwest::Client::new();
                if let Err(err) = client.post(&webhook_url).json(&payload).send().await {
                    tracing::warn!(error = %err, "failed to send webhook notification");
                }
            });
        }
    }
}

fn inject_env(command: &mut std::process::Command, event: &NotificationEvent<'_>) {
    command.env("KAPTAIND_EVENT", event.event_name());
    match event {
        NotificationEvent::Commit {
            version,
            score,
            msg,
            files_changed,
        } => {
            command
                .env("KAPTAIND_VERSION", *version)
                .env("KAPTAIND_SCORE", score.to_string())
                .env("KAPTAIND_MSG", *msg)
                .env("KAPTAIND_FILES", files_changed.to_string());
        }
        NotificationEvent::PushSuccess {
            version,
            branch,
            remote,
        } => {
            command
                .env("KAPTAIND_VERSION", *version)
                .env("KAPTAIND_BRANCH", *branch)
                .env("KAPTAIND_REMOTE", *remote);
        }
        NotificationEvent::PushFailure {
            error,
            branch,
            remote,
        } => {
            command
                .env("KAPTAIND_ERROR", *error)
                .env("KAPTAIND_BRANCH", *branch)
                .env("KAPTAIND_REMOTE", *remote);
        }
        NotificationEvent::Error { error, context } => {
            command
                .env("KAPTAIND_ERROR", *error)
                .env("KAPTAIND_CONTEXT", context.unwrap_or(""));
        }
        NotificationEvent::MonitorStart { repo_path }
        | NotificationEvent::MonitorStop { repo_path } => {
            command.env("KAPTAIND_REPO_PATH", *repo_path);
        }
    }
}

fn render(event: &NotificationEvent<'_>, nautical: bool) -> RenderedNotification {
    if nautical {
        render_nautical(event)
    } else {
        render_plain(event)
    }
}

fn render_nautical(event: &NotificationEvent<'_>) -> RenderedNotification {
    match event {
        NotificationEvent::Commit {
            version,
            score,
            msg,
            files_changed,
        } => {
            let title = format!("🚢 Ship's log updated — v{}", version);
            let body = format!(
                "Weighed anchor with {} changed sail(s).\nScore: {:.3}\n{}",
                files_changed,
                score,
                truncate(msg, 120)
            );
            let webhook = format!(
                "🚢 **Ship's log updated** — `v{}`\n**Changed sails:** {}\n**Chart score:** {:.3}\n**Log entry:**\n```\n{}\n```",
                version, files_changed, score, truncate(msg, 3500)
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::Normal,
            }
        }
        NotificationEvent::PushSuccess {
            version,
            branch,
            remote,
        } => {
            let title = "⛵ Charts delivered".to_string();
            let body = format!("v{} made it safely to {}/{}", version, remote, branch);
            let webhook = format!(
                "⛵ **Charts delivered** — `v{}` is now on `{}/{}`",
                version, remote, branch
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::Normal,
            }
        }
        NotificationEvent::PushFailure {
            error,
            branch,
            remote,
        } => {
            let title = "🆘 Mayday! Push ran aground".to_string();
            let body = format!(
                "Could not deliver charts to {}/{}:\n{}",
                remote,
                branch,
                truncate(error, 200)
            );
            let webhook = format!(
                "🆘 **Mayday!** Push to `{}/{}` ran aground.\n```\n{}\n```",
                remote,
                branch,
                truncate(error, 3500)
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::High,
            }
        }
        NotificationEvent::Error { error, context } => {
            let title = "🚨 Storm warning".to_string();
            let body = if let Some(ctx) = context {
                format!("{}\n{}", ctx, truncate(error, 200))
            } else {
                truncate(error, 200)
            };
            let webhook = format!(
                "🚨 **Storm warning**{}\n```\n{}\n```",
                context.map(|c| format!(" — {}", c)).unwrap_or_default(),
                truncate(error, 3500)
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::High,
            }
        }
        NotificationEvent::MonitorStart { repo_path } => {
            let title = "⚓ Ahoy! Kaptaind is on watch".to_string();
            let body = format!("Now scanning the repo waters of {}", repo_path);
            let webhook = format!("⚓ **Ahoy!** Kaptaind is on watch for `{}`", repo_path);
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::Low,
            }
        }
        NotificationEvent::MonitorStop { repo_path } => {
            let title = "🏴‍☠️ Kaptaind dropping anchor".to_string();
            let body = format!("Stopped monitoring {}", repo_path);
            let webhook = format!(
                "🏴‍☠️ **Dropping anchor** — stopped monitoring `{}`",
                repo_path
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::Low,
            }
        }
    }
}

fn render_plain(event: &NotificationEvent<'_>) -> RenderedNotification {
    match event {
        NotificationEvent::Commit {
            version,
            score,
            msg,
            files_changed,
        } => {
            let title = format!("🚀 Kaptaind v{}", version);
            let body = format!(
                "Score: {:.3}\n{} file(s) changed\n{}",
                score,
                files_changed,
                truncate(msg, 100)
            );
            let webhook = format!(
                "🚀 **Kaptaind** shipped `v{}`\n**Score:** {:.3}\n**Files:** {}\n**Message:**\n```\n{}\n```",
                version,
                score,
                files_changed,
                truncate(msg, 3500)
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::Normal,
            }
        }
        NotificationEvent::PushSuccess {
            version,
            branch,
            remote,
        } => {
            let title = "📤 Push complete".to_string();
            let body = format!("v{} pushed to {}/{}", version, remote, branch);
            let webhook = format!(
                "📤 **Push complete** — `v{}` is on `{}/{}`",
                version, remote, branch
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::Normal,
            }
        }
        NotificationEvent::PushFailure {
            error,
            branch,
            remote,
        } => {
            let title = "📤 Push failed".to_string();
            let body = format!(
                "Failed to push to {}/{}:\n{}",
                remote,
                branch,
                truncate(error, 200)
            );
            let webhook = format!(
                "📤 **Push failed** — `{}/{}`\n```\n{}\n```",
                remote,
                branch,
                truncate(error, 3500)
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::High,
            }
        }
        NotificationEvent::Error { error, context } => {
            let title = "🚨 Kaptaind Error".to_string();
            let body = if let Some(ctx) = context {
                format!("{}\n{}", ctx, truncate(error, 200))
            } else {
                truncate(error, 200)
            };
            let webhook = format!(
                "🚨 **Kaptaind Error**{}\n```\n{}\n```",
                context.map(|c| format!(" — {}", c)).unwrap_or_default(),
                truncate(error, 3500)
            );
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::High,
            }
        }
        NotificationEvent::MonitorStart { repo_path } => {
            let title = "Kaptaind started".to_string();
            let body = format!("Monitoring {}", repo_path);
            let webhook = format!("Kaptaind started monitoring `{}`", repo_path);
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::Low,
            }
        }
        NotificationEvent::MonitorStop { repo_path } => {
            let title = "Kaptaind stopped".to_string();
            let body = format!("Stopped monitoring {}", repo_path);
            let webhook = format!("Kaptaind stopped monitoring `{}`", repo_path);
            RenderedNotification {
                title,
                body,
                webhook,
                priority: Priority::Low,
            }
        }
    }
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
    notify(
        config,
        NotificationEvent::Commit {
            version,
            score,
            msg,
            files_changed,
        },
        webhook_enabled,
    );
}

/// Send a push-success notification.
pub fn notify_push_success(
    config: &NotifyConfig,
    version: &str,
    branch: &str,
    remote: &str,
    webhook_enabled: bool,
) {
    notify(
        config,
        NotificationEvent::PushSuccess {
            version,
            branch,
            remote,
        },
        webhook_enabled,
    );
}

/// Send a push-failure notification.
pub fn notify_push_failure(
    config: &NotifyConfig,
    error: &str,
    branch: &str,
    remote: &str,
    webhook_enabled: bool,
) {
    notify(
        config,
        NotificationEvent::PushFailure {
            error,
            branch,
            remote,
        },
        webhook_enabled,
    );
}

/// Send an error notification through all configured channels.
pub fn notify_error(
    config: &NotifyConfig,
    error: &str,
    context: Option<&str>,
    webhook_enabled: bool,
) {
    notify(
        config,
        NotificationEvent::Error { error, context },
        webhook_enabled,
    );
}

/// Send a daemon-start notification.
pub fn notify_start(config: &NotifyConfig, repo_path: &std::path::Path, webhook_enabled: bool) {
    notify(
        config,
        NotificationEvent::MonitorStart {
            repo_path: &repo_path.display().to_string(),
        },
        webhook_enabled,
    );
}

/// Send a daemon-stop notification.
pub fn notify_stop(config: &NotifyConfig, repo_path: &std::path::Path, webhook_enabled: bool) {
    notify(
        config,
        NotificationEvent::MonitorStop {
            repo_path: &repo_path.display().to_string(),
        },
        webhook_enabled,
    );
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

    #[test]
    fn nautical_commit_render_uses_maritime_phrasing() {
        let config = NotifyConfig {
            nautical_theme: true,
            ..NotifyConfig::default()
        };
        let rendered = render(
            &NotificationEvent::Commit {
                version: "1.2.3",
                score: 0.75,
                msg: "feat: add crow's nest",
                files_changed: 3,
            },
            config.nautical_theme,
        );
        assert!(rendered.title.contains("Ship's log"));
        assert!(rendered.body.contains("Weighed anchor"));
        assert!(rendered.webhook.contains("⚓") || rendered.webhook.contains("🚢"));
    }

    #[test]
    fn plain_commit_render_uses_existing_phrasing() {
        let rendered = render(
            &NotificationEvent::Commit {
                version: "1.2.3",
                score: 0.75,
                msg: "feat: add crow's nest",
                files_changed: 3,
            },
            false,
        );
        assert!(rendered.title.contains("Kaptaind"));
        assert!(rendered.body.contains("Score:"));
    }

    #[test]
    fn nautical_push_success_mentions_charts() {
        let rendered = render(
            &NotificationEvent::PushSuccess {
                version: "1.2.3",
                branch: "main",
                remote: "origin",
            },
            true,
        );
        assert!(rendered.title.contains("Charts delivered"));
        assert!(rendered.body.contains("origin/main"));
    }

    #[test]
    fn nautical_error_uses_mayday() {
        let rendered = render(
            &NotificationEvent::PushFailure {
                error: "permission denied",
                branch: "main",
                remote: "origin",
            },
            true,
        );
        assert!(rendered.title.contains("Mayday"));
        assert!(rendered.priority == Priority::High);
    }

    #[test]
    fn monitor_start_title_has_ahoy() {
        let rendered = render(
            &NotificationEvent::MonitorStart { repo_path: "/repo" },
            true,
        );
        assert!(rendered.title.contains("Ahoy"));
    }
}
