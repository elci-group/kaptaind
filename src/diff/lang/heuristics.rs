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

/// Classify a TypeScript/JavaScript export line into a more specific kind.
fn classify_ts_export(rest: &str) -> String {
    if rest.starts_with("default function ") || rest.starts_with("default class ") || rest == "default" || rest.starts_with("default ") {
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

/// syn-based AST visitor that extracts public API symbols from Rust source files.
/// Inspired by Furnace's SnapshotVisitor, focused on public API surface detection.
mod rust_syn {
    use super::Symbol;
    use syn::visit::Visit;

    #[derive(Default)]
    pub struct ApiVisitor {
        pub symbols: Vec<Symbol>,
    }

    impl ApiVisitor {
        fn is_pub(vis: &syn::Visibility) -> bool {
            matches!(vis, syn::Visibility::Public(_))
        }

        fn format_fn_sig(sig: &syn::Signature) -> String {
            let name = sig.ident.to_string();
            let args: Vec<String> = sig.inputs.iter().map(|arg| {
                match arg {
                    syn::FnArg::Receiver(_) => "self".to_string(),
                    syn::FnArg::Typed(pat_type) => {
                        if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                            pat_ident.ident.to_string()
                        } else {
                            "_".to_string()
                        }
                    }
                }
            }).collect();
            if args.is_empty() {
                format!("{name}()")
            } else {
                format!("{name}({})", args.join(", "))
            }
        }
    }

    impl<'ast> Visit<'ast> for ApiVisitor {
        fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
            if Self::is_pub(&node.vis) {
                let sig_str = Self::format_fn_sig(&node.sig);
                let kind = if node.sig.asyncness.is_some() {
                    "async_function"
                } else {
                    "function"
                };
                self.symbols.push(Symbol { name: sig_str, kind: kind.to_string() });
            }
            syn::visit::visit_item_fn(self, node);
        }

        fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
            if Self::is_pub(&node.vis) {
                let name = node.ident.to_string();
                self.symbols.push(Symbol { name: name.clone(), kind: "struct".to_string() });
                // Record public fields as separate symbols
                if let syn::Fields::Named(fields) = &node.fields {
                    for field in &fields.named {
                        if Self::is_pub(&field.vis) {
                            if let Some(ident) = &field.ident {
                                self.symbols.push(Symbol {
                                    name: format!("{name}.{ident}"),
                                    kind: "field".to_string(),
                                });
                            }
                        }
                    }
                }
            }
            syn::visit::visit_item_struct(self, node);
        }

        fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
            if Self::is_pub(&node.vis) {
                let name = node.ident.to_string();
                self.symbols.push(Symbol { name: name.clone(), kind: "enum".to_string() });
                for variant in &node.variants {
                    self.symbols.push(Symbol {
                        name: format!("{name}::{}", variant.ident),
                        kind: "variant".to_string(),
                    });
                }
            }
            syn::visit::visit_item_enum(self, node);
        }

        fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
            if Self::is_pub(&node.vis) {
                let name = node.ident.to_string();
                self.symbols.push(Symbol { name: name.clone(), kind: "trait".to_string() });
                for item in &node.items {
                    if let syn::TraitItem::Fn(method) = item {
                        self.symbols.push(Symbol {
                            name: format!("{name}::{}", method.sig.ident),
                            kind: "trait_method".to_string(),
                        });
                    }
                    if let syn::TraitItem::Type(assoc) = item {
                        self.symbols.push(Symbol {
                            name: format!("{name}::{}", assoc.ident),
                            kind: "associated_type".to_string(),
                        });
                    }
                }
            }
            syn::visit::visit_item_trait(self, node);
        }

        fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
            // Extract the impl target type name
            let type_name = if let syn::Type::Path(path) = &*node.self_ty {
                path.path.segments.last().map(|seg| seg.ident.to_string())
            } else {
                None
            };

            if let Some(type_name) = type_name {
                let trait_prefix = node.trait_.as_ref().and_then(|(_, path, _)| {
                    path.segments.last().map(|seg| format!("<{}>", seg.ident))
                }).unwrap_or_default();

                for item in &node.items {
                    if let syn::ImplItem::Fn(method) = item {
                        if Self::is_pub(&method.vis) || node.trait_.is_some() {
                            let sig_str = Self::format_fn_sig(&method.sig);
                            self.symbols.push(Symbol {
                                name: format!("{type_name}{trait_prefix}::{sig_str}"),
                                kind: "method".to_string(),
                            });
                        }
                    }
                }
            }
            syn::visit::visit_item_impl(self, node);
        }

        fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
            if Self::is_pub(&node.vis) {
                self.symbols.push(Symbol {
                    name: node.ident.to_string(),
                    kind: "type_alias".to_string(),
                });
            }
            syn::visit::visit_item_type(self, node);
        }

        fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
            if Self::is_pub(&node.vis) {
                self.symbols.push(Symbol {
                    name: node.ident.to_string(),
                    kind: "const".to_string(),
                });
            }
            syn::visit::visit_item_const(self, node);
        }

        fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
            if Self::is_pub(&node.vis) {
                self.symbols.push(Symbol {
                    name: node.ident.to_string(),
                    kind: "static".to_string(),
                });
            }
            syn::visit::visit_item_static(self, node);
        }
    }
}

