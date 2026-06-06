use crate::config::loader::PluginAdapterConfig;
use crate::diff::lang::adapter::{
    ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter, Symbol,
};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// JSON payload sent to the plugin over stdin.
#[derive(Serialize)]
struct PluginRequest<'a> {
    file: &'a Path,
}

/// JSON payload received from the plugin via stdout.
#[derive(Deserialize)]
struct PluginResponse {
    symbols: Vec<PluginSymbol>,
}

#[derive(Deserialize)]
struct PluginSymbol {
    name: String,
    kind: String,
}

/// Wraps an external script/binary as a `LanguageAdapter` using a JSON stdio
/// protocol.
///
/// **Protocol** (newline-delimited):
/// - stdin:  `{"file":"<absolute_path>"}\n`
/// - stdout: `{"symbols":[{"name":"<name>","kind":"<kind>"},...]}\n`
///
/// The plugin process is launched fresh for every `parse_ast` call (i.e. it is
/// short-lived).  For heavy workloads a persistent daemon plugin is a future
/// enhancement.
pub struct PluginAdapter {
    config: PluginAdapterConfig,
}

impl PluginAdapter {
    pub fn new(config: PluginAdapterConfig) -> Self {
        Self { config }
    }
}

impl LanguageAdapter for PluginAdapter {
    fn name(&self) -> &'static str {
        // Static lifetime isn't available for dynamic strings; leak once per
        // adapter instance (there are very few of them, created at startup).
        Box::leak(self.config.name.clone().into_boxed_str())
    }

    fn language(&self) -> Language {
        Language::Plugin
    }

    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                self.config
                    .extensions
                    .iter()
                    .any(|e| e.trim_start_matches('.').to_lowercase() == ext)
            })
            .cloned()
            .collect()
    }

    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        let request = serde_json::to_string(&PluginRequest { file })?;

        // Split the command string into program + args to support e.g.
        // `"python3 /path/to/adapter.py"`.
        let mut parts = self.config.command.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("plugin command is empty for '{}'", self.config.name))?;
        let args: Vec<&str> = parts.collect();

        let output = std::process::Command::new(program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child
                    .stdin
                    .take()
                    .expect("stdin piped")
                    .write_all(request.as_bytes())?;
                child.wait_with_output()
            })
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to run plugin '{}' ({}): {e}",
                    self.config.name,
                    self.config.command
                )
            })?;

        if !output.status.success() {
            anyhow::bail!(
                "plugin '{}' exited with status {}",
                self.config.name,
                output.status
            );
        }

        let response: PluginResponse = serde_json::from_slice(&output.stdout).map_err(|e| {
            anyhow::anyhow!("plugin '{}' returned invalid JSON: {e}", self.config.name)
        })?;

        let symbols: Vec<Symbol> = response
            .symbols
            .into_iter()
            .map(|s| Symbol {
                name: s.name,
                kind: s.kind,
            })
            .collect();

        let structure_hash = {
            let mut h = DefaultHasher::new();
            for s in &symbols {
                s.name.hash(&mut h);
                s.kind.hash(&mut h);
            }
            h.finish()
        };

        Ok(AstRepresentation {
            symbols,
            structure_hash,
            ..Default::default()
        })
    }

    fn extract_api(&self, ast: &AstRepresentation) -> ApiSurface {
        let public_symbols: Vec<Symbol> = ast
            .symbols
            .iter()
            .filter(|s| {
                matches!(
                    s.kind.as_str(),
                    "function" | "class" | "struct" | "interface" | "export" | "type_export"
                )
            })
            .cloned()
            .collect();
        let mut h = DefaultHasher::new();
        for s in &public_symbols {
            s.name.hash(&mut h);
        }
        ApiSurface {
            public_symbols,
            hash: h.finish(),
        }
    }

    fn diff_ast(&self, old: &AstRepresentation, new: &AstRepresentation) -> AstDiff {
        let added = new
            .symbols
            .iter()
            .filter(|s| !old.symbols.contains(s))
            .cloned()
            .collect();
        let removed = old
            .symbols
            .iter()
            .filter(|s| !new.symbols.contains(s))
            .cloned()
            .collect();
        AstDiff {
            added,
            removed,
            modified: vec![],
        }
    }

    fn detect_breaking_changes(&self, diff: &AstDiff) -> bool {
        !diff.removed.is_empty()
    }
}
