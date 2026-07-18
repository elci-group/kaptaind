//! Version writeback for single-crate projects and Cargo workspaces.
//!
//! Milestone W1 of `docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md`:
//! [`save_version`] is the single-crate path (unchanged behavior), and
//! [`save_workspace_version`] generalizes it — one cluster produces one bump
//! decision, applied to the set of manifests selected by the
//! `[versioning].workspace` policy. Every bumped package's `Cargo.toml` and
//! the shared `Cargo.lock` are updated in the same pass so the workspace
//! N-tuple (each manifest == its lock entry) never drifts.

use std::path::{Path, PathBuf};

use semver::Version;

use super::workspace::{Member, WorkspaceLayout};
use crate::config::loader::{LockSyncMode, WorkspacePolicy};
use kaptaind_diff::version::{apply, Bump};

/// Write `VERSION` and sync the root manifest(s) and lockfile — the whole
/// single-crate writeback. Also used for the root crate of a workspace
/// under `root_only` policy.
///
/// The monotonic guard refuses to write a version below the current
/// baseline (VERSION file or manifest). A missing/unresolvable baseline
/// skips the guard — the first save is what creates VERSION.
pub fn save_version(path: &Path, version: &Version, lock_sync: LockSyncMode) -> anyhow::Result<()> {
    let Some(repo_path) = path.parent() else {
        std::fs::write(path, version.to_string())?;
        return Ok(());
    };
    let snapshots = snapshot_files(&[
        path.to_path_buf(),
        repo_path.join("Cargo.toml"),
        repo_path.join("src-tauri/Cargo.toml"),
        repo_path.join("Cargo.lock"),
    ])?;
    let result = (|| {
        if let Ok(baseline) = super::resolve_baseline(repo_path) {
            anyhow::ensure!(
                version >= &baseline,
                "refusing version downgrade: next v{version} < current baseline v{baseline}"
            );
        }
        std::fs::write(path, version.to_string())?;
        let package_name = write_root_manifests(repo_path, version)?;
        let packages: Vec<(String, Version)> = package_name
            .map(|name| (name, version.clone()))
            .into_iter()
            .collect();
        sync_lock(repo_path, &packages, lock_sync)?;
        Ok(())
    })();
    if let Err(error) = result {
        if let Err(rollback_error) = restore_files(&snapshots) {
            tracing::error!(
                error = %error,
                rollback_error = %rollback_error,
                "version writeback and rollback both failed"
            );
            return Err(error.context(format!("rollback also failed: {rollback_error:#}")));
        }
        tracing::error!(error = %error, "version writeback failed; restored original files");
        return Err(error);
    }
    Ok(())
}

/// What a workspace bump wrote, and the versions it wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceBump {
    /// Every path the writeback touched (`VERSION`, manifests, `Cargo.lock`).
    /// Recorded in the daemon's self-write guard and added to cluster
    /// staging so the bump lands in the same commit as its trigger.
    pub written_paths: Vec<PathBuf>,
    /// `(package name, new version)` for each bumped package.
    pub bumped: Vec<(String, Version)>,
}

impl WorkspaceBump {
    /// The paths the single-crate [`save_version`] writeback touches — the
    /// exact set the daemon has always recorded and staged.
    pub fn single(repo_root: &Path) -> Self {
        Self {
            written_paths: vec![
                repo_root.join("VERSION"),
                repo_root.join("Cargo.toml"),
                repo_root.join("Cargo.lock"),
            ],
            bumped: Vec::new(),
        }
    }
}

