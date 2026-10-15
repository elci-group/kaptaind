//! Repository census. Three histories stay separate:
//! Kaptaind-labelled commits, Kaptaind-era commits, and Kaptaind-observed analyses.

use super::gitio::{self, RemoteRepo};
use super::schema::{
    CensusTotals, ClassCounts, EstateCensus, InaccessibleRepository, LanguageShare,
    RepositoryCensus, TrainingPermission, CENSUS_TOKENS_PER_CANDIDATE,
};
use super::{label, schema};
use anyhow::Context;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const LABEL_GREP: &str = r"kaptaind: (Major|Minor|Patch|None) ->|kaptaind: no-bump|^Kaptaind-";

#[derive(Debug, Clone)]
pub struct CensusOptions {
    pub roots: Vec<PathBuf>,
    pub org: Option<String>,
    pub max_depth: usize,
    pub first_party_orgs: Vec<String>,
    pub fallback_repo: PathBuf,
}

pub fn census_estate(options: &CensusOptions) -> anyhow::Result<EstateCensus> {
    let mut inaccessible = Vec::new();
    let remotes = match &options.org {
        Some(org) => match gitio::list_github_org(org) {
            Ok(repos) => repos,
            Err(error) => {
                inaccessible.push(InaccessibleRepository {
                    name: org.clone(),
                    path: None,
                    reason: error.to_string(),
                });
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    let roots = census_roots(options);
    let mut local = gitio::discover_repos(&roots, options.max_depth);
    if local.is_empty() && options.org.is_none() {
        local.push(options.fallback_repo.clone());
    }

    let mut by_slug: BTreeMap<(String, String), PathBuf> = BTreeMap::new();
    let mut repositories = Vec::new();
    for path in local {
        if !gitio::is_git_repo(&path) {
            inaccessible.push(InaccessibleRepository {
                name: path.display().to_string(),
                path: Some(path.display().to_string()),
                reason: "not a git repository".to_string(),
            });
            continue;
        }
        let slug = gitio::origin_url(&path).and_then(|url| gitio::github_slug(&url));
        if let Some(org) = &options.org {
            let matches = slug
                .as_ref()
                .is_some_and(|(remote_org, _)| remote_org.eq_ignore_ascii_case(org));
            if !matches {
                continue;
            }
        }
        if let Some(slug) = &slug {
            by_slug.insert(
                (slug.0.to_ascii_lowercase(), slug.1.to_ascii_lowercase()),
                path.clone(),
            );
        }
        let organisation = slug
            .as_ref()
            .map(|(org, _)| org.clone())
            .unwrap_or_else(|| options.org.clone().unwrap_or_else(|| "local".to_string()));
        let permission = permission_for(&path, &organisation, &options.first_party_orgs);
        match census_repository(&path, &organisation, permission) {
            Ok(census) => repositories.push(census),
            Err(error) => inaccessible.push(InaccessibleRepository {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string()),
                path: Some(path.display().to_string()),
                reason: error.to_string(),
            }),
        }
    }

    for remote in &remotes {
        let key = (
            options.org.clone().unwrap_or_default().to_ascii_lowercase(),
            remote.name.to_ascii_lowercase(),
        );
        if by_slug.contains_key(&key) {
            continue;
        }
        repositories.push(remote_only(remote, options.org.as_deref().unwrap_or("")));
    }

    repositories.sort_by(|left, right| left.repository_name.cmp(&right.repository_name));
    inaccessible.sort_by(|left, right| left.name.cmp(&right.name));
    let totals = totals_of(&repositories, inaccessible.len() as u64);
    Ok(EstateCensus {
        organisation: options.org.clone(),
        repositories,
        inaccessible,
        totals,
        model_readiness: "NOT READY — run extract, validate, and dedupe before judging a model"
            .to_string(),
    })
}

fn census_roots(options: &CensusOptions) -> Vec<PathBuf> {
    if !options.roots.is_empty() {
        return options.roots.clone();
    }
    let mut roots = vec![options.fallback_repo.clone()];
    if options.org.is_some() {
        if let Some(parent) = options.fallback_repo.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    roots
}

pub fn census_repository(
    path: &Path,
    organisation: &str,
    training_permission: TrainingPermission,
) -> anyhow::Result<RepositoryCensus> {
    let permission = read_permission_override(path).unwrap_or(training_permission);
    let latest = gitio::head_sha(path).context("repository has no commits")?;
    let total_commits = gitio::rev_count(path, &["HEAD"])?;
    let merges = gitio::rev_count(path, &["--merges", "HEAD"]).unwrap_or(0);
    let reverts = gitio::rev_count(
        path,
        &["--extended-regexp", "--grep", r"^(Revert |revert:)", "HEAD"],
    )
    .unwrap_or(0);
    let tag_names = gitio::tags(path).unwrap_or_default();
    let releases = tag_names.iter().filter(|tag| is_release_tag(tag)).count() as u64;
    let first_commit = earliest_matching(path, &["--max-parents=0"])
        .map(|(sha, _)| sha)
        .unwrap_or_else(|| latest.clone());

    let labelled = gitio::log_matching(path, LABEL_GREP).unwrap_or_default();
    let mut labelled_classes = ClassCounts::default();
    let mut observed_ids: BTreeSet<String> = BTreeSet::new();
    let mut estimated = 0u64;
    let mut earliest_scorecard: Option<(String, String)> = None;
    for commit in &labelled {
        if let Some(card) = label::parse_scorecard(&commit.message()) {
            labelled_classes.add(card.class);
            estimated += card.paths.max(1);
            if let Some(cluster) = &card.cluster_id {
                observed_ids.insert(cluster.clone());
            } else {
                observed_ids.insert(format!("commit:{}", commit.sha));
            }
            let replace = earliest_scorecard
                .as_ref()
                .is_none_or(|(_, at)| commit.committed_at < *at);
            if replace {
                earliest_scorecard = Some((commit.sha.clone(), commit.committed_at.clone()));
            }
        }
    }
    let explicitly_labelled_commits = labelled.len() as u64;

    let config_birth = earliest_matching(path, &["--diff-filter=A", "--", "kaptaind.toml"]);

    let first_kaptaind = match (config_birth, earliest_scorecard) {
        (Some((sha, at)), Some((other, other_at))) => {
            if at <= other_at {
                Some(sha)
            } else {
                Some(other)
            }
        }
        (Some((sha, _)), None) | (None, Some((sha, _))) => Some(sha),
        (None, None) => None,
    };

    let kaptaind_era_commits = match &first_kaptaind {
        Some(sha) => commits_from(path, sha).unwrap_or(0),
        None => 0,
    };

    for record in read_decisions(path) {
        if record.scores.is_none() && record.bump.is_none() {
            continue;
        }
        if observed_ids.insert(record.cluster_id.clone()) {
            estimated += (record.paths.len() as u64).max(1);
        }
    }
    for cluster in analysis_ids(path) {
        if observed_ids.insert(cluster) {
            estimated += 1;
        }
    }

    let name = gitio::origin_url(path)
        .and_then(|url| gitio::github_slug(&url))
        .map(|(_, name)| name)
        .or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "repository".to_string());
    let repository_id = if organisation == "local" {
        format!("local:{name}")
    } else {
        format!("{organisation}/{name}")
    };

    Ok(RepositoryCensus {
        repository_id,
        repository_name: name,
        organisation: organisation.to_string(),
        availability: "cloned".to_string(),
        local_path: Some(path.display().to_string()),
        remote_url: gitio::origin_url(path),
        training_permission: permission,
        first_commit,
        first_kaptaind_commit: first_kaptaind,
        latest_commit: latest,
        total_commits,
        kaptaind_era_commits,
        explicitly_labelled_commits,
        kaptaind_observed: observed_ids.len() as u64,
        languages: language_shares(path),
        merges,
        reverts,
        releases,
        tags: tag_names.len() as u64,
        estimated_observations: estimated,
        estimated_training_tokens: estimated.saturating_mul(CENSUS_TOKENS_PER_CANDIDATE),
        labelled_classes,
    })
}

/// Earliest commit matched by `git log` arguments. `--reverse -1` is not used:
/// git applies the commit limit before reversing, so that pair returns the
/// latest match.
fn earliest_matching(path: &Path, args: &[&str]) -> Option<(String, String)> {
    let mut full = vec!["log", "--format=%H%x09%cI"];
    full.extend_from_slice(args);
    let text = gitio::git_text(path, &full).ok()?;
    text.lines()
        .filter_map(|line| {
            let (sha, at) = line.split_once('\t')?;
            Some((sha.to_string(), at.to_string()))
        })
        .min_by(|left, right| left.1.cmp(&right.1))
}

fn commits_from(path: &Path, sha: &str) -> anyhow::Result<u64> {
    let parent = format!("{sha}^");
    if gitio::git_ok(path, &["rev-parse", "--verify", "--quiet", &parent]) {
        gitio::rev_count(path, &[&format!("{sha}^..HEAD")])
    } else {
        gitio::rev_count(path, &["HEAD"])
    }
}

fn remote_only(remote: &RemoteRepo, organisation: &str) -> RepositoryCensus {
    RepositoryCensus {
        repository_id: format!("{organisation}/{}", remote.name),
        repository_name: remote.name.clone(),
        organisation: organisation.to_string(),
        availability: "remote_only".to_string(),
        local_path: None,
        remote_url: Some(remote.url.clone()),
        training_permission: TrainingPermission::FirstParty,
        first_commit: String::new(),
        first_kaptaind_commit: None,
        latest_commit: String::new(),
        total_commits: 0,
        kaptaind_era_commits: 0,
        explicitly_labelled_commits: 0,
        kaptaind_observed: 0,
        languages: Vec::new(),
        merges: 0,
        reverts: 0,
        releases: 0,
        tags: 0,
        estimated_observations: 0,
        estimated_training_tokens: 0,
        labelled_classes: ClassCounts::default(),
    }
}

fn permission_for(path: &Path, organisation: &str, first_party: &[String]) -> TrainingPermission {
    if let Some(permission) = read_permission_override(path) {
        return permission;
    }
    if first_party
        .iter()
        .any(|org| org.eq_ignore_ascii_case(organisation))
    {
        TrainingPermission::FirstParty
    } else {
        TrainingPermission::EvaluationOnly
    }
}

fn read_permission_override(path: &Path) -> Option<TrainingPermission> {
    let text = std::fs::read_to_string(path.join(".kaptaind").join("training-permission")).ok()?;
    schema::TrainingPermission::parse_label(text.lines().next().unwrap_or(""))
}

fn read_decisions(path: &Path) -> Vec<crate::daemon::decisions::DecisionRecord> {
    let file = path.join(".kaptaind").join("decisions.jsonl");
    let Ok(content) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn analysis_ids(path: &Path) -> Vec<String> {
    let dir = path.join(".kaptaind").join("analysis");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
                .map(str::to_string)
        })
        .collect()
}

fn language_shares(path: &Path) -> Vec<LanguageShare> {
    let Ok(raw) = gitio::git_text(path, &["ls-tree", "-r", "-l", "HEAD"]) else {
        return Vec::new();
    };
    let mut bytes: BTreeMap<String, u64> = BTreeMap::new();
    for line in raw.lines() {
        let Some((meta, file)) = line.split_once('\t') else {
            continue;
        };
        let Some(size) = meta
            .split_whitespace()
            .nth(3)
            .and_then(|raw| raw.parse::<u64>().ok())
        else {
            continue;
        };
        if size == 0 || is_binary_path(file) {
            continue;
        }
        let language = super::represent::language_of_path(file);
        if language.as_str() == "unknown" {
            continue;
        }
        *bytes.entry(language.0).or_insert(0) += size;
    }
    let total = bytes.values().sum::<u64>().max(1);
    let mut shares: Vec<LanguageShare> = bytes
        .into_iter()
        .map(|(language, bytes)| LanguageShare {
            share: bytes as f32 / total as f32,
            language,
            bytes,
        })
        .collect();
    shares.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then(left.language.cmp(&right.language))
    });
    shares.truncate(8);
    shares
}

