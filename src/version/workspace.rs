//! Workspace layout discovery for multi-member Cargo repositories.
//!
//! Milestone W0 of `docs/planning/WORKSPACE_VERSION_BUMPING_PLAN.md`: pure
//! discovery and layout types — nothing calls this yet, so there is no
//! behavior change. W1 will route version writeback through these layouts so
//! a bump lands on the member crates a cluster actually touched instead of
//! always moving the root.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A workspace member crate resolved from the root manifest's
/// `[workspace].members` entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The member's `[package].name`.
    pub name: String,
    /// Absolute path of the member's `Cargo.toml`.
    pub manifest: PathBuf,
    /// `true` when the member declares `version.workspace = true`; its
    /// version then lives once in the root's `[workspace.package]` table
    /// and must never be written into the member manifest itself.
    pub inherits_version: bool,
}

/// Cargo workspace topology under a project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceLayout {
    /// Root crate (`[package]`) plus `[workspace]` members — e.g. kaptaind
    /// itself with `crates/kaptaind-diff`.
    RootCrate { members: Vec<Member> },
    /// `[workspace]` table only, no root `[package]`.
    Virtual { members: Vec<Member> },
    /// No `[workspace]` table — the single-crate layout today's writeback
    /// already handles.
    Single,
}

impl WorkspaceLayout {
    /// Discover the workspace topology under `project_root`.
    ///
    /// Returns `Single` when the root has no `Cargo.toml` or the manifest
    /// has no `[workspace]` table. Errors — never guesses — when the root
    /// manifest is unparseable, an explicitly declared member directory has
    /// no `Cargo.toml`, or a resolved member manifest is unparseable or
    /// lacks `[package].name`.
    ///
    /// Member entries support explicit relative paths (`"crates/alpha"`)
    /// and glob patterns (`"crates/*"`); `[workspace].exclude` entries
    /// remove directories again. Glob-matched directories without a
    /// `Cargo.toml` are skipped silently (matching Cargo), as are members
    /// whose manifest carries a `package.workspace` re-rooting key — those
    /// belong to a different workspace root and are logged, not guessed.
    pub fn discover(project_root: &Path) -> anyhow::Result<Self> {
        let root_manifest = project_root.join("Cargo.toml");
        if !root_manifest.exists() {
            return Ok(Self::Single);
        }
        let content = std::fs::read_to_string(&root_manifest)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", root_manifest.display()))?;
        let doc = content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", root_manifest.display()))?;
        let Some(workspace) = doc.get("workspace") else {
            return Ok(Self::Single);
        };

        let member_dirs = resolve_dirs(project_root, workspace.get("members"), EntryRole::Member)?;
        let exclude_dirs =
            resolve_dirs(project_root, workspace.get("exclude"), EntryRole::Exclude)?;

        let mut members = Vec::new();
        for dir in member_dirs {
            if exclude_dirs.contains(&dir) {
                continue;
            }
            let manifest = dir.join("Cargo.toml");
            // A `"."` member entry re-lists the root crate itself. It is
            // represented by the `RootCrate` root clause — keeping it as a
            // member too would bump the root manifest without `VERSION`.
            if manifest == root_manifest {
                continue;
            }
            if let Some(member) = read_member(&manifest)? {
                members.push(member);
            }
        }

        Ok(if doc.get("package").is_some() {
            Self::RootCrate { members }
        } else {
            Self::Virtual { members }
        })
    }

    /// All resolved member crates (`Single` has none).
    pub fn members(&self) -> &[Member] {
        match self {
            Self::RootCrate { members } | Self::Virtual { members } => members,
            Self::Single => &[],
        }
    }

    /// The one member whose subtree contains every cluster path, when the
    /// cluster is dominated by exactly that member — used to scope the
    /// conventional-commit subject (W2: `feat(kaptaind-diff): …`). Returns
    /// `None` when any path falls outside all member subtrees (the root
    /// participates) or the cluster spans members.
    pub fn dominant_member<'a>(
        layout: &'a WorkspaceLayout,
        cluster_paths: &[PathBuf],
        repo_root: &Path,
    ) -> Option<&'a Member> {
        let members = layout.members();
        if members.is_empty() || cluster_paths.is_empty() {
            return None;
        }
        let mut dominant: Option<&Member> = None;
        for path in cluster_paths {
            let abs = repo_root.join(path);
            // Nested member subtrees (exotic): the longest manifest path is
            // the most specific containing member.
            let containing = members
                .iter()
                .filter(|m| abs.starts_with(m.manifest.parent().unwrap_or(repo_root)))
                .max_by_key(|m| m.manifest.as_os_str().len())?;
            match dominant {
                Some(d) if d.name != containing.name => return None,
                Some(_) => {}
                None => dominant = Some(containing),
            }
        }
        dominant
    }
}

