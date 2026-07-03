use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use crate::diff::version::detector::parse_go_semver;
use std::path::{Path, PathBuf};

pub struct GoAdapter;

impl LanguageAdapter for GoAdapter {
    fn name(&self) -> &'static str {
        "Go"
    }
    fn language(&self) -> Language {
        Language::GO
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "go"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        go_parse(file, (1, 0))
    }
    fn parse_ast_versioned(&self, file: &Path, version: &str) -> anyhow::Result<AstRepresentation> {
        let ver = parse_go_semver(version);
        let mut ast = go_parse(file, ver)?;
        ast.version_tag = Some(version.to_string());
        Ok(ast)
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

fn go_parse(file: &Path, ver: (u32, u32)) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    if let Ok(lines) = read_lines_safe(file) {
        for line in lines {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("func ") {
                // Exported if first letter is uppercase
                let name_start = rest.chars().next().unwrap_or('a');
                if name_start.is_uppercase() {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "function".to_string(),
                    });
                }
                // Go 1.18+: generic functions look like `func Map[K comparable, V any](`
                if ver >= (1, 18) && rest.contains('[') {
                    let name = rest.split('[').next().unwrap_or("").trim();
                    if name.chars().next().unwrap_or('a').is_uppercase() {
                        symbols.push(Symbol {
                            name: format!("{name}[...]"),
                            kind: "generic_function".to_string(),
                        });
                    }
                }
            } else if let Some(rest) = line.strip_prefix("type ") {
                if rest.chars().next().unwrap_or('a').is_uppercase() {
                    let kind = if ver >= (1, 18) && rest.contains('[') {
                        "generic_type"
                    } else {
                        "type"
                    };
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: kind.to_string(),
                    });
                }
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
