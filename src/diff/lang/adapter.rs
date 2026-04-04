use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Go,
    TypeScript,
    JavaScript,
    Python,
    HtmlCss,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct AstRepresentation {
    pub symbols: Vec<Symbol>,
    pub structure_hash: u64,
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
    
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface;
    
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff;
    
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool;
}
