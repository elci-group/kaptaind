//! `.orb` file sanitization for commits that may reach a public remote.
//!
//! `.orb` files (see the `orbweaver` project's `orbweaver-connectors`
//! crate) are free-form TOML a repository carries to opt itself in to
//! Orbweaver's ELCI-connector discovery — binary hints, install policy,
//! and whatever fields that format grows over time. Nothing in the
//! format's current schema is a secret, but kaptaind has no way to know
//! in advance what a future field will hold, or what a maintainer might
//! paste into one by mistake (an internal endpoint, a stray token left
//! over from local testing). So every staged `.orb` file is scanned for
//! sensitive-looking keys and values before a commit that a public
//! remote could see, and matches are redacted — in the STAGED content
//! only, via `git hash-object` + `update-index --cacheinfo`. The working
//! tree file is never touched: a developer's local `.orb` can carry
//! whatever their own workflow needs; only what actually gets committed
//! to a public repo is scrubbed.
//!
//! This is deliberately *not* the existing `SECRET_DENYLIST` mechanism
//! in `commit::orchestrator` (which excludes a whole file from staging
//! outright). `.orb` is meant to be shared — a blunt block would defeat
//! the point of the file existing at all. Sanitizing preserves every
//! sharable field and only touches the ones that look sensitive.
//!
//! # Fail-closed on unknown visibility
//!
//! If the target remote's visibility can't be confirmed (`gh`
//! unavailable, not a GitHub remote, a network hiccup), the default is
//! to sanitize anyway. Skipping sanitization is only safe when a repo is
//! *confirmed* private; "we don't know" must never be treated the same
//! as "we know it's private" — that would make network flakiness a way
//! to accidentally leak a secret.

use crate::config::loader::OrbSanitizeConfig;
use crate::git::repo::RepoContext;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use toml_edit::{DocumentMut, Item, Table, Value};

const REDACTED_PLACEHOLDER: &str = "[REDACTED-BY-KAPTAIND-ORB-SANITIZE]";

/// Built-in key-name fragments (case-insensitive substring match against
/// the final segment of a dotted key) that mark a value as sensitive
/// regardless of its shape. Mirrors the spirit of `SECRET_DENYLIST` and
/// the `security_sensitive_rule` content pattern in `angler::selective`,
/// applied to arbitrary TOML keys instead of a fixed regex.
const SENSITIVE_KEY_FRAGMENTS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "access_key",
    "private_key",
    "credential",
    "auth",
    "session",
    "cookie",
    "client_secret",
];

/// Value prefixes recognisable as a specific provider's credential
/// format, regardless of what key they're stored under.
const KNOWN_SECRET_PREFIXES: &[&str] =
    &["gsk_", "ghp_", "gho_", "github_pat_", "sk-", "AKIA", "xox", "AIza", "-----BEGIN"];

fn is_sensitive_key_name(key: &str, extra_fragments: &[String]) -> bool {
    let lower = key.to_lowercase();
    SENSITIVE_KEY_FRAGMENTS.iter().any(|frag| lower.contains(frag))
        || extra_fragments.iter().any(|frag| lower.contains(&frag.to_lowercase()))
}

/// Whether a string value looks like a credential even under an
/// innocuously-named key. Deliberately conservative: a long
/// alphanumeric-with-separators string containing both letters and
/// digits with no whitespace, or a recognised provider prefix. False
/// negatives (a real secret with an unusual shape slipping through) are
/// safer to tolerate here than false positives flooding ordinary
/// descriptions/versions with redactions on every sanitize pass.
fn looks_like_a_secret_value(value: &str) -> bool {
    if KNOWN_SECRET_PREFIXES.iter().any(|p| value.starts_with(p)) {
        return true;
    }
    value.len() >= 24
        && !value.contains(' ')
        && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '+' | '='))
        && value.chars().any(|c| c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_alphabetic())
}

