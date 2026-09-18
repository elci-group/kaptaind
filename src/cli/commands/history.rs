//! Version-history command: emit a JSON timeline of versions, tags, commits,
//! and kaptaind-driven bump events for a repository.

use anyhow::Context;
use kaptaind::util::style::Colorize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use kaptaind::config::loader::Config;

#[derive(Debug, Clone, Serialize)]
pub struct HistoryReport {
    pub schema: String,
    pub current_version: String,
    pub latest_tag: Option<String>,
    pub events: Vec<HistoryEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryEvent {
    VersionBump {
        from: String,
        to: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        ts: Option<u64>,
    },
    Commit {
        sha: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        ts: Option<u64>,
    },
    Tag {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        ts: Option<u64>,
    },
    Push {
        r#ref: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        ts: Option<u64>,
    },
}

pub fn handle_history(config: &Config, json: bool) -> anyhow::Result<()> {
    let report = build_history(config)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_text(&report);
    }
    Ok(())
}

fn build_history(config: &Config) -> anyhow::Result<HistoryReport> {
    let repo_path = &config.repo_path;
    let current_version = read_version(repo_path);
    let tags = git_tags(repo_path)?;
    let latest_tag = tags.keys().last().cloned();

    let mut events = Vec::new();

    // Add tag events.
    for (name, info) in &tags {
        events.push(HistoryEvent::Tag {
            name: name.clone(),
            target: info.target.clone(),
            ts: info.ts,
        });
    }

    // Add recent commits.
    for commit in git_log(repo_path, 20)? {
        events.push(HistoryEvent::Commit {
            sha: commit.sha.clone(),
            message: commit.message.clone(),
            author: commit.author.clone(),
            ts: commit.ts,
        });
    }

    // Add kaptaind analysis/bump events.
    events.extend(kaptaind_bump_events(repo_path, &tags)?);

    // Add push events from reflog.
    events.extend(reflog_push_events(repo_path)?);

    // Sort by timestamp descending, then by kind for stability.
    events.sort_by(|a, b| {
        let a_ts = event_ts(a);
        let b_ts = event_ts(b);
        b_ts.cmp(&a_ts)
            .then_with(|| event_sort_key(a).cmp(&event_sort_key(b)))
    });

    Ok(HistoryReport {
        schema: "kaptaind.history.v1".to_string(),
        current_version,
        latest_tag,
        events,
    })
}

fn event_ts(event: &HistoryEvent) -> u64 {
    match event {
        HistoryEvent::VersionBump { ts, .. }
        | HistoryEvent::Commit { ts, .. }
        | HistoryEvent::Tag { ts, .. }
        | HistoryEvent::Push { ts, .. } => ts.unwrap_or(0),
    }
}

fn event_sort_key(event: &HistoryEvent) -> u8 {
    match event {
        HistoryEvent::Push { .. } => 0,
        HistoryEvent::VersionBump { .. } => 1,
        HistoryEvent::Tag { .. } => 2,
        HistoryEvent::Commit { .. } => 3,
    }
}

fn read_version(repo_path: &Path) -> String {
    let version_path = repo_path.join("VERSION");
    std::fs::read_to_string(&version_path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| {
            // Fall back to Cargo.toml [package].version.
            cargo_version(repo_path).unwrap_or_else(|| "0.0.0".to_string())
        })
}

fn cargo_version(repo_path: &Path) -> Option<String> {
    let manifest = std::fs::read_to_string(repo_path.join("Cargo.toml")).ok()?;
    let parsed: toml::Value = toml::from_str(&manifest).ok()?;
    parsed
        .get("package")?
        .get("version")?
        .as_str()
        .map(|s| s.to_string())
}

#[derive(Debug, Clone)]
struct TagInfo {
    target: Option<String>,
    ts: Option<u64>,
}

