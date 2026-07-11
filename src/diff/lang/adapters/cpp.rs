use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct CppAdapter;

impl LanguageAdapter for CppAdapter {
    fn name(&self) -> &'static str {
        "C++"
    }

    fn language(&self) -> Language {
        Language("cpp")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "cpp" | "cc" | "cxx" | "hpp" | "h"))
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        let mut signatures = HashMap::new();
        if let Ok(lines) = read_lines_safe(file) {
            // Track `/* ... */` regions so declarations inside block comments
            // are not mistaken for public API (measured messy-corpus FP, rev 24).
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

                if let Some(name) = extract_class(trimmed) {
                    symbols.push(Symbol {
                        name,
                        kind: "class".to_string(),
                    });
                } else if let Some(name) = extract_struct(trimmed) {
                    symbols.push(Symbol {
                        name,
                        kind: "struct".to_string(),
                    });
                } else if let Some(name) = extract_namespace(trimmed) {
                    symbols.push(Symbol {
                        name,
                        kind: "namespace".to_string(),
                    });
                } else if let Some(name) = extract_function_definition(trimmed) {
                    if let Some(sig) = cpp_signature(trimmed) {
                        signatures.insert(name.clone(), sig);
                    }
                    symbols.push(Symbol {
                        name,
                        kind: "function".to_string(),
                    });
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

fn take_name_after_prefix(trimmed: &str, prefix: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(prefix)?.trim();
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '{' || c == ':' || c == ';' || c == '<')
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn extract_class(trimmed: &str) -> Option<String> {
    if !trimmed.starts_with("class ") {
        return None;
    }
    // Ignore `class` used as a keyword in template constraints or friend declarations.
    take_name_after_prefix(trimmed, "class ")
}

fn extract_struct(trimmed: &str) -> Option<String> {
    if !trimmed.starts_with("struct ") {
        return None;
    }
    take_name_after_prefix(trimmed, "struct ")
}

fn extract_namespace(trimmed: &str) -> Option<String> {
    if !trimmed.starts_with("namespace ") {
        return None;
    }
    let rest = trimmed.strip_prefix("namespace ")?.trim();
    // Anonymous namespace: `namespace {` has no API name.
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '{' || c == ';')
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn extract_function_definition(trimmed: &str) -> Option<String> {
    // Skip control-flow and non-definition constructs.
    let skip_prefixes = [
        "if ",
        "if(",
        "for ",
        "for(",
        "while ",
        "while(",
        "switch ",
        "switch(",
        "return ",
        "return(",
        "static_assert",
        "assert(",
        "using ",
        "namespace ",
        "class ",
        "struct ",
        "enum ",
        "typedef ",
        "template ",
        "extern ",
        "#",
    ];
    if skip_prefixes.iter().any(|p| trimmed.starts_with(p)) {
        return None;
    }
    if !trimmed.contains('(') {
        return None;
    }
    // Declarations end with ';' and are not definitions. Function definitions either
    // open a body on the same line or continue with the brace on the next line.
    if trimmed.ends_with(';') {
        return None;
    }

    let before_paren = trimmed.split('(').next()?;
    let name = before_paren.split_whitespace().last()?;
    let name = name.trim_end_matches('*').trim_end_matches('&');
    if name.is_empty() || name == ">" {
        return None;
    }
    Some(name.to_string())
}

/// Return the balanced parameter list `( … )` of a C++ function-definition line, so arity /
/// parameter-type changes register as `modified` while the stable bare-identifier `name` is
/// kept. Body-independent: the scan stops at the closing `)`, so the `{ … }` body is not
/// captured. Control-flow / keyword prefixes and `;`-terminated declarations are filtered
/// upstream by `extract_function_definition`. Returns `None` if the line has no `(`.
fn cpp_signature(line: &str) -> Option<String> {
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
    fn detects_extensions() {
        let adapter = CppAdapter;
        let paths = vec![
            PathBuf::from("foo.cpp"),
            PathBuf::from("bar.cc"),
            PathBuf::from("baz.cxx"),
            PathBuf::from("qux.hpp"),
            PathBuf::from("quux.h"),
            PathBuf::from("other.py"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 5);
        assert!(detected.iter().all(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("cpp" | "cc" | "cxx" | "hpp" | "h")
            )
        }));
    }

    #[test]
    fn parses_public_symbols() {
        let src = r#"
class Foo {
public:
    void bar();
};

struct Baz {
    int x;
};

namespace qux {
    int add(int a, int b) {
        return a + b;
    }
}

void standalone() {}
"#;
        let (_dir, path) = temp_file(src, "cpp");
        let adapter = CppAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        let names: Vec<&str> = ast.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Foo"), "missing class Foo: {names:?}");
        assert!(names.contains(&"Baz"), "missing struct Baz: {names:?}");
        assert!(names.contains(&"qux"), "missing namespace qux: {names:?}");
        assert!(names.contains(&"add"), "missing function add: {names:?}");
        assert!(
            names.contains(&"standalone"),
            "missing function standalone: {names:?}"
        );
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "Foo".into(),
                kind: "class".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = CppAdapter.diff_ast(&old, &new);
        assert!(CppAdapter.detect_breaking_changes(&diff));
    }
}
