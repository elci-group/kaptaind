//! Selective change capture for Angler.
//!
//! This module provides pattern-based filtering and capture of file changes,
//! allowing fine-grained control over which changes trigger specific actions.
//! Similar to how an angler selectively catches fish, this system selectively
//! captures changes based on configurable rules.

use crate::angler::config::{CaptureAction, CaptureRule, ChangeType, SelectiveConfig};
use anyhow::{anyhow, Result};
use regex_lite::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Result of evaluating a capture rule against a file change.
#[derive(Debug, Clone)]
pub struct CaptureResult {
    /// Whether the rule matched
    pub matched: bool,
    /// The rule that matched (if any)
    pub rule_id: Option<String>,
    /// The action to take
    pub action: CaptureAction,
    /// Metadata extracted during evaluation
    pub metadata: HashMap<String, String>,
}

impl CaptureResult {
    /// Create a "no match" result with default action.
    pub fn no_match(default_action: CaptureAction) -> Self {
        Self {
            matched: false,
            rule_id: None,
            action: default_action,
            metadata: HashMap::new(),
        }
    }

    /// Create a matched result.
    pub fn matched(rule_id: String, action: CaptureAction) -> Self {
        Self {
            matched: true,
            rule_id: Some(rule_id),
            action,
            metadata: HashMap::new(),
        }
    }
}

/// File change event for selective capture.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Path relative to repository root
    pub path: PathBuf,
    /// Absolute path
    pub absolute_path: PathBuf,
    /// Type of change
    pub change_type: ChangeType,
    /// File size in bytes
    pub size: u64,
    /// New content (if available)
    pub new_content: Option<String>,
    /// Old content (if available)
    pub old_content: Option<String>,
}

impl FileChange {
    /// Create a new file change.
    pub fn new(path: impl AsRef<Path>, change_type: ChangeType) -> Self {
        let path = path.as_ref().to_path_buf();
        Self {
            absolute_path: path.clone(),
            path,
            change_type,
            size: 0,
            new_content: None,
            old_content: None,
        }
    }

    /// Load file metadata.
    pub fn with_metadata(&mut self, repo_root: &Path) -> Result<()> {
        self.absolute_path = repo_root.join(&self.path);

        if let Ok(metadata) = std::fs::metadata(&self.absolute_path) {
            self.size = metadata.len();
        }

        Ok(())
    }

    /// Load content (respecting max size).
    pub fn with_content(mut self, max_size: u64) -> Result<Self> {
        if max_size > 0 && self.size > max_size {
            debug!(
                component = module_path!(),
                "Skipping content load for {}: {} bytes exceeds max {}",
                self.path.display(),
                self.size,
                max_size
            );
            return Ok(self);
        }

        if let Ok(content) = std::fs::read_to_string(&self.absolute_path) {
            self.new_content = Some(content);
        }

        Ok(self)
    }
}

/// Selective capture engine.
pub struct SelectiveEngine {
    config: SelectiveConfig,
    compiled_rules: Vec<CompiledRule>,
}

/// Compiled rule with pre-built matchers.
struct CompiledRule {
    id: String,
    name: String,
    file_patterns: Vec<glob::Pattern>,
    content_patterns: Vec<Regex>,
    change_types: Vec<ChangeType>,
    action: CaptureAction,
    priority: i32,
    enabled: bool,
    max_file_size: u64,
    metadata: HashMap<String, String>,
}

impl SelectiveEngine {
    /// Create a new selective capture engine.
    pub fn new(config: &SelectiveConfig) -> Result<Self> {
        let mut compiled_rules = Vec::new();

        for rule in &config.rules {
            match Self::compile_rule(rule) {
                Ok(compiled) => {
                    compiled_rules.push(compiled);
                }
                Err(e) => {
                    warn!(
                        component = module_path!(),
                        "Failed to compile rule {}: {}", rule.id, e
                    );
                }
            }
        }

        // Sort by priority (highest first)
        compiled_rules.sort_by_key(|b| std::cmp::Reverse(b.priority));

        info!(
            component = module_path!(),
            "Selective engine initialized with {} rules",
            compiled_rules.len()
        );

        Ok(Self {
            config: config.clone(),
            compiled_rules,
        })
    }

