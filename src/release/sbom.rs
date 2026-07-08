use anyhow::{anyhow, Context};
use serde_json::json;
use std::path::{Path, PathBuf};

/// One dependency package discovered from a project lockfile.
#[derive(Debug, Clone)]
pub struct SbomPackage {
    pub name: String,
    pub version: String,
    pub checksum: Option<String>,
}

/// Generate an SBOM for the project at `repo_path`.
///
/// The `format` argument selects the output serialization; only `"spdx-json"`
/// is currently supported. The resulting file is written to
/// `.kaptaind/ship/<VERSION>/sbom.spdx.json`.
pub fn generate_sbom(repo_path: &Path, format: &str) -> anyhow::Result<PathBuf> {
    if format != "spdx-json" {
        anyhow::bail!("unsupported SBOM format: {}", format);
    }

    let version = read_version(repo_path)?;
    let packages = detect_packages(repo_path)?;
    let sbom = build_spdx_sbom(repo_path, &version, &packages);

    let output_dir = repo_path.join(".kaptaind").join("ship").join(&version);
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let output_path = output_dir.join("sbom.spdx.json");
    std::fs::write(&output_path, serde_json::to_string_pretty(&sbom)?)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    Ok(output_path)
}

fn read_version(repo_path: &Path) -> anyhow::Result<String> {
    let version_path = repo_path.join("VERSION");
    if !version_path.exists() {
        anyhow::bail!("VERSION file not found at {}", version_path.display());
    }
    let version = std::fs::read_to_string(&version_path)?.trim().to_string();
    if version.is_empty() {
        anyhow::bail!("VERSION file is empty");
    }
    Ok(version)
}

fn detect_packages(repo_path: &Path) -> anyhow::Result<Vec<SbomPackage>> {
    let cargo_lock = repo_path.join("Cargo.lock");
    if cargo_lock.exists() {
        return parse_cargo_lock(&cargo_lock);
    }

    let package_lock = repo_path.join("package-lock.json");
    if package_lock.exists() {
        return parse_package_lock(&package_lock);
    }

    Ok(Vec::new())
}

fn parse_cargo_lock(path: &Path) -> anyhow::Result<Vec<SbomPackage>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;

    let packages = value
        .get("package")
        .and_then(|p| p.as_array())
        .ok_or_else(|| anyhow!("Cargo.lock missing [[package]] array"))?;

    let mut out = Vec::new();
    for pkg in packages {
        let name = pkg
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| anyhow!("Cargo.lock package missing name"))?;
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Cargo.lock package missing version"))?;
        let checksum = pkg
            .get("checksum")
            .and_then(|c| c.as_str())
            .map(String::from);

        out.push(SbomPackage {
            name: name.to_string(),
            version: version.to_string(),
            checksum,
        });
    }

    Ok(out)
}

fn parse_package_lock(path: &Path) -> anyhow::Result<Vec<SbomPackage>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    let packages = value
        .get("packages")
        .and_then(|p| p.as_object())
        .ok_or_else(|| anyhow!("package-lock.json missing packages object"))?;

    let mut out = Vec::new();
    for (key, pkg) in packages {
        // Skip the root pseudo-package (empty key) unless it carries a version.
        if key.is_empty() {
            continue;
        }
        let name = pkg
            .get("name")
            .and_then(|n| n.as_str())
            .or_else(|| package_name_from_node_modules_path(key))
            .ok_or_else(|| anyhow!("package-lock.json package missing name"))?;
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("package-lock.json package missing version"))?;
        let checksum = pkg
            .get("integrity")
            .and_then(|c| c.as_str())
            .map(String::from);

        out.push(SbomPackage {
            name: name.to_string(),
            version: version.to_string(),
            checksum,
        });
    }

    Ok(out)
}

fn package_name_from_node_modules_path(path: &str) -> Option<&str> {
    // package-lock v2/v3 "node_modules/<scope>/<name>" paths.
    path.rfind("node_modules/")
        .map(|idx| &path[idx + "node_modules/".len()..])
}

fn build_spdx_sbom(repo_path: &Path, version: &str, packages: &[SbomPackage]) -> serde_json::Value {
    let project_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");
    let document_name = format!("{}-{}", project_name, version);
    let document_namespace = format!("https://kaptaind.dev/{}/sbom/{}", project_name, version);

    let root_spdx_id = spdx_id(project_name);

    let mut sbom_packages = vec![json!({
        "SPDXID": root_spdx_id,
        "name": project_name,
        "versionInfo": version,
        "downloadLocation": "NOASSERTION",
        "filesAnalyzed": false,
        "supplier": "NOASSERTION",
    })];

    let mut relationships = vec![json!({
        "spdxElementId": "SPDXRef-DOCUMENT",
        "relatedSpdxElement": root_spdx_id,
        "relationshipType": "DESCRIBES",
    })];

    for pkg in packages {
        let pkg_spdx_id = spdx_id(&pkg.name);
        let mut entry = json!({
            "SPDXID": pkg_spdx_id,
            "name": pkg.name,
            "versionInfo": pkg.version,
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": false,
            "supplier": "NOASSERTION",
        });

        if let Some(checksum) = &pkg.checksum {
            entry["checksums"] = json!([
                {
                    "algorithm": checksum_algorithm(checksum),
                    "checksumValue": strip_integrity_prefix(checksum),
                }
            ]);
        }

        sbom_packages.push(entry);
        relationships.push(json!({
            "spdxElementId": root_spdx_id,
            "relatedSpdxElement": pkg_spdx_id,
            "relationshipType": "DEPENDS_ON",
        }));
    }

    json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": document_name,
        "documentNamespace": document_namespace,
        "creationInfo": {
            "created": chrono::Utc::now().to_rfc3339(),
            "creators": ["Tool: kaptaind"],
        },
        "packages": sbom_packages,
        "relationships": relationships,
    })
}

