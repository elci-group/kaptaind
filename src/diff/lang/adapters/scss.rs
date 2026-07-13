use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct ScssAdapter;

impl LanguageAdapter for ScssAdapter {
    fn name(&self) -> &'static str {
        "SCSS/Sass/Less"
    }
    fn language(&self) -> Language {
        Language::SCSS
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
                ext == "scss" || ext == "sass" || ext == "less"
            })
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            // Track `/* ... */` regions so declarations inside block comments
            // are not mistaken for public API (measured messy-corpus FP, rev 26).
            let mut in_block_comment = false;
            for line in lines {
                let trimmed = line.trim();
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
                // SCSS/Sass variables: $primary: #000
                // Less variables: @primary: #000
                if (trimmed.starts_with('$') && trimmed.contains(':'))
                    || (trimmed.starts_with('@')
                        && trimmed.contains(':')
                        && !trimmed.starts_with("@media")
                        && !trimmed.starts_with("@import")
                        && !trimmed.starts_with("@include")
                        && !trimmed.starts_with("@mixin")
                        && !trimmed.starts_with("@use")
                        && !trimmed.starts_with("@forward"))
                {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "variable".to_string(),
                    });
                }
                // Mixins: @mixin name
                else if let Some(rest) = trimmed.strip_prefix("@mixin ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "mixin".to_string(),
                    });
                }
                // CSS custom properties
                else if trimmed.starts_with("--") && trimmed.contains(':') {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "css_var".to_string(),
                    });
                }
                // @forward / @use (Sass module system public API)
                else if trimmed.starts_with("@forward ") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "forward".to_string(),
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
        // Removing variables or mixins can break consumers
        diff.removed
            .iter()
            .any(|s| s.kind == "variable" || s.kind == "mixin" || s.kind == "forward")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn scss_detects_variables_and_mixins() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("theme.scss");
        std::fs::write(&file, "$primary: #007bff;\n$spacing: 1rem;\n@mixin flex-center {\n  display: flex;\n}\n--brand-color: #000;\n").unwrap();

        let adapter = ScssAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert_eq!(
            ast.symbols.iter().filter(|s| s.kind == "variable").count(),
            2
        );
        assert!(ast.symbols.iter().any(|s| s.kind == "mixin"));
        assert!(ast.symbols.iter().any(|s| s.kind == "css_var"));
    }

    #[test]
    fn scss_removing_mixin_is_breaking() {
        let adapter = ScssAdapter;
        let diff = AstDiff {
            added: vec![],
            removed: vec![Symbol {
                name: "flex-center".to_string(),
                kind: "mixin".to_string(),
            }],
            modified: vec![],
        };
        assert!(adapter.detect_breaking_changes(&diff));
    }

    #[test]
    fn less_detects_variables() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("vars.less");
        std::fs::write(
            &file,
            "@primary: #007bff;\n@media (max-width: 768px) {}\n@import 'base.less';\n",
        )
        .unwrap();

        let adapter = ScssAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        // Only @primary should be detected, not @media or @import
        assert_eq!(ast.symbols.len(), 1);
        assert_eq!(ast.symbols[0].kind, "variable");
    }
}
