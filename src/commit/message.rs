//! Deterministic commit message formatting.
//!
//! Extracted from the scheduler so both the daemon pipeline and
//! `kaptaind --dry-run` render the exact same message for a given cluster.
//!
//! Subjects follow conventional commits (`type(scope): description`, D2):
//! the type names the change class derived from the diff analysis, the scope
//! the dominant top-level directory of the cluster paths, and the description
//! lists the primary paths. The body keeps the kaptaind scorecard block.

use crate::cluster::engine::Cluster;
use crate::diff::DiffAnalysis;
use crate::version::Bump;
use crate::weight::WeightResult;
use semver::Version;
use std::collections::BTreeSet;
use std::path::Path;

/// Hard cap for the subject line (D2).
const SUBJECT_LIMIT: usize = 72;

/// Change class of a cluster, derived deterministically from the diff
/// analysis and the cluster paths (first match wins, in [`classify`] order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeClass {
    /// `api_breaking` — semver Major (`fix!` / `feat!`).
    Breaking,
    /// `api_added` — semver Minor (`feat`).
    Feature,
    /// A dependency manifest's dependency sections changed (`build(deps)`).
    Deps,
    /// Every touched path is documentation (`docs`).
    Docs,
    /// Every touched path is a test (`test`).
    Tests,
    /// Anything else in a bumping commit (`fix`).
    Fix,
}

/// Classify a cluster from the signals already computed by the pipeline.
fn classify(diff: &DiffAnalysis, paths: &[&Path]) -> ChangeClass {
    if diff.api_breaking {
        ChangeClass::Breaking
    } else if diff.api_added {
        ChangeClass::Feature
    } else if diff.dependency_manifests > 0 {
        ChangeClass::Deps
    } else if !paths.is_empty() && paths.iter().all(|p| is_docs_path(p)) {
        ChangeClass::Docs
    } else if !paths.is_empty() && paths.iter().all(|p| is_test_path(p)) {
        ChangeClass::Tests
    } else {
        ChangeClass::Fix
    }
}

/// Documentation extensions — the same set the scheduler's docs-only
/// classification uses for `[test] command_on = "code_only"` (C5).
fn is_docs_path(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    matches!(ext.as_deref(), Some("md" | "txt" | "rst" | "adoc"))
}

/// A path counts as a test when it lives under a `test(s)/` directory or its
/// file name follows a common test convention (`test_foo.py`, `foo_test.go`,
/// `foo.test.ts`, `foo.spec.js`, `foo_tests.rs`).
fn is_test_path(path: &Path) -> bool {
    if path.components().any(|c| {
        let part = c.as_os_str().to_string_lossy();
        part == "tests" || part == "test"
    }) {
        return true;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&name);
    stem.starts_with("test_")
        || stem.ends_with("_test")
        || stem.ends_with("_tests")
        || stem.ends_with(".test")
        || stem.ends_with(".spec")
}

/// Scope from the cluster paths: the single top-level directory shared by
/// every path, or `None` when paths are root-level or heterogeneous.
fn derive_scope(paths: &[&Path]) -> Option<String> {
    let first = paths.first()?;
    if first.components().count() < 2 {
        return None;
    }
    let dir = first.components().next()?.as_os_str().to_str()?;
    if !paths.iter().all(|p| {
        p.components().count() >= 2
            && p.components().next().and_then(|c| c.as_os_str().to_str()) == Some(dir)
    }) {
        return None;
    }
    let scope: String = dir
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    (!scope.is_empty()).then_some(scope)
}

