use super::super::adapter::{AstRepresentation, Symbol};
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::Path;

const MAX_PARSE_SIZE_BYTES: u64 = 5 * 1024 * 1024; // 5MB

pub fn read_lines_safe(file: &Path) -> anyhow::Result<impl Iterator<Item = String>> {
    let meta = std::fs::metadata(file)?;
    if meta.len() > MAX_PARSE_SIZE_BYTES {
        anyhow::bail!("File too large for AST parsing ({} bytes)", meta.len());
    }
    let f = File::open(file)?;
    let reader = BufReader::new(f);
    Ok(reader.lines().map_while(Result::ok))
}

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

// Basic diffing keyed by symbol name. `added`/`removed` are names present on only
// one side; `modified` holds symbols whose name is present on *both* sides but whose
// `kind` changed (e.g. `function` -> `class`, `method` -> `property`). Signature /
// return-type / arity changes are not yet representable because `Symbol` carries only
// `name` + `kind`; surfacing those needs a `Symbol.signature` extension (follow-up).
// `modified` is additive: downstream breaking logic keys off `removed`, so populating
// it does not change versioning decisions — it makes previously-invisible changes
// observable for calibration and future policy.
pub fn basic_diff(
    old: &AstRepresentation,
    new: &AstRepresentation,
) -> super::super::adapter::AstDiff {
    use super::super::adapter::AstDiff;
    let old_names: std::collections::HashSet<_> = old.symbols.iter().map(|s| &s.name).collect();
    let new_names: std::collections::HashSet<_> = new.symbols.iter().map(|s| &s.name).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();

    for s in &new.symbols {
        if !old_names.contains(&s.name) {
            added.push(s.clone());
        }
    }

    for s in &old.symbols {
        if !new_names.contains(&s.name) {
            removed.push(s.clone());
        }
    }

    let modified = modified_by_kind(old, new);

    AstDiff {
        added,
        removed,
        modified,
    }
}

/// Symbols present (by name) in both `old` and `new` whose `kind` differs OR whose recorded
/// `signature` differs (arity / return-type / parameter change). The post-change (`new`)
/// symbol is reported. Name->kind mapping is first-write-wins per side so duplicate names
/// resolve deterministically. A signature change counts only when BOTH sides recorded a
/// signature for that name; otherwise the kind signal alone decides (so adapters that leave
/// `signatures` empty are unaffected).
pub fn modified_by_kind(
    old: &AstRepresentation,
    new: &AstRepresentation,
) -> Vec<super::super::adapter::Symbol> {
    let mut old_kinds: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for s in &old.symbols {
        old_kinds.entry(s.name.as_str()).or_insert(s.kind.as_str());
    }
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut modified = Vec::new();
    for s in &new.symbols {
        if !seen.insert(s.name.as_str()) {
            continue;
        }
        if let Some(&old_kind) = old_kinds.get(s.name.as_str()) {
            let kind_changed = old_kind != s.kind.as_str();
            let sig_changed = match (old.signatures.get(&s.name), new.signatures.get(&s.name)) {
                (Some(os), Some(ns)) => os != ns,
                _ => false,
            };
            if kind_changed || sig_changed {
                modified.push(s.clone());
            }
        }
    }
    modified
}

/// Classify a TypeScript/JavaScript export line into a more specific kind.
pub fn classify_ts_export(rest: &str) -> String {
    if rest.starts_with("default function ")
        || rest.starts_with("default class ")
        || rest == "default"
        || rest.starts_with("default ")
    {
        "default_export".to_string()
    } else if rest.starts_with("function ") || rest.starts_with("async function ") {
        "function".to_string()
    } else if rest.starts_with("class ") {
        "class".to_string()
    } else if rest.starts_with("interface ") {
        "interface".to_string()
    } else if rest.starts_with("type ") {
        "type".to_string()
    } else if rest.starts_with("const ") || rest.starts_with("let ") || rest.starts_with("var ") {
        "binding".to_string()
    } else if rest.starts_with("enum ") {
        "enum".to_string()
    } else {
        "export".to_string()
    }
}

/// Extract the stable declared identifier from an `export <rest>` line remainder so the
/// symbol `name` does not embed the kind-bearing keyword (which otherwise makes
/// same-name/different-kind `modified` detection unreachable). Returns `"default"` for an
/// anonymous `export default` with no identifier. Shared by `ts_parse` (TypeScript) and the
/// JavaScript adapter.
pub fn export_name(rest: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "default",
        "async",
        "function",
        "class",
        "interface",
        "type",
        "const",
        "let",
        "var",
        "enum",
        "export",
    ];
    for token in rest.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')) {
        if token.is_empty() || KEYWORDS.contains(&token) {
            continue;
        }
        if let Some(first) = token.chars().next() {
            if first.is_alphabetic() || first == '_' || first == '$' {
                return token.to_string();
            }
        }
    }
    "default".to_string()
}

