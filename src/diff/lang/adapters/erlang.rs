use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct ErlangAdapter;

impl LanguageAdapter for ErlangAdapter {
    fn name(&self) -> &'static str {
        "Erlang"
    }

    fn language(&self) -> Language {
        Language("erlang")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "erl" | "hrl"))
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        erlang_parse(file)
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

fn erlang_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut exported: HashSet<String> = HashSet::new();

    if let Ok(lines) = read_lines_safe(file) {
        for line in lines {
            let trimmed = line.trim();

            // -module(foo).
            if let Some(rest) = trimmed.strip_prefix("-module(") {
                if let Some(name) = rest.split(')').next() {
                    let name = name.trim();
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "module".to_string(),
                        });
                    }
                }
                continue;
            }

            // -export([foo/1, bar/2]).
            if let Some(rest) = trimmed.strip_prefix("-export(") {
                if let Some(list) = rest.split(']').next() {
                    let list = list.trim_start_matches('[');
                    for item in list.split(',') {
                        let item = item.trim();
                        if !item.is_empty() {
                            exported.insert(item.to_string());
                        }
                    }
                }
                continue;
            }

            // -record(foo, { ... }).
            if let Some(rest) = trimmed.strip_prefix("-record(") {
                if let Some(name) = rest.split(',').next() {
                    let name = name.trim();
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "record".to_string(),
                        });
                    }
                }
                continue;
            }

            // -define(FOO, ...).
            if let Some(rest) = trimmed.strip_prefix("-define(") {
                if let Some(name) = rest.split(',').next() {
                    let name = name.trim();
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "macro".to_string(),
                        });
                    }
                }
                continue;
            }

            // Function definition: foo(Args) -> ...
            if let Some((head, _)) = trimmed.split_once(" ->") {
                let head = head.trim();
                if let Some(paren_idx) = head.find('(') {
                    let name = head[..paren_idx].trim();
                    let args_part = &head[paren_idx..];
                    if !name.is_empty()
                        && name.chars().next().is_some_and(|c| c.is_lowercase())
                        && args_part.ends_with(')')
                    {
                        let arity = function_arity(args_part);
                        let qualified = format!("{name}/{arity}");
                        if exported.contains(&qualified) {
                            symbols.push(Symbol {
                                name: qualified,
                                kind: "function".to_string(),
                            });
                        }
                    }
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

fn function_arity(args_part: &str) -> usize {
    let inner = args_part
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or("")
        .trim();
    if inner.is_empty() {
        0
    } else {
        inner.split(',').count()
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
    fn detects_extension() {
        let adapter = ErlangAdapter;
        let paths = vec![
            PathBuf::from("foo.erl"),
            PathBuf::from("bar.hrl"),
            PathBuf::from("baz.rs"),
        ];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 2);
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "foo.erl"));
        assert!(detected.iter().any(|p| p.file_name().unwrap() == "bar.hrl"));
    }

    #[test]
    fn parses_public_symbols() {
        let src = r#"
-module(my_mod).
-export([start/0, stop/1]).
-record(state, {count :: integer()}).
-define(MAX_LIMIT, 100).

start() -> ok.

stop(_Reason) -> ok.

private_helper() -> ok.
"#;
        let (_dir, path) = temp_file(src, "erl");
        let adapter = ErlangAdapter;
        let ast = adapter.parse_ast(&path).unwrap();

        let names: Vec<&str> = ast.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"my_mod"), "module symbol missing");
        assert!(
            names.contains(&"start/0"),
            "exported function start/0 missing"
        );
        assert!(
            names.contains(&"stop/1"),
            "exported function stop/1 missing"
        );
        assert!(names.contains(&"state"), "record state missing");
        assert!(names.contains(&"MAX_LIMIT"), "macro MAX_LIMIT missing");
        assert!(
            !names.contains(&"private_helper/0"),
            "non-exported function should not be public API"
        );
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "start/0".into(),
                kind: "function".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = ErlangAdapter.diff_ast(&old, &new);
        assert!(ErlangAdapter.detect_breaking_changes(&diff));
    }
}
