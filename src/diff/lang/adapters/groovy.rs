//! Groovy adapter: type declarations, public-by-default members, and
//! depth-tracked properties as public API surface.
//!
//! Structured line scanner (T2 depth). Groovy members are `public` by default,
//! so surface = every declaration without an explicit `private`/`protected`:
//! `class`/`interface`/`trait`/`enum`/`@interface` declarations, methods
//! (`def name(...)` or `Type name(...)`, including script-level methods),
//! constructors (PascalCase name with no return type), and properties —
//! Groovy's signature feature, a field without a visibility keyword generates
//! public getters/setters. Properties are only emitted at class-body depth 1
//! so method-local `def x`/`String x` declarations are not mistaken for them
//! (nested-class members below depth 1 are a documented miss). Method and
//! constructor signatures are recorded as canonical parameter-type tuples
//! (`(int,String)`; untyped parameters canonicalize to `def`, default values
//! and annotations are dropped), so parameter renames are invisible but type
//! changes register as modifications. Headers may span multiple lines; the
//! scanner accumulates to the `{`/`;` terminator at paren depth 0.
//! Born-correct comment handling per rev-24/26: `//` line comments, `/* ... */`
//! block comments, the `#!` shebang line, and `'''`/`"""` triple-quoted string
//! regions (whose bodies can hold declaration-shaped text). Known T2
//! limitations: PascalCase call-with-closure (`Frame(title: "x") { }`) is
//! indistinguishable from a constructor; `.gradle` DSL files are out of scope.

use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct GroovyAdapter;

impl LanguageAdapter for GroovyAdapter {
    fn name(&self) -> &'static str {
        "Groovy"
    }

    fn language(&self) -> Language {
        Language("groovy")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "groovy")
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        groovy_parse(file)
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
/// method declaration (`if (x) {`, `synchronized (lock) {`, `this(args)`).
const CONTROL: &[&str] = &[
    "if",
    "for",
    "while",
    "switch",
    "catch",
    "try",
    "return",
    "new",
    "throw",
    "assert",
    "else",
    "do",
    "synchronized",
    "super",
    "this",
];

/// Prefix tokens that prove a paren group is a call/statement, not a
/// declaration (`return foo(a)`, `throw new E(a)`).
const PREFIX_REJECT: &[&str] = &["return", "throw", "new", "assert", "case", "yield"];

