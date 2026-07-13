//! R adapter: function assignments, R6/S4 classes, and S4 generics as public
//! API surface.
//!
//! Structured line scanner (T2 depth). R defines functions by *assigning* a
//! `function(...)` value, so surface = top-level (brace depth 0) definitions:
//! `name <- function(params)`, `name = function(params)`, `name <<-
//! function(params)` (glued or spaced operators), R6 classes `Name <-
//! R6Class("Name", ...)`, and S4 `setClass("Name", ...)` / `setGeneric("name",
//! ...)`. R has no language-level visibility — package API is the NAMESPACE
//! export list (a documented follow-up) — so the adapter applies the
//! dot-prefix internal convention (`.helper` is not surface), matching the
//! convention-gated 0.8 confidence band. Method signatures are recorded as
//! canonical parameter-*name* tuples (`(x,y,...)` — defaults stripped): R
//! dispatch is untyped and callers bind arguments by name, so a parameter
//! rename IS an API change while a default-value change is not. Headers
//! complete on balanced parens, so multi-line definitions (including a whole
//! `R6Class(...)` call) accumulate. Right-assignment (`function(x) -> name`),
//! S3 methods, `setMethod`, and plain variable assignments are documented
//! exclusions. Born-correct comment handling per rev-24/26: `#` line comments
//! (including Roxygen `#'`); R has no block comments.

use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct RAdapter;

impl LanguageAdapter for RAdapter {
    fn name(&self) -> &'static str {
        "R"
    }

    fn language(&self) -> Language {
        Language("r")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "r" || e == "R")
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        r_parse(file)
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

fn is_ident(s: &str) -> bool {
    // R identifiers: letters, digits, `.`, `_`; must not start with a digit
    // (and a leading dot must not be followed by one). The scanner keeps the
    // common subset: start letter/dot/underscore, rest alnum/dot/underscore.
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '.' || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '.' || c == '_')
}

/// Push a surface symbol unless the name is dot-internal (`.helper`).
fn emit(symbols: &mut Vec<Symbol>, name: String, kind: &str) {
    if !name.starts_with('.') && is_ident(&name) {
        symbols.push(Symbol {
            name,
            kind: kind.into(),
        });
    }
}

/// First boundary-respecting occurrence of `word` (not glued inside a longer
/// identifier like `myfunction`).
fn find_keyword(text: &str, word: &str) -> Option<usize> {
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find(word) {
        let idx = search_from + rel;
        let before_ok = idx == 0
            || text[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_alphanumeric() && c != '.' && c != '_');
        let after = &text[idx + word.len()..];
        let after_ok = after
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '.' && c != '_');
        if before_ok && after_ok {
            return Some(idx);
        }
        search_from = idx + 1;
    }
    None
}

/// Open/close byte offsets of the first top-level paren group at or after
/// `from`.
fn paren_group(text: &str, from: usize) -> Option<(usize, usize)> {
    let start = text[from..].find('(')? + from;
    let mut depth = 0i32;
    for (i, c) in text[start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((start, start + i));
                }
            }
            _ => {}
        }
    }
    None
}