impl LanguageAdapter for RustAdapter {
    fn name(&self) -> &'static str { "Rust" }
    fn language(&self) -> Language { Language::Rust }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| p.extension().map_or(false, |e| e == "rs")).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let meta = std::fs::metadata(file)?;
        if meta.len() > MAX_PARSE_SIZE_BYTES {
            anyhow::bail!("File too large for AST parsing ({} bytes)", meta.len());
        }
        let content = std::fs::read_to_string(file)?;
        let syntax = syn::parse_file(&content).map_err(|e| anyhow::anyhow!("syn parse error: {e}"))?;

        let mut visitor = rust_syn::ApiVisitor::default();
        syn::visit::Visit::visit_file(&mut visitor, &syntax);

        let hash = calculate_hash(&visitor.symbols);
        Ok(AstRepresentation { symbols: visitor.symbols, structure_hash: hash })
    }
    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        ApiSurface { public_symbols: ast.symbols.clone(), hash: ast.structure_hash }
    }
    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        basic_diff(old, new)
    }
    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        // Removing public functions, struct fields, trait methods, or enum variants is breaking
        diff.removed.iter().any(|s| matches!(s.kind.as_str(),
            "function" | "async_function" | "method" | "trait_method"
            | "struct" | "field" | "enum" | "variant" | "trait"
            | "type_alias" | "associated_type"
        ))
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
                let trimmed = line.trim();
                // Exports (functions, classes, types, interfaces, consts)
                if let Some(rest) = trimmed.strip_prefix("export ") {
                    let kind = classify_ts_export(rest);
                    symbols.push(Symbol { name: rest.to_string(), kind });
                }
                // React hooks
                if (trimmed.starts_with("export function use") || trimmed.starts_with("export const use"))
                    && !trimmed.contains("// ") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "hook".to_string() });
                }
                // Next.js route exports: generateMetadata, generateStaticParams, etc.
                for marker in ["generateMetadata", "generateStaticParams", "getServerSideProps", "getStaticProps", "getStaticPaths"] {
                    if trimmed.contains(marker) && trimmed.contains("export") {
                        symbols.push(Symbol { name: marker.to_string(), kind: "route_export".to_string() });
                    }
                }
                // Middleware export
                if trimmed.starts_with("export function middleware") || trimmed.starts_with("export const middleware") {
                    symbols.push(Symbol { name: "middleware".to_string(), kind: "middleware".to_string() });
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
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("export ") {
                    let kind = classify_ts_export(rest);
                    symbols.push(Symbol { name: rest.to_string(), kind });
                } else if let Some(rest) = trimmed.strip_prefix("module.exports") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "cjs_export".to_string() });
                }
                // React hooks
                if (trimmed.starts_with("export function use") || trimmed.starts_with("export const use"))
                    && !trimmed.contains("// ") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "hook".to_string() });
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

pub struct VueAdapter;

