use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct ElixirAdapter;

impl LanguageAdapter for ElixirAdapter {
    fn name(&self) -> &'static str {
        "Elixir"
    }
    fn language(&self) -> Language {
        Language("elixir")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "ex" || e == "exs"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("defmodule ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "module".to_string(),
                    });
                } else if let Some(rest) = line.strip_prefix("defprotocol ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "protocol".to_string(),
                    });
                } else if let Some(rest) = line.strip_prefix("defmacro ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "macro".to_string(),
                    });
                } else if let Some(rest) = line.strip_prefix("def ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "function".to_string(),
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
        let adapter = ElixirAdapter;
        let paths = vec![
            PathBuf::from("foo.ex"),
            PathBuf::from("bar.exs"),
            PathBuf::from("baz.rb"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 2);
        assert!(detected
            .iter()
            .any(|p| p.extension().is_some_and(|e| e == "ex")));
        assert!(detected
            .iter()
            .any(|p| p.extension().is_some_and(|e| e == "exs")));
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
defmodule MyApp do
  defmodule Inner do
  end

  def hello(name) do
    "Hello, #{name}"
  end

  defmacro my_macro(expr) do
    quote do
      unquote(expr)
    end
  end
end

defprotocol Size do
  def size(data)
end
"#;
        let (_dir, path) = temp_file(source, "ex");
        let adapter = ElixirAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        assert_eq!(ast.symbols.len(), 6);
        let kinds: Vec<&str> = ast.symbols.iter().map(|s| s.kind.as_str()).collect();
        assert!(kinds.contains(&"module"));
        assert!(kinds.contains(&"function"));
        assert!(kinds.contains(&"macro"));
        assert!(kinds.contains(&"protocol"));
        assert_eq!(kinds.iter().filter(|&&k| k == "function").count(), 2);
        assert_eq!(kinds.iter().filter(|&&k| k == "module").count(), 2);
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "hello(name)".into(),
                kind: "function".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = ElixirAdapter.diff_ast(&old, &new);
        assert!(ElixirAdapter.detect_breaking_changes(&diff));
    }
}
