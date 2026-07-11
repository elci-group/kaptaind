use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Language(pub &'static str);

impl Language {
    pub const RUST: Language = Language("rust");
    pub const GO: Language = Language("go");
    pub const TYPESCRIPT: Language = Language("typescript");
    pub const JAVASCRIPT: Language = Language("javascript");
    pub const PYTHON: Language = Language("python");
    pub const HTML_CSS: Language = Language("htmlcss");
    pub const SWIFT: Language = Language("swift");
    pub const KOTLIN: Language = Language("kotlin");
    pub const VUE: Language = Language("vue");
    pub const SVELTE: Language = Language("svelte");
    pub const ASTRO: Language = Language("astro");
    pub const SCSS: Language = Language("scss");
    /// Handled by an external plugin adapter (JSON stdio protocol).
    pub const PLUGIN: Language = Language("plugin");

    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
}

/// How a file was parsed (stored in analysis artifacts for auditing).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserKind {
    /// Language-specific AST or structured scanner.
    #[default]
    Ast,
    /// Fell back to generic line-based heuristics.
    FallbackLineScanner,
}

/// Result of parsing a single file, including LV-SCL metadata.
#[derive(Debug, Clone, Default)]
pub struct AstRepresentation {
    pub symbols: Vec<Symbol>,
    pub structure_hash: u64,
    /// Optional per-symbol raw signature, keyed by the stable symbol `name`. Adapters that
    /// can produce a signature (e.g. JS/TS function exports) populate it so the diff can flag
    /// arity / return-type / parameter changes as `modified`. Empty by default; adapters
    /// opt in, so adding it changes nothing for adapters that leave it empty.
    pub signatures: std::collections::HashMap<String, String>,
    /// Language version used during this parse (e.g. "2021", "3.10").
    pub version_tag: Option<String>,
    /// Whether the fallback line scanner was used instead of a structured parser.
    pub fallback_used: bool,
    /// Which parser kind produced this representation.
    pub parser_kind: ParserKind,
}

/// Version match confidence level.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionMatch {
    /// Version was detected from a runtime source (highest confidence).
    Runtime,
    /// Version was detected from a manifest (high confidence).
    Manifest,
    /// Version was inferred or defaults were used (medium confidence).
    Inferred,
    /// Version could not be determined; system proceeded with defaults.
    #[default]
    Unknown,
}

/// Per-file parse metadata emitted into `.kaptaind/analysis/<uuid>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileParseMetadata {
    pub file: String,
    pub lang: String,
    pub version: String,
    pub parser_used: String,
    pub fallback_used: bool,
    /// Parser confidence (0.0–1.0). Adjusted down if version is uncertain or fallback was used.
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// How the version was resolved (runtime > manifest > inferred > unknown).
    #[serde(default)]
    pub version_match: VersionMatch,
}

fn default_confidence() -> f64 {
    1.0
}

#[derive(Debug, Clone, Default)]
pub struct ApiSurface {
    pub public_symbols: Vec<Symbol>,
    pub hash: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AstDiff {
    pub added: Vec<Symbol>,
    pub removed: Vec<Symbol>,
    pub modified: Vec<Symbol>,
}

#[derive(Debug, Clone)]
pub struct CrossLangLink {
    pub source: PathBuf,
    pub target: PathBuf,
    pub relation: String, // "Imports", "HTTP API", "DOM Binding"
}

pub trait LanguageAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn language(&self) -> Language;

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf>;

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation>;

    /// Version-aware parse. Default delegates to `parse_ast` and stamps the
    /// version tag. Adapters can override to enable version-specific syntax.
    fn parse_ast_versioned(&self, file: &Path, version: &str) -> anyhow::Result<AstRepresentation> {
        let mut ast = self.parse_ast(file)?;
        ast.version_tag = Some(version.to_string());
        Ok(ast)
    }

    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface;

    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff;

    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool;
}
