use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct LuaAdapter;

impl LanguageAdapter for LuaAdapter {
    fn name(&self) -> &'static str {
        "Lua"
    }
    fn language(&self) -> Language {
        Language("lua")
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "lua"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();

                // Module exports: M.foo = ...
                if let Some(eq_pos) = find_assignment_eq(trimmed) {
                    let lhs = &trimmed[..eq_pos].trim_end();
                    if lhs.starts_with("M.") {
                        let name = &lhs[2..];
                        if is_valid_lua_identifier(name) {
                            symbols.push(Symbol {
                                name: lhs.to_string(),
                                kind: "module_export".to_string(),
                            });
                        }
                    }
                }

                // Global (non-local) function definitions.
                // We intentionally skip `local function foo` because those are private.
                if trimmed.starts_with("function ") && !trimmed.starts_with("local function ") {
                    let after_keyword = &trimmed["function ".len()..];
                    if let Some(name) = extract_function_name(after_keyword) {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: "function".to_string(),
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

/// Extract the function name from text following the `function` keyword,
/// stopping at the opening parenthesis or end of valid identifier.
fn extract_function_name(after_keyword: &str) -> Option<&str> {
    let name_part = after_keyword.split('(').next().unwrap_or(after_keyword);
    let name = name_part.split_whitespace().next()?;
    if name.is_empty() || !is_valid_lua_identifier(name) {
        return None;
    }
    Some(name)
}

/// Locate the first `=` that is an assignment, skipping comparison operators
/// such as `==`, `~=`, `<=`, and `>=`.
fn find_assignment_eq(s: &str) -> Option<usize> {
    for (i, c) in s.char_indices() {
        if c == '=' {
            if i == 0 {
                return Some(i);
            }
            let prev = s[..i].chars().next_back().unwrap_or('=');
            if !matches!(prev, '=' | '~' | '<' | '>') {
                return Some(i);
            }
        }
    }
    None
}

/// Heuristic check that a string is a valid Lua identifier or dotted/colon path.
fn is_valid_lua_identifier(s: &str) -> bool {
    let mut first = true;
    for c in s.chars() {
        if c == '.' || c == ':' {
            first = true;
            continue;
        }
        let valid = if first {
            c.is_alphabetic() || c == '_'
        } else {
            c.is_alphanumeric() || c == '_'
        };
        if !valid {
            return false;
        }
        first = false;
    }
    !s.is_empty()
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
        let adapter = LuaAdapter;
        let paths = vec![PathBuf::from("foo.lua"), PathBuf::from("bar.other")];
        let detected = adapter.detect_files(&paths);
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].file_name().unwrap(), "foo.lua");
    }

    #[test]
    fn parses_public_symbols() {
        let source = r#"
local M = {}

function M.add(a, b)
    return a + b
end

function M.sub(a, b)
    return a - b
end

local function helper(x)
    return x * 2
end

M.CONSTANT = 42

M.power = function(base, exp)
    return base ^ exp
end

return M
"#;
        let (_dir, path) = temp_file(source, "lua");
        let adapter = LuaAdapter;
        let ast = adapter.parse_ast(&path).unwrap();
        let names: Vec<&str> = ast.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"M.add"));
        assert!(names.contains(&"M.sub"));
        assert!(names.contains(&"M.CONSTANT"));
        assert!(names.contains(&"M.power"));
        assert!(!names.contains(&"helper"));
    }

    #[test]
    fn detects_breaking_removal() {
        let old = AstRepresentation {
            symbols: vec![Symbol {
                name: "M.add".into(),
                kind: "module_export".into(),
            }],
            ..Default::default()
        };
        let new = AstRepresentation {
            symbols: vec![],
            ..Default::default()
        };
        let diff = LuaAdapter.diff_ast(&old, &new);
        assert!(LuaAdapter.detect_breaking_changes(&diff));
    }
}
