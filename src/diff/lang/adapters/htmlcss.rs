use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct HtmlCssAdapter;

impl LanguageAdapter for HtmlCssAdapter {
    fn name(&self) -> &'static str {
        "HTML/CSS"
    }
    fn language(&self) -> Language {
        Language::HTML_CSS
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
                ext == "html" || ext == "css"
            })
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if line.starts_with("--") && line.contains(':') {
                    symbols.push(Symbol {
                        name: line.to_string(),
                        kind: "css_var".to_string(),
                    });
                } else if line.starts_with('.') && line.contains('{') {
                    symbols.push(Symbol {
                        name: line.to_string(),
                        kind: "css_class".to_string(),
                    });
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation {
            symbols,
            structure_hash: hash,
            ..Default::default()
        })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface {
            public_symbols: ast.symbols.clone(),
            hash: ast.structure_hash,
        }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, _diff: &AstDiff) -> bool {
        false
    }
}
