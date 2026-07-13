//! Zig adapter: explicit-`pub` declarations — functions, typed `pub const`
//! containers (`struct`/`enum`/`union`/`opaque`), constants, variables, and
//! struct fields as public API surface.
//!
//! Structured line scanner (T2 depth). Zig's visibility model is explicit:
//! a declaration is public exactly when it carries the `pub` keyword (or
//! `export`, which implies a public C-ABI entry point), so this adapter sits
//! in the explicit-visibility 0.8 confidence band. Zig's types are `pub
//! const` declarations: `pub const Point = struct { ... }` (also `packed
//! struct` / `extern struct`), `enum`, `union` / `union(enum)`, and `opaque`.
//! Struct bodies are tracked by brace depth and their `name: Type` fields
//! are surface — Zig has no field-level privacy, so every field of an
//! accessible container is reachable. Enum/union members are NOT emitted
//! (type identity only, matching the NS_ENUM precedent). Methods are
//! container-level `pub fn` declarations; they are emitted flat under their
//! own name (groovy precedent — cross-type name collisions merge, a known
//! T2 limitation). Function signatures are recorded as canonical
//! parameter-type tuples: `name: Type` pairs reduce to their type, so a
//! parameter rename leaves the signature untouched while a type change
//! alters it (`comptime` prefixes dropped, variadic `...` skipped,
//! whitespace normalized). Multi-line headers accumulate to the `;`/`{`
//! terminator at paren depth 0. Born-correct comment handling: Zig has ONLY
//! `//` line comments (`///` doc, `//!` module — no block comments); the
//! stripper is string-aware so a `//` inside a URL default is kept, and
//! `\\`-prefixed multi-line string literal lines are never parsed.
//! Exclusions: non-`pub` declarations, plain `extern fn` imports,
//! `usingnamespace` re-exports, `test` blocks, `comptime` blocks, and fields
//! written on the container's opening line (single-line bodies — the type
//! symbol is still emitted).

use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct ZigAdapter;

impl LanguageAdapter for ZigAdapter {
    fn name(&self) -> &'static str {
        "Zig"
    }

    fn language(&self) -> Language {
        Language("zig")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "zig")
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        zig_parse(file)
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
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Strip a `//` line comment, honoring `"` string literals so `//` inside a
/// string (e.g. a URL default) does not truncate the line. Zig has no block
/// comments; `///` and `//!` doc comments start with `//` and are covered.
fn strip_line_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if in_string {
            if c == '"' && prev != '\\' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == '/' && prev == '/' {
            return &line[..i - 1];
        }
        prev = c;
    }
    line
}

/// Byte offset and kind of the first `;` or `{` at paren depth 0 — the
/// terminator of a declaration header.
fn header_end(text: &str) -> Option<(usize, char)> {
    let mut depth = 0i32;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' | '{' if depth == 0 => return Some((i, c)),
            _ => {}
        }
    }
    None
}