/// Apply one bump decision across a workspace per the `policy`.
///
/// Per-target baselines are resolved from each manifest at write time
/// (members never get `VERSION` files), so a `crates/foo`-only cluster bumps
/// foo alone — fixing the silent member-version deflation where every bump
/// moved only the root. Members declaring `version.workspace = true` move
/// the root `[workspace.package].version` once, never their own manifest.
/// After version edits, inter-member path-dependency requirements that no
/// longer match a bumped member are raised to the new version (never
/// widened), keeping `cargo build --locked` green.
pub fn save_workspace_version(
    layout: &WorkspaceLayout,
    policy: WorkspacePolicy,
    bump: Bump,
    cluster_paths: &[PathBuf],
    repo_root: &Path,
    lock_sync: LockSyncMode,
) -> anyhow::Result<WorkspaceBump> {
    let mut paths = vec![
        repo_root.join("VERSION"),
        repo_root.join("Cargo.toml"),
        repo_root.join("src-tauri/Cargo.toml"),
        repo_root.join("Cargo.lock"),
    ];
    paths.extend(
        layout
            .members()
            .iter()
            .map(|member| member.manifest.clone()),
    );
    let snapshots = snapshot_files(&paths)?;
    match save_workspace_version_inner(layout, policy, bump, cluster_paths, repo_root, lock_sync) {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            if let Err(rollback_error) = restore_files(&snapshots) {
                tracing::error!(
                    error = %error,
                    rollback_error = %rollback_error,
                    "workspace version writeback and rollback both failed"
                );
                return Err(error.context(format!("rollback also failed: {rollback_error:#}")));
            }
            tracing::error!(
                error = %error,
                "workspace version writeback failed; restored original files"
            );
            Err(error)
        }
    }
}

fn save_workspace_version_inner(
    layout: &WorkspaceLayout,
    policy: WorkspacePolicy,
    bump: Bump,
    cluster_paths: &[PathBuf],
    repo_root: &Path,
    lock_sync: LockSyncMode,
) -> anyhow::Result<WorkspaceBump> {
    let targets = select_targets(layout, policy, cluster_paths, repo_root);
    let mut outcome = WorkspaceBump::default();

    // Root crate: the single-crate writeback with the lock pass deferred so
    // every bumped package syncs in one write.
    if targets.root {
        let baseline = super::resolve_baseline(repo_root)?;
        let next = apply(baseline.clone(), bump);
        if next != baseline {
            let package_name = write_root_version_file_and_manifests(repo_root, &next)?;
            outcome.written_paths.push(repo_root.join("VERSION"));
            outcome.written_paths.push(repo_root.join("Cargo.toml"));
            let tauri = repo_root.join("src-tauri/Cargo.toml");
            if tauri.exists() {
                outcome.written_paths.push(tauri);
            }
            if let Some(name) = package_name {
                outcome.bumped.push((name, next));
            }
        }
    }

    // Inherited members share one version at the root `[workspace.package]`;
    // write it once no matter how many inheriting members were touched.
    let inheriting: Vec<&Member> = targets
        .members
        .iter()
        .filter(|m| m.inherits_version)
        .collect();
    if !inheriting.is_empty() {
        let root_manifest = repo_root.join("Cargo.toml");
        let shared = read_workspace_package_version(&root_manifest)?;
        let next = apply(shared.clone(), bump);
        if next != shared {
            write_workspace_package_version(&root_manifest, &next)?;
            outcome.written_paths.push(root_manifest);
            for member in inheriting {
                outcome.bumped.push((member.name.clone(), next.clone()));
            }
        }
    }

    for member in targets.members.iter().filter(|m| !m.inherits_version) {
        let baseline = read_manifest_version(&member.manifest)?;
        let next = apply(baseline.clone(), bump);
        if next == baseline {
            continue;
        }
        write_manifest_version(&member.manifest, &next)?;
        outcome.written_paths.push(member.manifest.clone());
        outcome.bumped.push((member.name.clone(), next));
    }

    outcome.written_paths.extend(raise_member_requirements(
        layout,
        &outcome.bumped,
        repo_root,
    )?);

    if !outcome.bumped.is_empty() {
        sync_lock(repo_root, &outcome.bumped, lock_sync)?;
        let lock = repo_root.join("Cargo.lock");
        if !matches!(lock_sync, LockSyncMode::Off) && lock.exists() {
            outcome.written_paths.push(lock);
        }
    }

    outcome.written_paths.sort();
    outcome.written_paths.dedup();
    Ok(outcome)
}

type FileSnapshot = (PathBuf, Option<Vec<u8>>);

