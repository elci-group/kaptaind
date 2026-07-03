use super::super::adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct JavaScriptAdapter;

impl LanguageAdapter for JavaScriptAdapter {
    fn name(&self) -> &'static str {
        "JavaScript"
    }
    fn language(&self) -> Language {
        Language::JAVASCRIPT
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
                ext == "js" || ext == "jsx" || ext == "cjs" || ext == "mjs"
            })
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("export ") {
                    let kind = classify_ts_export(rest);
                    symbols.push(super::super::adapter::Symbol {
                        name: rest.to_string(),
                        kind,
                    });
                } else if let Some(rest) = trimmed.strip_prefix("module.exports") {
                    symbols.push(super::super::adapter::Symbol {
                        name: rest.to_string(),
                        kind: "cjs_export".to_string(),
                    });
                }
                // React hooks
                if (trimmed.starts_with("export function use")
                    || trimmed.starts_with("export const use"))
                    && !trimmed.contains("// ")
                {
                    symbols.push(super::super::adapter::Symbol {
                        name: trimmed.to_string(),
                        kind: "hook".to_string(),
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
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}
