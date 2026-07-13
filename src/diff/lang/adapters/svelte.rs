use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct SvelteAdapter;

impl LanguageAdapter for SvelteAdapter {
    fn name(&self) -> &'static str {
        "Svelte"
    }
    fn language(&self) -> Language {
        Language::SVELTE
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "svelte"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        svelte_parse(file, false)
    }
    fn parse_ast_versioned(&self, file: &Path, version: &str) -> anyhow::Result<AstRepresentation> {
        // Svelte 5+ uses rune syntax; earlier versions use the options API
        let is_svelte5 = version
            .split('.')
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .map(|major| major >= 5)
            .unwrap_or(false);
        let mut ast = svelte_parse(file, is_svelte5)?;
        ast.version_tag = Some(version.to_string());
        Ok(ast)
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
        diff.removed
            .iter()
            .any(|s| s.kind == "prop" || s.kind == "rune_props")
    }
}

fn svelte_parse(file: &Path, is_svelte5: bool) -> anyhow::Result<AstRepresentation> {
    let mut symbols = Vec::new();
    if let Ok(lines) = read_lines_safe(file) {
        let mut in_script = false;
        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with("<script") {
                in_script = true;
                continue;
            }
            if trimmed.starts_with("</script") {
                in_script = false;
                continue;
            }
            if !in_script {
                continue;
            }
            // Comment lines are not API: substring rune matches would otherwise
            // leak `$props(` mentions out of comments (measured messy-corpus FP, rev 26).
            if trimmed.starts_with("//") {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("export let ") {
                symbols.push(Symbol {
                    name: rest.to_string(),
                    kind: "prop".to_string(),
                });
            } else if let Some(rest) = trimmed.strip_prefix("export const ") {
                symbols.push(Symbol {
                    name: rest.to_string(),
                    kind: "export".to_string(),
                });
            } else if let Some(rest) = trimmed.strip_prefix("export function ") {
                symbols.push(Symbol {
                    name: rest.to_string(),
                    kind: "export".to_string(),
                });
            }
            // Svelte 5 rune API ($props, $state, $derived, $effect)
            if is_svelte5 || trimmed.contains("$props(") {
                if trimmed.contains("$props(") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "rune_props".to_string(),
                    });
                }
                if trimmed.contains("$state(") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "rune_state".to_string(),
                    });
                }
                if trimmed.contains("$derived(") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "rune_derived".to_string(),
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
    use tempfile::tempdir;

    #[test]
    fn svelte_detects_exported_props() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("Card.svelte");
        std::fs::write(&file, "<script>\nexport let title = '';\nexport let variant = 'default';\n</script>\n<div>{title}</div>\n").unwrap();

        let adapter = SvelteAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert_eq!(ast.symbols.iter().filter(|s| s.kind == "prop").count(), 2);
    }
}