impl LanguageAdapter for VueAdapter {
    fn name(&self) -> &'static str { "Vue" }
    fn language(&self) -> Language { Language::Vue }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| p.extension().map_or(false, |e| e == "vue")).cloned().collect()
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
                if !in_script { continue; }
                // Detect defineProps, defineEmits, defineExpose (Vue 3 macros)
                if trimmed.contains("defineProps") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "props".to_string() });
                } else if trimmed.contains("defineEmits") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "emits".to_string() });
                } else if trimmed.contains("defineExpose") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "expose".to_string() });
                } else if let Some(rest) = trimmed.strip_prefix("export ") {
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
        // Removing props or emits is breaking for consumers
        diff.removed.iter().any(|s| s.kind == "props" || s.kind == "emits")
    }
}

pub struct SvelteAdapter;

impl LanguageAdapter for SvelteAdapter {
    fn name(&self) -> &'static str { "Svelte" }
    fn language(&self) -> Language { Language::Svelte }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| p.extension().map_or(false, |e| e == "svelte")).cloned().collect()
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
                if !in_script { continue; }
                // Svelte props: `export let name`
                if let Some(rest) = trimmed.strip_prefix("export let ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "prop".to_string() });
                } else if let Some(rest) = trimmed.strip_prefix("export const ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "export".to_string() });
                } else if let Some(rest) = trimmed.strip_prefix("export function ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "export".to_string() });
                }
                // Svelte 5 runes: $props(), $state(), $derived()
                if trimmed.contains("$props(") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "rune_props".to_string() });
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
        diff.removed.iter().any(|s| s.kind == "prop" || s.kind == "rune_props")
    }
}

pub struct AstroAdapter;

impl LanguageAdapter for AstroAdapter {
    fn name(&self) -> &'static str { "Astro" }
    fn language(&self) -> Language { Language::Astro }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| p.extension().map_or(false, |e| e == "astro")).cloned().collect()
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
                if !in_frontmatter { continue; }
                // Astro frontmatter is TypeScript
                if let Some(rest) = trimmed.strip_prefix("export ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "export".to_string() });
                }
                // Astro.props usage
                if trimmed.contains("Astro.props") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "props".to_string() });
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
        diff.removed.iter().any(|s| s.kind == "props")
    }
}

pub struct ScssAdapter;

