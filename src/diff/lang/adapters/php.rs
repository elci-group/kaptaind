use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct PhpAdapter;

impl LanguageAdapter for PhpAdapter {
    fn name(&self) -> &'static str {
        "PHP"
    }
    fn language(&self) -> Language {
        Language("php")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "php"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        let lines = read_lines_safe(file)?;
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("#") {
                continue;
            }

            // Namespaces are tracked as API boundaries.
            if let Some(rest) = trimmed.strip_prefix("namespace ") {
                let name = rest
                    .trim_end_matches(';')
                    .split_whitespace()
                    .next()
                    .unwrap_or(rest);
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: "namespace".to_string(),
                });
                continue;
            }

            // Top-level and public class/interface/trait/enum functions.
            if let Some(rest) = trimmed.strip_prefix("function ") {
                let name = rest
                    .split('(')
                    .next()
                    .unwrap_or(rest)
                    .split_whitespace()
                    .next()
                    .unwrap_or(rest);
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: "function".to_string(),
                });
                continue;
            }

            // Public methods and properties (including static variants).
            if trimmed.starts_with("public ") {
                let decl = trimmed.strip_prefix("public ").unwrap_or(trimmed).trim();
                let decl = decl.strip_prefix("static ").unwrap_or(decl).trim();

                if let Some(rest) = decl.strip_prefix("function ") {
                    let name = rest
                        .split('(')
                        .next()
                        .unwrap_or(rest)
                        .split_whitespace()
                        .next()
                        .unwrap_or(rest);
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "method".to_string(),
                    });
                } else if let Some(rest) = decl.strip_prefix("const ") {
                    let name = rest.split_whitespace().next().unwrap_or(rest);
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "class_constant".to_string(),
                    });
                } else {
                    // Property: "$name" or "type $name"
                    let tokens: Vec<&str> = decl.split_whitespace().collect();
                    if let Some(token) = tokens.last() {
                        let name = token.trim_end_matches(';').to_string();
                        if name.starts_with('$') {
                            symbols.push(Symbol {
                                name,
                                kind: "property".to_string(),
                            });
                        }
                    }
                }
                continue;
            }

            // Classes, interfaces, traits, enums.
            for (prefix, kind) in [
                ("class ", "class"),
                ("interface ", "interface"),
                ("trait ", "trait"),
                ("enum ", "enum"),
            ] {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    let name = rest.split_whitespace().next().unwrap_or(rest);
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: kind.to_string(),
                    });
                    break;
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
    use std::io::Write;

    fn temp_file(content: &str, ext: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("sample.{}", ext));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        (dir, path)
    }

    #[test]
    fn detects_extension() {
        let adapter = PhpAdapter;
        let paths = vec![PathBuf::from("foo.php"), PathBuf::from("bar.py")];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].file_name().unwrap(), "foo.php");
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"<?php
namespace App;

function helper() {}

interface Logger {
    public function log(string $message): void;
}

trait Timestampable {
    public $createdAt;
    public static function now() {}
}

enum Status {
    case Active;
    case Inactive;
}

class User {
    public const ROLE_ADMIN = 'admin';
    public string $name;
    private string $password;
    public function save() {}
}
"#;
        let (_dir, path) = temp_file(source, "php");
        let adapter = PhpAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        let names: Vec<&str> = ast.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"App"), "missing namespace: {:?}", names);
        assert!(names.contains(&"helper"), "missing function: {:?}", names);
        assert!(names.contains(&"Logger"), "missing interface: {:?}", names);
        assert!(names.contains(&"log"), "missing method: {:?}", names);
        assert!(
            names.contains(&"Timestampable"),
            "missing trait: {:?}",
            names
        );
        assert!(
            names.contains(&"$createdAt"),
            "missing property: {:?}",
            names
        );
        assert!(names.contains(&"now"), "missing static method: {:?}", names);
        assert!(names.contains(&"Status"), "missing enum: {:?}", names);
        assert!(names.contains(&"User"), "missing class: {:?}", names);
        assert!(
            names.contains(&"ROLE_ADMIN"),
            "missing class constant: {:?}",
            names
        );
        assert!(
            names.contains(&"$name"),
            "missing public property: {:?}",
            names
        );
        assert!(
            !names.contains(&"$password"),
            "private property should not be extracted: {:?}",
            names
        );
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "User".into(),
                kind: "class".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = PhpAdapter.diff_ast(&old, &new);
        assert!(PhpAdapter.detect_breaking_changes(&diff));
    }
}
