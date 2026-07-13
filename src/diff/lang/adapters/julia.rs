//! Julia adapter: modules, types, functions, macros, constants, and struct
//! fields as public API surface.
//!
//! Structured line scanner (T2 depth). Julia's public API is convention-gated:
//! top-level declarations whose names do not start with `_` (the internal
//! convention) — `module`/`baremodule`, `struct`/`mutable struct`,
//! `abstract type`, long-form `function name(params)`, short-form
//! `name(params) = expr`, `macro name(params)`, `const NAME = ...`, and the
//! fields of a struct body (dot-accessible, and their shape defines the
//! default constructor). Qualified definitions (`function Base.show(io, x)`)
//! emit the final dotted component. Declarations nested below block depth 1
//! (inside functions/loops/conditionals) are not surface. Method signatures
//! are recorded as canonical parameter-type tuples (`(Int,String)`; untyped
//! parameters canonicalize to `Any`, defaults are dropped, `where` clauses are
//! out of scope), so parameter renames are invisible but type changes register
//! as modifications. Headers may span lines; a header completes on balanced
//! parens (Julia needs no terminator). Underscore/export gating puts the
//! adapter in the 0.8 confidence band (export-list cross-referencing is a
//! documented follow-up). Born-correct comment handling per rev-24/26: `#`
//! line comments, `#= ... =#` block comments, and `"""` docstring/triple-quote
//! regions. Known T2 limits: operator overloads (`Base.:+`) and
//! `primitive type` are not tracked; `do`-blocks skew block depth.

use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct JuliaAdapter;

impl LanguageAdapter for JuliaAdapter {
    fn name(&self) -> &'static str {
        "Julia"
    }

    fn language(&self) -> Language {
        Language("julia")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "jl")
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        julia_parse(file)
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

/// Statement keywords that can precede a parenthesized group without being a
/// function declaration (`if (x)`, `for (k, v) in d`, `elseif (y)`).
const CONTROL: &[&str] = &[
    "if", "for", "while", "elseif", "else", "return", "throw", "assert", "do", "try", "catch",
    "finally", "begin", "let", "quote", "using", "import", "export", "in",
];

/// Keywords whose first-token appearance opens an `end`-terminated block.
/// `do`-blocks are deliberately excluded (documented depth skew).
fn opens_block(first: &str, second: Option<&&str>) -> bool {
    matches!(
        first,
        "module"
            | "baremodule"
            | "function"
            | "struct"
            | "macro"
            | "for"
            | "if"
            | "while"
            | "begin"
            | "let"
            | "quote"
            | "try"
    ) || (first == "mutable" && second == Some(&"struct"))
}

/// A line that closes a block: `end`, optionally followed by punctuation.
fn closes_block(trimmed: &str) -> bool {
    trimmed == "end"
        || (trimmed.starts_with("end")
            && trimmed[3..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_alphanumeric() && c != '_'))
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '!')
}

/// Push a surface symbol unless the name is underscore-internal.
fn emit(symbols: &mut Vec<Symbol>, name: String, kind: &str) {
    if !name.starts_with('_') {
        symbols.push(Symbol {
            name,
            kind: kind.into(),
        });
    }
}