fn snapshot_files(paths: &[PathBuf]) -> anyhow::Result<Vec<FileSnapshot>> {
    let mut unique = paths.to_vec();
    unique.sort();
    unique.dedup();
    unique
        .into_iter()
        .map(|path| {
            let contents = if path.exists() {
                Some(std::fs::read(&path)?)
            } else {
                None
            };
            Ok((path, contents))
        })
        .collect()
}

fn restore_files(snapshots: &[FileSnapshot]) -> anyhow::Result<()> {
    for (path, contents) in snapshots {
        match contents {
            Some(contents) => std::fs::write(path, contents)?,
            None if path.exists() => std::fs::remove_file(path)?,
            None => {}
        }
    }
    Ok(())
}

/// Which packages a decided bump applies to.
struct TargetSet {
    /// Bump the root crate (`VERSION` + root manifest).
    root: bool,
    /// Bump these members.
    members: Vec<Member>,
}

/// Target selection per `[versioning].workspace`:
/// - `root_only`: the root crate only (today's behavior).
/// - `touched`: members whose subtree contains at least one cluster path,
///   plus the root when any cluster path falls outside every member
///   subtree. Virtual workspaces never bump a root (there is none).
/// - `lockstep`: every member, plus the root for `RootCrate` layouts.
fn select_targets(
    layout: &WorkspaceLayout,
    policy: WorkspacePolicy,
    cluster_paths: &[PathBuf],
    repo_root: &Path,
) -> TargetSet {
    let has_root = matches!(layout, WorkspaceLayout::RootCrate { .. });
    match policy {
        WorkspacePolicy::RootOnly => TargetSet {
            root: has_root || matches!(layout, WorkspaceLayout::Single),
            members: Vec::new(),
        },
        WorkspacePolicy::Lockstep => TargetSet {
            root: has_root,
            members: layout.members().to_vec(),
        },
        WorkspacePolicy::Touched => {
            let members = layout.members();
            let touches = |member: &Member| {
                let dir = member.manifest.parent().unwrap_or(repo_root);
                cluster_paths
                    .iter()
                    .any(|p| repo_root.join(p).starts_with(dir))
            };
            TargetSet {
                root: has_root
                    && cluster_paths.iter().any(|p| {
                        let abs = repo_root.join(p);
                        !members
                            .iter()
                            .any(|m| abs.starts_with(m.manifest.parent().unwrap_or(repo_root)))
                    }),
                members: members.iter().filter(|m| touches(m)).cloned().collect(),
            }
        }
    }
}

/// Write `VERSION` and edit `[package].version` in `Cargo.toml` (and
/// `src-tauri/Cargo.toml` when present). Returns the root package name.
fn write_root_version_file_and_manifests(
    repo_path: &Path,
    version: &Version,
) -> anyhow::Result<Option<String>> {
    std::fs::write(repo_path.join("VERSION"), version.to_string())?;
    write_root_manifests(repo_path, version)
}

/// Edit `[package].version` in the root `Cargo.toml` (and
/// `src-tauri/Cargo.toml` when present). Returns the root package name.
fn write_root_manifests(repo_path: &Path, version: &Version) -> anyhow::Result<Option<String>> {
    let mut package_name: Option<String> = None;
    for cargo_rel in ["Cargo.toml", "src-tauri/Cargo.toml"] {
        let cargo_toml_path = repo_path.join(cargo_rel);
        if cargo_toml_path.exists() {
            let content = std::fs::read_to_string(&cargo_toml_path)?;
            let mut doc = content.parse::<toml_edit::DocumentMut>()?;
            if let Some(package) = doc.get_mut("package") {
                if package.get("version").is_some() {
                    if package_name.is_none() {
                        package_name = package
                            .get("name")
                            .and_then(|n| n.as_str())
                            .map(str::to_string);
                    }
                    package["version"] = toml_edit::value(version.to_string());
                    std::fs::write(&cargo_toml_path, doc.to_string())?;
                }
            }
        }
    }
    Ok(package_name)
}

