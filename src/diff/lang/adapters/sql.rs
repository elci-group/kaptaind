//! SQL adapter: database schema objects as public API surface.
//!
//! Structured line scanner (T2 depth). `CREATE <object> <name>` and
//! `DROP <object> <name>` statements are API surface; `SELECT`/`INSERT`/
//! `UPDATE`/`DELETE` data statements are not. SQL has no visibility model —
//! every schema object is public by definition — so the adapter sits in the
//! no-visibility confidence band (0.7 in `normalize()`). Handles `--` line
//! comments, `/* ... */` block comments (born-correct per rev-24/26),
//! `CREATE OR REPLACE` / `UNIQUE` / `TEMP[ORARY]` / `MATERIALIZED` modifiers,
//! `IF [NOT] EXISTS` clauses, quoted/backticked/bracketed and schema-qualified
//! identifiers, and case-insensitive keywords.

use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct SqlAdapter;

impl LanguageAdapter for SqlAdapter {
    fn name(&self) -> &'static str {
        "SQL"
    }

    fn language(&self) -> Language {
        Language("sql")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("sql"))
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        sql_parse(file)
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

fn object_kind(keyword: &str) -> Option<&'static str> {
    match keyword {
        "TABLE" => Some("table"),
        "VIEW" => Some("view"),
        "FUNCTION" => Some("function"),
        "PROCEDURE" => Some("procedure"),
        "INDEX" => Some("index"),
        "SEQUENCE" => Some("sequence"),
        "TYPE" => Some("type"),
        "TRIGGER" => Some("trigger"),
        "SCHEMA" => Some("schema"),
        "DATABASE" => Some("database"),
        _ => None,
    }
}

fn drop_kind(keyword: &str) -> Option<&'static str> {
    match keyword {
        "TABLE" => Some("drop_table"),
        "VIEW" => Some("drop_view"),
        "FUNCTION" => Some("drop_function"),
        "PROCEDURE" => Some("drop_procedure"),
        "INDEX" => Some("drop_index"),
        "SEQUENCE" => Some("drop_sequence"),
        "TYPE" => Some("drop_type"),
        "TRIGGER" => Some("drop_trigger"),
        "SCHEMA" => Some("drop_schema"),
        "DATABASE" => Some("drop_database"),
        _ => None,
    }
}

/// Reduce a raw token to a stable identifier: cut at the first argument list or
/// statement terminator, then strip identifier quoting (`"`, backtick, `[ ]`).
fn clean_ident(token: &str) -> String {
    let cut = token
        .find(['(', ';', ','])
        .map(|i| &token[..i])
        .unwrap_or(token);
    cut.trim_matches(['"', '`', '[', ']']).to_string()
}

