use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct ClojureAdapter;

impl LanguageAdapter for ClojureAdapter {
    fn name(&self) -> &'static str {
        "Clojure"
    }

    fn language(&self) -> Language {
        Language("clojure")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "clj" | "cljs" | "cljc"))
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        let mut signatures = HashMap::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                // Clojure comments start with `;`.
                if trimmed.starts_with(';') {
                    continue;
                }
                if let Some(rest) = trimmed.strip_prefix("(defn ") {
                    if let Some(name) = parse_symbol_name(rest) {
                        if let Some(sig) = clojure_signature(rest) {
                            signatures.insert(name.clone(), sig);
                        }
                        symbols.push(Symbol {
                            name,
                            kind: "defn".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("(defmacro ") {
                    if let Some(name) = parse_symbol_name(rest) {
                        symbols.push(Symbol {
                            name,
                            kind: "defmacro".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("(defprotocol ") {
                    if let Some(name) = parse_symbol_name(rest) {
                        symbols.push(Symbol {
                            name,
                            kind: "defprotocol".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("(def ") {
                    if let Some(name) = parse_symbol_name(rest) {
                        symbols.push(Symbol {
                            name,
                            kind: "def".to_string(),
                        });
                    }
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation {
            symbols,
            structure_hash: hash,
            signatures,
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

/// Extracts the first whitespace-separated token from the remainder of a Clojure
/// form, trimming any trailing delimiters (`(`, `)`, `[`, `]`, `{`, `}`).
fn parse_symbol_name(rest: &str) -> Option<String> {
    rest.split_whitespace()
        .next()
        .map(|token| {
            token.trim_matches(|c: char| {
                c == '(' || c == ')' || c == '[' || c == ']' || c == '{' || c == '}'
            })
        })
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
}

/// Return the balanced argument vector `[ … ]` of a Clojure `defn` (the text after `(defn `),
/// so arity changes register as `modified` while the stable bare-identifier `name` is kept.
/// Uses `[…]`, NOT parens: the first `(` on a `defn` line is the body, so a paren scan would
/// capture body and false-modify. Body-independent: the scan stops at the matching `]`, so a
/// trailing `(+ …)` body is not captured. Returns `None` if there is no `[`.
fn clojure_signature(rest: &str) -> Option<String> {
    let start = rest.find('[')?;
    let mut depth = 0i32;
    for (i, b) in rest.as_bytes().iter().enumerate().skip(start) {
        match b {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(rest[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    // Best-effort fallback for an unbalanced line (drop the form's trailing `)`).
    Some(rest[start..].trim_end_matches(')').trim().to_string())
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
        let adapter = ClojureAdapter;
        let paths = vec![
            PathBuf::from("foo.clj"),
            PathBuf::from("bar.cljs"),
            PathBuf::from("baz.cljc"),
            PathBuf::from("qux.rs"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 3);
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "foo.clj"));
        assert!(detected
            .iter()
            .any(|p| p.file_name().unwrap() == "bar.cljs"));
        assert!(detected
            .iter()
            .any(|p| p.file_name().unwrap() == "baz.cljc"));
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
; comment line
(defn foo [x] (+ x 1))
(def bar 42)
(defprotocol Foo
  (baz [this]))
(defmacro qux [form]
  `(do ~form))
"#;
        let (_dir, path) = temp_file(source, "clj");
        let adapter = ClojureAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        assert_eq!(ast.symbols.len(), 4);
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "foo" && s.kind == "defn"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "bar" && s.kind == "def"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "Foo" && s.kind == "defprotocol"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "qux" && s.kind == "defmacro"));
    }

    #[test]
    fn ignores_private_defn_and_comments() {
        let source = r#"
; (defn commented [x] x)
(defn- private-fn [x] x)
(defn public-fn [x] x)
"#;
        let (_dir, path) = temp_file(source, "clj");
        let adapter = ClojureAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        assert_eq!(ast.symbols.len(), 1);
        assert_eq!(ast.symbols[0].name, "public-fn");
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "foo".into(),
                kind: "defn".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = ClojureAdapter.diff_ast(&old, &new);
        assert!(ClojureAdapter.detect_breaking_changes(&diff));
    }
}
