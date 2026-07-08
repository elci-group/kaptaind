use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct RubyAdapter;

impl LanguageAdapter for RubyAdapter {
    fn name(&self) -> &'static str {
        "Ruby"
    }
    fn language(&self) -> Language {
        Language("ruby")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "rb" | "rake" | "gemspec"))
            })
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("module ") {
                    let name = rest.split_whitespace().next().unwrap_or(rest);
                    let name = name.split("::").next().unwrap_or(name);
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "module".to_string(),
                    });
                } else if let Some(rest) = trimmed.strip_prefix("class ") {
                    let name = rest.split_whitespace().next().unwrap_or(rest);
                    let name = name.split("::").next().unwrap_or(name);
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "class".to_string(),
                    });
                } else if let Some(rest) = trimmed.strip_prefix("def ") {
                    let name = rest.split(['(', ' ', ';']).next().unwrap_or(rest);
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "method".to_string(),
                    });
                } else if let Some(name) = trimmed.split('=').next() {
                    let name = name.trim();
                    if name.chars().all(|c| c.is_ascii_uppercase() || c == '_') && !name.is_empty()
                    {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "constant".to_string(),
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
        let adapter = RubyAdapter;
        let paths = vec![
            PathBuf::from("foo.rb"),
            PathBuf::from("bar.rake"),
            PathBuf::from("baz.gemspec"),
            PathBuf::from("qux.py"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 3);
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
module MyModule
  class MyClass
    FOO = 1
    def public_method(x)
    end
    def self.class_method
    end
  end
end
"#;
        let (_dir, path) = temp_file(source, "rb");
        let adapter = RubyAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        let names: Vec<_> = ast
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.as_str()))
            .collect();
        assert!(names.contains(&("MyModule", "module")));
        assert!(names.contains(&("MyClass", "class")));
        assert!(names.contains(&("FOO", "constant")));
        assert!(names.contains(&("public_method", "method")));
        assert!(names.contains(&("self.class_method", "method")));
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "foo".into(),
                kind: "method".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = RubyAdapter.diff_ast(&old, &new);
        assert!(RubyAdapter.detect_breaking_changes(&diff));
    }
}
