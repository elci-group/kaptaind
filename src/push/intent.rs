//! Intent-based routing for Git-provider-saturated stack.
//!
//! This module implements the inference routing problem where Git providers
//! become specialized execution environments in a software distribution mesh.

use crate::config::loader::{IntentRouting, RemoteConfig};
use anyhow::Result;

/// Detect commit intent based on file patterns and commit message.
pub fn detect_intent(files: &[String], commit_message: &str, routing: &IntentRouting) -> String {
    if !routing.enabled {
        return routing.default_intent.clone();
    }

    for pattern in &routing.intent_patterns {
        // Check file patterns (simple glob matching)
        for file_pattern in &pattern.file_patterns {
            for file in files {
                if simple_glob_match(file_pattern, file) {
                    tracing::debug!(
                        intent = %pattern.intent,
                        file = %file,
                        pattern = %file_pattern,
                        "intent matched by file pattern"
                    );
                    return pattern.intent.clone();
                }
            }
        }

        // Check message patterns (simple substring matching for now)
        for msg_pattern in &pattern.message_patterns {
            if commit_message.contains(msg_pattern) {
                tracing::debug!(
                    intent = %pattern.intent,
                    pattern = %msg_pattern,
                    "intent matched by message pattern"
                );
                return pattern.intent.clone();
            }
        }
    }

    routing.default_intent.clone()
}

/// Simple glob pattern matching (without external dependencies).
fn simple_glob_match(pattern: &str, text: &str) -> bool {
    // Convert glob pattern to regex-like pattern
    let mut pattern_chars = pattern.chars().peekable();
    let mut regex_pattern = String::from("^");

    while let Some(c) = pattern_chars.next() {
        match c {
            '*' => {
                if pattern_chars.peek() == Some(&'*') {
                    pattern_chars.next(); // consume second *
                    regex_pattern.push_str(".*");
                } else {
                    regex_pattern.push_str("[^/]*");
                }
            }
            '?' => regex_pattern.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\' => {
                regex_pattern.push('\\');
                regex_pattern.push(c);
            }
            _ => regex_pattern.push(c),
        }
    }

    regex_pattern.push('$');

    // Simple regex matching (very basic implementation)
    basic_regex_match(&regex_pattern, text)
}

/// Very basic regex matching for common patterns.
fn basic_regex_match(pattern: &str, text: &str) -> bool {
    // Handle .* wildcard
    if pattern == "^.*$" {
        return true;
    }

    // Handle patterns ending with *
    if let Some(prefix) = pattern.strip_suffix("[^/]*$") {
        let pattern_prefix = &prefix[1..]; // remove ^
        return text.starts_with(pattern_prefix);
    }

    // Handle patterns starting with *
    if let Some(suffix) = pattern.strip_prefix("^.*") {
        let pattern_suffix = &suffix[..suffix.len() - 1]; // remove $
        return text.ends_with(pattern_suffix);
    }

    // Exact match for simple patterns
    if pattern.starts_with('^') && pattern.ends_with('$') {
        let exact = &pattern[1..pattern.len() - 1];
        return text == exact;
    }

    false
}

/// Select providers based on detected intent and provider capabilities.
pub fn select_providers_by_intent(intent: &str, remotes: &[RemoteConfig]) -> Vec<RemoteConfig> {
    let mut selected: Vec<_> = remotes
        .iter()
        .filter(|r| r.enabled && (r.intents.is_empty() || r.intents.iter().any(|i| i == intent)))
        .cloned()
        .collect();

    // Sort by priority (lower numbers first)
    selected.sort_by_key(|r| r.priority);

    // Ensure canonical source is always included if enabled
    let canonical = remotes.iter().find(|r| r.canonical && r.enabled);
    if let Some(can) = canonical {
        if !selected.iter().any(|r| r.name == can.name) {
            selected.push(can.clone());
        }
    }

    tracing::debug!(
        intent = %intent,
        count = selected.len(),
        providers = ?selected.iter().map(|r| &r.name).collect::<Vec<_>>(),
        "selected providers for intent"
    );

    selected
}

/// Provider capability matrix based on saturated stack roles.
pub struct ProviderCapabilities;

impl ProviderCapabilities {
    /// Get optimal providers for open-source visibility intent.
    pub fn oss_visibility() -> Vec<&'static str> {
        vec!["github", "codeberg", "gitlab"]
    }

    /// Get optimal providers for security-sensitive work.
    pub fn security_sensitive() -> Vec<&'static str> {
        vec!["gitlab", "gerrit", "azure"]
    }

    /// Get optimal providers for enterprise customer work.
    pub fn enterprise_customer() -> Vec<&'static str> {
        vec!["azure", "bitbucket", "aws"]
    }

    /// Get optimal providers for autonomous agent work.
    pub fn autonomous_agent() -> Vec<&'static str> {
        vec!["forgejo", "gitea", "gitlab"]
    }

    /// Get optimal providers for long-term archival.
    pub fn long_term_archival() -> Vec<&'static str> {
        vec!["codeberg", "sourcehut", "savannah"]
    }

    /// Get optimal providers for binary asset management.
    pub fn binary_assets() -> Vec<&'static str> {
        vec!["perforce", "gitlab", "github"]
    }

    /// Get optimal providers for minimal Unix development.
    pub fn minimalist_unix() -> Vec<&'static str> {
        vec!["sourcehut", "gitlab", "gitea"]
    }

    /// Get optimal providers for Ubuntu ecosystem.
    pub fn ubuntu_ecosystem() -> Vec<&'static str> {
        vec!["launchpad", "github", "gitlab"]
    }

    /// Get optimal providers for FSF/GNU ecosystem.
    pub fn fsf_ecosystem() -> Vec<&'static str> {
        vec!["savannah", "codeberg", "sourcehut"]
    }

    /// Get optimal providers for Fedora/RHEL ecosystem.
    pub fn fedora_ecosystem() -> Vec<&'static str> {
        vec!["pagure", "github", "gitlab"]
    }
}

