use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct SwiftAdapter;

impl LanguageAdapter for SwiftAdapter {
    fn name(&self) -> &'static str {
        "Swift"
    }
    fn language(&self) -> Language {
        Language::SWIFT
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "swift"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                // Public/open declarations
                if trimmed.starts_with("public ") || trimmed.starts_with("open ") {
                    let rest = trimmed
                        .strip_prefix("public ")
                        .or_else(|| trimmed.strip_prefix("open "))
                        .unwrap_or("");
                    if let Some(name) = rest.strip_prefix("func ") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "function".to_string(),
                        });
                    } else if let Some(name) = rest.strip_prefix("class ") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "class".to_string(),
                        });
                    } else if let Some(name) = rest.strip_prefix("struct ") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "struct".to_string(),
                        });
                    } else if let Some(name) = rest.strip_prefix("enum ") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "enum".to_string(),
                        });
                    } else if let Some(name) = rest.strip_prefix("protocol ") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "protocol".to_string(),
                        });
                    } else if let Some(name) = rest.strip_prefix("var ") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "property".to_string(),
                        });
                    } else if let Some(name) = rest.strip_prefix("let ") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "property".to_string(),
                        });
                    } else if let Some(name) = rest.strip_prefix("typealias ") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "typealias".to_string(),
                        });
                    }
                }
                // @objc exposed methods
                if trimmed.starts_with("@objc") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "objc_export".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn swift_detects_public_api() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("API.swift");
        std::fs::write(
            &file,
            "public func greet() {}\npublic class Router {}\nprivate func helper() {}\n",
        )
        .unwrap();

        let adapter = SwiftAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        let api = adapter.extract_api(&ast);
        assert_eq!(api.public_symbols.len(), 2);
        assert!(api.public_symbols.iter().any(|s| s.kind == "function"));
        assert!(api.public_symbols.iter().any(|s| s.kind == "class"));
    }

    #[test]
    fn swift_detects_protocols_and_enums() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("Types.swift");
        std::fs::write(
            &file,
            "public protocol Networking {}\npublic enum AppError {}\nopen class Base {}\n",
        )
        .unwrap();

        let adapter = SwiftAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert_eq!(ast.symbols.len(), 3);
    }
}
