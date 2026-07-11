use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct CAdapter;

impl LanguageAdapter for CAdapter {
    fn name(&self) -> &'static str {
        "C"
    }
    fn language(&self) -> Language {
        Language("c")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "c" || e == "h")
            })
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        c_parse(file)
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

fn c_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut signatures = HashMap::new();
    if let Ok(lines) = read_lines_safe(file) {
        // Track `/* ... */` regions so declarations inside block comments are
        // not mistaken for public API (measured messy-corpus FP, rev 24).
        let mut in_block_comment = false;
        for line in lines {
            let trimmed = line.trim();
            if in_block_comment {
                if trimmed.contains("*/") {
                    in_block_comment = false;
                }
                continue;
            }
            if trimmed.starts_with("/*") {
                if !trimmed.contains("*/") {
                    in_block_comment = true;
                }
                continue;
            }
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            // #define MACRO
            if let Some(rest) = trimmed.strip_prefix("#define ") {
                let name = rest.split_whitespace().next().unwrap_or("");
                if is_valid_identifier(name) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "macro".to_string(),
                    });
                }
                continue;
            }

            // struct Name
            if let Some(rest) = trimmed.strip_prefix("struct ") {
                if let Some(name) = rest.split_whitespace().next() {
                    let name = name.trim_end_matches(['{', ';', '*']);
                    if is_valid_identifier(name) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "struct".to_string(),
                        });
                    }
                }
                continue;
            }

            // enum Name
            if let Some(rest) = trimmed.strip_prefix("enum ") {
                if let Some(name) = rest.split_whitespace().next() {
                    let name = name.trim_end_matches(['{', ';', '*']);
                    if is_valid_identifier(name) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "enum".to_string(),
                        });
                    }
                }
                continue;
            }

            // Function definition/declaration: "return_type name("
            // Heuristic: find the opening paren, then the token before it is the
            // function name, and there must be at least one preceding token that
            // looks like a return type.
            if let Some(open_paren) = trimmed.find('(') {
                let before_paren = &trimmed[..open_paren];
                let tokens: Vec<&str> = before_paren.split_whitespace().collect();
                if tokens.len() >= 2 {
                    let name = tokens[tokens.len() - 1];
                    let return_type = tokens[tokens.len() - 2];
                    if is_valid_identifier(name)
                        && is_valid_identifier(return_type)
                        && !is_control_keyword(name)
                    {
                        if let Some(sig) = c_signature(trimmed) {
                            signatures.insert(name.to_string(), sig);
                        }
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "function".to_string(),
                        });
                    }
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

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_control_keyword(s: &str) -> bool {
    matches!(
        s,
        "if" | "for" | "while" | "switch" | "return" | "goto" | "case" | "sizeof"
    )
}

/// Return the balanced parameter list `( … )` of a C function line, so arity / parameter-type
/// changes register as `modified` while the stable bare-identifier `name` is kept.
/// Body-independent: the scan stops at the closing `)`, so a trailing `;` (declaration) or
/// `{ … }` (definition) is not captured. Returns `None` if the line has no `(`.
fn c_signature(line: &str) -> Option<String> {
    let start = line.find('(')?;
    let mut depth = 0i32;
    for (i, b) in line.as_bytes().iter().enumerate().skip(start) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(line[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    // Best-effort fallback for an unbalanced line.
    Some(
        line[start..]
            .trim_end_matches(['{', ';'])
            .trim()
            .to_string(),
    )
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
        let adapter = CAdapter;
        let paths = vec![
            PathBuf::from("foo.c"),
            PathBuf::from("bar.h"),
            PathBuf::from("baz.rs"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 2);
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "foo.c"));
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "bar.h"));
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
#define MAX_SIZE 1024

struct point {
    int x;
    int y;
};

enum color {
    RED,
    GREEN,
    BLUE
};

int add(int a, int b);
void print_point(struct point *p);
"#;
        let (_dir, path) = temp_file(source, "h");
        let adapter = CAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        let names: Vec<&str> = ast.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"MAX_SIZE"),
            "missing macro MAX_SIZE: {:?}",
            names
        );
        assert!(
            names.contains(&"point"),
            "missing struct point: {:?}",
            names
        );
        assert!(names.contains(&"color"), "missing enum color: {:?}", names);
        assert!(names.contains(&"add"), "missing function add: {:?}", names);
        assert!(
            names.contains(&"print_point"),
            "missing function print_point: {:?}",
            names
        );
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "add".into(),
                kind: "function".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = CAdapter.diff_ast(&old, &new);
        assert!(CAdapter.detect_breaking_changes(&diff));
    }
}