/// Validate that remote configuration follows saturated stack best practices.
pub fn validate_saturated_config(remotes: &[RemoteConfig]) -> Result<()> {
    // Check for canonical source
    let canonical_count = remotes.iter().filter(|r| r.canonical).count();
    if canonical_count > 1 {
        anyhow::bail!(
            "multiple canonical sources configured ({}), only one allowed",
            canonical_count
        );
    }

    // Check for at least one public nexus if OSS intent is present
    let has_public_nexus = remotes.iter().any(|r| r.role == "public_nexus");
    let has_oss_intent = remotes.iter().any(|r| r.intents.iter().any(|i| i == "oss"));

    if has_oss_intent && !has_public_nexus {
        tracing::warn!("OSS intent configured but no public_nexus role found");
    }

    // Validate provider names
    let valid_providers = vec![
        "github",
        "gitlab",
        "bitbucket",
        "azure",
        "aws",
        "gcp",
        "codeberg",
        "sourcehut",
        "gitea",
        "forgejo",
        "gogs",
        "phabricator",
        "gerrit",
        "launchpad",
        "savannah",
        "pagure",
        "perforce",
    ];

    for remote in remotes {
        if !remote.provider.is_empty() && !valid_providers.contains(&remote.provider.as_str()) {
            tracing::warn!(
                provider = %remote.provider,
                "unknown provider type in saturated stack"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::loader::IntentPattern;

    #[test]
    fn test_intent_detection_by_file_pattern() {
        let routing = IntentRouting {
            enabled: true,
            default_intent: "general".to_string(),
            intent_patterns: vec![IntentPattern {
                intent: "oss".to_string(),
                file_patterns: vec!["README.md".to_string(), "LICENSE".to_string()],
                message_patterns: vec![],
            }],
        };

        let files = vec!["README.md".to_string(), "src/main.rs".to_string()];
        let intent = detect_intent(&files, "update readme", &routing);
        assert_eq!(intent, "oss");
    }

    #[test]
    fn test_intent_detection_by_message_pattern() {
        let routing = IntentRouting {
            enabled: true,
            default_intent: "general".to_string(),
            intent_patterns: vec![IntentPattern {
                intent: "security".to_string(),
                file_patterns: vec![],
                message_patterns: vec![
                    "security".to_string(),
                    "CVE".to_string(),
                    "vulnerability".to_string(),
                ],
            }],
        };

        let files = vec!["src/auth.rs".to_string()];
        let intent = detect_intent(&files, "fix security vulnerability CVE-2024-1234", &routing);
        assert_eq!(intent, "security");
    }

    #[test]
    fn test_provider_selection_by_intent() {
        let remotes = vec![
            RemoteConfig {
                name: "github".to_string(),
                provider: "github".to_string(),
                role: "public_nexus".to_string(),
                enabled: true,
                priority: 10,
                intents: vec!["oss".to_string(), "public".to_string()],
                canonical: true,
                backup: false,
                regional: false,
            },
            RemoteConfig {
                name: "gitlab".to_string(),
                provider: "gitlab".to_string(),
                role: "engineering_ops".to_string(),
                enabled: true,
                priority: 20,
                intents: vec!["security".to_string(), "ci".to_string()],
                canonical: false,
                backup: false,
                regional: false,
            },
        ];

        let selected = select_providers_by_intent("oss", &remotes);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "github");
    }

    #[test]
    fn test_canonical_always_included() {
        let remotes = vec![
            RemoteConfig {
                name: "github".to_string(),
                provider: "github".to_string(),
                role: "public_nexus".to_string(),
                enabled: true,
                priority: 10,
                intents: vec!["oss".to_string()],
                canonical: true,
                backup: false,
                regional: false,
            },
            RemoteConfig {
                name: "gitlab".to_string(),
                provider: "gitlab".to_string(),
                role: "engineering_ops".to_string(),
                enabled: true,
                priority: 20,
                intents: vec!["security".to_string()],
                canonical: false,
                backup: false,
                regional: false,
            },
        ];

        let selected = select_providers_by_intent("security", &remotes);
        assert_eq!(selected.len(), 2); // gitlab + canonical github
    }

    #[test]
    fn test_validate_single_canonical() {
        let remotes = vec![
            RemoteConfig {
                name: "github".to_string(),
                provider: "github".to_string(),
                role: "public_nexus".to_string(),
                enabled: true,
                priority: 10,
                intents: vec![],
                canonical: true,
                backup: false,
                regional: false,
            },
            RemoteConfig {
                name: "gitlab".to_string(),
                provider: "gitlab".to_string(),
                role: "engineering_ops".to_string(),
                enabled: true,
                priority: 20,
                intents: vec![],
                canonical: true, // Second canonical - should fail
                backup: false,
                regional: false,
            },
        ];

        let result = validate_saturated_config(&remotes);
        assert!(result.is_err());
    }
}