fn git_tags(repo_path: &Path) -> anyhow::Result<BTreeMap<String, TagInfo>> {
    let output = git(repo_path)
        .args(["tag", "--list", "--format=%(refname:short)\t%(objectname:short)\t%(creatordate:unix)"])
        .output()
        .context("failed to list git tags")?;

    if !output.status.success() {
        return Ok(BTreeMap::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut tags = BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        let name = parts.next().map(|s| s.trim().to_string());
        let target = parts.next().map(|s| s.trim().to_string());
        let ts = parts.next().and_then(|s| s.trim().parse().ok());
        if let Some(name) = name {
            // Treat semantic version tags as higher priority by prepending a
            // sortable key. Simple heuristic: keep raw name order, but v-prefixed
            // semver tags sort naturally because BTreeMap uses lexical order.
            tags.insert(
                name,
                TagInfo {
                    target: target.filter(|t| !t.is_empty()),
                    ts,
                },
            );
        }
    }
    Ok(tags)
}

#[derive(Debug, Clone)]
struct CommitInfo {
    sha: String,
    message: String,
    author: Option<String>,
    ts: Option<u64>,
}

fn git_log(repo_path: &Path, limit: usize) -> anyhow::Result<Vec<CommitInfo>> {
    let output = git(repo_path)
        .args([
            "log",
            "--pretty=format:%H\t%ct\t%an\t%s",
            "-n",
            &limit.to_string(),
        ])
        .output()
        .context("failed to run git log")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(4, '\t');
        let sha = parts.next().map(|s| s.trim().to_string());
        let ts = parts.next().and_then(|s| s.trim().parse().ok());
        let author = parts.next().map(|s| s.trim().to_string());
        let message = parts.next().unwrap_or("").trim().to_string();
        if let Some(sha) = sha {
            commits.push(CommitInfo {
                sha,
                message,
                author: author.filter(|a| !a.is_empty()),
                ts,
            });
        }
    }
    Ok(commits)
}

fn kaptaind_bump_events(
    repo_path: &Path,
    tags: &BTreeMap<String, TagInfo>,
) -> anyhow::Result<Vec<HistoryEvent>> {
    let mut events = Vec::new();
    let decisions_path = repo_path.join(".kaptaind").join("decisions.jsonl");
    if !decisions_path.exists() {
        return Ok(events);
    }

    let content = std::fs::read_to_string(&decisions_path).unwrap_or_default();
    for line in content.lines().filter(|l| !l.is_empty()).take(50) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            let _outcome = value.get("outcome").and_then(|v| v.as_str());
            let bump = value.get("bump").and_then(|v| v.as_str());
            let from = value
                .get("from_version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| previous_version_from_tags(tags));
            let to = value
                .get("to_version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| bump.map(|b| b.to_string()));
            let ts = value.get("ts").and_then(|v| v.as_u64());
            let commit = value
                .get("commit")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let (Some(from), Some(to)) = (from, to) {
                events.push(HistoryEvent::VersionBump { from, to, commit, ts });
            }
        }
    }
    Ok(events)
}

fn previous_version_from_tags(tags: &BTreeMap<String, TagInfo>) -> Option<String> {
    // The most recent tag before the latest.
    let mut iter = tags.keys().rev();
    iter.next()?; // skip latest
    iter.next().cloned()
}

fn reflog_push_events(repo_path: &Path) -> anyhow::Result<Vec<HistoryEvent>> {
    let output = git(repo_path)
        .args([
            "reflog",
            "show",
            "--pretty=format:%H\t%ct\t%gs",
            "-n",
            "20",
        ])
        .output()
        .context("failed to run git reflog")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut events = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, '\t');
        let sha = parts.next().map(|s| s.trim().to_string());
        let ts = parts.next().and_then(|s| s.trim().parse().ok());
        let gs = parts.next().unwrap_or("").trim();
        if let (Some(sha), Some(ts)) = (sha, ts) {
            if gs.starts_with("update by push") || gs.contains("push") {
                events.push(HistoryEvent::Push {
                    r#ref: "refs/heads/main".to_string(),
                    commit: Some(sha),
                    ts: Some(ts),
                });
            }
        }
    }
    Ok(events)
}