impl LanguageAdapter for ScssAdapter {
    fn name(&self) -> &'static str { "SCSS/Sass/Less" }
    fn language(&self) -> Language { Language::Scss }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths.iter().filter(|p| {
            let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
            ext == "scss" || ext == "sass" || ext == "less"
        }).cloned().collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let mut symbols = Vec::new();
        if let Ok(lines) = read_lines_safe(file) {
            for line in lines {
                let trimmed = line.trim();
                // SCSS/Sass variables: $primary: #000
                if trimmed.starts_with('$') && trimmed.contains(':') {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "variable".to_string() });
                }
                // Less variables: @primary: #000
                else if trimmed.starts_with('@') && trimmed.contains(':') && !trimmed.starts_with("@media") && !trimmed.starts_with("@import") && !trimmed.starts_with("@include") && !trimmed.starts_with("@mixin") && !trimmed.starts_with("@use") && !trimmed.starts_with("@forward") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "variable".to_string() });
                }
                // Mixins: @mixin name
                else if let Some(rest) = trimmed.strip_prefix("@mixin ") {
                    symbols.push(Symbol { name: rest.to_string(), kind: "mixin".to_string() });
                }
                // CSS custom properties
                else if trimmed.starts_with("--") && trimmed.contains(':') {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "css_var".to_string() });
                }
                // @forward / @use (Sass module system public API)
                else if trimmed.starts_with("@forward ") {
                    symbols.push(Symbol { name: trimmed.to_string(), kind: "forward".to_string() });
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
        // Removing variables or mixins can break consumers
        diff.removed.iter().any(|s| s.kind == "variable" || s.kind == "mixin" || s.kind == "forward")
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
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn swift_detects_public_api() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("API.swift");
        std::fs::write(&file, "public func greet() {}\npublic class Router {}\nprivate func helper() {}\n").unwrap();

        let adapter = SwiftAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        let api = adapter.extract_api(&ast);
        assert_eq!(api.public_symbols.len(), 2);
        assert!(api.public_symbols.iter().any(|s| s.kind == "function"));
        assert!(api.public_symbols.iter().any(|s| s.kind == "class"));
    }

    #[test]
    fn swift_detects_protocols_and_enums() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("Types.swift");
        std::fs::write(&file, "public protocol Networking {}\npublic enum AppError {}\nopen class Base {}\n").unwrap();

        let adapter = SwiftAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert_eq!(ast.symbols.len(), 3);
    }

    #[test]
    fn kotlin_detects_public_api() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("App.kt");
        std::fs::write(&file, "fun greet() {}\ndata class User(val name: String)\nprivate fun helper() {}\n").unwrap();

        let adapter = KotlinAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        let api = adapter.extract_api(&ast);
        assert_eq!(api.public_symbols.len(), 2);
    }

    #[test]
    fn kotlin_detects_composables() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("UI.kt");
        std::fs::write(&file, "@Composable\nfun Greeting() {}\nsealed class State {}\n").unwrap();

        let adapter = KotlinAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert!(ast.symbols.iter().any(|s| s.kind == "composable"));
        assert!(ast.symbols.iter().any(|s| s.kind == "sealed_class"));
    }

    #[test]
    fn typescript_detects_hooks_and_middleware() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("hooks.ts");
        std::fs::write(&file, "export function useAuth() {}\nexport const useTheme = () => {}\nexport function middleware(req: any) {}\n").unwrap();

        let adapter = TypeScriptAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert!(ast.symbols.iter().any(|s| s.kind == "hook"));
        assert!(ast.symbols.iter().any(|s| s.kind == "middleware"));
    }

    #[test]
    fn typescript_classifies_export_kinds() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("api.ts");
        std::fs::write(&file, "export default function main() {}\nexport interface Config {}\nexport type ID = string;\nexport const VERSION = 1;\n").unwrap();

        let adapter = TypeScriptAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        let kinds: Vec<&str> = ast.symbols.iter().map(|s| s.kind.as_str()).collect();
        assert!(kinds.contains(&"default_export"));
        assert!(kinds.contains(&"interface"));
        assert!(kinds.contains(&"type"));
        assert!(kinds.contains(&"binding"));
    }

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
            removed: vec![Symbol { name: "defineProps".to_string(), kind: "props".to_string() }],
            modified: vec![],
        };
        assert!(adapter.detect_breaking_changes(&diff));
    }

    #[test]
    fn svelte_detects_exported_props() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("Card.svelte");
        std::fs::write(&file, "<script>\nexport let title = '';\nexport let variant = 'default';\n</script>\n<div>{title}</div>\n").unwrap();

        let adapter = SvelteAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert_eq!(ast.symbols.iter().filter(|s| s.kind == "prop").count(), 2);
    }

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

    #[test]
    fn scss_detects_variables_and_mixins() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("theme.scss");
        std::fs::write(&file, "$primary: #007bff;\n$spacing: 1rem;\n@mixin flex-center {\n  display: flex;\n}\n--brand-color: #000;\n").unwrap();

        let adapter = ScssAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert_eq!(ast.symbols.iter().filter(|s| s.kind == "variable").count(), 2);
        assert!(ast.symbols.iter().any(|s| s.kind == "mixin"));
        assert!(ast.symbols.iter().any(|s| s.kind == "css_var"));
    }

    #[test]
    fn scss_removing_mixin_is_breaking() {
        let adapter = ScssAdapter;
        let diff = AstDiff {
            added: vec![],
            removed: vec![Symbol { name: "flex-center".to_string(), kind: "mixin".to_string() }],
            modified: vec![],
        };
        assert!(adapter.detect_breaking_changes(&diff));
    }

    #[test]
    fn less_detects_variables() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("vars.less");
        std::fs::write(&file, "@primary: #007bff;\n@media (max-width: 768px) {}\n@import 'base.less';\n").unwrap();

        let adapter = ScssAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        // Only @primary should be detected, not @media or @import
        assert_eq!(ast.symbols.len(), 1);
        assert_eq!(ast.symbols[0].kind, "variable");
    }
}
