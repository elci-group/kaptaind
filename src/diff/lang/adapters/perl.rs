use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct PerlAdapter;

impl LanguageAdapter for PerlAdapter {
    fn name(&self) -> &'static str {
        "Perl"
    }

    fn language(&self) -> Language {
        Language("perl")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "pl" || e == "pm")
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        let lines = read_lines_safe(file)?;

        for line in lines {
            let trimmed = line.trim();

            if let Some(rest) = trimmed.strip_prefix("package ") {
                let name = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or(rest)
                    .trim_end_matches(';');
                if !name.is_empty() {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "package".to_string(),
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("sub ") {
                let name = rest
                    .split(|c: char| {
                        c.is_whitespace() || c == '(' || c == '{' || c == ';' || c == ':'
                    })
                    .next()
                    .unwrap_or(rest)
                    .trim_end_matches(|c: char| c == '(' || c == '{' || c == ';' || c == ':');
                if !name.is_empty() {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "sub".to_string(),
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("use constant ") {
                let name = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or(rest)
                    .trim_end_matches(|c: char| c == '(' || c == '{' || c == ';' || c == ',');
                if !name.is_empty() {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "constant".to_string(),
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
        let adapter = PerlAdapter;
        let paths = vec![
            PathBuf::from("foo.pl"),
            PathBuf::from("bar.pm"),
            PathBuf::from("baz.py"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 2);
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "foo.pl"));
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "bar.pm"));
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"package Foo::Bar;

use constant PI => 3.14;
use constant MAX_SIZE => 100;

sub public_sub {
    return 1;
}

sub other_sub($self) :method {
    return 2;
}
"#;
        let (_dir, path) = temp_file(source, "pm");
        let adapter = PerlAdapter;
        let ast = adapter.parse_ast(&path).unwrap();

        let names: Vec<_> = ast
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.as_str()))
            .collect();
        assert!(names.contains(&("Foo::Bar", "package")));
        assert!(names.contains(&("PI", "constant")));
        assert!(names.contains(&("MAX_SIZE", "constant")));
        assert!(names.contains(&("public_sub", "sub")));
        assert!(names.contains(&("other_sub", "sub")));
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "public_sub".into(),
                kind: "sub".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = PerlAdapter.diff_ast(&old, &new);
        assert!(PerlAdapter.detect_breaking_changes(&diff));
    }
}
