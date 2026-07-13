//! Terraform/HCL adapter: labeled blocks as public API surface.
//!
//! Structured line scanner (T2 depth). A Terraform module's API is its labeled
//! blocks: `variable`/`output` (module input/output contract), `resource`/`data`
//! (managed infrastructure), `module`/`provider` (composition). Unlabeled blocks
//! (`terraform`, `locals`, `moved`, `import`, `check`, `removed`) are structural
//! and not surface. Resources and data sources emit their Terraform address
//! (`type.name`) — the stable identity used by `terraform state` and `moved`
//! blocks. HCL has no visibility model — every labeled block is public by
//! definition — so the adapter sits in the no-visibility confidence band (0.7).
//! Born-correct comment handling per rev-24/26: `#` and `//` line comments,
//! `/* ... */` block comments, and `<<TAG` heredocs (whose body may contain
//! block-shaped text).

use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct HclAdapter;

impl LanguageAdapter for HclAdapter {
    fn name(&self) -> &'static str {
        "HCL"
    }

    fn language(&self) -> Language {
        Language("hcl")
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                // `.tfvars` holds variable *values*, not declarations — excluded.
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "tf" || e == "hcl")
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        hcl_parse(file)
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

/// Strip the quotes from a block label token (`"web"` -> `web`).
fn unquote(token: &str) -> String {
    token.trim_matches('"').to_string()
}

/// Extract a heredoc tag from a line containing `<<TAG` or `<<-TAG`, if any.
/// HCL has no shift operator, so `<<` unambiguously starts a heredoc.
fn heredoc_tag(line: &str) -> Option<String> {
    let pos = line.find("<<")?;
    let after = &line[pos + 2..];
    let after = after.strip_prefix('-').unwrap_or(after);
    let tag: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if tag.is_empty() {
        None
    } else {
        Some(tag)
    }
}

fn hcl_parse(file: &Path) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    let mut in_block_comment = false;
    let mut heredoc: Option<String> = None;
    for line in read_lines_safe(file)? {
        let trimmed = line.trim();
        // Heredoc body: block-shaped text inside is data, not declarations.
        if let Some(tag) = &heredoc {
            if trimmed == tag {
                heredoc = None;
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
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if let Some(tag) = heredoc_tag(trimmed) {
            heredoc = Some(tag);
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let first = tokens.first().copied().unwrap_or("");
        match first {
            "variable" | "output" | "module" | "provider" => {
                if let Some(label) = tokens.get(1) {
                    let name = unquote(label);
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name,
                            kind: first.to_string(),
                        });
                    }
                }
            }
            "resource" | "data" => {
                // Terraform address: `type.name` — the stable identity.
                if let (Some(t), Some(n)) = (tokens.get(1), tokens.get(2)) {
                    let (t, n) = (unquote(t), unquote(n));
                    if !t.is_empty() && !n.is_empty() {
                        symbols.push(Symbol {
                            name: format!("{t}.{n}"),
                            kind: first.to_string(),
                        });
                    }
                }
            }
            _ => {}
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
        HclAdapter.parse_ast(f.path()).unwrap()
    }

    fn names(ast: &AstRepresentation) -> Vec<(String, String)> {
        ast.symbols
            .iter()
            .map(|s| (s.name.clone(), s.kind.clone()))
            .collect()
    }

    #[test]
    fn detects_labeled_blocks() {
        let ast = parse(
            "variable \"region\" {\n  type = string\n}\n\
             output \"endpoint\" {\n  value = \"x\"\n}\n\
             module \"network\" {\n  source = \"./mod\"\n}\n\
             provider \"aws\" {\n  region = \"us-east-1\"\n}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("region".into(), "variable".into())));
        assert!(got.contains(&("endpoint".into(), "output".into())));
        assert!(got.contains(&("network".into(), "module".into())));
        assert!(got.contains(&("aws".into(), "provider".into())));
    }

    #[test]
    fn resources_and_data_use_qualified_addresses() {
        let ast = parse(
            "resource \"aws_instance\" \"web\" {\n  ami = \"ami-123\"\n}\n\
             data \"aws_ami\" \"ubuntu\" {\n  most_recent = true\n}\n",
        );
        let got = names(&ast);
        assert!(got.contains(&("aws_instance.web".into(), "resource".into())));
        assert!(got.contains(&("aws_ami.ubuntu".into(), "data".into())));
    }

    #[test]
    fn skips_unlabeled_blocks() {
        let ast = parse(
            "terraform {\n  required_version = \">= 1.5\"\n}\n\
             locals {\n  name = \"internal\"\n}\n\
             moved {\n  from = aws_instance.a\n  to = aws_instance.b\n}\n",
        );
        assert!(ast.symbols.is_empty());
    }

    #[test]
    fn skips_comments_and_heredoc_bodies() {
        let ast = parse(
            "# resource \"aws_instance\" \"a\" {\n\
             // variable \"b\" {}\n\
             /*\nresource \"aws_s3_bucket\" \"c\" {\n}\n*/\n\
             locals {\n  script = <<-EOF\n    resource \"aws_instance\" \"heredoc_fake\" {\n    }\n  EOF\n}\n\
             resource \"aws_s3_bucket\" \"assets\" {\n  bucket = \"x\"\n}\n",
        );
        assert_eq!(
            names(&ast),
            vec![("aws_s3_bucket.assets".into(), "resource".into())]
        );
    }

    #[test]
    fn removed_block_is_breaking() {
        let old = parse("variable \"a\" {}\nvariable \"b\" {}\n");
        let new = parse("variable \"a\" {}\n");
        let diff = HclAdapter.diff_ast(&old, &new);
        assert!(HclAdapter.detect_breaking_changes(&diff));
        let same = HclAdapter.diff_ast(&old, &old);
        assert!(!HclAdapter.detect_breaking_changes(&same));
    }

    #[test]
    fn detect_files_matches_tf_and_hcl_not_tfvars() {
        let paths = vec![
            PathBuf::from("main.tf"),
            PathBuf::from("config.hcl"),
            PathBuf::from("prod.tfvars"),
            PathBuf::from("notes.md"),
        ];
        let got = HclAdapter.detect_files(&paths);
        assert_eq!(got.len(), 2);
    }
}