/// Open/close byte offsets of the first top-level paren group.
fn first_paren_group(text: &str) -> Option<(usize, usize)> {
    let start = text.find('(')?;
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

/// Split a parameter list on commas at bracket depth 0 so parametric types
/// (`Dict{String, Int}`, `Vector{Int}`) keep their inner commas.
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

/// Canonical parameter-type tuple in method-dispatch form: defaults (`= ...`)
/// are dropped, `name::Type` canonicalizes to `Type`, a bare parameter to
/// `Any`, and varargs keep their `...`. `x::Int` -> `Int`, `target` -> `Any`,
/// `v::Vector{Int}` -> `Vector{Int}`, `xs...` -> `Any...`.
fn canonical_params(params: &str) -> String {
    let types: Vec<String> = split_top_level(params)
        .iter()
        .map(|p| {
            let decl = p.split('=').next().unwrap_or(p).trim();
            if let Some((_, ty)) = decl.split_once("::") {
                ty.trim().to_string()
            } else if decl.ends_with("...") {
                "Any...".to_string()
            } else if decl.is_empty() {
                String::new()
            } else {
                "Any".to_string()
            }
        })
        .filter(|t| !t.is_empty())
        .collect();
    format!("({})", types.join(","))
}

/// Identifier immediately before the paren group at `open` (`Base.show(` ->
/// `show` — the final dotted component).
fn name_before(text: &str, open: usize) -> String {
    text[..open]
        .trim_end()
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '!')
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

/// Analyze a function/macro header (long form `function f(...)`, `macro m(...)`,
/// or short form `f(...) = expr` / `Base.show(...) = expr`) and push any
/// surface symbol + canonical signature.
fn analyze_callable(
    trimmed: &str,
    long_form: bool,
    symbols: &mut Vec<Symbol>,
    signatures: &mut HashMap<String, String>,
) {
    let Some((open, close)) = first_paren_group(trimmed) else {
        return;
    };
    let name = name_before(trimmed, open);
    if !is_ident(&name) || CONTROL.contains(&name.as_str()) {
        return;
    }
    let prefix = trimmed[..open].trim();
    if long_form {
        // `function` / `macro` keyword form — qualified names allowed.
        let kind = if prefix.starts_with("macro") {
            "macro"
        } else {
            "function"
        };
        emit(symbols, name.clone(), kind);
    } else {
        // Short form requires the assignment after the parameter group:
        // `f(x) = expr`. Without it the line is a call site. A prefix
        // containing `=`/`"`/`'` is an assignment or string, not a
        // declaration; a dotted prefix (`Base.show`) is a qualified
        // definition and stays.
        let rest = &trimmed[close + 1..];
        if !rest.contains('=') {
            return;
        }
        if prefix.contains('=') || prefix.contains('"') || prefix.contains('\'') {
            return;
        }
        if prefix.is_empty() && name.starts_with(|c: char| c.is_lowercase()) {
            // Bare lowercase call with a trailing `=` somewhere (`f(x) = y`
            // is the only shape that reaches here legitimately, and it IS a
            // declaration — keep it).
        }
        emit(symbols, name.clone(), "function");
    }
    if let Some(params) = trimmed.get(open + 1..close) {
        signatures.insert(name, canonical_params(params));
    }
}

fn julia_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut signatures = HashMap::new();
    let mut in_block_comment = false;
    let mut in_triple_string = false;
    // Keyword-delimited block depth (`module`/`function`/`struct`/... opened,
    // `end` closed). Declarations are surface only at depth <= 1 (script
    // top-level or module body).
    let mut depth = 0i32;
    // Block depth at which struct fields are expected (struct body level).
    let mut struct_field_depths: Vec<i32> = Vec::new();
    let mut pending: Option<String> = None;

    for line in read_lines_safe(file)? {
        let trimmed = line.trim();
        // Triple-quoted strings/docstrings may hold declaration-shaped text.
        if in_triple_string {
            if trimmed.matches("\"\"\"").count() % 2 == 1 {
                in_triple_string = false;
            }
            continue;
        }
        // Track `#= ... =#` block comments (rev-24/26 discipline).
        if in_block_comment {
            if trimmed.contains("=#") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("#=") {
            if !trimmed.contains("=#") {
                in_block_comment = true;
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.matches("\"\"\"").count() % 2 == 1 {
            in_triple_string = true;
            continue;
        }

        if let Some(buf) = pending.as_mut() {
            buf.push(' ');
            buf.push_str(trimmed);
            if paren_balanced(buf) {
                let header = buf.clone();
                pending = None;
                let long = header.starts_with("function ") || header.starts_with("macro ");
                if depth <= 1 {
                    analyze_callable(&header, long, &mut symbols, &mut signatures);
                }
            }
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let first = tokens.first().copied().unwrap_or("");
        let second = tokens.get(1);

        if closes_block(trimmed) {
            depth -= 1;
            struct_field_depths.retain(|d| *d <= depth);
            continue;
        }

        // Struct context takes precedence over the declaration branch: a
        // script-level struct body sits at depth 1 and must not fall through.
        if struct_field_depths.last() == Some(&depth) {
            if trimmed.contains('(') {
                // Inner constructor (`Point(a, b) = new(a, b)`) — callable shape.
                if paren_balanced(trimmed) {
                    analyze_callable(trimmed, false, &mut symbols, &mut signatures);
                } else {
                    pending = Some(trimmed.to_string());
                }
            } else {
                // Struct body: every other line is a field (`name::Type` or
                // bare `name`).
                let name: String = trimmed
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if is_ident(&name) {
                    emit(&mut symbols, name, "field");
                }
            }
        } else if depth <= 1 {
            match first {
                "module" | "baremodule" => {
                    if let Some(tok) = second {
                        let name: String = tok
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if is_ident(&name) {
                            emit(&mut symbols, name, "module");
                        }
                    }
                }
                "struct" => {
                    if let Some(tok) = second {
                        // Parametric structs (`Point{T}`) cut at the brace.
                        let name: String = tok
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if is_ident(&name) {
                            if !name.starts_with('_') {
                                struct_field_depths.push(depth + 1);
                            }
                            emit(&mut symbols, name, "struct");
                        }
                    }
                }
                "mutable" if second == Some(&"struct") => {
                    if let Some(tok) = tokens.get(2) {
                        let name: String = tok
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if is_ident(&name) {
                            if !name.starts_with('_') {
                                struct_field_depths.push(depth + 1);
                            }
                            emit(&mut symbols, name, "struct");
                        }
                    }
                }
                "abstract" if second == Some(&"type") => {
                    if let Some(tok) = tokens.get(2) {
                        let name: String = tok
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if is_ident(&name) {
                            emit(&mut symbols, name, "abstract");
                        }
                    }
                }
                "const" => {
                    if let Some(tok) = second {
                        let name: String = tok
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if is_ident(&name) {
                            emit(&mut symbols, name, "const");
                        }
                    }
                }
                "function" | "macro" => {
                    if paren_balanced(trimmed) {
                        analyze_callable(trimmed, true, &mut symbols, &mut signatures);
                    } else {
                        pending = Some(trimmed.to_string());
                    }
                }
                _ => {
                    if trimmed.contains('(') {
                        if paren_balanced(trimmed) {
                            analyze_callable(trimmed, false, &mut symbols, &mut signatures);
                        } else {
                            pending = Some(trimmed.to_string());
                        }
                    }
                }
            }
        }

        if opens_block(first, second) {
            depth += 1;
        }
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
        JuliaAdapter.parse_ast(f.path()).unwrap()
    }

    fn names(ast: &AstRepresentation) -> Vec<(String, String)> {
        ast.symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind.clone()))
            .collect()
    }

    #[test]
    fn detects_modules_and_types() {
        let ast = parse(
            "module Geometry\n\
             struct Point\n\
             \x20   x::Float64\n\
             end\n\
             mutable struct Vec\n\
             \x20   x::Float64\n\
             end\n\
             abstract type Shape end\n\
             end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Geometry".into(), "module".into())));
        assert!(got.contains(&("Point".into(), "struct".into())));
        assert!(got.contains(&("Vec".into(), "struct".into())));
        assert!(got.contains(&("Shape".into(), "abstract".into())));
    }

    #[test]
    fn functions_long_and_short_form() {
        let ast = parse(
            "function distance(a, b)\n\
             \x20   return abs(a - b)\n\
             end\n\
             area(p) = 0.0\n\
             function Base.show(io, x)\n\
             \x20   print(io, x)\n\
             end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("distance".into(), "function".into())));
        assert!(got.contains(&("area".into(), "function".into())));
        assert!(got.contains(&("show".into(), "function".into())));
    }

    #[test]
    fn macros_and_consts() {
        let ast = parse(
            "const MAX_SIZE = 100\n\
             macro logged(expr)\n\
             \x20   return expr\n\
             end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("MAX_SIZE".into(), "const".into())));
        assert!(got.contains(&("logged".into(), "macro".into())));
    }

    #[test]
    fn struct_fields_emitted_body_statements_skipped() {
        let ast = parse(
            "struct Point\n\
             \x20   x::Float64\n\
             \x20   y::Float64\n\
             \x20   Point(a, b) = new(a, b)\n\
             end\n\
             function run()\n\
             \x20   local_var = 5\n\
             \x20   z::Int = 3\n\
             end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("x".into(), "field".into())));
        assert!(got.contains(&("y".into(), "field".into())));
        assert!(!got.iter().any(|(n, _)| n == "local_var"));
        assert!(!got.iter().any(|(n, k)| n == "z" && k == "field"));
    }

    #[test]
    fn signatures_are_canonical_param_types() {
        let ast = parse(
            "function add(x::Int, y::String)\n\
             \x20   return x\n\
             end\n\
             mix(a, b::Float64 = 1.0) = a\n\
             collect_all(xs::Int...) = xs\n\
             lookup(d::Dict{String, Int}, k::String) = d[k]\n",
        );
        assert_eq!(
            ast.signatures.get("add").map(String::as_str),
            Some("(Int,String)")
        );
        assert_eq!(
            ast.signatures.get("mix").map(String::as_str),
            Some("(Any,Float64)")
        );
        assert_eq!(
            ast.signatures.get("collect_all").map(String::as_str),
            Some("(Int...)")
        );
        assert_eq!(
            ast.signatures.get("lookup").map(String::as_str),
            Some("(Dict{String, Int},String)")
        );
    }

    #[test]
    fn underscore_names_are_internal() {
        let ast = parse(
            "function _helper(x)\n\
             \x20   return x\n\
             end\n\
             struct _Internal\n\
             \x20   v::Int\n\
             end\n\
             const _SECRET = 1\n\
             function public_fn(x)\n\
             \x20   return x\n\
             end\n",
        );
        let got = names(&ast);
        assert_eq!(got, vec![("public_fn".into(), "function".into())]);
    }

    #[test]
    fn skips_comments_and_docstrings() {
        let ast = parse(
            "# function fake_hash(x) = x\n\
             #=\nstruct FakeBlock\n  a::Int\nend\n=#\n\
             \"\"\"\nfunction doc_fake(x)\nend\n\"\"\"\n\
             struct Real\n\
             \x20   v::Int\n\
             end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Real".into(), "struct".into())));
        assert!(got.contains(&("v".into(), "field".into())));
        assert!(!got.iter().any(|(n, _)| n == "fake_hash"));
        assert!(!got.iter().any(|(n, _)| n == "FakeBlock"));
        assert!(!got.iter().any(|(n, _)| n == "doc_fake"));
    }

    #[test]
    fn call_sites_are_not_functions() {
        let ast = parse(
            "println(\"hi\")\n\
             result = compute(a, b)\n\
             y = (a + b)\n\
             for (k, v) in dict\n\
             \x20   println(k, v)\n\
             end\n\
             @time work(x)\n\
             assert(check(x) == 1)\n\
             (a, b) = pair(x)\n",
        );
        assert!(ast.symbols.is_empty());
    }

    #[test]
    fn param_rename_keeps_signature() {
        let a = parse("f(count::Int) = count\n");
        let b = parse("f(total::Int) = total\n");
        let diff = JuliaAdapter.diff_ast(&a, &b);
        assert!(diff.modified.is_empty());
        let c = parse("f(count::String) = count\n");
        let diff = JuliaAdapter.diff_ast(&a, &c);
        assert!(!diff.modified.is_empty());
    }

    #[test]
    fn removed_function_is_breaking_underscore_is_not() {
        let old = parse("function a() = 1\nfunction b() = 2\n");
        let new = parse("function a() = 1\n");
        let diff = JuliaAdapter.diff_ast(&old, &new);
        assert!(JuliaAdapter.detect_breaking_changes(&diff));

        let old_u = parse("function a() = 1\nfunction _h() = 2\n");
        let new_u = parse("function a() = 1\n");
        let diff_u = JuliaAdapter.diff_ast(&old_u, &new_u);
        assert!(!JuliaAdapter.detect_breaking_changes(&diff_u));
    }

    #[test]
    fn detect_files_matches_jl() {
        let paths = vec![
            PathBuf::from("Geometry.jl"),
            PathBuf::from("Service.groovy"),
            PathBuf::from("notes.md"),
        ];
        let got = JuliaAdapter.detect_files(&paths);
        assert_eq!(got.len(), 1);
    }
}