    /// Evaluate all rules against a file change.
    pub fn evaluate(&self, change: &FileChange) -> CaptureResult {
        if !self.config.enabled {
            return CaptureResult::no_match(self.config.default_action.clone());
        }

        for rule in &self.compiled_rules {
            if !rule.enabled {
                continue;
            }

            if self.rule_matches(rule, change) {
                debug!(
                    component = module_path!(),
                    "Rule {} matched for file {}",
                    rule.id,
                    change.path.display()
                );

                let mut result = CaptureResult::matched(rule.id.clone(), rule.action.clone());
                result.metadata = rule.metadata.clone();
                result.metadata.insert(
                    "matched_pattern".to_string(),
                    format!("{:?}", rule.file_patterns),
                );

                return result;
            }
        }

        // No rules matched, return default
        CaptureResult::no_match(self.config.default_action.clone())
    }

    /// Evaluate multiple changes in batch.
    pub fn evaluate_batch(&self, changes: &[FileChange]) -> Vec<(FileChange, CaptureResult)> {
        changes
            .iter()
            .map(|change| {
                let result = self.evaluate(change);
                (change.clone(), result)
            })
            .collect()
    }

    /// Check if any changes match a specific action type.
    pub fn has_matching_changes(
        &self,
        changes: &[FileChange],
        action_predicate: impl Fn(&CaptureAction) -> bool,
    ) -> bool {
        changes
            .iter()
            .any(|change| action_predicate(&self.evaluate(change).action))
    }

    /// Get all changes that match a specific action type.
    pub fn filter_by_action(
        &self,
        changes: &[FileChange],
        action_predicate: impl Fn(&CaptureAction) -> bool,
    ) -> Vec<(FileChange, CaptureResult)> {
        changes
            .iter()
            .filter_map(|change| {
                let result = self.evaluate(change);
                if action_predicate(&result.action) {
                    Some((change.clone(), result))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all blocked changes.
    pub fn get_blocked_changes(&self, changes: &[FileChange]) -> Vec<(FileChange, String)> {
        self.filter_by_action(changes, |action| matches!(action, CaptureAction::Block))
            .into_iter()
            .map(|(change, result)| {
                let reason = format!("Blocked by rule: {}", result.rule_id.unwrap_or_default());
                (change, reason)
            })
            .collect()
    }

    /// Get all quarantined changes.
    pub fn get_quarantined_changes(&self, changes: &[FileChange]) -> Vec<(FileChange, String)> {
        self.filter_by_action(changes, |action| {
            matches!(action, CaptureAction::Quarantine)
        })
        .into_iter()
        .map(|(change, result)| {
            let reason = format!(
                "Quarantined by rule: {}",
                result.rule_id.unwrap_or_default()
            );
            (change, reason)
        })
        .collect()
    }

    /// Get changes grouped by their tags.
    pub fn get_tagged_changes(&self, changes: &[FileChange]) -> HashMap<String, Vec<FileChange>> {
        let mut tagged: HashMap<String, Vec<FileChange>> = HashMap::new();

        for change in changes {
            let result = self.evaluate(change);
            if let CaptureAction::Tag { tags } = &result.action {
                for tag in tags {
                    tagged.entry(tag.clone()).or_default().push(change.clone());
                }
            }
        }

        tagged
    }

    /// Add a new rule at runtime.
    pub fn add_rule(&mut self, rule: &CaptureRule) -> Result<()> {
        let compiled = Self::compile_rule(rule)?;
        self.compiled_rules.push(compiled);
        // Re-sort by priority
        self.compiled_rules
            .sort_by_key(|b| std::cmp::Reverse(b.priority));
        Ok(())
    }

    /// Remove a rule by ID.
    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        let initial_len = self.compiled_rules.len();
        self.compiled_rules.retain(|r| r.id != rule_id);
        self.compiled_rules.len() < initial_len
    }

    /// Enable a rule.
    pub fn enable_rule(&mut self, rule_id: &str) -> bool {
        if let Some(rule) = self.compiled_rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable a rule.
    pub fn disable_rule(&mut self, rule_id: &str) -> bool {
        if let Some(rule) = self.compiled_rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = false;
            true
        } else {
            false
        }
    }

    /// Get all rule IDs.
    pub fn list_rules(&self) -> Vec<(&str, &str, bool)> {
        self.compiled_rules
            .iter()
            .map(|r| (r.id.as_str(), r.name.as_str(), r.enabled))
            .collect()
    }

    /// Get statistics about rule matches.
    pub fn get_statistics(&self) -> SelectiveStatistics {
        SelectiveStatistics {
            total_rules: self.compiled_rules.len(),
            enabled_rules: self.compiled_rules.iter().filter(|r| r.enabled).count(),
            rules_by_priority: self
                .compiled_rules
                .iter()
                .map(|r| (r.id.clone(), r.priority))
                .collect(),
        }
    }

    // =============================================================================
    // Internal Methods
    // =============================================================================

    fn compile_rule(rule: &CaptureRule) -> Result<CompiledRule> {
        // Compile file patterns
        let mut file_patterns = Vec::new();
        for pattern in &rule.patterns {
            match glob::Pattern::new(pattern) {
                Ok(p) => file_patterns.push(p),
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        %pattern,
                        operation = "compile_file_pattern",
                        "capture rule file pattern is invalid"
                    );
                    return Err(anyhow!("Invalid file pattern '{}': {}", pattern, e));
                }
            }
        }

        // Compile content patterns
        let mut content_patterns = Vec::new();
        for pattern in &rule.content_patterns {
            match Regex::new(pattern) {
                Ok(r) => content_patterns.push(r),
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        %pattern,
                        operation = "compile_content_pattern",
                        "capture rule content pattern is invalid"
                    );
                    return Err(anyhow!("Invalid content pattern '{}': {}", pattern, e));
                }
            }
        }

