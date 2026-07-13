use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct AstroAdapter;

impl LanguageAdapter for AstroAdapter {
    fn name(&self) -> &'static str {
        "Astro"
    }
    fn language(&self) -> Language {
        Language::ASTRO
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "astro"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            let mut in_frontmatter = false;
            for line in lines {
                let trimmed = line.trim();
                if trimmed == "---" {
                    in_frontmatter = !in_frontmatter;
                    continue;
                }
                if !in_frontmatter {
                    continue;
                }
                // Comment lines are not API: the `Astro.props` substring match
                // would otherwise leak mentions out of comments (measured
                // messy-corpus FP, rev 26).
                if trimmed.starts_with("//") {
                    continue;
                }
                // Astro frontmatter is TypeScript
                if let Some(rest) = trimmed.strip_prefix("export ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "export".to_string(),
                    });
                }
                // Astro.props usage
                if trimmed.contains("Astro.props") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "props".to_string(),
                    });
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
        diff.removed.iter().any(|s| s.kind == "props")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn astro_detects_frontmatter_exports() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("Page.astro");
        std::fs::write(&file, "---\nexport const prerender = true;\nconst { title } = Astro.props;\n---\n<html>{title}</html>\n").unwrap();

        let adapter = AstroAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert!(ast.symbols.iter().any(|s| s.kind == "export"));
        assert!(ast.symbols.iter().any(|s| s.kind == "props"));
    }
}