fn paren_balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Split a parameter list on top-level commas.
fn split_top_level(params: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in params.char_indices() {
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&params[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < params.len() {
        out.push(&params[start..]);
    }
    out
}

/// Canonical parameter-NAME tuple: defaults (`= ...`) are dropped, `...` is
/// preserved. R callers bind arguments by name, so names are the contract.
fn canonical_params(params: &str) -> String {
    let names: Vec<&str> = split_top_level(params)
        .iter()
        .map(|p| p.split('=').next().unwrap_or(p).trim())
        .filter(|p| !p.is_empty())
        .collect();
    format!("({})", names.join(","))
}

/// First double- or single-quoted string literal's content (`setClass("Name"`
/// -> `Name`).
fn first_string_arg(text: &str) -> Option<String> {
    let quote_pos = text.find(['"', '\''])?;
    let quote = text[quote_pos..].chars().next()?;
    let rest = &text[quote_pos + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// Assignment target before a definition keyword: strip a trailing `<-`,
/// `<<-`, or `=` operator, then take the trailing identifier. Returns `None`
/// when the prefix is not an assignment (`lapply(xs,` before an anonymous
/// function) or contains a quote (definition-shaped text inside a string
/// literal).
fn assignment_target(prefix: &str) -> Option<String> {
    if prefix.contains(['"', '\'']) {
        return None;
    }
    let p = prefix.trim_end();
    let stripped = p
        .strip_suffix("<<-")
        .or_else(|| p.strip_suffix("<-"))
        .or_else(|| p.strip_suffix('='))?;
    let name: String = stripped
        .trim_end()
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Analyze a complete (paren-balanced) top-level line for R definitions and
/// push any surface symbol + canonical signature.
fn analyze_line(
    trimmed: &str,
    symbols: &mut Vec<Symbol>,
    signatures: &mut HashMap<String, String>,
) {
    // S4 first — the most specific shapes.
    if let Some(idx) = find_keyword(trimmed, "setClass") {
        if let Some(rest) = trimmed.get(idx + 8..) {
            if let Some(name) = first_string_arg(rest) {
                emit(symbols, name, "class");
                return;
            }
        }
    }
    if let Some(idx) = find_keyword(trimmed, "setGeneric") {
        if let Some(rest) = trimmed.get(idx + 10..) {
            if let Some(name) = first_string_arg(rest) {
                emit(symbols, name.clone(), "generic");
                // setGeneric("f", function(x, ...) ...) carries the signature.
                if let Some(fidx) = find_keyword(trimmed, "function") {
                    if let Some((open, close)) = paren_group(trimmed, fidx) {
                        if let Some(params) = trimmed.get(open + 1..close) {
                            signatures.insert(name, canonical_params(params));
                        }
                    }
                }
                return;
            }
        }
    }
    if let Some(idx) = find_keyword(trimmed, "R6Class") {
        if let Some(name) = assignment_target(&trimmed[..idx]) {
            emit(symbols, name, "class");
            return;
        }
    }
    if let Some(idx) = find_keyword(trimmed, "function") {
        if let Some(name) = assignment_target(&trimmed[..idx]) {
            emit(symbols, name.clone(), "function");
            if let Some((open, close)) = paren_group(trimmed, idx) {
                if let Some(params) = trimmed.get(open + 1..close) {
                    signatures.insert(name, canonical_params(params));
                }
            }
        }
    }
}

/// Net brace delta of a code line.
fn net_braces(line: &str) -> i32 {
    line.chars().fold(0i32, |acc, c| match c {
        '{' => acc + 1,
        '}' => acc - 1,
        _ => acc,
    })
}

fn r_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut signatures = HashMap::new();
    // Brace depth: definitions are surface only at depth 0.
    let mut depth = 0i32;
    // A definition header spanning multiple lines, accumulated until its
    // parens balance (a whole `R6Class(...)` call may accumulate).
    let mut pending: Option<String> = None;

    for line in read_lines_safe(file)? {
        let trimmed = line.trim();
        // R has only `#` line comments (Roxygen `#'` included).
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(buf) = pending.as_mut() {
            buf.push(' ');
            buf.push_str(trimmed);
            if paren_balanced(buf) {
                let header = buf.clone();
                pending = None;
                if depth == 0 {
                    analyze_line(&header, &mut symbols, &mut signatures);
                }
            }
            depth += net_braces(trimmed);
            continue;
        }

        if trimmed.contains('(') && depth == 0 {
            if paren_balanced(trimmed) {
                analyze_line(trimmed, &mut symbols, &mut signatures);
            } else {
                pending = Some(trimmed.to_string());
            }
        }
        depth += net_braces(trimmed);
    }
    let hash = calculate_hash(&symbols);
    Ok(AstRepresentation {
        symbols,
        structure_hash: hash,
        signatures,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn parse(body: &str) -> AstRepresentation {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        RAdapter.parse_ast(f.path()).unwrap()
    }

    fn names(ast: &AstRepresentation) -> Vec<(String, String)> {
        ast.symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind.clone()))
            .collect()
    }

    #[test]
    fn detects_function_assignments() {
        let ast = parse(
            "distance <- function(a, b) {\n  abs(a - b)\n}\n\
             area = function(shape) {\n  0\n}\n\
             reset <<- function() {\n  NULL\n}\n\
             glued<-function(x) {\n  x\n}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("distance".into(), "function".into())));
        assert!(got.contains(&("area".into(), "function".into())));
        assert!(got.contains(&("reset".into(), "function".into())));
        assert!(got.contains(&("glued".into(), "function".into())));
    }

    #[test]
    fn r6_and_s4_classes() {
        let ast = parse(
            "Point <- R6Class(\"Point\",\n  public = list(\n    x = 0\n  )\n)\n\
             setClass(\"Token\", slots = c(value = \"character\"))\n\
             setGeneric(\"describe\", function(obj) standardGeneric(\"describe\"))\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Point".into(), "class".into())));
        assert!(got.contains(&("Token".into(), "class".into())));
        assert!(got.contains(&("describe".into(), "generic".into())));
        assert_eq!(
            ast.signatures.get("describe").map(String::as_str),
            Some("(obj)")
        );
    }

    #[test]
    fn dot_prefixed_names_are_internal() {
        let ast = parse(
            ".helper <- function(x) {\n  x * 2\n}\n\
             public_fn <- function(x) {\n  x\n}\n",
        );
        assert_eq!(names(&ast), vec![("public_fn".into(), "function".into())]);
    }

    #[test]
    fn nested_functions_are_not_surface() {
        let ast = parse(
            "outer <- function(x) {\n\
             \x20 inner <- function(y) {\n    y + 1\n  }\n\
             \x20 inner(x)\n\
             }\n\
             if (TRUE) {\n  conditional <- function(z) z\n}\n",
        );
        assert_eq!(names(&ast), vec![("outer".into(), "function".into())]);
    }

    #[test]
    fn signatures_are_param_names() {
        let ast = parse(
            "f <- function(x, y = 10, ...) {\n  x + y\n}\n\
             g <- function() {\n  1\n}\n",
        );
        assert_eq!(
            ast.signatures.get("f").map(String::as_str),
            Some("(x,y,...)")
        );
        assert_eq!(ast.signatures.get("g").map(String::as_str), Some("()"));
    }

    #[test]
    fn param_rename_changes_signature_default_value_does_not() {
        let a = parse("f <- function(count, scale = 2) {\n  count * scale\n}\n");
        let b = parse("f <- function(total, scale = 2) {\n  total * scale\n}\n");
        let diff = RAdapter.diff_ast(&a, &b);
        assert!(!diff.modified.is_empty());
        let c = parse("f <- function(count, scale = 3) {\n  count * scale\n}\n");
        let diff_c = RAdapter.diff_ast(&a, &c);
        assert!(diff_c.modified.is_empty());
    }

    #[test]
    fn skips_comments_and_strings() {
        let ast = parse(
            "# fake <- function(x) x\n\
             #' @param x documented — not a definition\n\
             #' real <- function(x) x\n\
             msg <- \"use function(x) here\"\n\
             real <- function(x) {\n  x\n}\n",
        );
        assert_eq!(names(&ast), vec![("real".into(), "function".into())]);
    }

    #[test]
    fn anonymous_functions_are_skipped() {
        let ast = parse(
            "result <- lapply(xs, function(x) x^2)\n\
             myfunction <- function(x) {\n  x\n}\n",
        );
        assert_eq!(names(&ast), vec![("myfunction".into(), "function".into())]);
    }

    #[test]
    fn removed_function_is_breaking_dot_fn_is_not() {
        let old = parse("a <- function() 1\nb <- function() 2\n");
        let new = parse("a <- function() 1\n");
        let diff = RAdapter.diff_ast(&old, &new);
        assert!(RAdapter.detect_breaking_changes(&diff));

        let old_d = parse("a <- function() 1\n.h <- function() 2\n");
        let new_d = parse("a <- function() 1\n");
        let diff_d = RAdapter.diff_ast(&old_d, &new_d);
        assert!(!RAdapter.detect_breaking_changes(&diff_d));
    }

    #[test]
    fn detect_files_matches_both_case_extensions() {
        let paths = vec![
            PathBuf::from("analysis.R"),
            PathBuf::from("util.r"),
            PathBuf::from("script.jl"),
        ];
        let got = RAdapter.detect_files(&paths);
        assert_eq!(got.len(), 2);
    }
}
