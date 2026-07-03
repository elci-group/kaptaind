use super::super::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct KotlinAdapter;

impl LanguageAdapter for KotlinAdapter {
    fn name(&self) -> &'static str {
        "Kotlin"
    }
    fn language(&self) -> Language {
        Language::KOTLIN
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
                ext == "kt" || ext == "kts"
            })
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                // Skip private/protected/internal — treat unmarked as public (Kotlin default)
                if trimmed.starts_with("private ")
                    || trimmed.starts_with("protected ")
                    || trimmed.starts_with("internal ")
                {
                    continue;
                }
                if let Some(name) = trimmed.strip_prefix("fun ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "function".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("class ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "class".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("data class ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "data_class".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("sealed class ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "sealed_class".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("object ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "object".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("interface ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "interface".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("enum class ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "enum".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("typealias ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "typealias".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("val ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "property".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("var ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "property".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("suspend fun ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "suspend_function".to_string(),
                    });
                } else if let Some(name) = trimmed.strip_prefix("annotation class ") {
                    symbols.push(Symbol {
                        name: name.to_string(),
                        kind: "annotation".to_string(),
                    });
                }
                // @JvmStatic / @JvmField exposed to Java
                if trimmed.starts_with("@JvmStatic") || trimmed.starts_with("@JvmField") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "jvm_export".to_string(),
                    });
                }
                // Composable functions (Jetpack Compose / KMP)
                if trimmed.starts_with("@Composable") {
                    symbols.push(Symbol {
                        name: trimmed.to_string(),
                        kind: "composable".to_string(),
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
        !diff.removed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn kotlin_detects_public_api() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("App.kt");
        std::fs::write(
            &file,
            "fun greet() {}\ndata class User(val name: String)\nprivate fun helper() {}\n",
        )
        .unwrap();

        let adapter = KotlinAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        let api = adapter.extract_api(&ast);
        assert_eq!(api.public_symbols.len(), 2);
    }

    #[test]
    fn kotlin_detects_composables() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("UI.kt");
        std::fs::write(
            &file,
            "@Composable\nfun Greeting() {}\nsealed class State {}\n",
        )
        .unwrap();

        let adapter = KotlinAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert!(ast.symbols.iter().any(|s| s.kind == "composable"));
        assert!(ast.symbols.iter().any(|s| s.kind == "sealed_class"));
    }
}