        Ok(CompiledRule {
            id: rule.id.clone(),
            name: rule.name.clone(),
            file_patterns,
            content_patterns,
            change_types: rule.change_types.clone(),
            action: rule.action.clone(),
            priority: rule.priority,
            enabled: rule.enabled,
            max_file_size: rule.max_file_size,
            metadata: rule.metadata.clone(),
        })
    }

    fn rule_matches(&self, rule: &CompiledRule, change: &FileChange) -> bool {
        // Check if rule applies to this change type
        if !rule.change_types.is_empty() && !rule.change_types.contains(&change.change_type) {
            return false;
        }

        // Check file size limit
        if rule.max_file_size > 0 && change.size > rule.max_file_size {
            debug!(
                component = module_path!(),
                "Skipping rule {} for {}: file size {} exceeds limit {}",
                rule.id,
                change.path.display(),
                change.size,
                rule.max_file_size
            );
            return false;
        }

        // Check file patterns
        let path_str = change.path.to_string_lossy();
        let file_matches = rule
            .file_patterns
            .iter()
            .any(|pattern| pattern.matches(&path_str));

        if !file_matches {
            return false;
        }

        // Check content patterns if present
        if !rule.content_patterns.is_empty() {
            if let Some(ref content) = change.new_content {
                let content_matches = rule
                    .content_patterns
                    .iter()
                    .any(|pattern| pattern.is_match(content));

                if !content_matches {
                    return false;
                }
            } else {
                // Can't check content without loading it
                debug!(
                    component = module_path!(),
                    "Skipping content check for {} - content not loaded",
                    change.path.display()
                );
            }
        }

        true
    }
}

/// Statistics about the selective engine.
#[derive(Debug, Clone)]
pub struct SelectiveStatistics {
    pub total_rules: usize,
    pub enabled_rules: usize,
    pub rules_by_priority: Vec<(String, i32)>,
}

/// Predefined rule templates for common use cases.
pub mod templates {
    use super::*;

