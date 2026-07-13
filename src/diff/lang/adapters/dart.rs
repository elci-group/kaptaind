use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct DartAdapter;

impl LanguageAdapter for DartAdapter {
    fn name(&self) -> &'static str {
        "Dart"
    }
    fn language(&self) -> Language {
        Language("dart")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "dart"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        let mut signatures = HashMap::new();
        if let Ok(lines) = read_lines_safe(file) {
            // Track `/* ... */` regions so declarations inside block comments
            // are not mistaken for public API (measured messy-corpus FP, rev 26).
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
                // Only top-level declarations constitute the public API surface;
                // indented lines are class members or nested constructs.
                if line.starts_with(' ') || line.starts_with('\t') {
                    continue;
                }
                if let Some(name) = extract_class_name(trimmed) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "class".to_string(),
                    });
                } else if let Some(name) = extract_enum_name(trimmed) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "enum".to_string(),
                    });
                } else if let Some(name) = extract_extension_name(trimmed) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "extension".to_string(),
                    });
                } else if let Some(name) = extract_mixin_name(trimmed) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "mixin".to_string(),
                    });
                } else if let Some(name) = extract_top_level_function_name(trimmed) {
                    if let Some(sig) = dart_signature(trimmed) {
                        signatures.insert(name.to_string(), sig);
                    }
                    symbols.push(Symbol {
                        name: name.to_string(),
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

fn extract_class_name(trimmed: &str) -> Option<&str> {
    const MODIFIERS: &[&str] = &[
        "abstract class ",
        "final class ",
        "sealed class ",
        "base class ",
        "interface class ",
        "mixin class ",
    ];
    let rest = strip_any_prefix(trimmed, MODIFIERS);
    let rest = if rest == trimmed {
        trimmed.strip_prefix("class ")?
    } else {
        rest
    };
    let name = rest.split_whitespace().next()?;
    if name.starts_with('_') || name == "{" {
        return None;
    }
    clean_identifier(name)
}

fn extract_enum_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("enum ")?;
    let name = rest.split_whitespace().next()?;
    if name.starts_with('_') {
        return None;
    }
    clean_identifier(name)
}

fn extract_extension_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("extension ")?;
    let name = rest.split_whitespace().next()?;
    if name == "on" || name.starts_with('_') {
        return None;
    }
    Some(name)
}

fn extract_mixin_name(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with("mixin class ") {
        return None;
    }
    let rest = trimmed.strip_prefix("mixin ")?;
    let name = rest.split_whitespace().next()?;
    if name.starts_with('_') {
        return None;
    }
    clean_identifier(name)
}

fn extract_top_level_function_name(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with("class ")
        || trimmed.starts_with("abstract class ")
        || trimmed.starts_with("final class ")
        || trimmed.starts_with("sealed class ")
        || trimmed.starts_with("base class ")
        || trimmed.starts_with("interface class ")
        || trimmed.starts_with("mixin class ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("extension ")
        || trimmed.starts_with("mixin ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("export ")
        || trimmed.starts_with("part ")
        || trimmed.starts_with("library ")
        || trimmed.starts_with("typedef ")
        || trimmed.starts_with("//")
        || trimmed.starts_with('@')
        || !trimmed.contains('(')
    {
        return None;
    }

    // Skip assignment/call statements such as `final x = foo()`.
    if let Some((eq_pos, paren_pos)) = trimmed.find('=').zip(trimmed.find('(')) {
        if eq_pos < paren_pos {
            return None;
        }
    }

    let before_paren = trimmed.split('(').next()?.trim();
    let name = before_paren.split_whitespace().last()?;
    if name.is_empty() || name.starts_with('_') || name == "{" {
        return None;
    }
    clean_identifier(name)
}

fn strip_any_prefix<'a>(line: &'a str, prefixes: &[&str]) -> &'a str {
    for prefix in prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest;
        }
    }
    line
}

fn clean_identifier(token: &str) -> Option<&str> {
    let name = token.split('<').next()?;
    Some(name.trim_end_matches('{').trim())
}

/// Return the balanced parameter list `( … )` of a top-level Dart function line, so arity /
/// parameter-type changes register as `modified` while the stable bare-identifier `name` is
/// kept. Body-independent: the scan stops at the closing `)`, so an `=> …` expression body or
/// `{ … }` block is not captured. Returns `None` if the line has no `(`.
fn dart_signature(line: &str) -> Option<String> {
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
    fn detects_dart_extension() {
        let adapter = DartAdapter;
        let paths = vec![
            PathBuf::from("foo.dart"),
            PathBuf::from("bar.rs"),
            PathBuf::from("baz.go"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].file_name().unwrap(), "foo.dart");
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
class User {
  String name;
  User(this.name);
}

enum Status { active, inactive }

extension StringHelpers on String {
  String reversed() => 'todo';
}

mixin Logging {
  void log(String msg) => print(msg);
}

abstract class Repository {
  Future<void> load();
}

String greet(String name) => 'Hello, $name';

int add(int a, int b) => a + b;

Future<List<int>> fetch() async => [];

void generic<T>(T value) {}

class _PrivateClass {}

void _privateFn() {}
"#;
        let (_dir, path) = temp_file(source, "dart");
        let adapter = DartAdapter;
        let ast = adapter.parse_ast(&path).unwrap();

        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "User" && s.kind == "class"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "Status" && s.kind == "enum"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "StringHelpers" && s.kind == "extension"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "Logging" && s.kind == "mixin"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "Repository" && s.kind == "class"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "greet" && s.kind == "function"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "add" && s.kind == "function"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "fetch" && s.kind == "function"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.name == "generic" && s.kind == "function"));

        // Private names and class members should not appear in the API surface.
        assert!(!ast.symbols.iter().any(|s| s.name.starts_with('_')));
        assert!(!ast.symbols.iter().any(|s| s.name == "log"));
        assert!(!ast.symbols.iter().any(|s| s.name == "load"));
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "greet".into(),
                kind: "function".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = DartAdapter.diff_ast(&old, &new);
        assert!(DartAdapter.detect_breaking_changes(&diff));
    }
}
