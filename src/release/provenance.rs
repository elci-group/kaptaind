use crate::config::loader::ShipProvenanceConfig;
use crate::release::ship::ShipKind;
use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::process::Command as AsyncCommand;
use uuid::Uuid;

/// in-toto v1 statement type URI.
pub const IN_TOTO_STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";

/// SLSA v1.0 provenance predicate type URI.
pub const SLSA_PROVENANCE_PREDICATE_TYPE: &str = "https://slsa.dev/provenance/v1";

/// An in-toto v1 statement bundling a SLSA provenance predicate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InTotoStatement {
    #[serde(rename = "_type")]
    pub type_: String,
    pub subject: Vec<InTotoSubject>,
    #[serde(rename = "predicateType")]
    pub predicate_type: String,
    pub predicate: SlsaProvenancePredicate,
}

/// A subject identified by a set of cryptographic digests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InTotoSubject {
    pub name: String,
    pub digest: BTreeMap<String, String>,
}

/// SLSA v1.0 provenance predicate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlsaProvenancePredicate {
    #[serde(rename = "buildDefinition")]
    pub build_definition: BuildDefinition,
    #[serde(rename = "runDetails")]
    pub run_details: RunDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildDefinition {
    #[serde(rename = "buildType")]
    pub build_type: String,
    #[serde(rename = "externalParameters")]
    pub external_parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDetails {
    pub builder: Builder,
    pub metadata: RunMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Builder {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    #[serde(rename = "invocationId")]
    pub invocation_id: String,
    #[serde(rename = "startedOn")]
    pub started_on: String,
    #[serde(rename = "finishedOn")]
    pub finished_on: String,
}

/// Generate an in-toto/SLSA provenance attestation for a set of release artifacts.
///
/// The returned path points to `.kaptaind/ship/<version>/provenance.intoto.jsonl`.
/// Callers that want a detached GPG signature can use [`sign_provenance`] afterwards.
#[allow(clippy::too_many_arguments)]
pub fn generate_provenance(
    repo_path: &Path,
    version: &str,
    kind: ShipKind,
    targets: &[String],
    artifacts: &[PathBuf],
    cfg: &ShipProvenanceConfig,
) -> anyhow::Result<PathBuf> {
    let started_on = chrono::Utc::now().to_rfc3339();
    let invocation_id = Uuid::new_v4().to_string();

    let subjects = artifacts
        .iter()
        .filter(|p| p.exists())
        .map(|path| {
            let bytes = std::fs::read(path)
                .with_context(|| format!("failed to read artifact {}", path.display()))?;
            let hash = crate::util::hex::encode(Sha256::digest(&bytes));
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("artifact")
                .to_string();
            let mut digest = BTreeMap::new();
            digest.insert("sha256".to_string(), hash);
            Ok(InTotoSubject { name, digest })
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .context("failed to compute artifact digests for provenance")?;

    let repo_url = git_remote_url(repo_path).unwrap_or_default();
    let ref_ = match kind {
        ShipKind::Stable | ShipKind::Nightly => format!("refs/tags/v{}", version),
        ShipKind::Manual => "refs/heads/main".to_string(),
    };

    let external_parameters = json!({
        "repository": repo_url,
        "ref": ref_,
        "kind": kind.as_str(),
        "targets": targets,
    });

    let predicate = SlsaProvenancePredicate {
        build_definition: BuildDefinition {
            build_type: cfg.build_type.clone(),
            external_parameters,
        },
        run_details: RunDetails {
            builder: Builder {
                id: cfg.builder_id.clone(),
            },
            metadata: RunMetadata {
                invocation_id,
                started_on,
                finished_on: chrono::Utc::now().to_rfc3339(),
            },
        },
    };

    let statement = InTotoStatement {
        type_: IN_TOTO_STATEMENT_TYPE.to_string(),
        subject: subjects,
        predicate_type: SLSA_PROVENANCE_PREDICATE_TYPE.to_string(),
        predicate,
    };

    let ship_dir = repo_path.join(".kaptaind").join("ship").join(version);
    std::fs::create_dir_all(&ship_dir)
        .with_context(|| format!("failed to create ship directory {}", ship_dir.display()))?;
    let path = ship_dir.join("provenance.intoto.jsonl");
    let payload =
        serde_json::to_string(&statement).context("failed to serialize provenance statement")?;
    std::fs::write(&path, format!("{}\n", payload))
        .with_context(|| format!("failed to write provenance attestation {}", path.display()))?;
    Ok(path)
}

/// Create a detached ASCII-armored GPG signature for a provenance attestation.
///
/// The returned path is `<path>.asc`.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn sign_provenance(path: &Path, gpg_key_id: Option<&str>) -> anyhow::Result<PathBuf> {
    let mut args = vec![
        "--batch".to_string(),
        "--yes".to_string(),
        "--detach-sign".to_string(),
        "--armor".to_string(),
    ];
    if let Some(k) = gpg_key_id {
        args.push("--local-user".to_string());
        args.push(k.to_string());
    }
    args.push(
        path.to_str()
            .ok_or_else(|| anyhow!("invalid provenance path"))?
            .to_string(),
    );

    let output = AsyncCommand::new("gpg")
        .args(&args)
        .output()
        .await
        .context("failed to run gpg --detach-sign")?;
    if !output.status.success() {
        anyhow::bail!(
            "gpg --detach-sign failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut sig_path = path.as_os_str().to_owned();
    sig_path.push(".asc");
    Ok(PathBuf::from(sig_path))
}

fn git_remote_url(repo_path: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args([
            "-C",
            repo_path.to_str().unwrap_or("."),
            "remote",
            "get-url",
            "origin",
        ])
        .output()
        .context("failed to run git remote get-url origin")?;
    if !output.status.success() {
        anyhow::bail!(
            "git remote get-url origin failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::loader::ShipProvenanceConfig;
    use crate::release::ship::ShipKind;

    fn temp_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/example/kaptaind.git",
            ])
            .current_dir(path)
            .output()
            .unwrap();
        dir
    }

    fn default_cfg() -> ShipProvenanceConfig {
        ShipProvenanceConfig {
            enabled: true,
            builder_id: "https://kaptaind.dev/builder".to_string(),
            build_type: "https://kaptaind.dev/build".to_string(),
        }
    }

    #[test]
    fn provenance_generates_predicate_structure() {
        let dir = temp_git_repo();
        let artifact = dir.path().join("kaptaind.tar.gz");
        std::fs::write(&artifact, b"release artifact").unwrap();

        let path = generate_provenance(
            dir.path(),
            "1.2.3",
            ShipKind::Stable,
            &["x86_64-unknown-linux-gnu".to_string()],
            &[artifact],
            &default_cfg(),
        )
        .unwrap();

        assert_eq!(
            path,
            dir.path()
                .join(".kaptaind")
                .join("ship")
                .join("1.2.3")
                .join("provenance.intoto.jsonl")
        );

        let content = std::fs::read_to_string(&path).unwrap();
        let statement: serde_json::Value = serde_json::from_str(content.trim()).unwrap();

        assert_eq!(
            statement["_type"].as_str().unwrap(),
            "https://in-toto.io/Statement/v1"
        );
        assert_eq!(
            statement["predicateType"].as_str().unwrap(),
            "https://slsa.dev/provenance/v1"
        );

        let predicate = &statement["predicate"];
        assert_eq!(
            predicate["buildDefinition"]["buildType"].as_str().unwrap(),
            "https://kaptaind.dev/build"
        );
        assert_eq!(
            predicate["runDetails"]["builder"]["id"].as_str().unwrap(),
            "https://kaptaind.dev/builder"
        );
        assert!(predicate["runDetails"]["metadata"]["invocationId"]
            .as_str()
            .unwrap()
            .contains('-'));
        assert!(predicate["runDetails"]["metadata"]["startedOn"]
            .as_str()
            .unwrap()
            .starts_with("20"));
        assert!(predicate["runDetails"]["metadata"]["finishedOn"]
            .as_str()
            .unwrap()
            .starts_with("20"));

        let ext = &predicate["buildDefinition"]["externalParameters"];
        assert_eq!(
            ext["repository"].as_str().unwrap(),
            "https://github.com/example/kaptaind.git"
        );
        assert_eq!(ext["ref"].as_str().unwrap(), "refs/tags/v1.2.3");
        assert_eq!(ext["kind"].as_str().unwrap(), "stable");
        assert_eq!(
            ext["targets"][0].as_str().unwrap(),
            "x86_64-unknown-linux-gnu"
        );
    }

    #[test]
    fn provenance_computes_subject_digests() {
        let dir = temp_git_repo();
        let a1 = dir.path().join("bin.tar.gz");
        let a2 = dir.path().join("cli.tar.gz");
        std::fs::write(&a1, b"binary one").unwrap();
        std::fs::write(&a2, b"binary two").unwrap();

        let path = generate_provenance(
            dir.path(),
            "0.1.0",
            ShipKind::Manual,
            &["aarch64-apple-darwin".to_string()],
            &[a1.clone(), a2.clone()],
            &default_cfg(),
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let statement: InTotoStatement = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(statement.subject.len(), 2);

        let expected_one = crate::util::hex::encode(Sha256::digest(b"binary one"));
        let expected_two = crate::util::hex::encode(Sha256::digest(b"binary two"));

        let names: Vec<String> = statement.subject.iter().map(|s| s.name.clone()).collect();
        assert!(names.contains(&"bin.tar.gz".to_string()));
        assert!(names.contains(&"cli.tar.gz".to_string()));

        for subject in &statement.subject {
            assert_eq!(subject.digest.len(), 1);
            assert!(subject.digest.contains_key("sha256"));
        }

        let one = statement
            .subject
            .iter()
            .find(|s| s.name == "bin.tar.gz")
            .unwrap();
        assert_eq!(one.digest["sha256"], expected_one);

        let two = statement
            .subject
            .iter()
            .find(|s| s.name == "cli.tar.gz")
            .unwrap();
        assert_eq!(two.digest["sha256"], expected_two);
    }

    #[test]
    fn provenance_skips_missing_artifacts() {
        let dir = temp_git_repo();
        let present = dir.path().join("present.tar.gz");
        let missing = dir.path().join("missing.tar.gz");
        std::fs::write(&present, b"hello").unwrap();

        let path = generate_provenance(
            dir.path(),
            "2.0.0",
            ShipKind::Nightly,
            &[],
            &[present, missing],
            &default_cfg(),
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let statement: InTotoStatement = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(statement.subject.len(), 1);
        assert_eq!(statement.subject[0].name, "present.tar.gz");
    }

    #[test]
    fn provenance_uses_main_ref_for_manual() {
        let dir = temp_git_repo();
        let artifact = dir.path().join("a.tar.gz");
        std::fs::write(&artifact, b"x").unwrap();

        let path = generate_provenance(
            dir.path(),
            "1.0.0",
            ShipKind::Manual,
            &[],
            &[artifact],
            &default_cfg(),
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let statement: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(
            statement["predicate"]["buildDefinition"]["externalParameters"]["ref"]
                .as_str()
                .unwrap(),
            "refs/heads/main"
        );
    }

    #[test]
    fn provenance_uses_tag_ref_for_nightly() {
        let dir = temp_git_repo();
        let artifact = dir.path().join("a.tar.gz");
        std::fs::write(&artifact, b"x").unwrap();

        let path = generate_provenance(
            dir.path(),
            "1.0.0-nightly.20260707.abc1234",
            ShipKind::Nightly,
            &[],
            &[artifact],
            &default_cfg(),
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let statement: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(
            statement["predicate"]["buildDefinition"]["externalParameters"]["ref"]
                .as_str()
                .unwrap(),
            "refs/tags/v1.0.0-nightly.20260707.abc1234"
        );
    }

    #[test]
    fn provenance_works_without_version_file() {
        let dir = temp_git_repo();
        // Do not create a VERSION file; the function receives the version directly.
        let artifact = dir.path().join("artifact.tar.gz");
        std::fs::write(&artifact, b"data").unwrap();

        let path = generate_provenance(
            dir.path(),
            "0.5.0",
            ShipKind::Manual,
            &["x86_64-pc-windows-msvc".to_string()],
            &[artifact],
            &default_cfg(),
        )
        .unwrap();

        assert!(path.exists());
    }
}
