//! Objective-C adapter: `@interface`/`@protocol`/`@implementation`, methods
//! (selector identity), properties, and `NS_ENUM`/`NS_OPTIONS` as public API
//! surface.
//!
//! Structured line scanner (T2 depth). An Objective-C API is its runtime
//! surface: class and protocol declarations, and methods identified by their
//! *selector* — the full keyword name with colons (`greet:withPunctuation:`),
//! which is the stable identity used by message dispatch and `@selector`.
//! The selector is emitted as the symbol name directly, so renaming any
//! keyword segment registers as a removal/addition (breaking), matching ObjC
//! semantics. Objective-C has no method visibility — the header/implementation
//! split is a convention the line scanner cannot reconstruct (`.h` is owned by
//! the C adapter; this adapter claims `.m`/`.mm`) — so it applies the Apple
//! underscore-prefix internal convention and sits in the no-visibility 0.7
//! confidence band. Properties (`@property ... Type *name;`) and Apple's
//! `NS_ENUM`/`NS_OPTIONS` type macros round out the surface. Method headers
//! may span lines (one keyword segment per line); the scanner accumulates to
//! the `;`/`{` terminator at paren depth 0. Parameter types are not part of
//! ObjC dispatch, so no signatures are recorded. Born-correct comment handling
//! per rev-24/26: `//` line comments and `/* ... */` block comments.

use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct ObjCAdapter;

impl LanguageAdapter for ObjCAdapter {
    fn name(&self) -> &'static str {
        "Objective-C"
    }

    fn language(&self) -> Language {
        Language("objc")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "m" || e == "mm")
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        objc_parse(file)
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

/// Push a surface symbol unless the name is underscore-internal (Apple
/// reserves the prefix for private API). Selectors contain interior colons
/// (`setName:age:`); they are stripped for the identifier check.
fn emit(symbols: &mut Vec<Symbol>, name: String, kind: &str) {
    let ident_check = name.replace(':', "");
    if !name.starts_with('_') && is_ident(&ident_check) {
        symbols.push(Symbol {
            name,
            kind: kind.into(),
        });
    }
}

/// Byte offset of the first `;` or `{` at paren depth 0 — the terminator of a
/// declaration header.
fn header_end(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' | '{' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Byte range of the first top-level paren group (the return type of a method
/// declaration).
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

/// Build the selector from the text after a method's return-type group.
/// Keyword segments (`setName:(NSString *)name age:(NSInteger)age`) yield
/// `setName:age:`; a no-argument method yields its bare identifier. Parameter
/// types and names are dispatch-invisible and dropped. A keyword is an
/// identifier immediately followed by `:(` — the colon is fused to the
/// parameter-type paren with no intervening space, so whitespace tokenization
/// cannot see it and the scan runs at character level.
fn method_selector(rest: &str) -> Option<String> {
    let chars: Vec<char> = rest.chars().collect();
    let mut selector = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if chars.get(i) == Some(&':') && chars.get(i + 1) == Some(&'(') {
                selector.push_str(&word);
                selector.push(':');
                i += 1;
            } else if selector.is_empty() {
                // No-argument form: the first bare identifier is the selector.
                return Some(word);
            }
        } else {
            i += 1;
        }
    }
    if selector.is_empty() {
        None
    } else {
        Some(selector)
    }
}

/// `- (ReturnType)selector...` / `+ (ReturnType)selector...` header.
fn analyze_method(header: &str) -> Option<String> {
    let (_, close) = first_paren_group(header)?;
    method_selector(&header[close + 1..])
}

/// `@property (attrs) Type *name` — the name is the last identifier token,
/// star prefixes and the terminator stripped.
fn property_name(header: &str) -> Option<String> {
    let rest = header.strip_prefix("@property")?;
    let mut name = None;
    for tok in rest.split_whitespace() {
        let clean: String = tok
            .trim_start_matches('*')
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if is_ident(&clean) {
            name = Some(clean);
        }
    }
    name
}

