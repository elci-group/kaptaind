use super::super::adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter};
use super::common::*;
use crate::diff::version::detector::parse_ts_semver;
use std::path::{Path, PathBuf};

pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn name(&self) -> &'static str {
        "TypeScript"
    }
    fn language(&self) -> Language {
        Language::TYPESCRIPT
    }
    fn detect_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| {
                let ext = p.extension().map_or("", |e| e.to_str().unwrap_or(""));
                ext == "ts" || ext == "tsx"
            })
            .cloned()
            .collect()
    }
    fn parse_ast(&self, file: &Path) -> anyhow::Result<AstRepresentation> {
        ts_parse(file, (4, 0))
    }
    fn parse_ast_versioned(&self, file: &Path, version: &str) -> anyhow::Result<AstRepresentation> {
        let ver = parse_ts_semver(version);
        let mut ast = ts_parse(file, ver)?;
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
        !diff.removed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
}