fn print_text(report: &HistoryReport) {
    println!(
        "{} {}",
        "🚢".blue(),
        "Kaptaind Version History".bold().blue()
    );
    println!("{}", "==========================".blue());
    println!(
        "{} {}",
        "Current version:".bold().cyan(),
        report.current_version.clone().magenta()
    );
    if let Some(ref tag) = report.latest_tag {
        println!("{} {}", "Latest tag:".bold().cyan(), tag.green());
    }
    println!();
    for event in &report.events {
        match event {
            HistoryEvent::VersionBump { from, to, commit, ts } => {
                let ts_str = ts.map(|t| format!(" @ {t}")).unwrap_or_default();
                let commit_str = commit.as_deref().unwrap_or("?");
                println!(
                    "{} {} → {} ({}){}",
                    "🔼".yellow(),
                    from.yellow(),
                    to.green(),
                    commit_str.cyan(),
                    ts_str.dimmed()
                );
            }
            HistoryEvent::Commit {
                sha,
                message,
                author,
                ts,
            } => {
                let ts_str = ts.map(|t| format!(" @ {t}")).unwrap_or_default();
                let author_str = author.as_deref().unwrap_or("?");
                println!(
                    "{} {} {} {}{}",
                    "📝".cyan(),
                    sha[..7.min(sha.len())].to_string().cyan(),
                    message.white(),
                    format!("({author_str})").dimmed(),
                    ts_str.dimmed()
                );
            }
            HistoryEvent::Tag { name, target, ts } => {
                let ts_str = ts.map(|t| format!(" @ {t}")).unwrap_or_default();
                let target_str = target.as_deref().unwrap_or("?");
                println!(
                    "{} {} → {}{}",
                    "🏷️ ".magenta(),
                    name.magenta(),
                    target_str.cyan(),
                    ts_str.dimmed()
                );
            }
            HistoryEvent::Push { r#ref, commit, ts } => {
                let ts_str = ts.map(|t| format!(" @ {t}")).unwrap_or_default();
                let commit_str = commit.as_deref().unwrap_or("?");
                println!(
                    "{} {} {}{}",
                    "🚀".green(),
                    r#ref.blue(),
                    commit_str.cyan(),
                    ts_str.dimmed()
                );
            }
        }
    }
}

fn git(repo_path: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_path);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn event_ts_defaults_to_zero() {
        let event = HistoryEvent::Tag {
            name: "v0.0.0".into(),
            target: None,
            ts: None,
        };
        assert_eq!(event_ts(&event), 0);
    }

    #[test]
    fn sort_orders_by_ts_desc_then_kind() {
        let mut events = vec![
            HistoryEvent::Commit {
                sha: "a".into(),
                message: "later commit".into(),
                author: None,
                ts: Some(100),
            },
            HistoryEvent::Tag {
                name: "v1".into(),
                target: None,
                ts: Some(100),
            },
            HistoryEvent::Commit {
                sha: "b".into(),
                message: "earlier commit".into(),
                author: None,
                ts: Some(50),
            },
        ];
        events.sort_by(|a, b| {
            let a_ts = event_ts(a);
            let b_ts = event_ts(b);
            b_ts.cmp(&a_ts)
                .then_with(|| event_sort_key(a).cmp(&event_sort_key(b)))
        });
        // Same TS: push (0) before bump (1) before tag (2) before commit (3).
        assert!(matches!(events[0], HistoryEvent::Tag { .. }));
        assert!(matches!(events[1], HistoryEvent::Commit { .. }));
        assert!(matches!(events[2], HistoryEvent::Commit { .. }));
    }

    #[test]
    fn parse_decisions_jsonl_into_bump_events() {
        let dir = std::env::temp_dir().join(format!("kaptaind-history-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".kaptaind")).unwrap();
        std::fs::write(dir.join("VERSION"), "1.2.3\n").unwrap();
        {
            let mut f = std::fs::File::create(dir.join(".kaptaind/decisions.jsonl")).unwrap();
            writeln!(
                f,
                r#"{{"outcome":"committed","bump":"patch","from_version":"1.2.2","to_version":"1.2.3","ts":1000}}"#
            )
            .unwrap();
        }

        let config = Config::default();
        // Config::default() uses current dir; we can't easily point it at `dir`
        // without a public setter, so just test the parser directly.
        let tags = BTreeMap::new();
        let events = kaptaind_bump_events(&dir, &tags).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            HistoryEvent::VersionBump { from, to, .. } => {
                assert_eq!(from, "1.2.2");
                assert_eq!(to, "1.2.3");
            }
            _ => panic!("expected VersionBump"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