/// Sorted, deduplicated base names of the cluster paths.
fn path_names(paths: &[&Path]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_else(|| p.to_str().unwrap_or("unknown"))
                .to_string()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Assemble `type(scope): description (path, path, +N more)`, truncating the
/// path list (never mid-UTF8-char) so the subject stays within
/// [`SUBJECT_LIMIT`]. The parenthesized list is appended last, so truncation
/// can never break conventional-commit parsing of the head.
fn build_subject(ty: &str, scope: Option<&str>, description: &str, names: &[String]) -> String {
    let prefix = match scope {
        Some(scope) => format!("{ty}({scope}): {description}"),
        None => format!("{ty}: {description}"),
    };
    if names.is_empty() {
        return prefix;
    }

    let full = format!("{prefix} ({})", names.join(", "));
    if full.len() <= SUBJECT_LIMIT {
        return full;
    }

    // Greedily keep as many leading names as fit, reserving room for the
    // `, +N more` suffix that accounts for the remainder.
    let mut kept: Vec<&str> = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let remaining = names.len() - index - 1;
        let suffix = if remaining > 0 {
            format!(", +{remaining} more")
        } else {
            String::new()
        };
        let list_len = kept.iter().map(|k| k.len() + 2).sum::<usize>() + name.len();
        let total = prefix.len() + 2 + list_len + suffix.len() + 1; // " (" list suffix ")"
        if total <= SUBJECT_LIMIT {
            kept.push(name);
        } else {
            break;
        }
    }

    if kept.is_empty() {
        // Even the first name does not fit: truncate it on a char boundary.
        let remaining = names.len() - 1;
        let suffix = if remaining > 0 {
            format!(", +{remaining} more")
        } else {
            String::new()
        };
        let budget = SUBJECT_LIMIT.saturating_sub(prefix.len() + 2 + suffix.len() + 1 + 2); // room for ".."
        if budget < 4 {
            return prefix;
        }
        let name = &names[0];
        let mut end = budget.min(name.len());
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        return format!("{prefix} ({}..{suffix})", &name[..end]);
    }

    let remaining = names.len() - kept.len();
    let suffix = if remaining > 0 {
        format!(", +{remaining} more")
    } else {
        String::new()
    };
    format!("{prefix} ({}{suffix})", kept.join(", "))
}

/// Unique cluster paths, sorted for deterministic output.
fn cluster_paths(cluster: &Cluster) -> Vec<&Path> {
    cluster
        .events
        .iter()
        .flat_map(|event| event.paths.iter().map(AsRef::as_ref))
        .collect::<BTreeSet<&Path>>()
        .into_iter()
        .collect()
}

fn api_summary(diff: &DiffAnalysis) -> &'static str {
    if diff.api_breaking {
        "breaking-api"
    } else if diff.api_added {
        "api-added"
    } else {
        "api-stable"
    }
}

fn agent_info(agent_event: &Option<crate::aoc::AgentEvent>) -> String {
    if let Some(agent) = agent_event {
        let model = agent.model.as_deref().unwrap_or("unknown");
        format!("; agent={model}")
    } else {
        String::new()
    }
}

/// Conventional-commit subject for a bumping commit: the change class
/// becomes the type, the dominant directory the scope.
fn bump_subject(cluster: &Cluster, diff: &DiffAnalysis) -> String {
    let paths = cluster_paths(cluster);
    let class = classify(diff, &paths);
    let (ty, description) = match class {
        ChangeClass::Breaking if diff.api_added => ("feat!", "change public API"),
        ChangeClass::Breaking => ("fix!", "change public API"),
        ChangeClass::Feature => ("feat", "extend public API"),
        ChangeClass::Deps => ("build", "update dependencies"),
        ChangeClass::Docs => ("docs", "update documentation"),
        ChangeClass::Tests => ("test", "update tests"),
        ChangeClass::Fix => ("fix", "apply code changes"),
    };
    // Dependency changes keep the conventional `build(deps)` scope; other
    // classes use the dominant directory, if any.
    let scope = if class == ChangeClass::Deps {
        Some("deps".to_string())
    } else {
        derive_scope(&paths)
    };
    build_subject(ty, scope.as_deref(), description, &path_names(&paths))
}