    /// Security-sensitive file detection.
    pub fn security_sensitive_rule() -> CaptureRule {
        CaptureRule {
            id: "security-sensitive".to_string(),
            name: "Security Sensitive Files".to_string(),
            patterns: vec![
                "**/.env*".to_string(),
                "**/secrets*".to_string(),
                "**/*secret*".to_string(),
                "**/*password*".to_string(),
                "**/*key*.{pem,txt,key}".to_string(),
                "**/id_rsa*".to_string(),
                "**/*.p12".to_string(),
                "**/*.pfx".to_string(),
            ],
            content_patterns: vec![
                r#"(?i)(password|secret|api[_-]?key|token)\s*=\s*['"][^'"]+['"]"#.to_string(),
            ],
            change_types: vec![ChangeType::Added, ChangeType::Modified],
            action: CaptureAction::Block,
            priority: 100,
            enabled: true,
            max_file_size: 1024 * 1024, // 1MB
            metadata: {
                let mut m = HashMap::new();
                m.insert("category".to_string(), "security".to_string());
                m.insert("severity".to_string(), "critical".to_string());
                m
            },
        }
    }

    /// Large file detection.
    pub fn large_file_rule(max_size_mb: u64) -> CaptureRule {
        CaptureRule {
            id: "large-files".to_string(),
            name: "Large Files".to_string(),
            patterns: vec!["**/*".to_string()],
            content_patterns: vec![],
            change_types: vec![ChangeType::Added, ChangeType::Modified],
            action: CaptureAction::Quarantine,
            priority: 50,
            enabled: true,
            max_file_size: max_size_mb * 1024 * 1024,
            metadata: {
                let mut m = HashMap::new();
                m.insert("category".to_string(), "size".to_string());
                m.insert("max_size_mb".to_string(), max_size_mb.to_string());
                m
            },
        }
    }

    /// Documentation file tagging.
    pub fn documentation_rule() -> CaptureRule {
        CaptureRule {
            id: "documentation".to_string(),
            name: "Documentation Files".to_string(),
            patterns: vec![
                "**/*.md".to_string(),
                "**/README*".to_string(),
                "**/CHANGELOG*".to_string(),
                "**/docs/**".to_string(),
            ],
            content_patterns: vec![],
            change_types: vec![],
            action: CaptureAction::Tag {
                tags: vec!["documentation".to_string()],
            },
            priority: 10,
            enabled: true,
            max_file_size: 0,
            metadata: {
                let mut m = HashMap::new();
                m.insert("category".to_string(), "docs".to_string());
                m
            },
        }
    }

    /// Test file tagging.
    pub fn test_files_rule() -> CaptureRule {
        CaptureRule {
            id: "test-files".to_string(),
            name: "Test Files".to_string(),
            patterns: vec![
                "**/test*".to_string(),
                "**/*_test.*".to_string(),
                "**/*_spec.*".to_string(),
                "**/tests/**".to_string(),
                "**/__tests__/**".to_string(),
            ],
            content_patterns: vec![],
            change_types: vec![],
            action: CaptureAction::Tag {
                tags: vec!["tests".to_string()],
            },
            priority: 10,
            enabled: true,
            max_file_size: 0,
            metadata: {
                let mut m = HashMap::new();
                m.insert("category".to_string(), "tests".to_string());
                m
            },
        }
    }

