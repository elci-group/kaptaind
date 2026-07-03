use super::super::adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter};
use super::common::*;
use std::path::{Path, PathBuf};

pub struct RustAdapter;

/// syn-based AST visitor that extracts public API symbols from Rust source files.
/// Inspired by Furnace's SnapshotVisitor, focused on public API surface detection.
mod rust_syn {
    use super::super::super::adapter::Symbol;
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
            let args: Vec<String> = sig
                .inputs
                .iter()
                .map(|arg| match arg {
                    syn::FnArg::Receiver(_) => "self".to_string(),
                    syn::FnArg::Typed(pat_type) => {
                        if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                            pat_ident.ident.to_string()
                        } else {
                            "_".to_string()
                        }
                    }
                })
                .collect();
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
                self.symbols.push(Symbol {
                    name: sig_str,
                    kind: kind.to_string(),
                });
            }
            syn::visit::visit_item_fn(self, node);
        }

        fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
            if Self::is_pub(&node.vis) {
                let name = node.ident.to_string();
                self.symbols.push(Symbol {
                    name: name.clone(),
                    kind: "struct".to_string(),
                });
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
                self.symbols.push(Symbol {
                    name: name.clone(),
                    kind: "enum".to_string(),
                });
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
                self.symbols.push(Symbol {
                    name: name.clone(),
                    kind: "trait".to_string(),
                });
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
                let trait_prefix = node
                    .trait_
                    .as_ref()
                    .and_then(|(_, path, _)| {
                        path.segments.last().map(|seg| format!("<{}>", seg.ident))
                    })
                    .unwrap_or_default();

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
    fn name(&self) -> &'static str {
        "Rust"
    }
    fn language(&self) -> Language {
        Language::RUST
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        const MAX_PARSE_SIZE_BYTES: u64 = 5 * 1024 * 1024; // 5MB
        let meta = std::fs::metadata(file)?;
        if meta.len() > MAX_PARSE_SIZE_BYTES {
            anyhow::bail!("File too large for AST parsing ({} bytes)", meta.len());
        }
        let content = std::fs::read_to_string(file)?;
        let syntax =
            syn::parse_file(&content).map_err(|e| anyhow::anyhow!("syn parse error: {e}"))?;

        let mut visitor = rust_syn::ApiVisitor::default();
        syn::visit::Visit::visit_file(&mut visitor, &syntax);

        let hash = calculate_hash(&visitor.symbols);
        Ok(AstRepresentation {
            symbols: visitor.symbols,
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
        // Removing public functions, struct fields, trait methods, or enum variants is breaking
        diff.removed.iter().any(|s| {
            matches!(
                s.kind.as_str(),
                "function"
                    | "async_function"
                    | "method"
                    | "trait_method"
                    | "struct"
                    | "field"
                    | "enum"
                    | "variant"
                    | "trait"
                    | "type_alias"
                    | "associated_type"
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::adapter::Symbol;
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rust_syn_parses_pub_function_with_args() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(
            &file,
            r#"
pub fn greet(name: &str, count: usize) -> String {
    format!("{name}: {count}")
}
fn private() {}
"#,
        )
        .unwrap();

        let adapter = RustAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        let api = adapter.extract_api(&ast);
        assert_eq!(api.public_symbols.len(), 1);
        assert!(api.public_symbols[0].name.contains("greet"));
        assert!(api.public_symbols[0].name.contains("name"));
        assert!(api.public_symbols[0].name.contains("count"));
        assert_eq!(api.public_symbols[0].kind, "function");
    }

    #[test]
    fn rust_syn_parses_async_function() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(
            &file,
            "pub async fn fetch(url: &str) -> Result<String, Error> { todo!() }\n",
        )
        .unwrap();

        let adapter = RustAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert_eq!(ast.symbols.len(), 1);
        assert_eq!(ast.symbols[0].kind, "async_function");
    }

    #[test]
    fn rust_syn_parses_struct_with_pub_fields() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(
            &file,
            r#"
pub struct Config {
    pub host: String,
    pub port: u16,
    secret: String,  // private field
}
"#,
        )
        .unwrap();

        let adapter = RustAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        let kinds: Vec<&str> = ast.symbols.iter().map(|s| s.kind.as_str()).collect();
        assert!(kinds.contains(&"struct"));
        assert_eq!(ast.symbols.iter().filter(|s| s.kind == "field").count(), 2);
        // only pub fields
    }

    #[test]
    fn rust_syn_parses_trait_methods_and_assoc_types() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(
            &file,
            r#"
pub trait Handler {
    type Output;
    fn handle(&self, input: &[u8]) -> Self::Output;
    fn name(&self) -> &str;
}
"#,
        )
        .unwrap();

        let adapter = RustAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.kind == "trait" && s.name == "Handler"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.kind == "associated_type" && s.name.contains("Output")));
        assert_eq!(
            ast.symbols
                .iter()
                .filter(|s| s.kind == "trait_method")
                .count(),
            2
        );
    }

    #[test]
    fn rust_syn_parses_enum_variants() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(
            &file,
            r#"
pub enum Status {
    Active,
    Inactive,
    Pending,
}
"#,
        )
        .unwrap();

        let adapter = RustAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.kind == "enum" && s.name == "Status"));
        assert_eq!(
            ast.symbols.iter().filter(|s| s.kind == "variant").count(),
            3
        );
        assert!(ast.symbols.iter().any(|s| s.name == "Status::Active"));
    }

    #[test]
    fn rust_syn_parses_impl_pub_methods() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(
            &file,
            r#"
pub struct Db;
impl Db {
    pub fn connect(url: &str) -> Self { Db }
    pub fn query(&self, sql: &str) -> Vec<Row> { vec![] }
    fn internal(&self) {}
}
struct Row;
"#,
        )
        .unwrap();

        let adapter = RustAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        let methods: Vec<_> = ast.symbols.iter().filter(|s| s.kind == "method").collect();
        assert_eq!(methods.len(), 2); // connect and query, not internal
        assert!(methods.iter().any(|s| s.name.contains("connect")));
        assert!(methods.iter().any(|s| s.name.contains("query")));
    }

    #[test]
    fn rust_syn_parses_const_and_type_alias() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("lib.rs");
        std::fs::write(
            &file,
            r#"
pub const VERSION: &str = "1.0.0";
pub type Result<T> = std::result::Result<T, Error>;
pub static COUNTER: AtomicUsize = AtomicUsize::new(0);
struct Error;
"#,
        )
        .unwrap();

        let adapter = RustAdapter;
        let ast = adapter.parse_ast(&file).unwrap();
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.kind == "const" && s.name == "VERSION"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.kind == "type_alias" && s.name == "Result"));
        assert!(ast
            .symbols
            .iter()
            .any(|s| s.kind == "static" && s.name == "COUNTER"));
    }

    #[test]
    fn rust_syn_detects_breaking_removal() {
        let adapter = RustAdapter;
        let diff = AstDiff {
            added: vec![],
            removed: vec![Symbol {
                name: "handle(req)".to_string(),
                kind: "method".to_string(),
            }],
            modified: vec![],
        };
        assert!(adapter.detect_breaking_changes(&diff));
    }
}