/// Read `[package].version` from a member manifest. A member that neither
/// sets a version nor inherits via `version.workspace = true` is an error —
/// never guess a baseline.
fn read_manifest_version(manifest: &Path) -> anyhow::Result<Version> {
    let content = std::fs::read_to_string(manifest)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", manifest.display()))?;
    let doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", manifest.display()))?;
    let raw = doc
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no [package].version and does not inherit from the workspace",
                manifest.display()
            )
        })?;
    Version::parse(raw).map_err(|e| {
        anyhow::anyhow!(
            "{} [package].version is not valid semver: {e}",
            manifest.display()
        )
    })
}

/// Edit `[package].version` in a member manifest in place.
fn write_manifest_version(manifest: &Path, version: &Version) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(manifest)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", manifest.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", manifest.display()))?;
    doc["package"]["version"] = toml_edit::value(version.to_string());
    std::fs::write(manifest, doc.to_string())
        .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", manifest.display()))
}

/// Read the shared version from the root `[workspace.package]` table.
fn read_workspace_package_version(root_manifest: &Path) -> anyhow::Result<Version> {
    let content = std::fs::read_to_string(root_manifest)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", root_manifest.display()))?;
    let doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", root_manifest.display()))?;
    let raw = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no [workspace.package].version for members inheriting it",
                root_manifest.display()
            )
        })?;
    Version::parse(raw).map_err(|e| {
        anyhow::anyhow!(
            "{} [workspace.package].version is not valid semver: {e}",
            root_manifest.display()
        )
    })
}

/// Write the shared version into the root `[workspace.package]` table.
fn write_workspace_package_version(root_manifest: &Path, version: &Version) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(root_manifest)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", root_manifest.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", root_manifest.display()))?;
    doc["workspace"]["package"]["version"] = toml_edit::value(version.to_string());
    std::fs::write(root_manifest, doc.to_string())
        .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", root_manifest.display()))
}

/// Raise inter-member path-dependency version requirements that no longer
/// match a bumped member (plan goal G4). The floor is raised to the new
/// version — the only edit that keeps both manifests truthful and
/// `cargo build --locked` green; requirements are never widened. Returns
/// the manifests modified. Scans the root manifest and every member
/// manifest, not only bumped ones: an untouched member can still depend on
/// a bumped one.
fn raise_member_requirements(
    layout: &WorkspaceLayout,
    bumped: &[(String, Version)],
    repo_root: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    if bumped.is_empty() {
        return Ok(Vec::new());
    }
    let mut manifests = vec![repo_root.join("Cargo.toml")];
    manifests.extend(layout.members().iter().map(|m| m.manifest.clone()));

    let mut written = Vec::new();
    for manifest in manifests {
        if !manifest.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(mut doc) = content.parse::<toml_edit::DocumentMut>() else {
            continue;
        };
        let base = manifest.parent().unwrap_or(repo_root);
        let mut changed = false;
        for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(deps) = doc.get_mut(table_name).and_then(|t| t.as_table_like_mut()) else {
                continue;
            };
            let keys: Vec<String> = deps.iter().map(|(key, _)| key.to_string()).collect();
            for key in keys {
                let Some(entry) = deps.get_mut(&key) else {
                    continue;
                };
                let Some(rel) = entry.get("path").and_then(|p| p.as_str()) else {
                    continue; // registry dependency — no path, not inter-member
                };
                let dep_manifest = normalize_path(&base.join(rel).join("Cargo.toml"));
                // The bumped package living at that path, if any.
                let new_version = layout
                    .members()
                    .iter()
                    .find(|m| normalize_path(&m.manifest) == dep_manifest)
                    .and_then(|m| bumped.iter().find(|(name, _)| name == &m.name))
                    .map(|(_, version)| version.clone());
                let Some(new_version) = new_version else {
                    continue;
                };
                let Some(req_str) = entry
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                else {
                    continue; // path-only dependency: no requirement to check
                };
                let Ok(req) = semver::VersionReq::parse(&req_str) else {
                    continue;
                };
                if req.matches(&new_version) {
                    continue;
                }
                entry["version"] = toml_edit::value(new_version.to_string());
                changed = true;
                tracing::info!(
                    dependency = %key,
                    requirement = %req_str,
                    %new_version,
                    manifest = %manifest.display(),
                    "raised inter-member dependency floor to match bumped member"
                );
            }
        }
        if changed {
            std::fs::write(&manifest, doc.to_string())
                .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", manifest.display()))?;
            written.push(manifest);
        }
    }
    Ok(written)
}