/// Whether a `[workspace]` string-array entry names members or exclusions.
/// Only the error behavior differs: an explicitly declared member directory
/// without a manifest is a config error worth surfacing, while exclusions
/// and glob matches are best-effort filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryRole {
    Member,
    Exclude,
}

/// Expand one `[workspace]` string array (`members` / `exclude`) into the
/// set of directories it names, deduplicated and sorted for deterministic
/// output. Entries containing glob metacharacters are expanded relative to
/// `project_root` (same metacharacter set as `.kaptainignore`); other
/// entries are exact relative paths.
fn resolve_dirs(
    project_root: &Path,
    value: Option<&toml_edit::Item>,
    role: EntryRole,
) -> anyhow::Result<BTreeSet<PathBuf>> {
    let mut dirs = BTreeSet::new();
    let Some(array) = value.and_then(|v| v.as_array()) else {
        if value.is_some() {
            tracing::warn!(
                component = module_path!(),
                "[workspace] members/exclude is not a string array; ignoring it"
            );
        }
        return Ok(dirs);
    };
    for item in array {
        let Some(entry) = item.as_str() else {
            tracing::warn!(
                component = module_path!(),
                "non-string [workspace] entry ignored: {item}"
            );
            continue;
        };
        if entry.chars().any(|c| matches!(c, '*' | '?' | '[' | '{')) {
            let pattern = project_root.join(entry);
            let Some(pattern) = pattern.to_str() else {
                tracing::warn!(component = module_path!(), %entry, "workspace member pattern is not valid UTF-8; ignored");
                continue;
            };
            for path in glob::glob(pattern)
                .map_err(|e| anyhow::anyhow!("invalid [workspace] pattern '{entry}': {e}"))?
                .flatten()
            {
                if path.is_dir() && path.join("Cargo.toml").exists() {
                    dirs.insert(path);
                }
            }
        } else {
            let dir = project_root.join(entry);
            if role == EntryRole::Member && !dir.join("Cargo.toml").exists() {
                anyhow::bail!(
                    "declared workspace member '{entry}' has no Cargo.toml under {}",
                    project_root.display()
                );
            }
            dirs.insert(dir);
        }
    }
    Ok(dirs)
}

