use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct VueAdapter;

impl LanguageAdapter for VueAdapter {
    fn name(&self) -> &'static str {
        "Vue"
    }
    fn language(&self) -> Language {
        Language::VUE
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "vue"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
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
                // Comment lines are not API: the substring macro match would
                // otherwise leak `defineProps` mentions out of comments
                // (measured messy-corpus FP, rev 24).
                if trimmed.starts_with("//") {
                    continue;
                }
                // Detect defineProps, defineEmits, defineExpose (Vue 3 macros)
                if trimmed.contains("defineProps") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "props".to_string(),
                    });
                } else if trimmed.contains("defineEmits") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "emits".to_string(),
                    });
                } else if trimmed.contains("defineExpose") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "expose".to_string(),
                    });
                } else if let Some(rest) = trimmed.strip_prefix("export ") {
                    symbols.push(Symbol {
                        name: rest.to_string(),
                        kind: "export".to_string(),
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
        // Removing props or emits is breaking for consumers
        diff.removed
            .iter()
            .any(|s| s.kind == "props" || s.kind == "emits")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn vue_detects_define_props_and_emits() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("Button.vue");
        std::fs::write(&file, "<script setup>\nconst props = defineProps<{label: string}>()\nconst emit = defineEmits(['click'])\n</script>\n<template><button>{{ props.label }}</button></template>\n").unwrap();

        let adapter = VueAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert!(ast.symbols.iter().any(|s| s.kind == "props"));
        assert!(ast.symbols.iter().any(|s| s.kind == "emits"));
    }

    #[test]
    fn vue_removing_props_is_breaking() {
        let adapter = VueAdapter;
        let diff = AstDiff {
            added: vec![],
            removed: vec![Symbol {
                name: "defineProps".to_string(),
                kind: "props".to_string(),
            }],
            modified: vec![],
        };
        assert!(adapter.detect_breaking_changes(&diff));
    }
}