#[derive(Debug, Default)]
pub struct SanitizeReport {
    pub sanitized_toml: String,
    /// Dotted-path keys that had their value redacted, for logging —
    /// never the values themselves.
    pub redacted_keys: Vec<String>,
}

/// Redact sensitive values from a `.orb` TOML document, preserving
/// structure, formatting, and comments for everything else via
/// `toml_edit` — this is meant to stay genuinely readable and shareable,
/// not become a wall of redactions.
pub fn sanitize_orb_toml(content: &str, extra_fragments: &[String]) -> Result<SanitizeReport> {
    let mut doc: DocumentMut = content.parse().context("failed to parse .orb content as TOML")?;
    let mut redacted = Vec::new();
    sanitize_table(doc.as_table_mut(), "", extra_fragments, &mut redacted);
    Ok(SanitizeReport {
        sanitized_toml: doc.to_string(),
        redacted_keys: redacted,
    })
}

fn sanitize_table(table: &mut Table, path_prefix: &str, extra_fragments: &[String], redacted: &mut Vec<String>) {
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    for key in keys {
        let full_key = if path_prefix.is_empty() {
            key.clone()
        } else {
            format!("{path_prefix}.{key}")
        };
        let force = is_sensitive_key_name(&key, extra_fragments);
        if let Some(item) = table.get_mut(&key) {
            sanitize_item(item, &full_key, force, extra_fragments, redacted);
        }
    }
}

fn sanitize_item(item: &mut Item, full_key: &str, force: bool, extra_fragments: &[String], redacted: &mut Vec<String>) {
    match item {
        Item::Table(t) => sanitize_table(t, full_key, extra_fragments, redacted),
        Item::ArrayOfTables(arr) => {
            for (i, t) in arr.iter_mut().enumerate() {
                sanitize_table(t, &format!("{full_key}[{i}]"), extra_fragments, redacted);
            }
        }
        Item::Value(v) => sanitize_value(v, full_key, force, extra_fragments, redacted),
        Item::None => {}
    }
}

fn sanitize_value(value: &mut Value, full_key: &str, force: bool, extra_fragments: &[String], redacted: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            let raw = s.value().clone();
            if force || looks_like_a_secret_value(&raw) {
                *value = Value::from(REDACTED_PLACEHOLDER);
                redacted.push(full_key.to_string());
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter_mut().enumerate() {
                sanitize_value(v, &format!("{full_key}[{i}]"), force, extra_fragments, redacted);
            }
        }
        Value::InlineTable(it) => {
            let keys: Vec<String> = it.iter().map(|(k, _)| k.to_string()).collect();
            for key in keys {
                let full = format!("{full_key}.{key}");
                let sub_force = force || is_sensitive_key_name(&key, extra_fragments);
                if let Some(v) = it.get_mut(&key) {
                    sanitize_value(v, &full, sub_force, extra_fragments, redacted);
                }
            }
        }
        _ => {}
    }
}