pub fn ts_parse(file: &Path, ver: (u32, u32)) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut signatures = HashMap::new();
    if let Ok(lines) = read_lines_safe(file) {
        for line in lines {
            let trimmed = line.trim();
            // TS 3.8+: `import type` / `export type` — separate type-only re-exports
            if ver >= (3, 8) {
                if let Some(rest) = trimmed.strip_prefix("export type ") {
                    symbols.push(Symbol {
                        name: export_name(rest),
                        kind: "type_export".to_string(),
                    });
                }
            }
            if let Some(rest) = trimmed.strip_prefix("export ") {
                let kind = classify_ts_export(rest);
                let name = export_name(rest);
                signatures.insert(name.clone(), rest.to_string());
                symbols.push(Symbol { name, kind });
            }
            // React hooks
            if (trimmed.starts_with("export function use")
                || trimmed.starts_with("export const use"))
                && !trimmed.contains("// ")
            {
                symbols.push(Symbol {
                    name: export_name(trimmed.strip_prefix("export ").unwrap_or(trimmed)),
                    kind: "hook".to_string(),
                });
            }
            // Next.js route exports. Comment lines are excluded: the substring
            // match would otherwise leak markers out of `//` comments (measured
            // messy-corpus FP, rev 24).
            for marker in [
                "generateMetadata",
                "generateStaticParams",
                "getServerSideProps",
                "getStaticProps",
                "getStaticPaths",
            ] {
                if trimmed.contains(marker)
                    && trimmed.contains("export")
                    && !trimmed.starts_with("//")
                {
                    symbols.push(Symbol {
                        name: marker.to_string(),
                        kind: "route_export".to_string(),
                    });
                }
            }
            // Middleware
            if trimmed.starts_with("export function middleware")
                || trimmed.starts_with("export const middleware")
            {
                symbols.push(Symbol {
                    name: "middleware".to_string(),
                    kind: "middleware".to_string(),
                });
            }
            // TS 5.0+: const type parameters on type aliases
            if ver >= (5, 0) && trimmed.starts_with("type ") && trimmed.contains("=") {
                if let Some(rest) = trimmed.strip_prefix("type ") {
                    symbols.push(Symbol {
                        name: export_name(rest),
                        kind: "type_alias".to_string(),
                    });
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
    use super::super::super::adapter::{AstRepresentation, Symbol};

    fn sym(name: &str, kind: &str) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: kind.to_string(),
        }
    }

    fn rep(symbols: Vec<Symbol>) -> AstRepresentation {
        AstRepresentation {
            symbols,
            ..Default::default()
        }
    }

    #[test]
    fn added_and_removed_keyed_by_name() {
        let old = rep(vec![sym("a", "function"), sym("b", "function")]);
        let new = rep(vec![sym("b", "function"), sym("c", "function")]);
        let diff = super::basic_diff(&old, &new);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].name, "c");
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].name, "a");
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn kind_change_is_modified_not_added_or_removed() {
        let old = rep(vec![sym("foo", "function")]);
        let new = rep(vec![sym("foo", "class")]);
        let diff = super::basic_diff(&old, &new);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert_eq!(diff.modified.len(), 1);
        assert_eq!(diff.modified[0].name, "foo");
        assert_eq!(diff.modified[0].kind, "class");
    }

    #[test]
    fn same_name_same_kind_is_not_modified() {
        let old = rep(vec![sym("foo", "function")]);
        let new = rep(vec![sym("foo", "function")]);
        let diff = super::basic_diff(&old, &new);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn modified_reports_new_symbol_kind() {
        let old = rep(vec![sym("x", "method")]);
        let new = rep(vec![sym("x", "property")]);
        let m = super::modified_by_kind(&old, &new);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].kind, "property");
    }

    #[test]
    fn modified_ignores_names_only_on_one_side() {
        let old = rep(vec![sym("gone", "function")]);
        let new = rep(vec![sym("fresh", "function")]);
        assert!(super::modified_by_kind(&old, &new).is_empty());
    }

    #[test]
    fn signature_change_is_modified_when_both_sides_record_signature() {
        let mut old = rep(vec![sym("connect", "function")]);
        let mut new = rep(vec![sym("connect", "function")]);
        old.signatures
            .insert("connect".to_string(), "function connect(host)".to_string());
        new.signatures.insert(
            "connect".to_string(),
            "function connect(host, port)".to_string(),
        );
        let diff = super::basic_diff(&old, &new);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert_eq!(diff.modified.len(), 1);
        assert_eq!(diff.modified[0].name, "connect");
    }

    #[test]
    fn signature_change_ignored_when_only_one_side_records() {
        let mut old = rep(vec![sym("connect", "function")]);
        let new = rep(vec![sym("connect", "function")]);
        old.signatures
            .insert("connect".to_string(), "function connect(host)".to_string());
        // `new` recorded no signature, so the kind signal alone (same kind) decides.
        let diff = super::basic_diff(&old, &new);
        assert!(diff.modified.is_empty());
    }
}