/// Text between the first `(` and its matching `)`.
fn extract_params(header: &str) -> Option<&str> {
    let start = header.find('(')?;
    let mut depth = 0i32;
    for (i, c) in header[start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&header[start + 1..start + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a parameter list on commas at bracket depth 0 so function-pointer
/// types (`fn (i32, u8) void`) and array brackets keep their commas. Zig has
/// no angle-bracket generics, so `<`/`>` are not tracked.
fn split_top_level(params: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in params.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
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

/// Canonical parameter-type tuple: `name: Type` pairs reduce to their type,
/// so a parameter rename leaves the signature untouched while a type change
/// alters it. `comptime` prefixes are dropped, variadic `...` skipped, and
/// whitespace normalized. `a: Point, b: *const u8` -> `(Point,*const u8)`.
fn canonical_params(params: &str) -> String {
    let types: Vec<String> = split_top_level(params)
        .iter()
        .filter_map(|p| {
            let trimmed = p.trim();
            let decl = trimmed.strip_prefix("comptime ").unwrap_or(trimmed);
            if decl.is_empty() || decl == "..." {
                return None;
            }
            let colon = decl.find(':')?;
            let ty = decl[colon + 1..]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if ty.is_empty() {
                None
            } else {
                Some(ty)
            }
        })
        .collect();
    format!("({})", types.join(","))
}

/// Identifier prefix of a token (`distance(` -> `distance`).
fn decl_name(tok: Option<&&str>) -> Option<String> {
    let raw = tok.copied()?;
    let name: String = raw
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if is_ident(&name) {
        Some(name)
    } else {
        None
    }
}

/// Analyze a complete declaration header (text before the `;`/`{`
/// terminator) and push any surface symbol + optional canonical signature.
/// Returns true when the header opens a struct body at the terminator, so
/// the caller can gate field detection by brace depth.
fn analyze_header(
    header: &str,
    terminator: char,
    symbols: &mut Vec<Symbol>,
    signatures: &mut HashMap<String, String>,
) -> bool {
    let rest = header.strip_prefix("pub ").unwrap_or(header);
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let mut idx = 0;
    // Calling-convention modifiers between `pub` and `fn`.
    if tokens.get(idx) == Some(&"export") {
        idx += 1;
    } else if tokens.get(idx) == Some(&"extern") {
        idx += 1;
        if tokens.get(idx).is_some_and(|t| t.starts_with('"')) {
            idx += 1;
        }
    }
    match tokens.get(idx).copied().unwrap_or("") {
        "fn" => {
            if let Some(name) = decl_name(tokens.get(idx + 1)) {
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: "function".into(),
                });
                if let Some(params) = extract_params(header) {
                    signatures.insert(name, canonical_params(params));
                }
            }
            false
        }
        "var" => {
            if let Some(name) = decl_name(tokens.get(idx + 1)) {
                symbols.push(Symbol {
                    name,
                    kind: "variable".into(),
                });
            }
            false
        }
        "const" => {
            let Some(name) = decl_name(tokens.get(idx + 1)) else {
                return false;
            };
            let rhs = header.split_once('=').map(|(_, r)| r.trim()).unwrap_or("");
            let kind = if rhs.starts_with("packed struct")
                || rhs.starts_with("extern struct")
                || rhs.starts_with("struct")
            {
                "struct"
            } else if rhs.starts_with("enum") {
                "enum"
            } else if rhs.starts_with("union") {
                "union"
            } else if rhs.starts_with("opaque") {
                "opaque"
            } else {
                "const"
            };
            symbols.push(Symbol {
                name,
                kind: kind.into(),
            });
            kind == "struct" && terminator == '{'
        }
        _ => false,
    }
}

/// Keywords that may appear at the start of a struct-body line and must not
/// be mistaken for a field name.
const FIELD_KEYWORDS: &[&str] = &[
    "const",
    "var",
    "fn",
    "pub",
    "test",
    "comptime",
    "usingnamespace",
    "return",
    "if",
    "else",
    "while",
    "for",
    "switch",
    "struct",
    "enum",
    "union",
    "opaque",
];

/// Struct-body field `name: Type` — the identifier before the first `:`.
/// Enum/union members are excluded by depth gating (their containers are not
/// tracked as struct bodies); keyword-led declarations never match.
fn field_name(trimmed: &str) -> Option<String> {
    let (head, _) = trimmed.split_once(':')?;
    let name = head.trim();
    if is_ident(name) && !FIELD_KEYWORDS.contains(&name) {
        Some(name.to_string())
    } else {
        None
    }
}

/// Net brace delta of a code line (comment-stripped; braces inside string
/// literals on code lines can skew it — rare in declarations).
fn net_braces(line: &str) -> i32 {
    line.chars().fold(0i32, |acc, c| match c {
        '{' => acc + 1,
        '}' => acc - 1,
        _ => acc,
    })
}

fn zig_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut signatures = HashMap::new();
    // Brace depth for field gating: a struct body lives at the depth pushed
    // when its header terminated on `{`.
    let mut depth = 0i32;
    let mut struct_depths: Vec<i32> = Vec::new();
    // A declaration header spanning multiple lines (idiomatic Zig puts one
    // parameter per line), accumulated until its `;`/`{` terminator.
    let mut pending: Option<String> = None;

    for line in read_lines_safe(file)? {
        let trimmed = strip_line_comment(&line).trim();
        if trimmed.is_empty() {
            continue;
        }
        // `\\` lines are Zig multi-line string literal content, never code.
        if trimmed.starts_with("\\\\") {
            continue;
        }

        if let Some(buf) = pending.as_mut() {
            buf.push(' ');
            buf.push_str(trimmed);
            if let Some((end, term)) = header_end(buf) {
                let header = buf[..end].to_string();
                pending = None;
                if analyze_header(&header, term, &mut symbols, &mut signatures) {
                    struct_depths.push(depth + 1);
                }
            }
            depth += net_braces(trimmed);
            struct_depths.retain(|d| *d <= depth);
            continue;
        }

        if trimmed.starts_with("pub ") || trimmed.starts_with("export ") {
            if let Some((end, term)) = header_end(trimmed) {
                if analyze_header(&trimmed[..end], term, &mut symbols, &mut signatures) {
                    struct_depths.push(depth + 1);
                }
            } else {
                pending = Some(trimmed.to_string());
            }
        } else if struct_depths.last() == Some(&depth) {
            if let Some(name) = field_name(trimmed) {
                symbols.push(Symbol {
                    name,
                    kind: "field".into(),
                });
            }
        }
        depth += net_braces(trimmed);
        struct_depths.retain(|d| *d <= depth);
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
        ZigAdapter.parse_ast(f.path()).unwrap()
    }

    fn names(ast: &AstRepresentation) -> Vec<(String, String)> {
        ast.symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind.clone()))
            .collect()
    }

    #[test]
    fn detects_pub_declarations() {
        let ast = parse(
            "pub const VERSION = \"1.0.0\";\n\
             pub var counter: u32 = 0;\n\
             pub fn greet(name: []const u8) void {\n    _ = name;\n}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("VERSION".into(), "const".into())));
        assert!(got.contains(&("counter".into(), "variable".into())));
        assert!(got.contains(&("greet".into(), "function".into())));
    }

    #[test]
    fn typed_const_containers() {
        let ast = parse(
            "pub const Point = struct {\n    x: f32,\n};\n\
             pub const Color = enum {\n    red,\n    green,\n};\n\
             pub const Value = union(enum) {\n    i: i32,\n};\n\
             pub const Handle = opaque {};\n\
             pub const Bits = packed struct {\n    a: u1,\n};\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Point".into(), "struct".into())));
        assert!(got.contains(&("Color".into(), "enum".into())));
        assert!(got.contains(&("Value".into(), "union".into())));
        assert!(got.contains(&("Handle".into(), "opaque".into())));
        assert!(got.contains(&("Bits".into(), "struct".into())));
    }

    #[test]
    fn struct_fields_emitted_enum_members_not() {
        let ast = parse(
            "pub const Point = struct {\n    x: f32,\n    y: f32 = 0,\n};\n\
             pub const Color = enum {\n    red,\n    green,\n};\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("x".into(), "field".into())));
        assert!(got.contains(&("y".into(), "field".into())));
        assert!(!got.iter().any(|(n, _)| n == "red" || n == "green"));
    }

    #[test]
    fn non_pub_is_not_surface() {
        let ast = parse(
            "const secret = 1;\n\
             var mutable: i32 = 0;\n\
             fn helper() void {}\n\
             extern fn puts(s: [*:0]const u8) c_int;\n\
             pub fn visible() void {}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("visible".into(), "function".into())));
        assert!(!got.iter().any(|(n, _)| n == "secret" || n == "mutable"));
        assert!(!got.iter().any(|(n, _)| n == "helper" || n == "puts"));
    }

    #[test]
    fn export_and_extern_fn_forms() {
        let ast = parse(
            "export fn cEntry(x: c_int) c_int {\n    return x;\n}\n\
             pub export fn pubC(x: c_int) c_int {\n    return x;\n}\n\
             pub extern \"c\" fn imported(x: c_int) c_int;\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("cEntry".into(), "function".into())));
        assert!(got.contains(&("pubC".into(), "function".into())));
        assert!(got.contains(&("imported".into(), "function".into())));
    }

    #[test]
    fn canonical_param_type_signatures() {
        let ast = parse(
            "pub fn distance(a: Point, b: Point) f32 {\n    return 0;\n}\n\
             pub fn write(buf: []const u8, comptime T: type, cb: fn (i32, u8) void) void {\n\
             \x20   _ = buf;\n    _ = T;\n    _ = cb;\n}\n",
        );
        assert_eq!(
            ast.signatures.get("distance").map(String::as_str),
            Some("(Point,Point)")
        );
        assert_eq!(
            ast.signatures.get("write").map(String::as_str),
            Some("([]const u8,type,fn (i32, u8) void)")
        );
    }

    #[test]
    fn multiline_fn_headers() {
        let ast = parse(
            "pub fn create(\n    allocator: std.mem.Allocator,\n    size: usize,\n) !Foo {\n\
             \x20   _ = allocator;\n    _ = size;\n}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("create".into(), "function".into())));
        assert_eq!(
            ast.signatures.get("create").map(String::as_str),
            Some("(std.mem.Allocator,usize)")
        );
    }

    #[test]
    fn methods_inside_structs_are_flat_functions() {
        let ast = parse(
            "pub const Point = struct {\n    x: f32,\n\n\
             \x20   pub fn distance(a: Point, b: Point) f32 {\n        return 0;\n    }\n\n\
             \x20   fn reset(self: *Point) void {\n        _ = self;\n    }\n};\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Point".into(), "struct".into())));
        assert!(got.contains(&("x".into(), "field".into())));
        assert!(got.contains(&("distance".into(), "function".into())));
        // Non-pub method and method-body locals are not surface.
        assert!(!got.iter().any(|(n, _)| n == "reset" || n == "self"));
    }

    #[test]
    fn skips_comments_and_multiline_strings() {
        let ast = parse(
            "// pub fn fakeOne() void {}\n\
             /// pub fn fakeTwo() void {}\n\
             //! pub const FakeThree = struct {};\n\
             const text =\n    \\\\pub fn fakeFour() void {}\n    \\\\pub const FakeFive = enum { a };\n;\n\
             pub const url = \"http://example.com\";\n\
             pub fn genuine() void {}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("url".into(), "const".into())));
        assert!(got.contains(&("genuine".into(), "function".into())));
        assert!(!got
            .iter()
            .any(|(n, _)| n.starts_with("fake") || n.starts_with("Fake")));
    }

    #[test]
    fn removed_fn_is_breaking_non_pub_is_not() {
        let old = parse("pub fn a() void {}\npub fn b() void {}\n");
        let new = parse("pub fn a() void {}\n");
        let diff = ZigAdapter.diff_ast(&old, &new);
        assert!(ZigAdapter.detect_breaking_changes(&diff));

        let old_u = parse("pub fn a() void {}\nfn hidden() void {}\n");
        let new_u = parse("pub fn a() void {}\n");
        let diff_u = ZigAdapter.diff_ast(&old_u, &new_u);
        assert!(!ZigAdapter.detect_breaking_changes(&diff_u));
    }

    #[test]
    fn detect_files_matches_zig() {
        let paths = vec![
            PathBuf::from("main.zig"),
            PathBuf::from("main.zig.zon"),
            PathBuf::from("notes.md"),
        ];
        let got = ZigAdapter.detect_files(&paths);
        assert_eq!(got.len(), 1);
    }
}