fn sql_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut in_block_comment = false;
    for line in read_lines_safe(file)? {
        let trimmed = line.trim();
        // Track `/* ... */` regions so commented-out DDL is not mistaken for
        // live schema (measured messy-corpus discipline, rev-24/26).
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
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let upper: Vec<String> = tokens.iter().map(|t| t.to_uppercase()).collect();
        let first = upper.first().map(String::as_str).unwrap_or("");

        // (object kind, index of the name token)
        let parsed: Option<(&'static str, usize)> = match first {
            "CREATE" => {
                let mut i = 1;
                // OR REPLACE / UNIQUE / TEMP[ORARY] / MATERIALIZED modifiers.
                while i < upper.len()
                    && matches!(
                        upper[i].as_str(),
                        "OR" | "REPLACE" | "UNIQUE" | "TEMP" | "TEMPORARY" | "MATERIALIZED"
                    )
                {
                    i += 1;
                }
                match upper.get(i).and_then(|k| object_kind(k)) {
                    Some(kind) => {
                        // Optional IF NOT EXISTS between object and name.
                        let mut n = i + 1;
                        if upper.get(n).map(String::as_str) == Some("IF")
                            && upper.get(n + 1).map(String::as_str) == Some("NOT")
                            && upper.get(n + 2).map(String::as_str) == Some("EXISTS")
                        {
                            n += 3;
                        }
                        Some((kind, n))
                    }
                    None => None,
                }
            }
            "DROP" => match upper.get(1).and_then(|k| drop_kind(k)) {
                Some(kind) => {
                    // Optional IF EXISTS between object and name.
                    let mut n = 2;
                    if upper.get(n).map(String::as_str) == Some("IF")
                        && upper.get(n + 1).map(String::as_str) == Some("EXISTS")
                    {
                        n += 2;
                    }
                    Some((kind, n))
                }
                None => None,
            },
            _ => None,
        };

        if let Some((kind, name_idx)) = parsed {
            if let Some(token) = tokens.get(name_idx) {
                let name = clean_ident(token);
                if !name.is_empty() {
                    symbols.push(Symbol {
                        name,
                        kind: kind.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn parse(body: &str) -> AstRepresentation {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        SqlAdapter.parse_ast(f.path()).unwrap()
    }

    fn names(ast: &AstRepresentation) -> Vec<(String, String)> {
        ast.symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind.clone()))
            .collect()
    }

    #[test]
    fn detects_schema_objects() {
        let ast = parse(
            "CREATE TABLE users (\n    id SERIAL PRIMARY KEY\n);\n\
             CREATE OR REPLACE VIEW active_users AS SELECT 1;\n\
             CREATE UNIQUE INDEX idx_users_email ON users (email);\n\
             CREATE SEQUENCE order_seq;\n\
             CREATE PROCEDURE refresh() LANGUAGE plpgsql AS $$ BEGIN END; $$;\n\
             CREATE SCHEMA analytics;\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("users".into(), "table".into())));
        assert!(got.contains(&("active_users".into(), "view".into())));
        assert!(got.contains(&("idx_users_email".into(), "index".into())));
        assert!(got.contains(&("order_seq".into(), "sequence".into())));
        assert!(got.contains(&("refresh".into(), "procedure".into())));
        assert!(got.contains(&("analytics".into(), "schema".into())));
    }

    #[test]
    fn detects_drop_statements_as_distinct_kind() {
        let ast = parse("DROP TABLE IF EXISTS legacy_sessions;\nDROP VIEW old_v;\n");
        let got = names(&ast);
        assert!(got.contains(&("legacy_sessions".into(), "drop_table".into())));
        assert!(got.contains(&("old_v".into(), "drop_view".into())));
    }

    #[test]
    fn skips_comments_and_dml() {
        let ast = parse(
            "-- CREATE TABLE fake (id int);\n\
             /*\nCREATE TABLE also_fake (id int);\n*/\n\
             SELECT * FROM users;\n\
             INSERT INTO users VALUES (1);\n",
        );
        assert!(ast.symbols.is_empty());
    }

    #[test]
    fn keywords_are_case_insensitive() {
        let ast = parse("create table lower_case (id int);\n");
        assert_eq!(names(&ast), vec![("lower_case".into(), "table".into())]);
    }

    #[test]
    fn handles_if_not_exists_and_quoted_names() {
        let ast = parse(
            "CREATE TABLE IF NOT EXISTS sessions (id int);\n\
             CREATE TABLE \"Order\" (id int);\n\
             CREATE TABLE public.accounts (id int);\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("sessions".into(), "table".into())));
        assert!(got.contains(&("Order".into(), "table".into())));
        assert!(got.contains(&("public.accounts".into(), "table".into())));
    }

    #[test]
    fn removed_object_is_breaking() {
        let old = parse("CREATE TABLE a (id int);\nCREATE TABLE b (id int);\n");
        let new = parse("CREATE TABLE a (id int);\n");
        let diff = SqlAdapter.diff_ast(&old, &new);
        assert!(SqlAdapter.detect_breaking_changes(&diff));
        let same = SqlAdapter.diff_ast(&old, &old);
        assert!(!SqlAdapter.detect_breaking_changes(&same));
    }

    #[test]
    fn detect_files_matches_sql_extension() {
        let paths = vec![
            PathBuf::from("schema.sql"),
            PathBuf::from("SCHEMA.SQL"),
            PathBuf::from("query.sql.bak"),
            PathBuf::from("notes.md"),
        ];
        let got = SqlAdapter.detect_files(&paths);
        assert_eq!(got.len(), 2);
    }
}
