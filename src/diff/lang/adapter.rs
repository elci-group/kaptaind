use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Go,
    TypeScript,
    JavaScript,
    Python,
    HtmlCss,
    Swift,
    Kotlin,
    Vue,
    Svelte,
    Astro,
    Scss,
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
    /// Language version used during this parse (e.g. "2021", "3.10").
    pub version_tag: Option<String>,
    /// Whether the fallback line scanner was used instead of a structured parser.
    pub fallback_used: bool,
    /// Which parser kind produced this representation.
    pub parser_kind: ParserKind,
}

/// Per-file parse metadata emitted into `.kaptaind/analysis/<uuid>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileParseMetadata {
    pub file: String,
    pub lang: String,
    pub version: String,
    pub parser_used: String,
    pub fallback_used: bool,
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
