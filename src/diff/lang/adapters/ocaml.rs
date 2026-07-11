use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct OcamlAdapter;

impl LanguageAdapter for OcamlAdapter {
    fn name(&self) -> &'static str {
        "OCaml"
    }
    fn language(&self) -> Language {
        Language("ocaml")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "ml" | "mli"))
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
                if trimmed.starts_with("(*") {
                    continue;
                }

                if let Some(rest) = trimmed.strip_prefix("module type ") {
                    if let Some(name) = first_ocaml_name(rest) {
                        symbols.push(Symbol {
                            name,
                            kind: "module_type".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("module ") {
                    if let Some(name) = first_ocaml_name(rest) {
                        symbols.push(Symbol {
                            name,
                            kind: "module".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("let ") {
                    if let Some(name) = first_ocaml_name(rest) {
                        if let Some(sig) = ocaml_signature(rest, &name) {
                            signatures.insert(name.clone(), sig);
                        }
                        symbols.push(Symbol {
                            name,
                            kind: "let".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("type ") {
                    if let Some(name) = first_ocaml_name(rest) {
                        symbols.push(Symbol {
                            name,
                            kind: "type".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("val ") {
                    if let Some(name) = first_ocaml_name(rest) {
                        symbols.push(Symbol {
                            name,
                            kind: "val".to_string(),
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

/// Extract the first valid OCaml identifier from `rest`, skipping type parameters
/// such as `'a` or `('a, 'b)` that may appear before the actual name.
fn first_ocaml_name(rest: &str) -> Option<String> {
    let mut first = true;
    for token in rest.split_whitespace() {
        // Reaching `=` means no binding name preceded it (e.g. `let () = ...`
        // or `let _ = ...`); the tokens after it are the value, not a name.
        if token == "=" {
            return None;
        }
        if token.starts_with('\'') || token.starts_with('(') {
            first = false;
            continue;
        }
        // `let rec foo` declares a recursive binding named `foo`.
        if first && token == "rec" {
            first = false;
            continue;
        }
        let name = token.trim_end_matches([',', ')', '=', ':']);
        if !name.is_empty() && name.chars().next().unwrap_or('0').is_alphabetic() {
            return Some(name.to_string());
        }
        first = false;
    }
    None
}

/// Return the whitespace-separated argument tokens of an OCaml `let` binding — the text between
/// the binding `name` and the `=` — so arity changes register as `modified` while the stable
/// bare-identifier `name` is kept. Body-independent: the right-hand side after `=` is not
/// captured. Returns `None` for value bindings with no arguments (`let x = 1`).
fn ocaml_signature(rest: &str, name: &str) -> Option<String> {
    let pos = rest.find(name)?;
    let after = &rest[pos + name.len()..];
    let eq = after.find('=')?;
    let args = after[..eq].trim();
    if args.is_empty() {
        None
    } else {
        Some(args.to_string())
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
        let adapter = OcamlAdapter;
        let paths = vec![
            PathBuf::from("foo.ml"),
            PathBuf::from("bar.mli"),
            PathBuf::from("baz.rs"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 2);
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "foo.ml"));
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "bar.mli"));
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"module Foo
module type FOO_SIG
let add x y = x + y
let rec factorial n =
  if n <= 1 then 1 else n * factorial (n - 1)
type 'a tree = Leaf | Node of 'a * 'a tree
type ('a, 'b) pair = 'a * 'b
val sum : int list -> int
(* comment ignored *)
"#;
        let (_dir, path) = temp_file(source, "ml");
        let adapter = OcamlAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        let names: Vec<_> = ast
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Foo", "module"),
                ("FOO_SIG", "module_type"),
                ("add", "let"),
                ("factorial", "let"),
                ("tree", "type"),
                ("pair", "type"),
                ("sum", "val"),
            ]
        );
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "foo".into(),
                kind: "let".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = OcamlAdapter.diff_ast(&old, &new);
        assert!(OcamlAdapter.detect_breaking_changes(&diff));
    }
}