/// Modifiers allowed before a type keyword in a declaration line.
const MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "abstract",
    "final",
    "static",
    "sealed",
    "strictfp",
];

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Leading identifier of a token (`Foo<Bar>` -> `Foo`).
fn decl_name(tok: Option<&&str>) -> Option<String> {
    let raw = tok?;
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

/// Byte offset of the first `{` or `;` at paren depth 0 — the terminator of a
/// declaration header.
fn header_end(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '{' | ';' if depth == 0 => return Some(i),
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

/// Split a parameter list on commas at bracket depth 0 so generic type
/// arguments (`Map<String, Integer>`) and array brackets keep their commas.
fn split_top_level(params: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in params.char_indices() {
        match c {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' | '>' => depth -= 1,
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

/// Canonical parameter-type tuple: default values (`= ...`), parameter
/// annotations, and the `final` keyword are dropped, the trailing parameter
/// name is removed, and a bare (dynamically typed) parameter canonicalizes to
/// `def`. `List<String> items` -> `List<String>`, `String... args` ->
/// `String...`, `int a = 1` -> `int`, `target` -> `def`.
fn canonical_params(params: &str) -> String {
    let types: Vec<String> = split_top_level(params)
        .iter()
        .map(|p| {
            let decl = p.split('=').next().unwrap_or(p);
            let toks: Vec<&str> = decl
                .split_whitespace()
                .filter(|t| !t.starts_with('@') && *t != "final")
                .collect();
            if toks.len() > 1 {
                toks[..toks.len() - 1].join(" ")
            } else {
                // A lone token is a bare, dynamically typed parameter.
                "def".to_string()
            }
        })
        .filter(|t| !t.is_empty())
        .collect();
    format!("({})", types.join(","))
}

/// `class`/`interface`/`trait`/`enum`/`@interface` declaration on a tokenized
/// line, allowing leading modifiers and annotations (`abstract class Foo`,
/// `@Deprecated interface Bar`). The type keyword must appear at token index
/// ≤ 2 so usages deeper in a statement cannot impersonate a declaration.
fn type_declaration(tokens: &[&str]) -> Option<(String, String)> {
    if tokens.first() == Some(&"@interface") {
        return decl_name(tokens.get(1)).map(|n| (n, "annotation".to_string()));
    }
    for kw in ["class", "interface", "trait", "enum"] {
        if let Some(idx) = tokens.iter().position(|t| *t == kw) {
            if idx > 2 {
                return None;
            }
            let mods_ok = tokens[..idx]
                .iter()
                .all(|t| MODIFIERS.contains(t) || t.starts_with('@'));
            if !mods_ok {
                return None;
            }
            return decl_name(tokens.get(idx + 1)).map(|n| (n, kw.to_string()));
        }
    }
    None
}

/// Property (field without `private`/`protected`) on a class-body line. Lines
/// containing `(` are left to the method path — this trades fields with
/// call-shaped initializers (a rare false negative) for immunity to the far
/// more common call sites. Enum constants are rejected by their trailing
/// commas.
fn property_declaration(trimmed: &str) -> Option<String> {
    if trimmed.contains('(') {
        return None;
    }
    let decl = trimmed.split('=').next().unwrap_or(trimmed);
    let tokens: Vec<&str> = decl.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    if tokens.iter().any(|t| t.ends_with(',')) {
        return None;
    }
    if tokens.iter().any(|t| *t == "private" || *t == "protected") {
        return None;
    }
    decl_name(tokens.last())
}

/// Return types a method prefix may end with: `def`/`var`, primitives, or a
/// PascalCase type (incl. generics/arrays). A lowercase bareword last token
/// marks a call statement (`println greet("world")`), not a declaration.
const PREFIX_TYPES: &[&str] = &[
    "def", "var", "void", "int", "long", "double", "float", "boolean", "char", "byte", "short",
];

/// True when the text's parens are balanced and never go negative — an
/// abstract/interface method header with no `{`/`;` terminator.
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

/// Analyze a complete method/constructor header (text before the `{`/`;`
/// terminator, or the whole balanced line) and push any surface symbol +
/// canonical signature.
fn analyze_method_header(
    header: &str,
    symbols: &mut Vec<Symbol>,
    signatures: &mut HashMap<String, String>,
) {
    let Some(paren) = header.find('(') else {
        return;
    };
    let before = &header[..paren];
    let name: String = before
        .trim_end()
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if !is_ident(&name) || CONTROL.contains(&name.as_str()) {
        return;
    }
    let prefix = before[..before.len() - name.len()].trim();
    // A declaration prefix is `def`/modifiers/type tokens only. `=` marks an
    // assignment, `)`/`.` mark a chained call.
    if prefix.contains('=') || prefix.contains(')') || prefix.contains('.') {
        return;
    }
    let ptokens: Vec<&str> = prefix.split_whitespace().collect();
    if ptokens.iter().any(|t| PREFIX_REJECT.contains(t)) {
        return;
    }
    // A pure-annotation prefix (`@Max(10)`, `@GET @Path("/x")`) is usage, not
    // a declaration. Annotations followed by `def`/a type are fine.
    if !ptokens.is_empty() && ptokens.iter().all(|t| t.starts_with('@')) {
        return;
    }
    if ptokens.iter().any(|t| *t == "private" || *t == "protected") {
        return;
    }
    let kind = if ptokens.is_empty() {
        // No return type: a constructor by Groovy/Java naming convention.
        // Lowercase bare calls (`foo(a) {`) never reach here usefully.
        if !name.starts_with(|c: char| c.is_uppercase()) {
            return;
        }
        "constructor"
    } else {
        // The prefix must end with a return type (`def`, primitive, or
        // PascalCase type); a trailing lowercase bareword is a call.
        let last = ptokens.last().copied().unwrap_or("");
        let type_like =
            PREFIX_TYPES.contains(&last) || last.starts_with(|c: char| c.is_uppercase());
        if !type_like {
            return;
        }
        "method"
    };
    symbols.push(Symbol {
        name: name.clone(),
        kind: kind.into(),
    });
    if let Some(params) = extract_params(header) {
        signatures.insert(name, canonical_params(params));
    }
}

/// Net brace delta of a code line (naive — braces inside string literals on
/// code lines can skew it; balanced `${}` interpolation does not).
fn net_braces(line: &str) -> i32 {
    line.chars().fold(0i32, |acc, c| match c {
        '{' => acc + 1,
        '}' => acc - 1,
        _ => acc,
    })
}

fn groovy_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut signatures = HashMap::new();
    let mut in_block_comment = false;
    let mut in_triple_string = false;
    // Brace depth for property gating: class body is depth 1.
    let mut depth = 0i32;
    // A method header spanning multiple lines, accumulated until its
    // `{`/`;` terminator at paren depth 0.
    let mut pending: Option<String> = None;

    for (idx, line) in read_lines_safe(file)?.enumerate() {
        let trimmed = line.trim();
        // Triple-quoted string bodies may hold declaration-shaped text.
        if in_triple_string {
            let toggles = trimmed.matches("'''").count() + trimmed.matches("\"\"\"").count();
            if toggles % 2 == 1 {
                in_triple_string = false;
            }
            continue;
        }
        // Track `/* ... */` regions (measured messy-corpus discipline, rev-24/26).
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if idx == 0 && trimmed.starts_with("#!") {
            continue;
        }
        let toggles = trimmed.matches("'''").count() + trimmed.matches("\"\"\"").count();
        if toggles % 2 == 1 {
            in_triple_string = true;
            continue;
        }

        if let Some(buf) = pending.as_mut() {
            buf.push(' ');
            buf.push_str(trimmed);
            // Complete on the `{`/`;` terminator, or on balanced parens alone
            // (abstract/interface methods end without either in Groovy).
            if let Some(end) = header_end(buf) {
                let header = buf[..end].to_string();
                pending = None;
                analyze_method_header(&header, &mut symbols, &mut signatures);
            } else if paren_balanced(buf) {
                let header = buf.clone();
                pending = None;
                analyze_method_header(&header, &mut symbols, &mut signatures);
            }
            depth += net_braces(trimmed);
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if let Some((name, kind)) = type_declaration(&tokens) {
            symbols.push(Symbol { name, kind });
        } else if trimmed.contains('(') {
            if let Some(end) = header_end(trimmed) {
                analyze_method_header(&trimmed[..end], &mut symbols, &mut signatures);
            } else if paren_balanced(trimmed) {
                analyze_method_header(trimmed, &mut symbols, &mut signatures);
            } else {
                pending = Some(trimmed.to_string());
            }
        } else if depth == 1 {
            if let Some(name) = property_declaration(trimmed) {
                symbols.push(Symbol {
                    name,
                    kind: "property".into(),
                });
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
        GroovyAdapter.parse_ast(f.path()).unwrap()
    }

    fn names(ast: &AstRepresentation) -> Vec<(String, String)> {
        ast.symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind.clone()))
            .collect()
    }

    #[test]
    fn detects_type_declarations() {
        let ast = parse(
            "class Greeter {\n}\n\
             abstract class Base {\n}\n\
             interface Closeable {\n}\n\
             trait Loggable {\n}\n\
             enum Status {\n  OK,\n  ERROR\n}\n\
             @interface Marker {\n}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Greeter".into(), "class".into())));
        assert!(got.contains(&("Base".into(), "class".into())));
        assert!(got.contains(&("Closeable".into(), "interface".into())));
        assert!(got.contains(&("Loggable".into(), "trait".into())));
        assert!(got.contains(&("Status".into(), "enum".into())));
        assert!(got.contains(&("Marker".into(), "annotation".into())));
    }

    #[test]
    fn methods_public_by_default_private_skipped() {
        let ast = parse(
            "class C {\n\
             \x20   def open(String a) { return a }\n\
             \x20   String typed(int n) { return \"x\" }\n\
             \x20   static void util() {}\n\
             \x20   private def hidden() {}\n\
             \x20   protected def guarded() {}\n\
             }\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("open".into(), "method".into())));
        assert!(got.contains(&("typed".into(), "method".into())));
        assert!(got.contains(&("util".into(), "method".into())));
        assert!(!got.iter().any(|(n, _)| n == "hidden"));
        assert!(!got.iter().any(|(n, _)| n == "guarded"));
    }

    #[test]
    fn constructors_detected_by_convention() {
        let ast = parse(
            "class Token {\n\
             \x20   Token(String name) {\n\
             \x20   }\n\
             \x20   def value() { return 1 }\n\
             }\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Token".into(), "constructor".into())));
        assert!(got.contains(&("Token".into(), "class".into())));
        assert_eq!(
            ast.signatures.get("Token").map(String::as_str),
            Some("(String)")
        );
    }

    #[test]
    fn properties_at_class_depth_locals_skipped() {
        let ast = parse(
            "class C {\n\
             \x20   String name\n\
             \x20   def count = 0\n\
             \x20   private String secret\n\
             \x20   static final int MAX = 10\n\
             \x20   def run() {\n\
             \x20       def local = 5\n\
             \x20       String another = \"x\"\n\
             \x20   }\n\
             }\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("name".into(), "property".into())));
        assert!(got.contains(&("count".into(), "property".into())));
        assert!(got.contains(&("MAX".into(), "property".into())));
        assert!(!got.iter().any(|(n, _)| n == "secret"));
        assert!(!got.iter().any(|(n, _)| n == "local"));
        assert!(!got.iter().any(|(n, _)| n == "another"));
    }

    #[test]
    fn signatures_are_canonical_param_types() {
        let ast = parse(
            "class C {\n\
             \x20   def add(int a, String b) { return a }\n\
             \x20   def dynamic(a, b) { return a }\n\
             \x20   def defaulted(String x = \"hi\", int n = 1) { return x }\n\
             \x20   def generic(List<String> items, Map<String, Integer> counts) { return items }\n\
             \x20   def varargs(String... args) { return args }\n\
             }\n",
        );
        assert_eq!(
            ast.signatures.get("add").map(String::as_str),
            Some("(int,String)")
        );
        assert_eq!(
            ast.signatures.get("dynamic").map(String::as_str),
            Some("(def,def)")
        );
        assert_eq!(
            ast.signatures.get("defaulted").map(String::as_str),
            Some("(String,int)")
        );
        assert_eq!(
            ast.signatures.get("generic").map(String::as_str),
            Some("(List<String>,Map<String, Integer>)")
        );
        assert_eq!(
            ast.signatures.get("varargs").map(String::as_str),
            Some("(String...)")
        );
    }

    #[test]
    fn call_sites_are_not_methods() {
        let ast = parse(
            "class C {\n\
             \x20   def run() {\n\
             \x20       println(\"hi\")\n\
             \x20       println greet(\"world\")\n\
             \x20       return compute(a, b)\n\
             \x20       def r = compute(a, b)\n\
             \x20       items.each { println it }\n\
             \x20       if (r) { return }\n\
             \x20       throw new IllegalStateException(\"x\")\n\
             \x20   }\n\
             }\n",
        );
        assert_eq!(
            names(&ast),
            vec![
                ("C".into(), "class".into()),
                ("run".into(), "method".into()),
            ]
        );
    }

    #[test]
    fn interface_methods_without_terminator() {
        let ast = parse(
            "interface Repository {\n\
             \x20   def find(String id)\n\
             \n\
             \x20   def save(Entity entity)\n\
             }\n\
             trait T {\n\
             \x20   def touch() { return 1 }\n\
             }\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Repository".into(), "interface".into())));
        assert!(got.contains(&("find".into(), "method".into())));
        assert!(got.contains(&("save".into(), "method".into())));
        assert!(got.contains(&("T".into(), "trait".into())));
        assert!(got.contains(&("touch".into(), "method".into())));
    }

    #[test]
    fn skips_comments_and_triple_quoted_strings() {
        let ast = parse(
            "#!/usr/bin/env groovy\n\
             // class FakeOne {\n\
             //     def hacked() {}\n\
             // }\n\
             /*\ninterface IFake {\n  def stolen()\n}\n*/\n\
             def text = '''\n\
             class InString {\n\
                 def fake() {}\n\
             }\n\
             '''\n\
             class Real {\n\
             \x20   def genuine() { return 1 }\n\
             }\n",
        );
        assert_eq!(
            names(&ast),
            vec![
                ("Real".into(), "class".into()),
                ("genuine".into(), "method".into()),
            ]
        );
    }

    #[test]
    fn param_rename_keeps_signature() {
        let a = parse("class C {\n  def f(int count) { return count }\n}\n");
        let b = parse("class C {\n  def f(int total) { return total }\n}\n");
        let diff = GroovyAdapter.diff_ast(&a, &b);
        assert!(diff.modified.is_empty());
        let c = parse("class C {\n  def f(String count) { return count }\n}\n");
        let diff = GroovyAdapter.diff_ast(&a, &c);
        assert!(!diff.modified.is_empty());
    }

    #[test]
    fn removed_method_is_breaking_private_is_not() {
        let old = parse("class C {\n  def a() { return 1 }\n  def b() { return 2 }\n}\n");
        let new = parse("class C {\n  def a() { return 1 }\n}\n");
        let diff = GroovyAdapter.diff_ast(&old, &new);
        assert!(GroovyAdapter.detect_breaking_changes(&diff));

        let old_p = parse("class C {\n  def a() { return 1 }\n  private def h() { return 2 }\n}\n");
        let new_p = parse("class C {\n  def a() { return 1 }\n}\n");
        let diff_p = GroovyAdapter.diff_ast(&old_p, &new_p);
        assert!(!GroovyAdapter.detect_breaking_changes(&diff_p));
    }

    #[test]
    fn detect_files_matches_groovy() {
        let paths = vec![
            PathBuf::from("Service.groovy"),
            PathBuf::from("build.gradle"),
            PathBuf::from("Main.java"),
        ];
        let got = GroovyAdapter.detect_files(&paths);
        assert_eq!(got.len(), 1);
    }
}
