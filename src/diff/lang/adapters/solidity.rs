//! Solidity adapter: contract-level declarations as public API surface.
//!
//! Structured line scanner (T2 depth). A Solidity contract's API is its ABI:
//! `public`/`external` functions, `public` state variables (which generate
//! getters), `event`s and `error`s, the special entry points (`constructor`,
//! `fallback`, `receive`), plus the `struct`/`enum`/`modifier` declarations
//! derived contracts inherit and the `contract`/`interface`/`library`
//! declarations themselves. `internal`/`private` functions and state variables
//! are not surface. File-level (free) functions carry no visibility keyword and
//! are importable, so a function header without any visibility keyword is also
//! treated as surface (modern Solidity requires visibility on contract
//! functions, so only free functions and pre-0.5 code reach that branch).
//!
//! Function/event/error/constructor signatures are recorded as canonical
//! parameter types (`(address,uint256)`) — the same tuple that feeds the
//! Solidity selector/topic0 — so parameter-*name* changes do not register as
//! API changes but parameter-*type* changes do. Headers may span multiple
//! lines; the scanner accumulates until the header terminator (`{` or `;` at
//! paren depth 0). Solidity has an explicit visibility model and this adapter
//! honors it, so it sits in the 0.8 confidence band. Born-correct comment
//! handling per rev-24/26: `//` line comments (including NatSpec `///`) and
//! `/* ... */` block comments.

use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct SolidityAdapter;

impl LanguageAdapter for SolidityAdapter {
    fn name(&self) -> &'static str {
        "Solidity"
    }

    fn language(&self) -> Language {
        Language("solidity")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "sol")
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        sol_parse(file)
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

/// Leading identifier of a declaration token (`Transfer(address` -> `Transfer`).
/// Returns `None` when the token does not start with an identifier — e.g. the
/// `(` of a function-*type* declaration `function (uint256) external ...`.
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

/// Header text split into words on every non-identifier character, so
/// visibility keywords are matched whole even when glued to punctuation
/// (`private;`, `external)`).
fn header_words(header: &str) -> Vec<&str> {
    header
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .collect()
}

/// Byte offset of the first `{` or `;` at paren depth 0 — the terminator of a
/// declaration header. Bodies and parameter lists never contain one at depth 0.
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

/// Split a parameter list on commas at bracket depth 0 (tuple parameters keep
/// their inner commas).
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

/// Canonical parameter-type tuple in selector form: data-location and mutability
/// keywords (`memory`/`calldata`/`storage`/`indexed`/`payable`) and the trailing
/// parameter name are dropped, so `address indexed to` canonicalizes to
/// `address`. Parameter-name changes leave the signature untouched; type
/// changes alter it.
fn canonical_params(params: &str) -> String {
    let types: Vec<String> = split_top_level(params)
        .iter()
        .map(|p| {
            let toks: Vec<&str> = p
                .split_whitespace()
                .filter(|t| {
                    !matches!(
                        *t,
                        "memory" | "calldata" | "storage" | "indexed" | "payable"
                    )
                })
                .collect();
            // A lone token is an unnamed parameter (allowed in interfaces) —
            // it is already the type.
            if toks.len() > 1 {
                toks[..toks.len() - 1].join(" ")
            } else {
                toks.concat()
            }
        })
        .filter(|t| !t.is_empty())
        .collect();
    format!("({})", types.join(","))
}

/// Analyze a complete declaration header (text before the `{`/`;` terminator)
/// and push any surface symbol + optional selector-form signature.
fn analyze_header(
    header: &str,
    symbols: &mut Vec<Symbol>,
    signatures: &mut HashMap<String, String>,
) {
    let tokens: Vec<&str> = header.split_whitespace().collect();
    let Some(&kw_raw) = tokens.first() else {
        return;
    };
    // Keywords may be glued to a paren: `constructor(uint256 x)`.
    let kw: String = kw_raw
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    match kw.as_str() {
        "event" | "error" => {
            if let Some(name) = decl_name(tokens.get(1)) {
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: kw.to_string(),
                });
                if let Some(params) = extract_params(header) {
                    signatures.insert(name, canonical_params(params));
                }
            }
        }
        "modifier" => {
            if let Some(name) = decl_name(tokens.get(1)) {
                symbols.push(Symbol {
                    name,
                    kind: "modifier".into(),
                });
            }
        }
        "constructor" => {
            symbols.push(Symbol {
                name: "constructor".into(),
                kind: "constructor".into(),
            });
            if let Some(params) = extract_params(header) {
                signatures.insert("constructor".into(), canonical_params(params));
            }
        }
        "fallback" | "receive" => {
            // Spec requires `external`; they are unnamed ABI entry points.
            if header_words(header).contains(&"external") {
                symbols.push(Symbol {
                    name: kw,
                    kind: "function".into(),
                });
            }
        }
        "function" => {
            let Some(name) = decl_name(tokens.get(1)) else {
                return;
            };
            let words = header_words(header);
            // `internal`/`private` functions are not surface. Absent visibility
            // means a file-level free function (importable) or pre-0.5 code —
            // modern Solidity requires visibility on contract functions.
            if words.iter().any(|w| *w == "internal" || *w == "private") {
                return;
            }
            symbols.push(Symbol {
                name: name.clone(),
                kind: "function".into(),
            });
            if let Some(params) = extract_params(header) {
                signatures.insert(name, canonical_params(params));
            }
        }
        _ => {}
    }
}

