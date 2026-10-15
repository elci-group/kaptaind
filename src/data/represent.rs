//! Textual, structural, and semantic views of one change, plus the fingerprints
//! that keep duplicates on one side of an evaluation split.

use super::schema::{
    Compatibility, DeterministicFeatures, Fingerprints, Language, SemanticFact, SemverClass,
    StructuralFact,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn observation_id(repo: &str, commit: &str, file: &str, symbol: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [repo, commit, file, symbol] {
        hasher.update(part.as_bytes());
        hasher.update([0xff]);
    }
    hasher.update(super::schema::KOU_SCHEMA_VERSION.to_le_bytes());
    let hex = format!("{:x}", hasher.finalize());
    format!("kou-{}", &hex[..20])
}

pub fn language_of_path(path: &str) -> Language {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let ext = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    let language = match ext {
        "rs" => "rust",
        "go" => "go",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "vue" => "vue",
        "svelte" => "svelte",
        "astro" => "astro",
        "py" => "python",
        "scss" | "sass" | "less" => "scss",
        "html" | "htm" => "html",
        "css" => "css",
        "java" => "java",
        "rb" => "ruby",
        "ex" | "exs" => "elixir",
        "php" => "php",
        "cs" => "csharp",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        "json" | "yaml" | "yml" | "toml" => "config",
        "md" | "rst" | "adoc" => "markdown",
        _ => "unknown",
    };
    Language::new(language)
}

pub fn is_generated_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    matches!(
        name,
        "cargo.lock"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "bun.lockb"
            | "poetry.lock"
            | "composer.lock"
    ) || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || path_has_segment(&lower, "dist")
        || path_has_segment(&lower, "target")
        || path_has_segment(&lower, "node_modules")
        || lower == ".kaptaind"
        || lower.starts_with(".kaptaind/")
        || lower.contains("/.kaptaind/")
        || lower == "version"
}

fn path_has_segment(path: &str, segment: &str) -> bool {
    path.split('/').any(|part| part == segment)
}

pub fn is_secret_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name == "id_rsa"
        || name == "id_dsa"
        || name.contains("credentials")
        || name.contains("secret")
}

pub fn is_docs_path(path: &str) -> bool {
    matches!(language_of_path(path).as_str(), "markdown")
        || path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .eq_ignore_ascii_case("license")
}

pub fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("_test.")
        || lower.contains(".spec.")
        || lower.contains(".test.")
}

pub fn is_dependency_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    matches!(
        name.as_str(),
        "cargo.toml"
            | "cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "requirements.txt"
            | "poetry.lock"
            | "go.mod"
            | "go.sum"
    )
}

pub fn is_runtime_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("dockerfile")
        || lower.ends_with(".service")
        || lower.contains("/deploy/")
        || lower.contains("/k8s/")
        || lower.ends_with(".sh")
        || lower.starts_with(".env")
}

pub fn redact_secrets(text: &str) -> (String, bool) {
    let mut redacted = false;
    let mut out = secret_assignment()
        .replace_all(text, |caps: &regex_lite::Captures| {
            redacted = true;
            format!(
                "{}[redacted]",
                caps.get(1).map(|m| m.as_str()).unwrap_or("")
            )
        })
        .into_owned();
    for (needle, replacement) in [
        ("ghp_", "ghp_[redacted]"),
        ("github_pat_", "github_pat_[redacted]"),
        ("sk-", "sk-[redacted]"),
        ("xoxb-", "xoxb-[redacted]"),
        ("AKIA", "AKIA[redacted]"),
    ] {
        if out.contains(needle) {
            redacted = true;
            out = redact_token_prefix(&out, needle, replacement);
        }
    }
    if out.contains("-----BEGIN ") {
        redacted = true;
        out = "-----BEGIN [redacted private material]-----".to_string();
    }
    (out, redacted)
}

fn redact_token_prefix(text: &str, prefix: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(pos) = text[index..].find(prefix) {
            let start = index + pos;
            out.push_str(&text[index..start]);
            let mut end = start + prefix.len();
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            out.push_str(replacement);
            index = end;
        } else {
            out.push_str(&text[index..]);
            break;
        }
    }
    out
}