/// Parse a member manifest into a `Member`. Returns `Ok(None)` — with a
/// warning — when the manifest re-roots via `package.workspace`: that crate
/// belongs to a different workspace and must not be guessed at.
fn read_member(manifest: &Path) -> anyhow::Result<Option<Member>> {
    let content = std::fs::read_to_string(manifest)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", manifest.display()))?;
    let doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", manifest.display()))?;
    let package = doc
        .get("package")
        .ok_or_else(|| anyhow::anyhow!("{} has no [package] table", manifest.display()))?;
    if package.get("workspace").is_some() {
        tracing::warn!(
            component = module_path!(),
            "{} declares package.workspace; skipping — it belongs to a different workspace root",
            manifest.display()
        );
        return Ok(None);
    }
    let name = package
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| anyhow::anyhow!("{} [package] has no name", manifest.display()))?
        .to_string();
    let inherits_version = package
        .get("version")
        .and_then(|v| v.as_table_like())
        .and_then(|t| t.get("workspace"))
        .and_then(|w| w.as_bool())
        .unwrap_or(false);
    Ok(Some(Member {
        name,
        manifest: manifest.to_path_buf(),
        inherits_version,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::tempdir;

    /// Write a root manifest with the given `[workspace]` body (or none).
    fn write_root(root: &Path, package: bool, workspace: Option<&str>) {
        let mut content = String::new();
        if package {
            content.push_str("[package]\nname = \"root-crate\"\nversion = \"1.0.0\"\n\n");
        }
        if let Some(ws) = workspace {
            content.push_str("[workspace]\n");
            content.push_str(ws);
        }
        std::fs::write(root.join("Cargo.toml"), content).expect("root manifest");
    }

    /// Create `root/<rel>/Cargo.toml` for a crate named `name`, with the
    /// given extra `[package]` lines (e.g. `version = "0.1.0"`).
    fn write_crate(root: &Path, rel: &str, name: &str, extra_package_lines: &str) {
        let dir = root.join(rel);
        std::fs::create_dir_all(&dir).expect("member dir");
        std::fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\n{extra_package_lines}\n"),
        )
        .expect("member manifest");
    }

    #[test]
    fn missing_manifest_is_single() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(
            WorkspaceLayout::discover(dir.path()).expect("discover"),
            WorkspaceLayout::Single
        );
    }

    #[test]
    fn single_crate_without_workspace_table_is_single() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, None);
        assert_eq!(
            WorkspaceLayout::discover(dir.path()).expect("discover"),
            WorkspaceLayout::Single
        );
    }

    #[test]
    fn root_crate_with_explicit_members() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, Some("members = [\"crates/alpha\"]\n"));
        write_crate(dir.path(), "crates/alpha", "alpha", "version = \"0.1.0\"");
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
        let WorkspaceLayout::RootCrate { members } = layout else {
            panic!("expected RootCrate, got {layout:?}");
        };
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "alpha");
        assert_eq!(
            members[0].manifest,
            dir.path().join("crates/alpha/Cargo.toml")
        );
        assert!(!members[0].inherits_version);
    }

    #[test]
    fn virtual_workspace_has_no_root_package() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), false, Some("members = [\"crates/alpha\"]\n"));
        write_crate(dir.path(), "crates/alpha", "alpha", "version = \"0.1.0\"");
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
        let WorkspaceLayout::Virtual { members } = layout else {
            panic!("expected Virtual, got {layout:?}");
        };
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "alpha");
    }

    #[test]
    fn glob_members_resolve_and_skip_dirs_without_manifests() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, Some("members = [\"crates/*\"]\n"));
        write_crate(dir.path(), "crates/beta", "beta", "version = \"0.1.0\"");
        write_crate(dir.path(), "crates/alpha", "alpha", "version = \"0.1.0\"");
        // A directory under crates/ that is not a crate: no manifest.
        std::fs::create_dir_all(dir.path().join("crates/docs")).expect("docs dir");
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
        let names: Vec<&str> = layout.members().iter().map(|m| m.name.as_str()).collect();
        // Sorted by manifest path for deterministic output.
        assert_eq!(names, ["alpha", "beta"]);
    }

    #[test]
    fn exclude_removes_members() {
        let dir = tempdir().expect("tempdir");
        write_root(
            dir.path(),
            true,
            Some("members = [\"crates/*\"]\nexclude = [\"crates/beta\"]\n"),
        );
        write_crate(dir.path(), "crates/alpha", "alpha", "version = \"0.1.0\"");
        write_crate(dir.path(), "crates/beta", "beta", "version = \"0.1.0\"");
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
        let names: Vec<&str> = layout.members().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["alpha"]);
    }

    #[test]
    fn inherited_version_detected() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, Some("members = [\"crates/alpha\"]\n"));
        write_crate(
            dir.path(),
            "crates/alpha",
            "alpha",
            "version.workspace = true",
        );
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
        assert!(layout.members()[0].inherits_version);
    }

    #[test]
    fn declared_member_without_manifest_errors() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, Some("members = [\"crates/ghost\"]\n"));
        let err = WorkspaceLayout::discover(dir.path()).expect_err("must not guess");
        assert!(err.to_string().contains("crates/ghost"));
    }

    #[test]
    fn unparseable_root_manifest_errors() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("Cargo.toml"), "not = [valid\n").expect("manifest");
        let err = WorkspaceLayout::discover(dir.path()).expect_err("must error");
        assert!(err.to_string().contains("failed to parse"));
    }

    #[test]
    fn unparseable_member_manifest_errors() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, Some("members = [\"crates/*\"]\n"));
        let bad = dir.path().join("crates/bad");
        std::fs::create_dir_all(&bad).expect("member dir");
        std::fs::write(bad.join("Cargo.toml"), "[[[broken").expect("member manifest");
        let err = WorkspaceLayout::discover(dir.path()).expect_err("must error");
        assert!(err.to_string().contains("failed to parse"));
    }

    #[test]
    fn member_without_name_errors() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, Some("members = [\"crates/alpha\"]\n"));
        let member = dir.path().join("crates/alpha");
        std::fs::create_dir_all(&member).expect("member dir");
        std::fs::write(
            member.join("Cargo.toml"),
            "[package]\nversion = \"0.1.0\"\n",
        )
        .expect("member manifest");
        let err = WorkspaceLayout::discover(dir.path()).expect_err("must error");
        assert!(err.to_string().contains("no name"));
    }

    #[test]
    fn package_workspace_member_is_skipped() {
        let dir = tempdir().expect("tempdir");
        write_root(
            dir.path(),
            true,
            Some("members = [\"crates/alpha\", \"crates/foreign\"]\n"),
        );
        write_crate(dir.path(), "crates/alpha", "alpha", "version = \"0.1.0\"");
        write_crate(
            dir.path(),
            "crates/foreign",
            "foreign",
            "version = \"0.1.0\"\nworkspace = \"../elsewhere\"",
        );
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
        let names: Vec<&str> = layout.members().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["alpha"]);
    }

    #[test]
    fn root_member_dot_is_not_duplicated() {
        // kaptaind's own layout: `members = [".", "crates/*"]`. The root
        // crate is the `RootCrate` clause, not a member.
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, Some("members = [\".\", \"crates/*\"]\n"));
        write_crate(dir.path(), "crates/alpha", "alpha", "version = \"0.1.0\"");
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
        let WorkspaceLayout::RootCrate { members } = layout else {
            panic!("expected RootCrate, got {layout:?}");
        };
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["alpha"]);
    }

    #[test]
    fn dominant_member_detects_single_member_clusters() {
        let dir = tempdir().expect("tempdir");
        write_root(dir.path(), true, Some("members = [\"crates/*\"]\n"));
        write_crate(dir.path(), "crates/alpha", "alpha", "version = \"0.1.0\"");
        write_crate(dir.path(), "crates/beta", "beta", "version = \"0.1.0\"");
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");

        // All paths inside one member subtree.
        let paths = vec![
            dir.path().join("crates/alpha/src/lib.rs"),
            dir.path().join("crates/alpha/Cargo.toml"),
        ];
        assert_eq!(
            WorkspaceLayout::dominant_member(&layout, &paths, dir.path()).map(|m| m.name.as_str()),
            Some("alpha")
        );

        // Spanning two members: no dominance.
        let paths = vec![
            dir.path().join("crates/alpha/src/lib.rs"),
            dir.path().join("crates/beta/src/lib.rs"),
        ];
        assert!(WorkspaceLayout::dominant_member(&layout, &paths, dir.path()).is_none());

        // A path outside member subtrees: the root participates.
        let paths = vec![
            dir.path().join("crates/alpha/src/lib.rs"),
            dir.path().join("src/main.rs"),
        ];
        assert!(WorkspaceLayout::dominant_member(&layout, &paths, dir.path()).is_none());

        // Relative cluster paths are resolved against the repo root.
        let paths = vec![PathBuf::from("crates/beta/src/lib.rs")];
        assert_eq!(
            WorkspaceLayout::dominant_member(&layout, &paths, dir.path()).map(|m| m.name.as_str()),
            Some("beta")
        );
    }

    #[test]
    fn duplicate_and_overlapping_entries_dedupe() {
        let dir = tempdir().expect("tempdir");
        write_root(
            dir.path(),
            true,
            Some("members = [\"crates/*\", \"crates/alpha\", \"crates/alpha\"]\n"),
        );
        write_crate(dir.path(), "crates/alpha", "alpha", "version = \"0.1.0\"");
        write_crate(dir.path(), "crates/beta", "beta", "version = \"0.1.0\"");
        let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
        let names: Vec<&str> = layout.members().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta"]);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Glob discovery must return exactly the hand-computed set of
        /// created crates minus excluded ones — sorted, deduplicated, and
        /// pointing at manifests that exist.
        #[test]
        fn glob_discovery_matches_hand_computed_set(
            crates in prop::collection::hash_set("[a-z][a-z0-9]{0,7}", 0..6),
            excluded in prop::collection::hash_set("[a-z][a-z0-9]{0,7}", 0..6),
        ) {
            let dir = tempdir().expect("tempdir");
            let exclude_body: String = excluded
                .iter()
                .map(|e| format!("\"crates/{e}\", "))
                .collect();
            let workspace = format!("members = [\"crates/*\"]\nexclude = [{exclude_body}]\n");
            write_root(dir.path(), true, Some(&workspace));
            for name in &crates {
                write_crate(dir.path(), &format!("crates/{name}"), name, "version = \"0.1.0\"");
            }

            let layout = WorkspaceLayout::discover(dir.path()).expect("discover");
            let mut expected: Vec<&String> = crates.difference(&excluded).collect();
            expected.sort();
            let found: Vec<&str> = layout.members().iter().map(|m| m.name.as_str()).collect();
            prop_assert_eq!(found.len(), expected.len());
            prop_assert!(expected.iter().all(|e| found.contains(&e.as_str())));
            for member in layout.members() {
                prop_assert!(member.manifest.exists());
            }
            // Deterministic order: manifests sorted lexicographically.
            let mut sorted = found.clone();
            sorted.sort();
            prop_assert_eq!(found, sorted);
        }

        /// Discovery must never panic on arbitrary root manifest content,
        /// however malformed — only return Ok or Err.
        #[test]
        fn discovery_never_panics_on_arbitrary_input(input in "\\PC{0,200}") {
            let dir = tempdir().expect("tempdir");
            std::fs::write(dir.path().join("Cargo.toml"), input).expect("manifest");
            let _ = WorkspaceLayout::discover(dir.path());
        }
    }
}
