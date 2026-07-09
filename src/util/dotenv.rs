//! Minimal, dependency-free `.env` loader.
//!
//! Loads `KEY=value` pairs from a `.env` file (if present) into the process
//! environment. Supports optional single/double quotes and skips comments/blank
//! lines. Does not implement variable interpolation, command substitution, or
//! multiline values.
//!
//! Security posture: `.env` may live inside an untrusted repository, so this
//! loader is deliberately conservative:
//!   * it **never overrides** a variable that is already present in the real
//!     environment (dotenvy-compatible semantics), and
//!   * it only accepts an allowlist of provider/kaptaind key prefixes, so a
//!     planted `.env` cannot hijack `PATH`, `LD_PRELOAD`, `*_PROXY`,
//!     `SSL_CERT_FILE`, `HOME`, etc.

use std::path::Path;

/// Environment-key prefixes that `.env` is permitted to set. Anything outside
/// this list is ignored.
const ALLOWED_PREFIXES: &[&str] = &[
    "KAPTAIND_",
    "ELEVENLABS_",
    "OPENAI_",
    "AZURE_SPEECH_",
    "GOOGLE_",
    "CARTESIA_",
    "MOONSHOT_",
    "KIMI_",
    "ANTHROPIC_",
    "OLLAMA_",
    "AWS_",
    "S3_",
    "GITHUB_",
];

fn is_allowed_key(key: &str) -> bool {
    ALLOWED_PREFIXES.iter().any(|p| key.starts_with(p))
}

/// Load `.env` from the current working directory, if it exists.
pub fn load() -> Result<(), Box<dyn std::error::Error>> {
    load_from(Path::new(".env"))
}

/// Load environment variables from the given path.
///
/// Existing environment variables are preserved (no override), and only keys
/// matching [`ALLOWED_PREFIXES`] are applied.
pub fn load_from(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let value = strip_quotes(value);
        if key.is_empty() || !is_allowed_key(key) {
            continue;
        }
        // Never override an already-present environment variable.
        if std::env::var_os(key).is_some() {
            continue;
        }
        std::env::set_var(key, value);
    }

    Ok(())
}

fn strip_quotes(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_simple_values() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        writeln!(temp, "KAPTAIND_FOO=bar\n# comment\n\nKAPTAIND_BAZ=qux").unwrap();
        std::env::remove_var("KAPTAIND_FOO");
        std::env::remove_var("KAPTAIND_BAZ");
        load_from(temp.path()).unwrap();
        assert_eq!(std::env::var("KAPTAIND_FOO").unwrap(), "bar");
        assert_eq!(std::env::var("KAPTAIND_BAZ").unwrap(), "qux");
        std::env::remove_var("KAPTAIND_FOO");
        std::env::remove_var("KAPTAIND_BAZ");
    }

    #[test]
    fn strips_quotes() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            temp,
            "KAPTAIND_SINGLE='one'\nKAPTAIND_DOUBLE=\"two\"\nKAPTAIND_MIXED='three\""
        )
        .unwrap();
        for k in ["KAPTAIND_SINGLE", "KAPTAIND_DOUBLE", "KAPTAIND_MIXED"] {
            std::env::remove_var(k);
        }
        load_from(temp.path()).unwrap();
        assert_eq!(std::env::var("KAPTAIND_SINGLE").unwrap(), "one");
        assert_eq!(std::env::var("KAPTAIND_DOUBLE").unwrap(), "two");
        assert_eq!(std::env::var("KAPTAIND_MIXED").unwrap(), "'three\"");
        for k in ["KAPTAIND_SINGLE", "KAPTAIND_DOUBLE", "KAPTAIND_MIXED"] {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn does_not_override_existing_env() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        writeln!(temp, "KAPTAIND_OVERRIDE=from-file").unwrap();
        std::env::set_var("KAPTAIND_OVERRIDE", "from-env");
        load_from(temp.path()).unwrap();
        assert_eq!(std::env::var("KAPTAIND_OVERRIDE").unwrap(), "from-env");
        std::env::remove_var("KAPTAIND_OVERRIDE");
    }

    #[test]
    fn rejects_dangerous_keys() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            temp,
            "PATH=/tmp/evil\nLD_PRELOAD=/tmp/evil.so\nHTTPS_PROXY=http://evil\nHOME=/tmp/evil\nRANDOM=1"
        )
        .unwrap();
        let before_path = std::env::var_os("PATH");
        load_from(temp.path()).unwrap();
        // PATH must be unchanged (never set from file, and not allowlisted).
        assert_eq!(std::env::var_os("PATH"), before_path);
        assert!(
            std::env::var_os("LD_PRELOAD").is_none()
                || std::env::var("LD_PRELOAD").unwrap() != "/tmp/evil.so"
        );
        assert!(std::env::var_os("RANDOM").is_none());
    }

    #[test]
    fn missing_file_is_ok() {
        load_from(Path::new("/nonexistent/path/.env")).unwrap();
    }
}