fn secret_assignment() -> &'static regex_lite::Regex {
    static RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)((?:api[_-]?key|secret|token|password|authorization)\s*[:=]\s*)\S+",
        )
        .expect("secret assignment pattern")
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureEdit {
    pub name: String,
    pub kind: String,
    pub change: String,
    pub before: String,
    pub after: String,
    pub required_parameter_added: bool,
    pub parameter_removed: bool,
}

pub fn structural_from_diff(path: &str, diff: &str) -> (Vec<StructuralFact>, Vec<SignatureEdit>) {
    let mut removed: Vec<(String, String, String)> = Vec::new();
    let mut added: Vec<(String, String, String)> = Vec::new();
    for line in diff.lines() {
        let (bucket, content) = if let Some(content) = line.strip_prefix('+') {
            if line.starts_with("+++") {
                continue;
            }
            (&mut added, content)
        } else if let Some(content) = line.strip_prefix('-') {
            if line.starts_with("---") {
                continue;
            }
            (&mut removed, content)
        } else {
            continue;
        };
        if let Some((kind, name)) = signature_name(content) {
            bucket.push((kind, name, content.trim().to_string()));
        }
    }

    let mut edits = Vec::new();
    let mut consumed_added = vec![false; added.len()];
    for (kind, name, before) in &removed {
        if let Some(index) = added
            .iter()
            .position(|(add_kind, add_name, _)| add_kind == kind && add_name == name)
        {
            consumed_added[index] = true;
            let after = &added[index].2;
            if before != after {
                let (required_parameter_added, parameter_removed) =
                    parameter_delta(before, after, path);
                edits.push(SignatureEdit {
                    name: name.clone(),
                    kind: kind.clone(),
                    change: "modified".to_string(),
                    before: before.clone(),
                    after: after.clone(),
                    required_parameter_added,
                    parameter_removed,
                });
            }
        } else {
            edits.push(SignatureEdit {
                name: name.clone(),
                kind: kind.clone(),
                change: "removed".to_string(),
                before: before.clone(),
                after: String::new(),
                required_parameter_added: false,
                parameter_removed: true,
            });
        }
    }
    for (index, (kind, name, after)) in added.iter().enumerate() {
        if consumed_added[index] {
            continue;
        }
        edits.push(SignatureEdit {
            name: name.clone(),
            kind: kind.clone(),
            change: "added".to_string(),
            before: String::new(),
            after: after.clone(),
            required_parameter_added: false,
            parameter_removed: false,
        });
    }

    let facts = edits
        .iter()
        .map(|edit| {
            let mut attributes = BTreeMap::new();
            attributes.insert("path".to_string(), path.to_string());
            attributes.insert("kind".to_string(), edit.kind.clone());
            attributes.insert("name".to_string(), edit.name.clone());
            attributes.insert("change".to_string(), edit.change.clone());
            attributes.insert(
                "visibility".to_string(),
                if edit.before.contains("pub ")
                    || edit.after.contains("pub ")
                    || edit.before.contains("export ")
                    || edit.after.contains("export ")
                    || edit.kind != "function"
                {
                    "public".to_string()
                } else {
                    "unknown".to_string()
                },
            );
            attributes.insert(
                "required_parameter_added".to_string(),
                edit.required_parameter_added.to_string(),
            );
            StructuralFact {
                kind: format!("{}_CHANGE", edit.kind.to_ascii_uppercase()),
                attributes,
            }
        })
        .collect();
    (facts, edits)
}

fn signature_name(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let patterns: &[(&str, &str)] = &[
        ("pub fn ", "function"),
        ("pub async fn ", "function"),
        ("pub struct ", "struct"),
        ("pub enum ", "enum"),
        ("pub trait ", "trait"),
        ("fn ", "function"),
        ("func ", "function"),
        ("def ", "function"),
        ("class ", "class"),
        ("export function ", "function"),
        ("export async function ", "function"),
        ("export class ", "class"),
        ("export interface ", "interface"),
        ("export type ", "type"),
    ];
    for (prefix, kind) in patterns {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name = rest
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                return Some(((*kind).to_string(), name.to_string()));
            }
        }
    }
    None
}

