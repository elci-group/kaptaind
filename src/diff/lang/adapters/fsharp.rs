use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct FsharpAdapter;

impl LanguageAdapter for FsharpAdapter {
    fn name(&self) -> &'static str {
        "F#"
    }

    fn language(&self) -> Language {
        Language("fsharp")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "fs" | "fsx" | "fsi"))
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        for line in read_lines_safe(file)? {
            let trimmed = line.trim_start();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("(*")
                || trimmed.starts_with("#")
                || trimmed.starts_with("open ")
            {
                continue;
            }
            if let Some(symbol) = parse_fsharp_line(trimmed) {
                symbols.push(symbol);
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

fn parse_fsharp_line(line: &str) -> Option<Symbol> {
    let line = strip_leading_attributes(line);

    if let Some(rest) = line.strip_prefix("module ") {
        return parse_decl(rest, "module");
    }
    if let Some(rest) = line.strip_prefix("type ") {
        return parse_decl(rest, "type");
    }
    if let Some(rest) = line.strip_prefix("let ") {
        return parse_decl(rest, "value");
    }
    if let Some(rest) = line.strip_prefix("val ") {
        return parse_decl(rest, "value");
    }

    None
}

fn parse_decl(rest: &str, kind: &str) -> Option<Symbol> {
    let rest = strip_leading_attributes(rest);
    let mut s = rest.trim_start();

    // Skip non-public declarations.
    if s.starts_with("private ") || s.starts_with("internal ") {
        return None;
    }

    // Skip modifiers that do not affect visibility.
    loop {
        if let Some(t) = s.strip_prefix("rec ") {
            s = t;
            continue;
        }
        if let Some(t) = s.strip_prefix("inline ") {
            s = t;
            continue;
        }
        if let Some(t) = s.strip_prefix("mutable ") {
            s = t;
            continue;
        }
        if let Some(t) = s.strip_prefix("global ") {
            s = t;
            continue;
        }
        break;
    }

    let name = take_identifier(s)?;
    Some(Symbol {
        name,
        kind: kind.to_string(),
    })
}

fn strip_leading_attributes(s: &str) -> &str {
    let mut s = s;
    loop {
        let t = s.trim_start();
        if let Some(rest) = t.strip_prefix("[<") {
            if let Some(end) = find_attr_end(rest) {
                s = &rest[end..];
                continue;
            }
        }
        return t;
    }
}

fn find_attr_end(s: &str) -> Option<usize> {
    let mut depth = 1;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '[' {
            if let Some(&(_, next)) = chars.peek() {
                if next == '<' {
                    depth += 1;
                    chars.next();
                }
            }
        } else if c == '>' {
            if let Some(&(_, next)) = chars.peek() {
                if next == ']' {
                    depth -= 1;
                    chars.next();
                    if depth == 0 {
                        return Some(i + 2);
                    }
                }
            }
        }
    }
    None
}

fn take_identifier(s: &str) -> Option<String> {
    let mut ident = String::new();
    for c in s.chars() {
        if c.is_whitespace() {
            break;
        }
        match c {
            '(' | ')' | '<' | '>' | ':' | '=' | '[' | ']' | '{' | '}' => break,
            _ => ident.push(c),
        }
    }
    let ident = ident.trim_end().to_string();
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(content: &str, ext: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("sample.{}", ext));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        (dir, path)
    }

    #[test]
    fn detects_extensions() {
        let adapter = FsharpAdapter;
        let paths = vec![
            PathBuf::from("Api.fsi"),
            PathBuf::from("Script.fsx"),
            PathBuf::from("Library.fs"),
            PathBuf::from("other.txt"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 3);

        let names: Vec<_> = detected
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"Api.fsi"));
        assert!(names.contains(&"Script.fsx"));
        assert!(names.contains(&"Library.fs"));
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
module MyModule

open System

// a comment
(* a block comment *)

type MyType =
    | A
    | B

let myValue = 1

let private hidden () = ()

[<Literal>]
let LiteralValue = 42

type internal InternalType = class end

val publicSignature : int
"#;
        let (_dir, path) = temp_file(source, "fs");
        let ast = FsharpAdapter.parse_ast(&path).unwrap();
        let names_and_kinds: Vec<(&str, &str)> = ast
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.as_str()))
            .collect();

        assert!(names_and_kinds.contains(&("MyModule", "module")));
        assert!(names_and_kinds.contains(&("MyType", "type")));
        assert!(names_and_kinds.contains(&("myValue", "value")));
        assert!(names_and_kinds.contains(&("LiteralValue", "value")));
        assert!(names_and_kinds.contains(&("publicSignature", "value")));

        assert!(!names_and_kinds.iter().any(|(n, _)| *n == "hidden"));
        assert!(!names_and_kinds.iter().any(|(n, _)| *n == "InternalType"));
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "MyType".into(),
                kind: "type".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = FsharpAdapter.diff_ast(&old, &new);
        assert!(FsharpAdapter.detect_breaking_changes(&diff));
    }
}