fn is_binary_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let ext = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "ico"
            | "webp"
            | "pdf"
            | "zip"
            | "gz"
            | "wasm"
            | "woff"
            | "woff2"
            | "ttf"
            | "eot"
            | "mp4"
            | "bin"
            | "lockb"
    )
}

fn is_release_tag(tag: &str) -> bool {
    let bare = tag.strip_prefix('v').unwrap_or(tag);
    if semver::Version::parse(bare).is_ok() {
        return true;
    }
    tag.rfind("-v")
        .is_some_and(|index| semver::Version::parse(&tag[index + 2..]).is_ok())
}

fn totals_of(repositories: &[RepositoryCensus], inaccessible: u64) -> CensusTotals {
    let mut totals = CensusTotals {
        repositories: repositories.len() as u64,
        cloned: 0,
        remote_only: 0,
        inaccessible,
        kaptaind_era_repositories: 0,
        commits: 0,
        kaptaind_era_commits: 0,
        explicitly_labelled_commits: 0,
        kaptaind_observed: 0,
        estimated_observations: 0,
        estimated_training_tokens: 0,
        merges: 0,
        reverts: 0,
        releases: 0,
        tags: 0,
        labelled_classes: ClassCounts::default(),
    };
    for repo in repositories {
        if repo.availability == "remote_only" {
            totals.remote_only += 1;
            continue;
        }
        totals.cloned += 1;
        if repo.first_kaptaind_commit.is_some() {
            totals.kaptaind_era_repositories += 1;
        }
        totals.commits += repo.total_commits;
        totals.kaptaind_era_commits += repo.kaptaind_era_commits;
        totals.explicitly_labelled_commits += repo.explicitly_labelled_commits;
        totals.kaptaind_observed += repo.kaptaind_observed;
        totals.estimated_observations += repo.estimated_observations;
        totals.estimated_training_tokens += repo.estimated_training_tokens;
        totals.merges += repo.merges;
        totals.reverts += repo.reverts;
        totals.releases += repo.releases;
        totals.tags += repo.tags;
        totals.labelled_classes.no_bump += repo.labelled_classes.no_bump;
        totals.labelled_classes.patch += repo.labelled_classes.patch;
        totals.labelled_classes.minor += repo.labelled_classes.minor;
        totals.labelled_classes.major += repo.labelled_classes.major;
        totals.labelled_classes.ambiguous += repo.labelled_classes.ambiguous;
    }
    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::decisions::{
        append_decision, DecisionRecord, DecisionScores, DecisionThresholds,
    };
    use chrono::Utc;

    fn git(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_AUTHOR_NAME", "KOU Test")
            .env("GIT_AUTHOR_EMAIL", "kou@example.com")
            .env("GIT_COMMITTER_NAME", "KOU Test")
            .env("GIT_COMMITTER_EMAIL", "kou@example.com")
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init(repo: &Path) {
        std::fs::create_dir_all(repo).unwrap();
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "kou@example.com"]);
        git(repo, &["config", "user.name", "KOU Test"]);
        git(repo, &["config", "commit.gpgsign", "false"]);
    }

    #[test]
    fn census_keeps_labelled_era_and_observed_apart() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init(repo);
        std::fs::write(repo.join("README.md"), "before kaptaind\n").unwrap();
        git(repo, &["add", "README.md"]);
        git(repo, &["commit", "-m", "initial import"]);

        std::fs::write(repo.join("kaptaind.toml"), "[watch]\npath = \".\"\n").unwrap();
        git(repo, &["add", "kaptaind.toml"]);
        git(repo, &["commit", "-m", "chore: start watching"]);

        std::fs::write(repo.join("src.rs"), "pub fn connect() {}\n").unwrap();
        git(repo, &["add", "src.rs"]);
        git(
            repo,
            &[
                "commit",
                "-m",
                "fix: adjust connect\n\nkaptaind: Patch -> v0.1.1 [api-stable; paths=1; api_touches=0; deps=0; runtime=0; score=0.200; cluster=cluster-labelled]\n",
            ],
        );
        git(repo, &["tag", "v0.1.1"]);

        std::fs::write(repo.join("README.md"), "still docs\n").unwrap();
        git(repo, &["add", "README.md"]);
        git(repo, &["commit", "-m", "Revert \"adjust connect\""]);

        let record = DecisionRecord {
            timestamp: Utc::now(),
            cluster_id: "cluster-local-only".to_string(),
            outcome: "observed".to_string(),
            scores: Some(DecisionScores {
                score: 0.4,
                api: 0.2,
                deps: 0.0,
                runtime: 0.0,
            }),
            thresholds: DecisionThresholds {
                minor: 0.6,
                patch: 0.1,
            },
            bump: Some("Minor".to_string()),
            version: Some("0.2.0".to_string()),
            members_bumped: None,
            exit_code: None,
            output_tail: None,
            missing_program: None,
            reason: "observed".to_string(),
            paths: vec!["src.rs".to_string(), "other.rs".to_string()],
        };
        append_decision(repo, &record).unwrap();

        let census = census_repository(repo, "local", TrainingPermission::EvaluationOnly).unwrap();
        assert_eq!(census.total_commits, 4);
        assert_eq!(census.kaptaind_era_commits, 3);
        assert_eq!(census.explicitly_labelled_commits, 1);
        assert_eq!(census.kaptaind_observed, 2);
        assert_eq!(census.reverts, 1);
        assert_eq!(census.releases, 1);
        assert_eq!(census.labelled_classes.patch, 1);
        assert!(census.estimated_observations >= 3);
        assert_ne!(
            census.explicitly_labelled_commits, census.kaptaind_observed,
            "labelled commits and observed analyses must not be collapsed"
        );
        assert!(census
            .languages
            .iter()
            .any(|share| share.language == "rust"));
    }

    #[test]
    fn external_checkout_is_not_training_eligible_by_default() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path());
        std::fs::write(dir.path().join("README.md"), "x\n").unwrap();
        git(dir.path(), &["add", "README.md"]);
        git(dir.path(), &["commit", "-m", "init"]);
        let census = census_repository(
            dir.path(),
            "someone-else",
            TrainingPermission::EvaluationOnly,
        )
        .unwrap();
        assert_eq!(
            census.training_permission,
            TrainingPermission::EvaluationOnly
        );
        assert_eq!(census.kaptaind_era_commits, 0);
        assert_eq!(census.explicitly_labelled_commits, 0);
        assert_eq!(census.kaptaind_observed, 0);
    }
}