fn spdx_id(name: &str) -> String {
    // SPDX IDs must be alphanumeric plus '.', '-', and '_' per the spec.
    let safe: String = name
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => c,
            _ => '-',
        })
        .collect();
    format!("SPDXRef-Package-{}", safe)
}

fn checksum_algorithm(integrity: &str) -> &'static str {
    if integrity.starts_with("sha512-") || integrity.starts_with("sha512-") {
        "SHA512"
    } else if integrity.starts_with("sha384-") {
        "SHA384"
    } else if integrity.starts_with("sha256-") {
        "SHA256"
    } else if integrity.starts_with("sha1-") {
        "SHA1"
    } else {
        "OTHER"
    }
}

fn strip_integrity_prefix(integrity: &str) -> &str {
    integrity
        .find('-')
        .map(|idx| &integrity[idx + 1..])
        .unwrap_or(integrity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo_with_version(version: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("VERSION"), version).unwrap();
        dir
    }

    #[test]
    fn parse_cargo_lock_extracts_name_version_checksum() {
        let dir = temp_repo_with_version("1.0.0");
        let lock = r#"version = 4

[[package]]
name = "anyhow"
version = "1.0.86"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "7f21f05c9f45ae9728a4b9db314f4e6fdf7e9e53f8e7115c8479b26218463632"

[[package]]
name = "serde"
version = "1.0.204"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "9e8c8cf938e98f79d741e11d46a6c9aa6ed1e622a5a4e145b7b627d4b657244c"
"#;
        std::fs::write(dir.path().join("Cargo.lock"), lock).unwrap();

        let packages = detect_packages(dir.path()).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "anyhow");
        assert_eq!(packages[0].version, "1.0.86");
        assert!(packages[0].checksum.is_some());
        assert_eq!(packages[1].name, "serde");
        assert_eq!(packages[1].version, "1.0.204");
    }

    #[test]
    fn parse_cargo_lock_skips_packages_without_checksum() {
        let dir = temp_repo_with_version("0.1.0");
        let lock = r#"version = 4

[[package]]
name = "local-crate"
version = "0.1.0"
"#;
        std::fs::write(dir.path().join("Cargo.lock"), lock).unwrap();

        let packages = detect_packages(dir.path()).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "local-crate");
        assert_eq!(packages[0].version, "0.1.0");
        assert!(packages[0].checksum.is_none());
    }

    #[test]
    fn generate_sbom_creates_spdx_json() {
        let dir = temp_repo_with_version("2.3.4");
        let lock = r#"version = 4

[[package]]
name = "tokio"
version = "1.38.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "ba4f4a02a7a60d76a03025a8bb2c012dd4e9ed468a8e67c98e7a397aacebe0b1"
"#;
        std::fs::write(dir.path().join("Cargo.lock"), lock).unwrap();

        let path = generate_sbom(dir.path(), "spdx-json").unwrap();
        assert_eq!(path, dir.path().join(".kaptaind/ship/2.3.4/sbom.spdx.json"));

        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["spdxVersion"], "SPDX-2.3");
        assert_eq!(value["SPDXID"], "SPDXRef-DOCUMENT");
        assert!(value["packages"].as_array().unwrap().len() >= 2);

        let pkg_names: Vec<String> = value["packages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect();
        assert!(pkg_names.iter().any(|n| n == "tokio"));
    }

    #[test]
    fn generate_sbom_rejects_unknown_format() {
        let dir = temp_repo_with_version("1.0.0");
        let err = generate_sbom(dir.path(), "cyclonedx-xml").unwrap_err();
        assert!(err.to_string().contains("unsupported SBOM format"));
    }

    #[test]
    fn generate_sbom_errors_without_version_file() {
        let dir = tempfile::tempdir().unwrap();
        let err = generate_sbom(dir.path(), "spdx-json").unwrap_err();
        assert!(err.to_string().contains("VERSION file not found"));
    }

    #[test]
    fn parse_package_lock_extracts_dependencies() {
        let dir = temp_repo_with_version("1.0.0");
        let lock = r#"{
  "name": "demo",
  "version": "1.0.0",
  "lockfileVersion": 3,
  "packages": {
    "": {
      "name": "demo",
      "version": "1.0.0"
    },
    "node_modules/next": {
      "version": "14.2.4",
      "resolved": "https://registry.npmjs.org/next/-/next-14.2.4.tgz",
      "integrity": "sha512-R8/V7reugCSwCZtv/V68MoGxUAE36dkcM3I5beRbgwO/4p9RgmqG8SQ+BdwSnO6z4mEkg5IO0ibBjKzVchx5gzQ=="
    },
    "node_modules/react": {
      "version": "18.3.1",
      "resolved": "https://registry.npmjs.org/react/-/react-18.3.1.tgz",
      "integrity": "sha512-wS+hAgJShR0KhEvPJArfuPVN1+Hz1t0Y6n5jLrGQbkb4urgPE/0RvePdJtHqscpqIa8GScC/8O3Vf5Q=="
    }
  }
}"#;
        std::fs::write(dir.path().join("package-lock.json"), lock).unwrap();

        let packages = detect_packages(dir.path()).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "next");
        assert_eq!(packages[0].version, "14.2.4");
        assert!(packages[0].checksum.is_some());
        assert_eq!(packages[1].name, "react");
        assert_eq!(packages[1].version, "18.3.1");
    }

    #[test]
    fn spdx_id_sanitizes_special_characters() {
        assert_eq!(spdx_id("serde_json"), "SPDXRef-Package-serde_json");
        assert_eq!(spdx_id("@scope/pkg"), "SPDXRef-Package--scope-pkg");
    }
}
