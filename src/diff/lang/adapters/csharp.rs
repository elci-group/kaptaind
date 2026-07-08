use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct CsharpAdapter;

impl LanguageAdapter for CsharpAdapter {
    fn name(&self) -> &'static str {
        "C#"
    }

    fn language(&self) -> Language {
        Language("csharp")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "cs"))
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") {
                    continue;
                }

                // Type declarations (class, interface, struct, enum). These may be
                // preceded by access modifiers or other keywords such as `partial`.
                if let Some(keyword) = type_keyword(trimmed) {
                    if let Some(name) = extract_type_name(trimmed, keyword) {
                        symbols.push(Symbol {
                            name,
                            kind: keyword.to_string(),
                        });
                    }
                    continue;
                }

                // Public methods and properties.
                if trimmed.contains("public ") {
                    if let Some((name, kind)) = extract_public_member(trimmed) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: kind.to_string(),
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

fn type_keyword(line: &str) -> Option<&'static str> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    ["class", "interface", "struct", "enum"]
        .into_iter()
        .find(|&kw| tokens.contains(&kw))
}

fn extract_type_name(line: &str, keyword: &str) -> Option<String> {
    let mut tokens = line.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == keyword {
            let name_token = tokens.next()?;
            let end =
                name_token.find(['<', ':', '{', '(', ';']);
            let name = match end {
                Some(i) => &name_token[..i],
                None => name_token,
            };
            if is_valid_identifier(name) {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn extract_public_member(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_end_matches(';').trim();

    // Method: identifier immediately before the opening parenthesis.
    if let Some(paren_idx) = line.rfind('(') {
        let before = &line[..paren_idx];
        let name = before.split_whitespace().last()?;
        let name = strip_generics(name);
        if is_valid_identifier(name) {
            return Some((name, "method"));
        }
    }

    // Property with expression body (`=>`).
    if let Some(idx) = line.find("=>") {
        let before = &line[..idx];
        let name = before.split_whitespace().last()?;
        let name = strip_generics(name);
        if is_valid_identifier(name) {
            return Some((name, "property"));
        }
    }

    // Property with getter/setter block.
    if let Some(idx) = line.find('{') {
        let before = &line[..idx];
        let name = before.split_whitespace().last()?;
        let name = strip_generics(name);
        if is_valid_identifier(name) {
            return Some((name, "property"));
        }
    }

    None
}

fn strip_generics(name: &str) -> &str {
    match name.find('<') {
        Some(i) => &name[..i],
        None => name,
    }
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
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
        let adapter = CsharpAdapter;
        let paths = vec![PathBuf::from("foo.cs"), PathBuf::from("bar.txt")];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].file_name().unwrap(), "foo.cs");
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
// Namespace and usings are ignored
using System;

namespace MyApp
{
    public class Greeter
    {
        public string Name { get; set; }

        public void SayHello()
        {
            Console.WriteLine("Hello");
        }

        public int Add(int a, int b) => a + b;

        private void Hidden() {}
    }

    interface IGreeter
    {
        void Greet();
    }

    public enum Color { Red, Green, Blue }

    public struct Point
    {
        public int X;
    }
}
"#;
        let (_dir, path) = temp_file(source, "cs");
        let adapter = CsharpAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        let names: Vec<_> = ast
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.as_str()))
            .collect();

        assert!(names.contains(&("Greeter", "class")));
        assert!(names.contains(&("Name", "property")));
        assert!(names.contains(&("SayHello", "method")));
        assert!(names.contains(&("Add", "method")));
        assert!(names.contains(&("IGreeter", "interface")));
        assert!(names.contains(&("Color", "enum")));
        assert!(names.contains(&("Point", "struct")));
        // Private members should not appear in the public API surface.
        assert!(!names.iter().any(|(n, _)| *n == "Hidden"));
        assert!(!names.iter().any(|(n, _)| *n == "X"));
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "Greeter".into(),
                kind: "class".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = CsharpAdapter.diff_ast(&old, &new);
        assert!(CsharpAdapter.detect_breaking_changes(&diff));
    }
}
