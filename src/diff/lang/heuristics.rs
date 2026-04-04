use super::adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader};

const MAX_PARSE_SIZE_BYTES: u64 = 5 * 1024 * 1024; // 5MB

fn read_lines_safe(file: &Path) -> anyhow::Result<impl Iterator<Item = String>> {
    let meta = std::fs::metadata(file)?;
    if meta.len() > MAX_PARSE_SIZE_BYTES {
        anyhow::bail!("File too large for AST parsing ({} bytes)", meta.len());
    }
    let f = File::open(file)?;
    let reader = BufReader::new(f);
    Ok(reader.lines().filter_map(Result::ok))
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

// Basic diffing based on names and kinds
fn basic_diff(old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
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

    AstDiff { added, removed, modified }
}

pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn name(&self) -> &'static str { "Rust" }
    fn language(&self) -> Language { Language::Rust }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| p.extension().map_or(false, |e| e == "rs")).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("pub fn ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "function".to_string() });
                } else if let Some(rest) = line.strip_prefix("pub async fn ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "function".to_string() });
                } else if let Some(rest) = line.strip_prefix("pub struct ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "struct".to_string() });
                } else if let Some(rest) = line.strip_prefix("pub enum ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "enum".to_string() });
                } else if let Some(rest) = line.strip_prefix("pub trait ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "trait".to_string() });
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation { symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}

pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn name(&self) -> &'static str { "TypeScript" }
    fn language(&self) -> Language { Language::TypeScript }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| {
            let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
            ext == "ts" || ext == "tsx"
        }).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("export ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "export".to_string() });
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation { symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}

pub struct JavaScriptAdapter;

impl LanguageAdapter for JavaScriptAdapter {
    fn name(&self) -> &'static str { "JavaScript" }
    fn language(&self) -> Language { Language::JavaScript }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| {
            let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
            ext == "js" || ext == "jsx" || ext == "cjs" || ext == "mjs"
        }).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("export ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "export".to_string() });
                } else if let Some(rest) = line.strip_prefix("module.exports") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "export".to_string() });
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation { symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn name(&self) -> &'static str { "Python" }
    fn language(&self) -> Language { Language::Python }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| p.extension().map_or(false, |e| e == "py")).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("def ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "function".to_string() });
                } else if let Some(rest) = line.strip_prefix("class ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "class".to_string() });
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation { symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}

pub struct GoAdapter;

impl LanguageAdapter for GoAdapter {
    fn name(&self) -> &'static str { "Go" }
    fn language(&self) -> Language { Language::Go }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| p.extension().map_or(false, |e| e == "go")).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("func ") {
                    // simple heur: if first letter is uppercase, it's exported
                    if rest.chars().next().unwrap_or('a').is_uppercase() {
                        symbols.push(Symbol { name: rest.to_string(), kind: "function".to_string() });
                    }
                } else if let Some(rest) = line.strip_prefix("type ") {
                    if rest.chars().next().unwrap_or('a').is_uppercase() {
                        symbols.push(Symbol { name: rest.to_string(), kind: "type".to_string() });
                    }
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation { symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}

pub struct SwiftAdapter;

impl LanguageAdapter for SwiftAdapter {
    fn name(&self) -> &'static str { "Swift" }
    fn language(&self) -> Language { Language::Swift }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| p.extension().map_or(false, |e| e == "swift")).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                // Public/open declarations
                if trimmed.starts_with("public ") || trimmed.starts_with("open ") {
                    let rest = trimmed.strip_prefix("public ").or_else(|| trimmed.strip_prefix("open ")).unwrap_or("");
                    if let Some(name) = rest.strip_prefix("func ") {
                        symbols.push(Symbol { name: name.to_string(), kind: "function".to_string() });
                    } else if let Some(name) = rest.strip_prefix("class ") {
                        symbols.push(Symbol { name: name.to_string(), kind: "class".to_string() });
                    } else if let Some(name) = rest.strip_prefix("struct ") {
                        symbols.push(Symbol { name: name.to_string(), kind: "struct".to_string() });
                    } else if let Some(name) = rest.strip_prefix("enum ") {
                        symbols.push(Symbol { name: name.to_string(), kind: "enum".to_string() });
                    } else if let Some(name) = rest.strip_prefix("protocol ") {
                        symbols.push(Symbol { name: name.to_string(), kind: "protocol".to_string() });
                    } else if let Some(name) = rest.strip_prefix("var ") {
                        symbols.push(Symbol { name: name.to_string(), kind: "property".to_string() });
                    } else if let Some(name) = rest.strip_prefix("let ") {
                        symbols.push(Symbol { name: name.to_string(), kind: "property".to_string() });
                    } else if let Some(name) = rest.strip_prefix("typealias ") {
                        symbols.push(Symbol { name: name.to_string(), kind: "typealias".to_string() });
                    }
                }
                // @objc exposed methods
                if trimmed.starts_with("@objc") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "objc_export".to_string() });
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation { symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}

pub struct KotlinAdapter;

impl LanguageAdapter for KotlinAdapter {
    fn name(&self) -> &'static str { "Kotlin" }
    fn language(&self) -> Language { Language::Kotlin }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| {
            let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
            ext == "kt" || ext == "kts"
        }).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                // Skip private/protected/internal — treat unmarked as public (Kotlin default)
                if trimmed.starts_with("private ") || trimmed.starts_with("protected ") || trimmed.starts_with("internal ") {
                    continue;
                }
                if let Some(name) = trimmed.strip_prefix("fun ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "function".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("class ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "class".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("data class ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "data_class".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("sealed class ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "sealed_class".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("object ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "object".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("interface ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "interface".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("enum class ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "enum".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("typealias ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "typealias".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("val ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "property".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("var ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "property".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("suspend fun ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "suspend_function".to_string() });
                } else if let Some(name) = trimmed.strip_prefix("annotation class ") {
                    symbols.push(Symbol { name: name.to_string(), kind: "annotation".to_string() });
                }
                // @JvmStatic / @JvmField exposed to Java
                if trimmed.starts_with("@JvmStatic") || trimmed.starts_with("@JvmField") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "jvm_export".to_string() });
                }
                // Composable functions (Jetpack Compose / KMP)
                if trimmed.starts_with("@Composable") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "composable".to_string() });
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation { symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}

pub struct HtmlCssAdapter;

impl LanguageAdapter for HtmlCssAdapter {
    fn name(&self) -> &'static str { "HTML/CSS" }
    fn language(&self) -> Language { Language::HtmlCss }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| {
            let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
            ext == "html" || ext == "css"
        }).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let line = line.trim();
                if line.starts_with("--") && line.contains(':') {
                    symbols.push(Symbol { name: line.to_string(), kind: "css_var".to_string() });
                } else if line.starts_with('.') && line.contains('{') {
                    symbols.push(Symbol { name: line.to_string(), kind: "css_class".to_string() });
                }
            }
        }
        let hash = calculate_hash(&symbols);
        Ok(AstRepresentation { symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        // html/css changes alone are rarely breaking APIs in the traditional backend sense,
        // unless linked to JS, so we'll be conservative.
        false
    }
}