fn parameter_delta(before: &str, after: &str, path: &str) -> (bool, bool) {
    let before_count = param_count(before);
    let after_count = param_count(after);
    if after_count <= before_count {
        return (false, after_count < before_count);
    }
    let optional = after.contains('=')
        && matches!(
            language_of_path(path).as_str(),
            "python" | "typescript" | "javascript"
        );
    (!optional, false)
}

fn param_count(signature: &str) -> usize {
    let Some(start) = signature.find('(') else {
        return 0;
    };
    let mut depth = 0i32;
    let mut count = 0usize;
    let mut token = false;
    for ch in signature[start + 1..].chars() {
        match ch {
            '(' | '<' | '[' => depth += 1,
            ')' if depth == 0 => break,
            ')' | '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                if token {
                    count += 1;
                }
                token = false;
            }
            ch if ch.is_whitespace() => {}
            _ => token = true,
        }
    }
    if token {
        count += 1;
    }
    count
}

pub fn predict_class(
    path: &str,
    edit: Option<&SignatureEdit>,
    features: &DeterministicFeatures,
) -> (SemverClass, String) {
    if let Some(edit) = edit {
        let language = language_of_path(path);
        let exported_name = edit
            .name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase());
        let public = edit.before.contains("pub ")
            || edit.after.contains("pub ")
            || edit.before.contains("export ")
            || edit.after.contains("export ")
            || (language.as_str() == "go" && exported_name)
            || (language.as_str() == "python" && !edit.name.starts_with('_'));
        if !public && edit.kind == "function" {
            return (SemverClass::NoBump, "PRIVATE_SIGNATURE_CHANGE".to_string());
        }
        return match edit.change.as_str() {
            "removed" => (SemverClass::Major, "PUBLIC_SYMBOL_REMOVED".to_string()),
            "added" => (SemverClass::Minor, "PUBLIC_SYMBOL_ADDED".to_string()),
            "modified" if edit.required_parameter_added => (
                SemverClass::Major,
                "REQUIRED_PUBLIC_PARAMETER_ADDED".to_string(),
            ),
            "modified" if edit.parameter_removed => {
                (SemverClass::Major, "PUBLIC_PARAMETER_REMOVED".to_string())
            }
            "modified" => (
                SemverClass::Ambiguous,
                "PUBLIC_SIGNATURE_CHANGED".to_string(),
            ),
            _ => (SemverClass::Ambiguous, "UNCLASSIFIED_SYMBOL".to_string()),
        };
    }
    if is_docs_path(path) || is_test_path(path) {
        return (SemverClass::NoBump, "NON_SHIPPED_PATH".to_string());
    }
    if is_generated_path(path) {
        return (SemverClass::NoBump, "GENERATED_FILE".to_string());
    }
    if features.required_parameter_added {
        return (
            SemverClass::Major,
            "REQUIRED_PUBLIC_PARAMETER_ADDED".to_string(),
        );
    }
    (SemverClass::Ambiguous, "NO_SYMBOL_SIGNAL".to_string())
}

pub fn semantic_fact(path: &str, class: SemverClass, reason: &str) -> SemanticFact {
    let (consumer, source, behaviour) = match class {
        SemverClass::Major => (
            Compatibility::Breaking,
            Compatibility::Breaking,
            Compatibility::Unknown,
        ),
        SemverClass::Minor => (
            Compatibility::Additive,
            Compatibility::Compatible,
            Compatibility::Unknown,
        ),
        SemverClass::Patch | SemverClass::NoBump => (
            Compatibility::Compatible,
            Compatibility::Compatible,
            if is_runtime_path(path) {
                Compatibility::Unknown
            } else {
                Compatibility::Internal
            },
        ),
        SemverClass::Ambiguous => (
            Compatibility::Unknown,
            Compatibility::Unknown,
            Compatibility::Unknown,
        ),
    };
    let kind = if is_dependency_path(path) {
        "DEPENDENCY_CHANGE"
    } else if is_runtime_path(path) {
        "RUNTIME_CHANGE"
    } else if reason.starts_with("PUBLIC") || reason.starts_with("REQUIRED") {
        "API_CHANGE"
    } else {
        "CHANGE"
    };
    SemanticFact {
        kind: kind.to_string(),
        consumer_compatibility: consumer,
        source_compatibility: source,
        behavioural_compatibility: behaviour,
    }
}