    /// Configuration file tagging.
    pub fn config_files_rule() -> CaptureRule {
        CaptureRule {
            id: "config-files".to_string(),
            name: "Configuration Files".to_string(),
            patterns: vec![
                "**/*.toml".to_string(),
                "**/*.yaml".to_string(),
                "**/*.yml".to_string(),
                "**/*.json".to_string(),
                "**/config.*".to_string(),
                "**/.kaptaind.toml".to_string(),
            ],
            content_patterns: vec![],
            change_types: vec![],
            action: CaptureAction::Tag {
                tags: vec!["config".to_string()],
            },
            priority: 20,
            enabled: true,
            max_file_size: 0,
            metadata: {
                let mut m = HashMap::new();
                m.insert("category".to_string(), "config".to_string());
                m
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_change_creation() {
        let change = FileChange::new("test.rs", ChangeType::Added);
        assert_eq!(change.path, PathBuf::from("test.rs"));
        assert!(matches!(change.change_type, ChangeType::Added));
    }

    #[test]
    fn test_capture_result() {
        let result = CaptureResult::no_match(CaptureAction::Pass);
        assert!(!result.matched);
        assert!(matches!(result.action, CaptureAction::Pass));

        let result = CaptureResult::matched("rule1".to_string(), CaptureAction::Block);
        assert!(result.matched);
        assert_eq!(result.rule_id, Some("rule1".to_string()));
        assert!(matches!(result.action, CaptureAction::Block));
    }

    #[test]
    fn test_engine_with_simple_rule() {
        let config = SelectiveConfig {
            enabled: true,
            rules: vec![CaptureRule {
                id: "test".to_string(),
                name: "Test Rule".to_string(),
                patterns: vec!["**/*.rs".to_string()],
                content_patterns: vec![],
                change_types: vec![],
                action: CaptureAction::Tag {
                    tags: vec!["rust".to_string()],
                },
                priority: 10,
                enabled: true,
                max_file_size: 0,
                metadata: HashMap::new(),
            }],
            default_action: CaptureAction::Pass,
        };

        let engine = SelectiveEngine::new(&config).unwrap();

        let rust_change = FileChange::new("test.rs", ChangeType::Added);
        let result = engine.evaluate(&rust_change);
        assert!(result.matched);
        assert!(matches!(result.action, CaptureAction::Tag { .. }));

        let js_change = FileChange::new("test.js", ChangeType::Added);
        let result = engine.evaluate(&js_change);
        assert!(!result.matched);
    }

    #[test]
    fn test_security_rule() {
        let rule = templates::security_sensitive_rule();
        let config = SelectiveConfig {
            enabled: true,
            rules: vec![rule],
            default_action: CaptureAction::Pass,
        };

        let engine = SelectiveEngine::new(&config).unwrap();

        // Should match .env file
        let env_change = FileChange::new(".env", ChangeType::Added);
        let result = engine.evaluate(&env_change);
        assert!(result.matched);
        assert!(matches!(result.action, CaptureAction::Block));

        // Should not match regular file
        let normal_change = FileChange::new("main.rs", ChangeType::Added);
        let result = engine.evaluate(&normal_change);
        assert!(!result.matched);
    }

    #[test]
    fn test_priority_ordering() {
        let config = SelectiveConfig {
            enabled: true,
            rules: vec![
                CaptureRule {
                    id: "low".to_string(),
                    name: "Low Priority".to_string(),
                    patterns: vec!["**/*".to_string()],
                    content_patterns: vec![],
                    change_types: vec![],
                    action: CaptureAction::Pass,
                    priority: 1,
                    enabled: true,
                    max_file_size: 0,
                    metadata: HashMap::new(),
                },
                CaptureRule {
                    id: "high".to_string(),
                    name: "High Priority".to_string(),
                    patterns: vec!["**/*".to_string()],
                    content_patterns: vec![],
                    change_types: vec![],
                    action: CaptureAction::Block,
                    priority: 100,
                    enabled: true,
                    max_file_size: 0,
                    metadata: HashMap::new(),
                },
            ],
            default_action: CaptureAction::Pass,
        };

        let engine = SelectiveEngine::new(&config).unwrap();
        let change = FileChange::new("test.txt", ChangeType::Added);
        let result = engine.evaluate(&change);

        // High priority rule should match first
        assert!(result.matched);
        assert_eq!(result.rule_id, Some("high".to_string()));
        assert!(matches!(result.action, CaptureAction::Block));
    }

    #[test]
    fn test_add_remove_rules() {
        let config = SelectiveConfig {
            enabled: true,
            rules: vec![],
            default_action: CaptureAction::Pass,
        };

        let mut engine = SelectiveEngine::new(&config).unwrap();
        assert_eq!(engine.list_rules().len(), 0);

        let rule = CaptureRule {
            id: "test".to_string(),
            name: "Test".to_string(),
            patterns: vec!["**/*.rs".to_string()],
            content_patterns: vec![],
            change_types: vec![],
            action: CaptureAction::Pass,
            priority: 10,
            enabled: true,
            max_file_size: 0,
            metadata: HashMap::new(),
        };

        engine.add_rule(&rule).unwrap();
        assert_eq!(engine.list_rules().len(), 1);

        assert!(engine.remove_rule("test"));
        assert_eq!(engine.list_rules().len(), 0);
        assert!(!engine.remove_rule("test"));
    }

    #[test]
    fn test_enable_disable_rules() {
        let rule = CaptureRule {
            id: "test".to_string(),
            name: "Test".to_string(),
            patterns: vec!["**/*.rs".to_string()],
            content_patterns: vec![],
            change_types: vec![],
            action: CaptureAction::Block,
            priority: 10,
            enabled: true,
            max_file_size: 0,
            metadata: HashMap::new(),
        };

        let config = SelectiveConfig {
            enabled: true,
            rules: vec![rule],
            default_action: CaptureAction::Pass,
        };

        let mut engine = SelectiveEngine::new(&config).unwrap();

        let change = FileChange::new("test.rs", ChangeType::Added);

        // Rule is enabled, should match
        let result = engine.evaluate(&change);
        assert!(result.matched);

        // Disable rule
        assert!(engine.disable_rule("test"));
        let result = engine.evaluate(&change);
        assert!(!result.matched);

        // Enable rule
        assert!(engine.enable_rule("test"));
        let result = engine.evaluate(&change);
        assert!(result.matched);

        // Non-existent rule
        assert!(!engine.disable_rule("nonexistent"));
    }

    #[test]
    fn test_blocked_changes() {
        let config = SelectiveConfig {
            enabled: true,
            rules: vec![CaptureRule {
                id: "block-test".to_string(),
                name: "Block Test".to_string(),
                patterns: vec!["**/*.secret".to_string()],
                content_patterns: vec![],
                change_types: vec![],
                action: CaptureAction::Block,
                priority: 10,
                enabled: true,
                max_file_size: 0,
                metadata: HashMap::new(),
            }],
            default_action: CaptureAction::Pass,
        };

        let engine = SelectiveEngine::new(&config).unwrap();

        let changes = vec![
            FileChange::new("test.secret", ChangeType::Added),
            FileChange::new("normal.txt", ChangeType::Added),
        ];

        let blocked = engine.get_blocked_changes(&changes);
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].0.path, PathBuf::from("test.secret"));
    }

    #[test]
    fn test_tagged_changes() {
        let config = SelectiveConfig {
            enabled: true,
            rules: vec![
                templates::documentation_rule(),
                templates::test_files_rule(),
            ],
            default_action: CaptureAction::Pass,
        };

        let engine = SelectiveEngine::new(&config).unwrap();

        let changes = vec![
            FileChange::new("README.md", ChangeType::Modified),
            FileChange::new("test_main.rs", ChangeType::Added),
            FileChange::new("main.rs", ChangeType::Modified),
        ];

        let tagged = engine.get_tagged_changes(&changes);
        assert!(tagged.contains_key("documentation"));
        assert!(tagged.contains_key("tests"));
        assert_eq!(tagged["documentation"].len(), 1);
        assert_eq!(tagged["tests"].len(), 1);
    }

    #[test]
    fn test_change_type_filtering() {
        let config = SelectiveConfig {
            enabled: true,
            rules: vec![CaptureRule {
                id: "added-only".to_string(),
                name: "Added Only".to_string(),
                patterns: vec!["**/*".to_string()],
                content_patterns: vec![],
                change_types: vec![ChangeType::Added],
                action: CaptureAction::Tag {
                    tags: vec!["new".to_string()],
                },
                priority: 10,
                enabled: true,
                max_file_size: 0,
                metadata: HashMap::new(),
            }],
            default_action: CaptureAction::Pass,
        };

        let engine = SelectiveEngine::new(&config).unwrap();

        let added = FileChange::new("test.rs", ChangeType::Added);
        let modified = FileChange::new("test.rs", ChangeType::Modified);

        assert!(engine.evaluate(&added).matched);
        assert!(!engine.evaluate(&modified).matched);
    }
}