/// Lexically normalize a path (resolving `.` and `..`) without touching the
/// filesystem, so dependency paths like `../alpha` compare equal to the
/// discovered member manifest paths.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Apply the `[versioning].lock_sync` policy after manifest writeback.
fn sync_lock(
    repo_root: &Path,
    packages: &[(String, Version)],
    lock_sync: LockSyncMode,
) -> anyhow::Result<()> {
    match lock_sync {
        LockSyncMode::Off => {}
        LockSyncMode::Patch => {
            sync_cargo_lock(&repo_root.join("Cargo.lock"), packages)?;
        }
        // Let Cargo regenerate the lockfile itself rather than editing it
        // by hand. Offline: the bump introduces no new dependencies, and the
        // daemon must never block on the network. Fall back to patching so
        // the version N-tuple stays consistent even when Cargo fails.
        LockSyncMode::Cargo => {
            if !repo_root.join("Cargo.toml").exists() {
                return Ok(()); // nothing for Cargo to resolve
            }
            let ok = std::process::Command::new("cargo")
                .args(["metadata", "--format-version", "1", "--offline"])
                .current_dir(repo_root)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                tracing::warn!(
                    "cargo metadata --offline failed; falling back to patching Cargo.lock"
                );
                sync_cargo_lock(&repo_root.join("Cargo.lock"), packages)?;
            }
            verify_lock_versions(&repo_root.join("Cargo.lock"), packages)?;
        }
    }
    Ok(())
}

/// Update `[[package]]` entries in a Cargo.lock for every `(name, version)`
/// pair in one pass. Existing lockfiles are treated as an invariant: parse,
/// package lookup, and write failures abort writeback instead of committing a
/// manifest/lock mismatch.
fn sync_cargo_lock(lock_path: &Path, packages: &[(String, Version)]) -> anyhow::Result<()> {
    if packages.is_empty() || !lock_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(lock_path)?;
    let mut doc = content.parse::<toml_edit::DocumentMut>()?;
    let mut changed = false;
    let mut found = std::collections::BTreeSet::new();
    if let Some(entries) = doc
        .get_mut("package")
        .and_then(|p| p.as_array_of_tables_mut())
    {
        for pkg in entries.iter_mut() {
            let name = pkg.get("name").and_then(|n| n.as_str()).map(str::to_string);
            let Some(new_version) = name
                .as_deref()
                .and_then(|name| packages.iter().find(|(n, _)| n == name))
                .map(|(_, version)| version)
            else {
                continue;
            };
            pkg["version"] = toml_edit::value(new_version.to_string());
            if let Some(name) = name {
                found.insert(name);
            }
            changed = true;
        }
    }
    for (name, _) in packages {
        anyhow::ensure!(
            found.contains(name),
            "Cargo.lock has no package entry for {name}"
        );
    }
    if changed {
        std::fs::write(lock_path, doc.to_string())?;
    }
    verify_lock_versions(lock_path, packages)
}