pub fn normalize_diff(diff: &str) -> String {
    diff.lines()
        .filter(|line| !line.starts_with("index ") && !line.starts_with("diff --git"))
        .map(|line| {
            if line.starts_with("@@") {
                "@@".to_string()
            } else {
                line.trim_end().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn fingerprint_set(
    commit_sha: &str,
    diff: &str,
    structural: &[StructuralFact],
    symbol_key: &str,
    semantic: &[SemanticFact],
) -> Fingerprints {
    Fingerprints {
        commit: sha256_hex(commit_sha.as_bytes()),
        diff: hash_nonempty(&normalize_diff(diff)),
        ast: hash_nonempty(
            &structural
                .iter()
                .map(|fact| format!("{}:{:?}", fact.kind, fact.attributes))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        symbol: hash_nonempty(symbol_key),
        semantic: hash_nonempty(
            &semantic
                .iter()
                .map(|fact| {
                    format!(
                        "{}:{}:{}:{}",
                        fact.kind,
                        fact.consumer_compatibility.as_str(),
                        fact.source_compatibility.as_str(),
                        fact.behavioural_compatibility.as_str()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    }
}

fn hash_nonempty(value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        sha256_hex(value.as_bytes())
    }
}

pub fn estimate_tokens(textual: &str) -> u64 {
    80 + (textual.chars().count() as u64 / 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_rust_parameter_is_a_major_structural_change() {
        let diff = "\
--- a/src/lib.rs
+++ b/src/lib.rs
@@
-pub fn connect(host: &str) -> Connection
+pub fn connect(host: &str, timeout: Duration) -> Connection
";
        let (facts, edits) = structural_from_diff("src/lib.rs", diff);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].name, "connect");
        assert!(edits[0].required_parameter_added);
        assert_eq!(facts[0].kind, "FUNCTION_CHANGE");
        let features = DeterministicFeatures::default();
        let (class, reason) = predict_class("src/lib.rs", Some(&edits[0]), &features);
        assert_eq!(class, SemverClass::Major);
        assert_eq!(reason, "REQUIRED_PUBLIC_PARAMETER_ADDED");
        let semantic = semantic_fact("src/lib.rs", class, &reason);
        assert_eq!(semantic.kind, "API_CHANGE");
        assert_eq!(semantic.consumer_compatibility, Compatibility::Breaking);
    }

    #[test]
    fn optional_python_parameter_is_not_forced_to_major() {
        let diff = "\
-def connect(host):
+def connect(host, timeout=1):
";
        let (_, edits) = structural_from_diff("net.py", diff);
        assert_eq!(edits.len(), 1);
        assert!(!edits[0].required_parameter_added);
        let (class, _) =
            predict_class("net.py", Some(&edits[0]), &DeterministicFeatures::default());
        assert_eq!(class, SemverClass::Ambiguous);
    }

    #[test]
    fn secret_assignments_and_tokens_are_stripped() {
        let (text, redacted) =
            redact_secrets("token = supersecretvalue\nghp_abcdefghijklmnopqrstuvwxyz");
        assert!(redacted);
        assert!(!text.contains("supersecretvalue"));
        assert!(!text.contains("ghp_abcdefghijklmnopqrstuvwxyz"));
        assert!(is_secret_path(".env"));
        assert!(is_secret_path("config/credentials.json"));
        assert!(is_generated_path("Cargo.lock"));
        assert!(is_generated_path(".kaptaind/analysis/cluster.json"));
        assert!(is_generated_path("VERSION"));
        assert!(is_generated_path("target/debug/build/libssh2.h"));
        assert!(is_generated_path("crates/foo/target/debug/lib.rs"));
        assert!(!is_generated_path("src/version/mod.rs"));
    }

    #[test]
    fn empty_diff_does_not_share_a_fingerprint_with_other_empties() {
        let left = fingerprint_set("abc", "", &[], "", &[]);
        let right = fingerprint_set("def", "", &[], "", &[]);
        assert!(left.diff.is_empty());
        assert_ne!(left.commit, right.commit);
    }
}