fn sol_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut signatures = HashMap::new();
    let mut in_block_comment = false;
    // A function/event/error/modifier header spanning multiple lines,
    // accumulated until its `{`/`;` terminator at paren depth 0.
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
                analyze_header(&header, &mut symbols, &mut signatures);
            }
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let first = tokens.first().copied().unwrap_or("");
        // Keywords may be glued to a paren: `constructor(uint256 x) {`.
        let first_ident: String = first
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        match first_ident.as_str() {
            "pragma" | "import" | "using" => {}
            "abstract" => {
                if tokens.get(1) == Some(&"contract") {
                    if let Some(name) = decl_name(tokens.get(2)) {
                        symbols.push(Symbol {
                            name,
                            kind: "contract".into(),
                        });
                    }
                }
            }
            "contract" | "interface" | "library" | "struct" | "enum" | "type" => {
                if let Some(name) = decl_name(tokens.get(1)) {
                    symbols.push(Symbol {
                        name,
                        kind: first_ident.clone(),
                    });
                }
            }
            "function" | "event" | "error" | "modifier" | "constructor" | "fallback"
            | "receive" => {
                if let Some(end) = header_end(trimmed) {
                    analyze_header(&trimmed[..end], &mut symbols, &mut signatures);
                } else {
                    pending = Some(trimmed.to_string());
                }
            }
            _ => {
                // `<type> public [constant|immutable] <name> ...;` — public state
                // variables generate getters, so they are ABI surface. `public`
                // cannot appear on locals, so no brace-depth tracking is needed.
                // Function-like lines were consumed by the branches above.
                if trimmed.contains(';') {
                    if let Some(pos) = tokens.iter().position(|w| *w == "public") {
                        let mut i = pos + 1;
                        while matches!(tokens.get(i), Some(&"constant") | Some(&"immutable")) {
                            i += 1;
                        }
                        if let Some(name_tok) = tokens.get(i) {
                            let name: String = name_tok
                                .chars()
                                .take_while(|c| c.is_alphanumeric() || *c == '_')
                                .collect();
                            if is_ident(&name) {
                                symbols.push(Symbol {
                                    name,
                                    kind: "variable".into(),
                                });
                            }
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
        SolidityAdapter.parse_ast(f.path()).unwrap()
    }

    fn names(ast: &AstRepresentation) -> Vec<(String, String)> {
        ast.symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind.clone()))
            .collect()
    }

    #[test]
    fn detects_contract_kinds() {
        let ast = parse(
            "contract Token is ERC20 {\n}\n\
             abstract contract Base {\n}\n\
             interface IERC20 {\n}\n\
             library Math {\n}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Token".into(), "contract".into())));
        assert!(got.contains(&("Base".into(), "contract".into())));
        assert!(got.contains(&("IERC20".into(), "interface".into())));
        assert!(got.contains(&("Math".into(), "library".into())));
    }

    #[test]
    fn public_surface_internal_private_skipped() {
        let ast = parse(
            "contract C {\n\
             \x20   uint256 public count;\n\
             \x20   uint256 private secret;\n\
             \x20   uint256 internal mid;\n\
             \x20   uint256 defaulted;\n\
             \x20   function open() external {}\n\
             \x20   function alsoOpen() public {}\n\
             \x20   function hidden() internal {}\n\
             \x20   function buried() private {}\n\
             }\n\
             function freeFn(uint256 x) pure returns (uint256) { return x; }\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("count".into(), "variable".into())));
        assert!(got.contains(&("open".into(), "function".into())));
        assert!(got.contains(&("alsoOpen".into(), "function".into())));
        assert!(got.contains(&("freeFn".into(), "function".into())));
        assert!(!got.iter().any(|(n, _)| n == "secret"));
        assert!(!got.iter().any(|(n, _)| n == "mid"));
        assert!(!got.iter().any(|(n, _)| n == "defaulted"));
        assert!(!got.iter().any(|(n, _)| n == "hidden"));
        assert!(!got.iter().any(|(n, _)| n == "buried"));
    }

    #[test]
    fn events_errors_modifiers_structs_enums() {
        let ast = parse(
            "contract C {\n\
             \x20   event Transfer(address indexed from, address indexed to, uint256 value);\n\
             \x20   error InsufficientBalance(address account, uint256 available);\n\
             \x20   modifier onlyOwner() { _; }\n\
             \x20   struct Point { uint256 x; uint256 y; }\n\
             \x20   enum State { Open, Closed }\n\
             }\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("Transfer".into(), "event".into())));
        assert!(got.contains(&("InsufficientBalance".into(), "error".into())));
        assert!(got.contains(&("onlyOwner".into(), "modifier".into())));
        assert!(got.contains(&("Point".into(), "struct".into())));
        assert!(got.contains(&("State".into(), "enum".into())));
    }

    #[test]
    fn special_functions_are_entry_points() {
        let ast = parse(
            "contract C {\n\
             \x20   constructor(uint256 initial) {\n\
             \x20   }\n\
             \x20   fallback() external payable {}\n\
             \x20   receive() external payable {}\n\
             }\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("constructor".into(), "constructor".into())));
        assert!(got.contains(&("fallback".into(), "function".into())));
        assert!(got.contains(&("receive".into(), "function".into())));
        assert_eq!(
            ast.signatures.get("constructor").map(String::as_str),
            Some("(uint256)")
        );
    }

    #[test]
    fn signatures_are_selector_param_types() {
        let ast = parse(
            "contract C {\n\
             \x20   function transfer(address to, uint256 amount) external returns (bool) {\n\
             \x20       return true;\n\
             \x20   }\n\
             \x20   function multi(\n\
             \x20       address to,\n\
             \x20       uint256[] memory amounts\n\
             \x20   ) external {}\n\
             \x20   event Moved(address indexed from, uint256 value);\n\
             }\n",
        );
        assert_eq!(
            ast.signatures.get("transfer").map(String::as_str),
            Some("(address,uint256)")
        );
        assert_eq!(
            ast.signatures.get("multi").map(String::as_str),
            Some("(address,uint256[])")
        );
        assert_eq!(
            ast.signatures.get("Moved").map(String::as_str),
            Some("(address,uint256)")
        );
    }

    #[test]
    fn param_rename_keeps_signature() {
        let a = parse("contract C {\n  function f(address to, uint256 amount) external {}\n}\n");
        let b =
            parse("contract C {\n  function f(address recipient, uint256 amount) external {}\n}\n");
        let diff = SolidityAdapter.diff_ast(&a, &b);
        assert!(diff.modified.is_empty());
        let c = parse("contract C {\n  function f(address to, uint128 amount) external {}\n}\n");
        let diff = SolidityAdapter.diff_ast(&a, &c);
        assert!(!diff.modified.is_empty());
    }

    #[test]
    fn skips_comments() {
        let ast = parse(
            "// contract FakeOne {\n\
             //     function hacked() external {}\n\
             // }\n\
             /*\ninterface IFake {\n  function stolen() external;\n}\n*/\n\
             contract Real {\n\
             \x20   /// NatSpec: function documented() external {}\n\
             \x20   function genuine() external view returns (uint256) {\n\
             \x20       return 1;\n\
             \x20   }\n\
             }\n",
        );
        assert_eq!(
            names(&ast),
            vec![
                ("Real".into(), "contract".into()),
                ("genuine".into(), "function".into()),
            ]
        );
    }

    #[test]
    fn removed_public_is_breaking_internal_is_not() {
        let old =
            parse("contract C {\n  function a() external {}\n  function b() external {}\n}\n");
        let new = parse("contract C {\n  function a() external {}\n}\n");
        let diff = SolidityAdapter.diff_ast(&old, &new);
        assert!(SolidityAdapter.detect_breaking_changes(&diff));

        let old_i =
            parse("contract C {\n  function a() external {}\n  function h() internal {}\n}\n");
        let new_i = parse("contract C {\n  function a() external {}\n}\n");
        let diff_i = SolidityAdapter.diff_ast(&old_i, &new_i);
        assert!(!SolidityAdapter.detect_breaking_changes(&diff_i));
    }

    #[test]
    fn detect_files_matches_sol() {
        let paths = vec![
            PathBuf::from("Token.sol"),
            PathBuf::from("main.tf"),
            PathBuf::from("notes.md"),
        ];
        let got = SolidityAdapter.detect_files(&paths);
        assert_eq!(got.len(), 1);
    }
}