/// Best-effort GitHub visibility check for `repo_path`'s `origin`
/// remote. `None` means "couldn't determine" (no `gh`, not a GitHub
/// remote, network failure, private-without-access) — callers must
/// treat that as "assume public," never as "assume private."
fn is_confirmed_private(repo_path: &Path) -> Option<bool> {
    let remote_url = git_output(repo_path, &["remote", "get-url", "origin"])?;
    let (owner, repo) = parse_github_owner_repo(remote_url.trim())?;

    let output = Command::new("gh")
        .args(["repo", "view", &format!("{owner}/{repo}"), "--json", "isPrivate", "--jq", ".isPrivate"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    match String::from_utf8_lossy(&output.stdout).trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Parse `owner/repo` out of either SSH (`git@github.com:owner/repo.git`)
/// or HTTPS (`https://github.com/owner/repo.git`) remote URL forms.
fn parse_github_owner_repo(url: &str) -> Option<(String, String)> {
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.splitn(2, '/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

fn git_output(repo_path: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(repo_path).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Whether staged `.orb` files should be sanitized for this repo, given
/// config and (fail-closed) visibility detection.
fn should_sanitize(repo_path: &Path, config: &OrbSanitizeConfig) -> bool {
    if !config.enabled {
        return false;
    }
    match is_confirmed_private(repo_path) {
        Some(true) => false,
        Some(false) => true,
        None => config.sanitize_when_visibility_unknown,
    }
}

/// List currently-staged paths (relative to `repo_path`) whose filename
/// is exactly `.orb`.
fn staged_orb_paths(repo_path: &Path) -> Result<Vec<String>> {
    let output = git_output(repo_path, &["diff", "--cached", "--name-only", "--diff-filter=ACM"])
        .context("failed to list staged paths")?;
    Ok(output
        .lines()
        .filter(|line| Path::new(line).file_name().and_then(|n| n.to_str()) == Some(".orb"))
        .map(str::to_string)
        .collect())
}

/// After the normal staging step, redact sensitive values from any
/// staged `.orb` file when this repo's remote visibility isn't confirmed
/// private. Rewrites the git INDEX entry for each affected path directly
/// (`hash-object -w` + `update-index --cacheinfo`) — the working tree
/// file is never modified. Returns a summary per file for logging
/// (`path -> [redacted.key.names]`, never the redacted values).
pub fn sanitize_staged_orb_files(ctx: &RepoContext, config: &OrbSanitizeConfig) -> Result<Vec<(String, Vec<String>)>> {
    if !should_sanitize(&ctx.git_root, config) {
        return Ok(Vec::new());
    }

    let paths = staged_orb_paths(&ctx.git_root)?;
    let mut summary = Vec::new();

    for path in paths {
        // Read the STAGED (index) content, not the working-tree file —
        // they're normally identical right after `git add`, but the
        // index is the thing actually about to be committed.
        let Some(staged_content) = git_output(&ctx.git_root, &["show", &format!(":{path}")]) else {
            continue;
        };

        let report = match sanitize_orb_toml(&staged_content, &config.extra_sensitive_key_fragments) {
            Ok(report) => report,
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "failed to parse staged .orb as TOML — leaving it unsanitized");
                continue;
            }
        };

        if report.redacted_keys.is_empty() {
            continue;
        }

        replace_staged_blob(&ctx.git_root, &path, &report.sanitized_toml)?;
        summary.push((path, report.redacted_keys));
    }

    Ok(summary)
}

/// Write `content` as a new git blob and point the index entry for
/// `path` at it, without touching the working tree file.
fn replace_staged_blob(repo_path: &Path, path: &str, content: &str) -> Result<()> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["hash-object", "-w", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to spawn git hash-object")?;
    child
        .stdin
        .take()
        .context("hash-object stdin unavailable")?
        .write_all(content.as_bytes())
        .context("failed to write to git hash-object stdin")?;
    let output = child.wait_with_output().context("git hash-object failed")?;
    anyhow::ensure!(output.status.success(), "git hash-object failed for {path}");
    let blob_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let status = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["update-index", "--cacheinfo", &format!("100644,{blob_sha},{path}")])
        .status()
        .context("failed to run git update-index")?;
    anyhow::ensure!(status.success(), "git update-index failed for {path}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_sensitive_key_names_but_keeps_everything_else() {
        let toml = r#"
version = 1

[tool]
binaries = ["kaptaind", "kaptaind-cli"]
group = "release-management"

[install]
installable = true
prefer_release_binary = true

[groq]
api_key = "gsk_FAKEKEYFAKEKEYFAKEKEYFAKEKEYFAKEKEYFA"
model = "openai/gpt-oss-120b"
"#;
        let report = sanitize_orb_toml(toml, &[]).unwrap();
        assert_eq!(report.redacted_keys, vec!["groq.api_key".to_string()]);
        assert!(report.sanitized_toml.contains(REDACTED_PLACEHOLDER));
        // Sharable data survives untouched.
        assert!(report.sanitized_toml.contains(r#"group = "release-management""#));
        assert!(report.sanitized_toml.contains(r#"model = "openai/gpt-oss-120b""#));
        assert!(report.sanitized_toml.contains("installable = true"));
        assert!(!report.sanitized_toml.contains("gsk_FAKEKEYFAKEKEYFAKEKEYFAKEKEYFAKEKEYFA"));
    }

    #[test]
    fn redacts_secret_shaped_values_under_innocuous_keys() {
        let toml = r#"
[tool]
endpoint_note = "gho_1234567890abcdefghijklmnopqrstuvwx"
"#;
        let report = sanitize_orb_toml(toml, &[]).unwrap();
        assert_eq!(report.redacted_keys, vec!["tool.endpoint_note".to_string()]);
    }

    #[test]
    fn does_not_flag_ordinary_short_or_wordy_values() {
        let toml = r#"
version = 1

[tool]
group = "release-management"
description = "Deterministic build-surface maintenance and hygiene agent"

[install]
installable = true
"#;
        let report = sanitize_orb_toml(toml, &[]).unwrap();
        assert!(report.redacted_keys.is_empty(), "unexpected redactions: {:?}", report.redacted_keys);
        assert_eq!(report.sanitized_toml, toml);
    }

    #[test]
    fn extra_configured_fragments_are_honoured() {
        let toml = r#"
[tool]
internal_hostname_hint = "corp-internal-01"
"#;
        let no_extra = sanitize_orb_toml(toml, &[]).unwrap();
        assert!(no_extra.redacted_keys.is_empty());

        let with_extra = sanitize_orb_toml(toml, &["hostname".to_string()]).unwrap();
        assert_eq!(with_extra.redacted_keys, vec!["tool.internal_hostname_hint".to_string()]);
    }

    #[test]
    fn nested_arrays_of_tables_are_walked() {
        let toml = r#"
[[endpoints]]
name = "primary"
token = "ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[[endpoints]]
name = "secondary"
token = "ghp_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
"#;
        let report = sanitize_orb_toml(toml, &[]).unwrap();
        assert_eq!(report.redacted_keys.len(), 2);
        assert!(report.redacted_keys.contains(&"endpoints[0].token".to_string()));
        assert!(report.redacted_keys.contains(&"endpoints[1].token".to_string()));
        assert!(report.sanitized_toml.contains(r#"name = "primary""#));
        assert!(report.sanitized_toml.contains(r#"name = "secondary""#));
    }

    #[test]
    fn github_owner_repo_parses_both_ssh_and_https_remotes() {
        assert_eq!(
            parse_github_owner_repo("git@github.com:elci-group/kaptaind.git"),
            Some(("elci-group".to_string(), "kaptaind".to_string()))
        );
        assert_eq!(
            parse_github_owner_repo("https://github.com/elci-group/kaptaind.git"),
            Some(("elci-group".to_string(), "kaptaind".to_string()))
        );
        assert_eq!(parse_github_owner_repo("https://gitlab.com/elci-group/kaptaind.git"), None);
    }

    #[test]
    fn should_sanitize_fails_closed_when_disabled_flag_is_off() {
        let config = OrbSanitizeConfig {
            enabled: false,
            ..OrbSanitizeConfig::default()
        };
        // Even a path with no git remote at all (visibility unknowable)
        // must stay off when the master switch is off.
        assert!(!should_sanitize(Path::new("/nonexistent-path-for-test"), &config));
    }

    #[test]
    fn malformed_orb_content_is_an_error_not_a_panic() {
        let result = sanitize_orb_toml("this is not { valid toml", &[]);
        assert!(result.is_err());
    }

    struct TestRepo {
        dir: tempfile::TempDir,
    }

    impl TestRepo {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let repo = Self { dir };
            repo.run(&["init"]);
            repo.run(&["config", "user.name", "Kaptaind Test"]);
            repo.run(&["config", "user.email", "kaptaind@example.com"]);
            repo
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }

        fn write(&self, path: &str, contents: &str) {
            let full = self.dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, contents).unwrap();
        }

        fn read(&self, path: &str) -> String {
            std::fs::read_to_string(self.dir.path().join(path)).unwrap()
        }

        fn run(&self, args: &[&str]) {
            let output = self.output(args);
            assert!(output.status.success(), "git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
        }

        fn output(&self, args: &[&str]) -> std::process::Output {
            Command::new("git").arg("-C").arg(self.path()).args(args).output().unwrap()
        }

        fn staged_content(&self, path: &str) -> String {
            let output = self.output(&["show", &format!(":{path}")]);
            assert!(output.status.success(), "git show :{path} failed");
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
    }

    /// End-to-end: a real staged `.orb` file with a real secret shape,
    /// sanitized in place in the git index — proving the whole mechanism
    /// this module exists for, not just the pure TOML logic in isolation.
    #[test]
    fn sanitizes_staged_orb_content_without_touching_the_working_tree() {
        let repo = TestRepo::new();
        let orb_content = "version = 1\n\n[tool]\nbinaries = [\"widget\"]\n\n[groq]\napi_key = \"gsk_FAKEKEYFAKEKEYFAKEKEYFAKEKEYFAKEKEYFA\"\n";
        repo.write(".orb", orb_content);
        repo.run(&["add", "-A"]);

        let ctx = RepoContext::single(repo.path());
        let config = OrbSanitizeConfig::default();
        let redactions = sanitize_staged_orb_files(&ctx, &config).unwrap();

        assert_eq!(redactions.len(), 1);
        assert_eq!(redactions[0].0, ".orb");
        assert_eq!(redactions[0].1, vec!["groq.api_key".to_string()]);

        // Working tree is untouched — the developer's local file still
        // has the real value.
        assert_eq!(repo.read(".orb"), orb_content);

        // The STAGED content is sanitized.
        let staged = repo.staged_content(".orb");
        assert!(staged.contains(REDACTED_PLACEHOLDER));
        assert!(!staged.contains("gsk_FAKEKEYFAKEKEYFAKEKEYFAKEKEYFAKEKEYFA"));
        assert!(staged.contains("binaries = [\"widget\"]"));

        // Committing now must not resurrect the secret.
        repo.run(&["commit", "-m", "add .orb"]);
        let committed = repo.output(&["show", "HEAD:.orb"]);
        let committed_text = String::from_utf8_lossy(&committed.stdout);
        assert!(!committed_text.contains("gsk_FAKEKEYFAKEKEYFAKEKEYFAKEKEYFAKEKEYFA"));
    }

    #[test]
    fn leaves_staged_orb_content_alone_when_nothing_is_sensitive() {
        let repo = TestRepo::new();
        let orb_content = "version = 1\n\n[tool]\nbinaries = [\"widget\"]\n";
        repo.write(".orb", orb_content);
        repo.run(&["add", "-A"]);

        let ctx = RepoContext::single(repo.path());
        let config = OrbSanitizeConfig::default();
        let redactions = sanitize_staged_orb_files(&ctx, &config).unwrap();

        assert!(redactions.is_empty());
        assert_eq!(repo.staged_content(".orb"), orb_content);
    }

    #[test]
    fn does_nothing_when_disabled() {
        let repo = TestRepo::new();
        let orb_content = "[groq]\napi_key = \"gsk_FAKEKEYFAKEKEYFAKEKEYFAKEKEYFAKEKEYFA\"\n";
        repo.write(".orb", orb_content);
        repo.run(&["add", "-A"]);

        let ctx = RepoContext::single(repo.path());
        let config = OrbSanitizeConfig {
            enabled: false,
            ..OrbSanitizeConfig::default()
        };
        let redactions = sanitize_staged_orb_files(&ctx, &config).unwrap();

        assert!(redactions.is_empty());
        assert_eq!(repo.staged_content(".orb"), orb_content);
    }
}