/// `typedef NS_ENUM(NSInteger, Name)` / `NS_OPTIONS(...)` — the name is the
/// second macro argument.
fn ns_enum_name(header: &str) -> Option<String> {
    let idx = header
        .find("NS_ENUM(")
        .or_else(|| header.find("NS_OPTIONS("))?;
    let (open, close) = first_paren_group(&header[idx..])?;
    let inner = &header[idx + open + 1..idx + close];
    let second = inner.split(',').nth(1)?.trim();
    let name: String = second
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if is_ident(&name) {
        Some(name)
    } else {
        None
    }
}

/// Analyze a complete declaration header (text before the `;`/`{` terminator).
fn analyze_header(header: &str, symbols: &mut Vec<Symbol>) {
    if let Some(name) = ns_enum_name(header) {
        emit(symbols, name, "enum");
        return;
    }
    if header.starts_with("- (") || header.starts_with("+ (") {
        if let Some(selector) = analyze_method(header) {
            emit(symbols, selector, "method");
        }
        return;
    }
    if header.starts_with("@property") {
        if let Some(name) = property_name(header) {
            emit(symbols, name, "property");
        }
    }
}

fn objc_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut in_block_comment = false;
    // A declaration header spanning multiple lines (one keyword segment per
    // line is idiomatic ObjC formatting), accumulated until its `;`/`{`
    // terminator at paren depth 0.
    let mut pending: Option<String> = None;

    for line in read_lines_safe(file)? {
        let trimmed = line.trim();
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

        if let Some(buf) = pending.as_mut() {
            buf.push(' ');
            buf.push_str(trimmed);
            if let Some(end) = header_end(buf) {
                let header = buf[..end].to_string();
                pending = None;
                analyze_header(&header, &mut symbols);
            }
            continue;
        }

        if trimmed.starts_with("@interface")
            || trimmed.starts_with("@implementation")
            || trimmed.starts_with("@protocol")
        {
            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            let kind = match tokens.first().copied().unwrap_or("") {
                "@interface" => "class",
                "@implementation" => "class",
                _ => "protocol",
            };
            if let Some(tok) = tokens.get(1) {
                // `Name : Super`, `Name (Category)`, or bare `Name`.
                let name: String = tok
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                emit(&mut symbols, name, kind);
            }
            continue;
        }

        let starts_decl = trimmed.starts_with("- (")
            || trimmed.starts_with("+ (")
            || trimmed.starts_with("@property")
            || trimmed.contains("NS_ENUM(")
            || trimmed.contains("NS_OPTIONS(");
        if starts_decl {
            if let Some(end) = header_end(trimmed) {
                analyze_header(&trimmed[..end], &mut symbols);
            } else {
                pending = Some(trimmed.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn parse(body: &str) -> AstRepresentation {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        ObjCAdapter.parse_ast(f.path()).unwrap()
    }

    fn names(ast: &AstRepresentation) -> Vec<(String, String)> {
        ast.symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind.clone()))
            .collect()
    }

    #[test]
    fn detects_interface_protocol_implementation() {
        let ast = parse(
            "@interface Greeter : NSObject\n@end\n\
             @protocol GreeterDelegate\n@end\n\
             @implementation Greeter\n@end\n\
             @interface Greeter (Private)\n@end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Greeter".into(), "class".into())));
        assert!(got.contains(&("GreeterDelegate".into(), "protocol".into())));
        // @interface + @implementation + category all emit the class kind.
        assert_eq!(
            got.iter()
                .filter(|(n, k)| n == "Greeter" && k == "class")
                .count(),
            3
        );
    }

    #[test]
    fn methods_use_full_selectors() {
        let ast = parse(
            "@interface C : NSObject\n\
             - (void)doSomething;\n\
             + (instancetype)sharedInstance;\n\
             - (void)setName:(NSString *)name age:(NSInteger)age;\n\
             @end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("doSomething".into(), "method".into())));
        assert!(got.contains(&("sharedInstance".into(), "method".into())));
        assert!(got.contains(&("setName:age:".into(), "method".into())));
    }

    #[test]
    fn multiline_method_headers() {
        let ast = parse(
            "@interface C : NSObject\n\
             - (void)setName:(NSString *)name\n\
             \x20           age:(NSInteger)age\n\
             \x20           active:(BOOL)active;\n\
             @end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("setName:age:active:".into(), "method".into())));
    }

    #[test]
    fn properties_detected() {
        let ast = parse(
            "@interface C : NSObject\n\
             @property (nonatomic, copy) NSString *title;\n\
             @property (nonatomic, assign) NSInteger count;\n\
             @property (nonatomic, strong) NSMutableArray<NSString *> *items;\n\
             @end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("title".into(), "property".into())));
        assert!(got.contains(&("count".into(), "property".into())));
        assert!(got.contains(&("items".into(), "property".into())));
    }

    #[test]
    fn ns_enum_and_ns_options() {
        let ast = parse(
            "typedef NS_ENUM(NSInteger, Status) {\n  StatusOff,\n  StatusOn\n};\n\
             typedef NS_OPTIONS(NSUInteger, Permissions) {\n  PermRead = 1\n};\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Status".into(), "enum".into())));
        assert!(got.contains(&("Permissions".into(), "enum".into())));
    }

    #[test]
    fn underscore_names_are_internal() {
        let ast = parse(
            "@interface C : NSObject\n\
             - (void)_privateHelper;\n\
             @property (nonatomic, strong) NSString *_secret;\n\
             - (void)publicMethod;\n\
             @end\n",
        );
        let got = names(&ast);
        assert!(!got.iter().any(|(n, _)| n.contains("_privateHelper")));
        assert!(!got.iter().any(|(n, _)| n.contains("_secret")));
        assert!(got.contains(&("publicMethod".into(), "method".into())));
    }

    #[test]
    fn skips_comments() {
        let ast = parse(
            "// @interface FakeOne : NSObject\n\
             // - (void)hacked;\n\
             /*\n@interface FakeTwo : NSObject\n- (void)stolen;\n@end\n*/\n\
             @interface Real : NSObject\n\
             - (void)genuine;\n\
             @end\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Real".into(), "class".into())));
        assert!(got.contains(&("genuine".into(), "method".into())));
        assert!(!got.iter().any(|(n, _)| n.contains("Fake")));
        assert!(!got.iter().any(|(n, _)| n.contains("hacked")));
        assert!(!got.iter().any(|(n, _)| n.contains("stolen")));
    }

    #[test]
    fn removed_method_is_breaking_underscore_is_not() {
        let old = parse("@interface C : NSObject\n- (void)a;\n- (void)b;\n@end\n");
        let new = parse("@interface C : NSObject\n- (void)a;\n@end\n");
        let diff = ObjCAdapter.diff_ast(&old, &new);
        assert!(ObjCAdapter.detect_breaking_changes(&diff));

        let old_u = parse("@interface C : NSObject\n- (void)a;\n- (void)_h;\n@end\n");
        let new_u = parse("@interface C : NSObject\n- (void)a;\n@end\n");
        let diff_u = ObjCAdapter.diff_ast(&old_u, &new_u);
        assert!(!ObjCAdapter.detect_breaking_changes(&diff_u));
    }

    #[test]
    fn detect_files_matches_m_and_mm() {
        let paths = vec![
            PathBuf::from("Greeter.m"),
            PathBuf::from("Greeter.mm"),
            PathBuf::from("Greeter.h"),
            PathBuf::from("notes.md"),
        ];
        let got = ObjCAdapter.detect_files(&paths);
        assert_eq!(got.len(), 2);
    }
}
