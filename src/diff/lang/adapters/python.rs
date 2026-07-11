use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use crate::diff::version::detector::parse_python_semver;
use std::path::{Path, PathBuf};

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn name(&self) -> &'static str {
        "Python"
    }
    fn language(&self) -> Language {
        Language::PYTHON
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "py"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        python_parse(file, (3, 0))
    }
    fn parse_ast_versioned(&self, file: &Path, version: &str) -> anyhow::Result<AstRepresentation> {
        let ver = parse_python_semver(version);
        let mut ast = python_parse(file, ver)?;
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

fn python_parse(file: &Path, ver: (u32, u32)) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    if let Ok(lines) = read_lines_safe(file) {
        // PEP 8: a single leading underscore marks an internal name; those are
        // not public API surface. Dunder names (`__init__`, ...) are kept.
        let is_public = |rest: &str| {
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            !ident.starts_with('_') || ident.starts_with("__")
        };
        let mut in_docstring = false;
        for line in lines {
            let line = line.trim();
            // Track triple-quoted regions (docstrings) so `def`/`class` text
            // inside them is not mistaken for public API (measured messy FP, rev 24).
            let triples = line.matches("\"\"\"").count() + line.matches("'''").count();
            if in_docstring {
                if triples % 2 == 1 {
                    in_docstring = false;
                }
                continue;
            }
            if triples % 2 == 1 {
                in_docstring = true;
                continue;
            }
            if let Some(rest) = line.strip_prefix("def ") {
                if is_public(rest) {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "function".to_string(),
                    });
                }
            } else if let Some(rest) = line.strip_prefix("class ") {
                if is_public(rest) {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "class".to_string(),
                    });
                }
            } else if let Some(rest) = line.strip_prefix("async def ") {
                if is_public(rest) {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "async_function".to_string(),
                    });
                }
            }
            // Python 3.10+: match/case structural pattern matching
            if ver >= (3, 10) {
                if let Some(rest) = line.strip_prefix("match ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "match_statement".to_string(),
                    });
                }
            }
            // Python 3.12+: soft type aliases  (type X = ...)
            if ver >= (3, 12) {
                if let Some(rest) = line.strip_prefix("type ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "type_alias".to_string(),
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