fn verify_lock_versions(lock_path: &Path, packages: &[(String, Version)]) -> anyhow::Result<()> {
    if packages.is_empty() || !lock_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(lock_path)?;
    let doc = content.parse::<toml_edit::DocumentMut>()?;
    let entries = doc
        .get("package")
        .and_then(|p| p.as_array_of_tables())
        .ok_or_else(|| anyhow::anyhow!("{} has no [[package]] entries", lock_path.display()))?;
    for (name, version) in packages {
        let expected = version.to_string();
        let matches = entries.iter().any(|pkg| {
            pkg.get("name").and_then(|v| v.as_str()) == Some(name.as_str())
                && pkg.get("version").and_then(|v| v.as_str()) == Some(expected.as_str())
        });
        anyhow::ensure!(
            matches,
            "{} is missing {name} v{version}",
            lock_path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// A workspace fixture: root crate at `0.2.0` (+ VERSION + lock) with
    /// `crates/alpha` and `crates/beta` at `0.1.0` and `crates/gamma`
    /// inheriting `[workspace.package].version = "0.3.0"`. beta carries a
    /// path+version dependency on alpha.
    struct WorkspaceFixture {
        dir: tempfile::TempDir,
    }

    impl WorkspaceFixture {
        fn new(virtual_root: bool) -> Self {
            let dir = tempdir().expect("tempdir");
            let root = dir.path();
            let mut manifest = String::new();
            if !virtual_root {
                manifest.push_str("[package]\nname = \"root-crate\"\nversion = \"0.2.0\"\n\n");
            }
            manifest.push_str(
                "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nversion = \"0.3.0\"\n",
            );
            std::fs::write(root.join("Cargo.toml"), manifest).expect("root manifest");
            if !virtual_root {
                std::fs::write(root.join("VERSION"), "0.2.0").expect("VERSION");
            }

            for (name, version) in [("alpha", "0.1.0"), ("beta", "0.1.0")] {
                let member = root.join(format!("crates/{name}"));
                std::fs::create_dir_all(&member).expect("member dir");
                let mut content =
                    format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n");
                if name == "beta" {
                    content.push_str(
                        "\n[dependencies]\nalpha = { path = \"../alpha\", version = \"0.1.0\" }\n",
                    );
                }
                std::fs::write(member.join("Cargo.toml"), content).expect("member manifest");
            }
            let gamma = root.join("crates/gamma");
            std::fs::create_dir_all(&gamma).expect("gamma dir");
            std::fs::write(
                gamma.join("Cargo.toml"),
                "[package]\nname = \"gamma\"\nversion.workspace = true\n",
            )
            .expect("gamma manifest");

            let mut lock =
                String::from("# This file is automatically @generated by Cargo.\nversion = 4\n");
            for (name, version) in [
                ("root-crate", "0.2.0"),
                ("alpha", "0.1.0"),
                ("beta", "0.1.0"),
                ("gamma", "0.3.0"),
            ] {
                lock.push_str(&format!(
                    "\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n"
                ));
            }
            std::fs::write(root.join("Cargo.lock"), lock).expect("Cargo.lock");

            Self { dir }
        }

        fn root(&self) -> &Path {
            self.dir.path()
        }

        fn layout(&self) -> WorkspaceLayout {
            WorkspaceLayout::discover(self.root()).expect("discover")
        }

        fn read(&self, rel: &str) -> String {
            std::fs::read_to_string(self.root().join(rel)).expect("read")
        }

        fn manifest_version(&self, rel: &str) -> Version {
            read_manifest_version(&self.root().join(rel)).expect("member version")
        }

        fn lock_version(&self, name: &str) -> Option<String> {
            let content = self.read("Cargo.lock");
            let doc = content
                .parse::<toml_edit::DocumentMut>()
                .expect("lock parses");
            doc.get("package")
                .and_then(|p| p.as_array_of_tables())
                .and_then(|entries| {
                    entries.iter().find_map(|pkg| {
                        (pkg.get("name").and_then(|n| n.as_str()) == Some(name))
                            .then(|| pkg.get("version").and_then(|v| v.as_str()))
                            .flatten()
                            .map(str::to_string)
                    })
                })
        }
    }

    fn touch(rel: &Path) -> Vec<PathBuf> {
        vec![rel.to_path_buf()]
    }

    #[test]
    fn touched_member_only_bumps_member_not_root() {
        let fixture = WorkspaceFixture::new(false);
        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        let outcome = save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert_eq!(
            fixture.manifest_version("crates/alpha/Cargo.toml"),
            Version::new(0, 1, 1)
        );
        assert_eq!(
            fixture.read("VERSION"),
            "0.2.0",
            "root VERSION must not move"
        );
        assert_eq!(
            fixture.manifest_version("crates/beta/Cargo.toml"),
            Version::new(0, 1, 0)
        );
        assert!(outcome.bumped.iter().any(|(n, _)| n == "alpha"));
        assert!(!outcome.bumped.iter().any(|(n, _)| n == "root-crate"));
    }

    #[test]
    fn touched_root_paths_bump_root_not_members() {
        let fixture = WorkspaceFixture::new(false);
        let cluster = touch(&fixture.root().join("src/main.rs"));
        save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert_eq!(fixture.read("VERSION"), "0.2.1");
        assert_eq!(
            fixture.manifest_version("Cargo.toml"),
            Version::new(0, 2, 1)
        );
        assert_eq!(
            fixture.manifest_version("crates/alpha/Cargo.toml"),
            Version::new(0, 1, 0)
        );
    }

    #[test]
    fn touched_cross_member_bumps_all_touched() {
        let fixture = WorkspaceFixture::new(false);
        let cluster = vec![
            fixture.root().join("crates/alpha/src/lib.rs"),
            fixture.root().join("crates/beta/src/lib.rs"),
        ];
        save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Minor,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert_eq!(
            fixture.manifest_version("crates/alpha/Cargo.toml"),
            Version::new(0, 2, 0)
        );
        assert_eq!(
            fixture.manifest_version("crates/beta/Cargo.toml"),
            Version::new(0, 2, 0)
        );
        assert_eq!(
            fixture.read("VERSION"),
            "0.2.0",
            "root outside all touched paths"
        );
        assert_eq!(
            fixture.manifest_version("Cargo.toml"),
            Version::new(0, 2, 0)
        );
        // gamma was not in the cluster: shared workspace version untouched.
        let root = fixture.read("Cargo.toml");
        assert!(root.contains("[workspace.package]\nversion = \"0.3.0\""));
    }

    #[test]
    fn workspace_lock_consistent_after_every_bump() {
        let fixture = WorkspaceFixture::new(false);
        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert_eq!(fixture.lock_version("alpha").as_deref(), Some("0.1.1"));
        assert_eq!(fixture.lock_version("beta").as_deref(), Some("0.1.0"));
        assert_eq!(fixture.lock_version("root-crate").as_deref(), Some("0.2.0"));
    }

    #[test]
    fn inherited_version_written_at_root_once() {
        let fixture = WorkspaceFixture::new(false);
        let cluster = touch(&fixture.root().join("crates/gamma/src/lib.rs"));
        let outcome = save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        let root = fixture.read("Cargo.toml");
        assert!(
            root.contains("[workspace.package]\nversion = \"0.3.1\""),
            "{root}"
        );
        // The inheriting member manifest is never written.
        let gamma = fixture.read("crates/gamma/Cargo.toml");
        assert_eq!(
            gamma,
            "[package]\nname = \"gamma\"\nversion.workspace = true\n"
        );
        assert!(outcome
            .bumped
            .iter()
            .any(|(n, v)| n == "gamma" && v == &Version::new(0, 3, 1)));
        assert_eq!(fixture.lock_version("gamma").as_deref(), Some("0.3.1"));
    }

    #[test]
    fn inter_member_requirement_stays_satisfiable() {
        let fixture = WorkspaceFixture::new(false);
        // Minor bump on alpha: beta's `version = "0.1.0"` caret requirement
        // no longer matches 0.2.0 and must be raised, not widened.
        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        let outcome = save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Minor,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        let beta = fixture.read("crates/beta/Cargo.toml");
        assert!(
            beta.contains("alpha = { path = \"../alpha\", version = \"0.2.0\" }"),
            "requirement floor must follow the bumped member:\n{beta}"
        );
        // beta itself was not bumped — only its requirement moved.
        assert!(beta.contains("version = \"0.1.0\""), "{beta}");
        assert!(outcome
            .written_paths
            .contains(&fixture.root().join("crates/beta/Cargo.toml")));
    }

    #[test]
    fn satisfied_inter_member_requirement_is_untouched() {
        let fixture = WorkspaceFixture::new(false);
        // Patch bump on alpha to 0.1.1: caret "0.1.0" still matches.
        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        let outcome = save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        let beta = fixture.read("crates/beta/Cargo.toml");
        assert!(beta.contains("version = \"0.1.0\" }"), "unchanged:\n{beta}");
        assert!(!outcome
            .written_paths
            .contains(&fixture.root().join("crates/beta/Cargo.toml")));
    }

    #[test]
    fn lockstep_bumps_everything() {
        let fixture = WorkspaceFixture::new(false);
        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Lockstep,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert_eq!(fixture.read("VERSION"), "0.2.1");
        assert_eq!(
            fixture.manifest_version("crates/alpha/Cargo.toml"),
            Version::new(0, 1, 1)
        );
        assert_eq!(
            fixture.manifest_version("crates/beta/Cargo.toml"),
            Version::new(0, 1, 1)
        );
        let root = fixture.read("Cargo.toml");
        assert!(root.contains("[workspace.package]\nversion = \"0.3.1\""));
        assert_eq!(fixture.lock_version("gamma").as_deref(), Some("0.3.1"));
    }

    #[test]
    fn virtual_workspace_has_no_root_bump() {
        let fixture = WorkspaceFixture::new(true);
        // A path outside every member subtree: no root exists to bump.
        let cluster = touch(&fixture.root().join("README.md"));
        let outcome = save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert!(!fixture.root().join("VERSION").exists());
        assert!(outcome.bumped.is_empty());

        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert_eq!(
            fixture.manifest_version("crates/alpha/Cargo.toml"),
            Version::new(0, 1, 1)
        );
        assert!(!fixture.root().join("VERSION").exists());
    }

    #[test]
    fn root_only_policy_ignores_members() {
        let fixture = WorkspaceFixture::new(false);
        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        let outcome = save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::RootOnly,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert_eq!(fixture.read("VERSION"), "0.2.1");
        assert_eq!(
            fixture.manifest_version("crates/alpha/Cargo.toml"),
            Version::new(0, 1, 0)
        );
        assert_eq!(fixture.lock_version("root-crate").as_deref(), Some("0.2.1"));
        assert_eq!(fixture.lock_version("alpha").as_deref(), Some("0.1.0"));
        assert!(outcome.bumped.iter().all(|(n, _)| n == "root-crate"));
    }

    #[test]
    fn bump_none_writes_nothing() {
        let fixture = WorkspaceFixture::new(false);
        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        let outcome = save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::None,
            &cluster,
            fixture.root(),
            LockSyncMode::Patch,
        )
        .expect("save");
        assert!(
            outcome.written_paths.is_empty(),
            "{:?}",
            outcome.written_paths
        );
        assert!(outcome.bumped.is_empty());
        assert_eq!(fixture.read("VERSION"), "0.2.0");
        assert_eq!(
            fixture.manifest_version("crates/alpha/Cargo.toml"),
            Version::new(0, 1, 0)
        );
    }

    #[test]
    fn lock_sync_off_leaves_lock_untouched() {
        let fixture = WorkspaceFixture::new(false);
        let lock_before = fixture.read("Cargo.lock");
        let cluster = touch(&fixture.root().join("crates/alpha/src/lib.rs"));
        save_workspace_version(
            &fixture.layout(),
            WorkspacePolicy::Touched,
            Bump::Patch,
            &cluster,
            fixture.root(),
            LockSyncMode::Off,
        )
        .expect("save");
        assert_eq!(fixture.read("Cargo.lock"), lock_before);
        assert_eq!(
            fixture.manifest_version("crates/alpha/Cargo.toml"),
            Version::new(0, 1, 1)
        );
    }

    #[test]
    fn existing_unparseable_lock_aborts_writeback() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("VERSION"), "0.1.0").unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("Cargo.lock"), "not valid = [").unwrap();

        let error = save_version(
            &dir.path().join("VERSION"),
            &Version::new(0, 1, 1),
            LockSyncMode::Patch,
        )
        .expect_err("invalid existing lock must abort");

        assert!(error.to_string().contains("TOML"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("VERSION")).unwrap(),
            "0.1.0"
        );
        assert!(std::fs::read_to_string(dir.path().join("Cargo.toml"))
            .unwrap()
            .contains("version = \"0.1.0\""));
    }
}
