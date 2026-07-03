use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct JavaAdapter;

impl LanguageAdapter for JavaAdapter {
    fn name(&self) -> &'static str {
        "Java"
    }
    fn language(&self) -> Language {
        Language("java")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "java"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("public class ") {
                    if let Some(name) = extract_type_name(rest) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "class".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("public interface ") {
                    if let Some(name) = extract_type_name(rest) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "interface".to_string(),
                        });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("public enum ") {
                    if let Some(name) = extract_type_name(rest) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "enum".to_string(),
                        });
                    }
                } else if is_public_method_line(trimmed) {
                    if let Some(name) = extract_method_name(trimmed) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "method".to_string(),
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

/// Extract the simple type name, stopping at the first whitespace, generic
/// bracket, or opening brace. For `Foo<T>` this returns `Foo`; for
/// `Foo implements Bar {` it returns `Foo`.
fn extract_type_name(rest: &str) -> Option<&str> {
    let name = rest
        .split(|c: char| c == '{' || c == ' ' || c == '<')
        .next()?
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn is_public_method_line(line: &str) -> bool {
    line.starts_with("public ")
        && !line.contains(" class ")
        && !line.contains(" interface ")
        && !line.contains(" enum ")
        && line.contains('(')
}

fn extract_method_name(line: &str) -> Option<&str> {
    let before_paren = line.split('(').next()?;
    let name = before_paren.split_whitespace().last()?;
    if name.is_empty() {
        None
    } else {
        Some(name)
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
        let adapter = JavaAdapter;
        let paths = vec![
            PathBuf::from("foo.java"),
            PathBuf::from("bar.rb"),
            PathBuf::from("baz"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].file_name().unwrap(), "foo.java");
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
public class HelloWorld {
    public static void main(String[] args) {
        System.out.println("hello");
    }

    public String greet(String name) {
        return "Hello, " + name;
    }
}

public interface Greeter {
    void greet(String name);
}

public enum Status {
    OK, ERROR
}
"#;
        let (_dir, path) = temp_file(source, "java");
        let adapter = JavaAdapter;
        let ast = adapter.parse_ast(&path).unwrap();

        let names: Vec<_> = ast
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.as_str()))
            .collect();
        assert!(names.contains(&("HelloWorld", "class")));
        assert!(names.contains(&("main", "method")));
        assert!(names.contains(&("greet", "method")));
        assert!(names.contains(&("Greeter", "interface")));
        assert!(names.contains(&("Status", "enum")));
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "greet".into(),
                kind: "method".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = JavaAdapter.diff_ast(&old, &new);
        assert!(JavaAdapter.detect_breaking_changes(&diff));
    }
}
