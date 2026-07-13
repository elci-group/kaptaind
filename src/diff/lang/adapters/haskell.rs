use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct HaskellAdapter;

impl LanguageAdapter for HaskellAdapter {
    fn name(&self) -> &'static str {
        "Haskell"
    }

    fn language(&self) -> Language {
        Language("haskell")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "hs" | "lhs"))
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let is_lhs = file.extension().is_some_and(|e| e == "lhs");
        let mut symbols = Vec::new();

        // Track `{- ... -}` block comments so commented-out decls are not
        // mistaken for public API (measured messy-corpus FP, rev 26).
        let mut in_block_comment = false;
        for line in read_lines_safe(file)? {
            let code = if is_lhs && line.starts_with('>') {
                line.strip_prefix('>')
                    .unwrap_or(&line)
                    .strip_prefix(' ')
                    .unwrap_or(&line[1..])
            } else {
                line.as_str()
            };

            let trimmed = code.trim_start();
            if trimmed.is_empty() {
                continue;
            }
            if in_block_comment {
                if trimmed.contains("-}") {
                    in_block_comment = false;
                }
                continue;
            }
            if trimmed.starts_with("{-") {
                if !trimmed.contains("-}") {
                    in_block_comment = true;
                }
                continue;
            }

            // Top-level declarations start at column 0 in the source line.
            let is_top_level = !code.starts_with(|c: char| c.is_whitespace());

            if is_top_level {
                // Function type signature (foo :: ...) or equation (foo x = ...).
                if let Some((first, rest)) = trimmed.split_once(' ') {
                    let name = first.trim_end();
                    if is_function_identifier(name) {
                        let rest = rest.trim_start();
                        if rest.starts_with("::") || rest.contains('=') {
                            symbols.push(Symbol {
                                name: name.to_string(),
                                kind: "function".to_string(),
                            });
                            continue;
                        }
                    }
                }
            }

            // Type-level declarations.
            if let Some(rest) = trimmed.strip_prefix("data ") {
                if let Some(name) = first_type_token(rest) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "data".to_string(),
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("newtype ") {
                if let Some(name) = first_type_token(rest) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "newtype".to_string(),
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("class ") {
                if let Some(name) = first_type_token(rest) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "class".to_string(),
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("type ") {
                if let Some(name) = first_type_token(rest) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "type".to_string(),
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

fn is_function_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
        return false;
    }
    if !name
        .chars()
        .skip(1)
        .all(|c| c.is_alphanumeric() || c == '_' || c == '\'')
    {
        return false;
    }
    !matches!(
        name,
        "module"
            | "import"
            | "data"
            | "newtype"
            | "type"
            | "class"
            | "where"
            | "let"
            | "in"
            | "if"
            | "then"
            | "else"
            | "case"
            | "of"
            | "do"
    )
}

/// Extract the first type/class name from a declaration tail.
///
/// Handles simple declarations and contexts such as
/// `class (Eq a) => Foo a` by looking after the last `=>`.
fn first_type_token(s: &str) -> Option<&str> {
    let s = s.trim_start();
    let after_context = if let Some(pos) = s.rfind("=>") {
        s[pos + 2..].trim_start()
    } else {
        s
    };
    after_context
        .split_whitespace()
        .next()
        .map(strip_trailing_punctuation)
}

fn strip_trailing_punctuation(token: &str) -> &str {
    token.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '\'')
}

#[cfg(test)]
mod tests {
    use super::super::super::adapter::{AstRepresentation, Symbol};
    use super::*;

    fn temp_file(content: &str, ext: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("sample.{}", ext));
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn detects_extension() {
        let adapter = HaskellAdapter;
        let paths = vec![
            PathBuf::from("src/Main.hs"),
            PathBuf::from("src/Lib.lhs"),
            PathBuf::from("README.md"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 2);
        assert!(detected.iter().any(|p| p.extension().unwrap() == "hs"));
        assert!(detected.iter().any(|p| p.extension().unwrap() == "lhs"));
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
module Example where

-- Top-level function with a type signature and an equation.
add :: Int -> Int -> Int
add x y = x + y

-- Algebraic data type.
data Result a = Ok a | Err String

-- Newtype.
newtype Identity a = Identity a

-- Type class.
class Functor f where
    fmap :: (a -> b) -> f a -> f b

-- Type alias.
type StringMap a = [(String, a)]

-- Another top-level function, equation only.
secret = 42
"#;
        let (_dir, path) = temp_file(source, "hs");
        let adapter = HaskellAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        let by_kind: std::collections::HashMap<&str, Vec<&str>> =
            ast.symbols
                .iter()
                .fold(std::collections::HashMap::new(), |mut acc, s| {
                    acc.entry(&s.kind).or_default().push(&s.name);
                    acc
                });

        assert!(by_kind
            .get("function")
            .unwrap_or(&Vec::new())
            .contains(&"add"));
        assert!(by_kind
            .get("function")
            .unwrap_or(&Vec::new())
            .contains(&"secret"));
        assert!(by_kind
            .get("data")
            .unwrap_or(&Vec::new())
            .contains(&"Result"));
        assert!(by_kind
            .get("newtype")
            .unwrap_or(&Vec::new())
            .contains(&"Identity"));
        assert!(by_kind
            .get("class")
            .unwrap_or(&Vec::new())
            .contains(&"Functor"));
        assert!(by_kind
            .get("type")
            .unwrap_or(&Vec::new())
            .contains(&"StringMap"));
    }

    #[test]
    fn parses_lhs_code_lines() {
        let source = "> double :: Int -> Int\n> double x = x * 2\n";
        let (_dir, path) = temp_file(source, "lhs");
        let adapter = HaskellAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "double" && s.kind == "function"));
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "add".to_string(),
                kind: "function".to_string(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = HaskellAdapter.diff_ast(&old, &new);
        assert!(HaskellAdapter.detect_breaking_changes(&diff));
    }

    #[test]
    fn ignores_nested_local_bindings() {
        let source = r#"
top :: Int
top =
    let nested = 1
    in nested + 1
"#;
        let (_dir, path) = temp_file(source, "hs");
        let ast = HaskellAdapter.parse_ast(&path).unwrap();
        assert!(ast.symbols.iter().any(|s| s.name == "top"));
        assert!(!ast.symbols.iter().any(|s| s.name == "nested"));
    }
}
