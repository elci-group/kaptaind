use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct ScalaAdapter;

impl LanguageAdapter for ScalaAdapter {
    fn name(&self) -> &'static str {
        "Scala"
    }
    fn language(&self) -> Language {
        Language("scala")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "scala" | "sc"))
            })
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if line.is_empty() || line.starts_with("//") {
                    continue;
                }

                // Strip simple single-line /* ... */ comments.
                let line = strip_block_comment_on_line(line);

                // Skip non-public declarations up front.
                if has_non_public_access_modifier(&line) {
                    continue;
                }

                // Strip common Scala modifiers.
                let rest = strip_leading_modifiers(&line);

                if let Some(name) = rest
                    .strip_prefix("case class ")
                    .and_then(extract_identifier)
                {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "case_class".to_string(),
                    });
                } else if let Some(name) = rest.strip_prefix("class ").and_then(extract_identifier) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "class".to_string(),
                    });
                } else if let Some(name) = rest.strip_prefix("object ").and_then(extract_identifier) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "object".to_string(),
                    });
                } else if let Some(name) = rest.strip_prefix("trait ").and_then(extract_identifier) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "trait".to_string(),
                    });
                } else if let Some(name) = rest.strip_prefix("def ").and_then(extract_identifier) {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "def".to_string(),
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

/// Strip a /* ... */ fragment if it begins and ends on the same line.
fn strip_block_comment_on_line(line: &str) -> String {
    if let Some(start) = line.find("/*") {
        if let Some(end) = line[start..].find("*/") {
            let before = &line[..start];
            let after = &line[start + end + 2..];
            return format!("{}{}", before, after);
        }
    }
    line.to_string()
}

/// Returns true if the line begins with a private or protected access modifier.
fn has_non_public_access_modifier(s: &str) -> bool {
    let s = s.trim_start();
    for prefix in ["private[", "protected[", "private", "protected"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace() || c == '[');
        }
    }
    false
}

/// Remove common Scala leading modifiers so we can test for public declarations.
fn strip_leading_modifiers(mut s: &str) -> &str {
    loop {
        let trimmed = s.trim_start();
        if trimmed.is_empty() {
            return trimmed;
        }
        s = trimmed;

        let modifiers = [
            "abstract", "final", "sealed", "lazy", "implicit", "inline", "opaque",
            "transparent", "open", "override",
        ];
        let mut matched = false;
        for m in &modifiers {
            if let Some(rest) = s.strip_prefix(m) {
                // Make sure we matched a whole word (e.g. not "definitely" for "def").
                if rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace()) {
                    s = rest;
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            return s;
        }
    }
}

/// Extract the first identifier token from `s`, stopping at whitespace or
/// structural punctuation (`[`, `(`, `{`, `:`, `=`).
fn extract_identifier(s: &str) -> Option<&str> {
    s.split(|c: char| c.is_whitespace() || c == '[' || c == '(' || c == '{' || c == ':' || c == '=')
        .next()
        .filter(|n| !n.is_empty())
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
    fn detects_scala_extensions() {
        let adapter = ScalaAdapter;
        let paths = vec![
            PathBuf::from("foo.scala"),
            PathBuf::from("bar.sc"),
            PathBuf::from("baz.java"),
            PathBuf::from("qux.rs"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 2);
        assert_eq!(detected[0].file_name().unwrap(), "foo.scala");
        assert_eq!(detected[1].file_name().unwrap(), "bar.sc");
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
// Top-level comment
package example

import scala.util.Try

object Config {
  val default = 42
}

trait Logger {
  def log(msg: String): Unit
}

class FileLogger extends Logger {
  override def log(msg: String): Unit = println(msg)
  private def internal = "hidden"
}

case class User(name: String, age: Int)

sealed abstract class Status
object Active extends Status
"#;
        let (_dir, path) = temp_file(source, "scala");
        let adapter = ScalaAdapter;
        let ast = adapter.parse_ast(&path).unwrap();

        let names: Vec<&str> = ast.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Config"), "missing object Config: {:?}", names);
        assert!(names.contains(&"Logger"), "missing trait Logger: {:?}", names);
        assert!(names.contains(&"FileLogger"), "missing class FileLogger: {:?}", names);
        assert!(names.contains(&"log"), "missing def log: {:?}", names);
        assert!(names.contains(&"User"), "missing case class User: {:?}", names);
        assert!(names.contains(&"Status"), "missing class Status: {:?}", names);
        assert!(names.contains(&"Active"), "missing object Active: {:?}", names);
        assert!(
            !names.contains(&"internal"),
            "private def internal should not be public API: {:?}",
            names
        );
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "User".into(),
                kind: "case_class".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = ScalaAdapter.diff_ast(&old, &new);
        assert!(ScalaAdapter.detect_breaking_changes(&diff));
    }
}