/// Render the deterministic kaptaind commit message for a cluster decision.
pub fn format_commit(
    cluster: &Cluster,
    diff: &DiffAnalysis,
    weight: &WeightResult,
    bump: Bump,
    version: &Version,
    agent_event: &Option<crate::aoc::AgentEvent>,
) -> String {
    let subject = bump_subject(cluster, diff);
    let body = format!(
        "kaptaind: {bump:?} -> v{version} [{}; paths={}; api_touches={}; deps={}; runtime={}; score={:.3}; cluster={}{}]",
        api_summary(diff),
        diff.touched_paths,
        diff.api_touches,
        diff.dependency_nodes,
        diff.runtime_paths,
        weight.score,
        cluster.id,
        agent_info(agent_event),
    );
    format!("{subject}\n\n{body}")
}

/// Render the deterministic non-bumping chore message used when
/// `[commit] require_bump = false` and the cluster scores below the patch
/// threshold (D1): the work is still captured, but VERSION, Cargo.toml and
/// Cargo.lock are left untouched.
///
/// The subject stays ≤72 chars and conventional-commit parseable, always
/// with the plain `chore:` type (the description names the change class);
/// the body keeps the same scorecard block as [`format_commit`].
pub fn format_chore_commit(
    cluster: &Cluster,
    diff: &DiffAnalysis,
    weight: &WeightResult,
    agent_event: &Option<crate::aoc::AgentEvent>,
) -> String {
    let paths = cluster_paths(cluster);
    let description = match classify(diff, &paths) {
        ChangeClass::Breaking | ChangeClass::Feature => "change public API",
        ChangeClass::Deps => "update dependencies",
        ChangeClass::Docs => "update documentation",
        ChangeClass::Tests => "update tests",
        ChangeClass::Fix => "apply changes",
    };
    let subject = build_subject("chore", None, description, &path_names(&paths));
    let body = format!(
        "kaptaind: no-bump [{}; paths={}; api_touches={}; deps={}; runtime={}; score={:.3}; cluster={}{}]",
        api_summary(diff),
        diff.touched_paths,
        diff.api_touches,
        diff.dependency_nodes,
        diff.runtime_paths,
        weight.score,
        cluster.id,
        agent_info(agent_event),
    );
    format!("{subject}\n\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::{FsEvent, FsEventKind};
    use chrono::Utc;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn sample_cluster() -> Cluster {
        cluster_with_paths(&["README.md"])
    }

    fn cluster_with_paths(paths: &[&str]) -> Cluster {
        let timestamp = Utc::now();
        Cluster {
            id: Uuid::new_v4(),
            started_at: timestamp,
            ended_at: timestamp,
            events: vec![FsEvent {
                paths: paths.iter().map(PathBuf::from).collect(),
                kind: FsEventKind::Modify,
                timestamp,
            }],
        }
    }

    fn weight(score: f32) -> WeightResult {
        WeightResult {
            score,
            api_breaking: false,
            api_added: false,
        }
    }

    #[test]
    fn chore_subject_is_conventional_and_short() {
        let cluster = sample_cluster();
        let diff = DiffAnalysis {
            touched_paths: 3,
            ..DiffAnalysis::default()
        };
        let weight = weight(0.042);

        let message = format_chore_commit(&cluster, &diff, &weight, &None);
        let subject = message.lines().next().expect("subject line");
        assert!(subject.starts_with("chore: "), "subject: {subject}");
        assert!(subject.len() <= 72, "subject too long: {subject}");
        assert!(message.contains("score=0.042"));
    }

    #[test]
    fn bump_subject_names_class_and_paths() {
        let cluster = cluster_with_paths(&["src/cli/main.rs", "src/cli/args.rs"]);
        let diff = DiffAnalysis {
            touched_paths: 2,
            ..DiffAnalysis::default()
        };
        let message = format_commit(
            &cluster,
            &diff,
            &weight(0.5),
            Bump::Patch,
            &Version::new(0, 1, 1),
            &None,
        );
        let subject = message.lines().next().expect("subject line");
        assert_eq!(subject, "fix(src): apply code changes (args.rs, main.rs)");
        assert!(message.contains("kaptaind: Patch -> v0.1.1 [api-stable; paths=2"));
    }

    /// D2 lint (finding #16): every generated subject, across a matrix of
    /// representative clusters and diffs, must parse as a conventional
    /// commit, fit in 72 chars, and carry a non-empty description.
    #[test]
    fn commit_message_lint() {
        let long = "crates/some-very-long-crate-name/src/a_really_extremely_long_module_file_name_that_keeps_going_on.rs";
        let cases: Vec<(&str, Cluster, DiffAnalysis)> = vec![
            (
                "docs-only",
                cluster_with_paths(&["README.md", "docs/guide.md"]),
                DiffAnalysis {
                    touched_paths: 2,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "deps-only",
                cluster_with_paths(&["Cargo.toml", "Cargo.lock"]),
                DiffAnalysis {
                    touched_paths: 2,
                    dependency_manifests: 1,
                    dependency_nodes: 3,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "tests-only",
                cluster_with_paths(&["tests/regressions.rs", "src/foo_test.rs"]),
                DiffAnalysis {
                    touched_paths: 2,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "api-breaking",
                cluster_with_paths(&["src/lib.rs"]),
                DiffAnalysis {
                    touched_paths: 1,
                    api_breaking: true,
                    api_touches: 2,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "api-added",
                cluster_with_paths(&["src/lib.rs"]),
                DiffAnalysis {
                    touched_paths: 1,
                    api_added: true,
                    api_touches: 1,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "mixed",
                cluster_with_paths(&["Cargo.toml", "install-scotia.sh", "README.md"]),
                DiffAnalysis {
                    touched_paths: 3,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "single-path",
                cluster_with_paths(&["src/main.rs"]),
                DiffAnalysis {
                    touched_paths: 1,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "many-paths",
                cluster_with_paths(&[
                    "src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs", "src/e.rs", "src/f.rs",
                    "src/g.rs", "src/h.rs", "src/i.rs", "src/j.rs",
                ]),
                DiffAnalysis {
                    touched_paths: 10,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "long-single-path",
                cluster_with_paths(&[long]),
                DiffAnalysis {
                    touched_paths: 1,
                    ..DiffAnalysis::default()
                },
            ),
            (
                "non-ascii-path",
                cluster_with_paths(&["docs/café-über-guide.md"]),
                DiffAnalysis {
                    touched_paths: 1,
                    ..DiffAnalysis::default()
                },
            ),
        ];

        for (name, cluster, diff) in cases {
            let bump = if diff.api_breaking {
                Bump::Major
            } else if diff.api_added {
                Bump::Minor
            } else {
                Bump::Patch
            };
            let messages = [
                format_commit(
                    &cluster,
                    &diff,
                    &weight(0.5),
                    bump,
                    &Version::new(1, 2, 3),
                    &None,
                ),
                format_chore_commit(&cluster, &diff, &weight(0.05), &None),
            ];
            for message in messages {
                let subject = message.lines().next().expect("subject line");
                assert!(
                    is_conventional_subject(subject),
                    "[{name}] not conventional-commit parseable: {subject}"
                );
                assert!(
                    subject.len() <= SUBJECT_LIMIT,
                    "[{name}] subject over {} chars ({}): {subject}",
                    SUBJECT_LIMIT,
                    subject.len()
                );
                assert!(
                    message.contains("\n\nkaptaind: "),
                    "[{name}] scorecard body missing: {message}"
                );
            }
        }
    }

    /// Minimal conventional-commit check: `type(scope)!: description` with a
    /// standard type, optional scope, and a non-empty description.
    fn is_conventional_subject(subject: &str) -> bool {
        const TYPES: &[&str] = &[
            "feat", "fix", "chore", "docs", "style", "refactor", "test", "build", "ci", "perf",
        ];
        let Some((head, description)) = subject.split_once(": ") else {
            return false;
        };
        if description.trim().is_empty() {
            return false;
        }
        let (ty, scope) = match head.split_once('(') {
            Some((ty, rest)) => match rest.strip_suffix(')') {
                Some(scope) => (ty, Some(scope)),
                None => return false,
            },
            None => (head, None),
        };
        let ty = ty.strip_suffix('!').unwrap_or(ty);
        TYPES.contains(&ty)
            && scope.is_none_or(|s| {
                !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            })
    }
}
