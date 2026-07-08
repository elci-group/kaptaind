use super::super::adapter::{AstRepresentation, Symbol};
use std::collections::hash_map::DefaultHasher;
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

// Basic diffing based on names and kinds
pub fn basic_diff(
    old: &AstRepresentation,
    new: &AstRepresentation,
) -> super::super::adapter::AstDiff {
    use super::super::adapter::AstDiff;
    let old_names: std::collections::HashSet<_> = old.symbols.iter().map(|s| &s.name).collect();
    let new_names: std::collections::HashSet<_> = new.symbols.iter().map(|s| &s.name).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let modified = Vec::new();

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

    AstDiff {
        added,
        removed,
        modified,
    }
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

pub fn ts_parse(file: &Path, ver: (u32, u32)) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    if let Ok(lines) = read_lines_safe(file) {
        for line in lines {
            let trimmed = line.trim();
            // TS 3.8+: `import type` / `export type` — separate type-only re-exports
            if ver >= (3, 8) {
                if let Some(rest) = trimmed.strip_prefix("export type ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "type_export".to_string(),
                    });
                }
            }
            if let Some(rest) = trimmed.strip_prefix("export ") {
                let kind = classify_ts_export(rest);
                symbols.push(Symbol {
                    name: rest.to_string(),
                    kind,
                });
            }
            // React hooks
            if (trimmed.starts_with("export function use")
                || trimmed.starts_with("export const use"))
                && !trimmed.contains("// ")
            {
                symbols.push(Symbol {
                    name: trimmed.to_string(),
                    kind: "hook".to_string(),
                });
            }
            // Next.js route exports
            for marker in [
                "generateMetadata",
                "generateStaticParams",
                "getServerSideProps",
                "getStaticProps",
                "getStaticPaths",
            ] {
                if trimmed.contains(marker) && trimmed.contains("export") {
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
                        name: rest.to_string(),
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
        ..Default::default()
    })
}
