use crate::config::loader::{Config, ReleaseIntent};
use crate::qualification::engine::{evaluate, QualificationResult};
use crate::release::builder;
use crate::stability::model::StabilityEntry;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Index entry for a completed release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseIndexEntry {
    pub version: String,
    pub commit: String,
    pub released_at: i64,
    pub stability: f64,
    pub intent: String,
    pub tarball: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleaseIndex {
    pub releases: Vec<ReleaseIndexEntry>,
}

/// Entry point called by the daemon scheduler after every successful commit.
///
/// Runs the full post-commit pipeline:
///   1. Run build (if configured)
///   2. Update stability score
///   3. Evaluate qualification
///   4. If qualified and intent ≠ None: package + distribute
pub async fn post_commit(
    repo_path: &Path,
    config: &Config,
    version: &str,
    commit_hash: &str,
    tests_passed: bool,
    diff_score: f64,
    runtime_paths: u32,
) {
    if !config.qualification.enabled {
        return;
    }

    // 1. Run build step
    let build_status = builder::run(&config.build, repo_path).await;
    if let builder::BuildStatus::Failed { ref stderr, .. } = build_status {
        tracing::warn!(stderr = stderr, "build step failed; penalising stability");
    }

    // 2. Update stability
    let mut stability = match crate::stability::engine::load(repo_path) {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(error = %err, "failed to load stability record; starting fresh");
            crate::stability::model::StabilityRecord::default()
        }
    };

    let now_ts = chrono::Utc::now().timestamp();
    let delta_t_mins = if stability.last_updated > 0 {
        ((now_ts - stability.last_updated) as f64 / 60.0).max(0.0)
    } else {
        0.0
    };

    let entry = StabilityEntry {
        commit: commit_hash.to_string(),
        delta_score: diff_score,
        tests: if tests_passed { "pass" } else { "fail" }.to_string(),
        build: build_status.as_str().to_string(),
        runtime_flags: runtime_paths,
        resulting_score: 0.0, // filled in by engine
        timestamp: now_ts,
    };

    crate::stability::engine::update(&mut stability, entry, delta_t_mins);

    if let Err(err) = crate::stability::engine::save(repo_path, &stability) {
        tracing::warn!(error = %err, "failed to save stability record");
    }

    tracing::info!(
        stability = stability.score,
        commit = commit_hash,
        "stability updated"
    );

    // Update extended telemetry
    crate::daemon::telemetry::update_release_metrics(repo_path, stability.score, false);

    // 3. Evaluate qualification
    let index = load_index(repo_path);
    let last_release_ts = index.releases.last().map(|e| e.released_at);
    let latest_diff = diff_score;

    let qual = evaluate(
        &stability,
        &config.qualification,
        tests_passed,
        build_status.passed() || matches!(build_status, builder::BuildStatus::Skipped),
        latest_diff,
        last_release_ts,
    );

    match &qual {
        QualificationResult::Qualified => {
            tracing::info!(stability = stability.score, "qualification passed");
        }
        QualificationResult::Rejected(reason) => {
            tracing::debug!(reason = %reason, "qualification rejected");
            return;
        }
    }

    // 4. Determine intent and run release pipeline
    let intent = &config.release.intent;
    if *intent == ReleaseIntent::None {
        tracing::debug!("release intent is None; skipping pipeline");
        return;
    }

    // Idempotency: skip if this version was already released
    if index.releases.iter().any(|e| e.version == version) {
        tracing::debug!(version = version, "version already released; skipping");
        return;
    }

    // Write RELEASE_VERSION
    let release_version_path = repo_path.join(".kaptaind").join("release_version");
    if let Err(err) = std::fs::write(&release_version_path, version) {
        tracing::warn!(error = %err, "failed to write release_version");
    }

    // Preview intent: build only, no packaging
    if *intent == ReleaseIntent::Preview {
        tracing::info!(version = version, "preview release: build complete, packaging skipped");
        append_index(repo_path, version, commit_hash, stability.score, "preview", None);
        crate::daemon::telemetry::update_release_metrics(repo_path, stability.score, true);
        return;
    }

    // Internal / Public: package + distribute
    let releases_dir = repo_path.join(".kaptaind").join("releases");
    match crate::release::packager::create(
        version,
        commit_hash,
        stability.score,
        &config.build,
        &releases_dir,
    ) {
        Ok(pkg) => {
            tracing::info!(
                version = version,
                checksum = pkg.manifest.checksum,
                "release packaged"
            );

            if let Err(err) = crate::release::distributor::distribute(&pkg, &config.distribution, repo_path) {
                tracing::warn!(error = %err, "distribution step failed");
                crate::daemon::telemetry::update_release_metrics(repo_path, stability.score, false);
                // Penalise stability for failed release
                if let Ok(mut s) = crate::stability::engine::load(repo_path) {
                    s.score = (s.score - 0.1).max(0.0);
                    let _ = crate::stability::engine::save(repo_path, &s);
                }
                return;
            }

            let tarball_name = pkg
                .tarball
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
            append_index(
                repo_path,
                version,
                commit_hash,
                stability.score,
                &format!("{:?}", intent).to_lowercase(),
                tarball_name,
            );
            crate::daemon::telemetry::update_release_metrics(repo_path, stability.score, true);
            tracing::info!(version = version, intent = ?intent, "release pipeline complete");
        }
        Err(err) => {
            tracing::warn!(error = %err, "packaging failed; aborting release");
            crate::daemon::telemetry::update_release_metrics(repo_path, stability.score, false);
        }
    }
}

// --- Index helpers ---

fn load_index(repo_path: &Path) -> ReleaseIndex {
    let path = repo_path.join(".kaptaind").join("releases").join("index.json");
    if !path.exists() {
        return ReleaseIndex::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn append_index(
    repo_path: &Path,
    version: &str,
    commit: &str,
    stability: f64,
    intent: &str,
    tarball: Option<String>,
) {
    let mut index = load_index(repo_path);
    index.releases.push(ReleaseIndexEntry {
        version: version.to_string(),
        commit: commit.to_string(),
        released_at: chrono::Utc::now().timestamp(),
        stability,
        intent: intent.to_string(),
        tarball,
    });
    let releases_dir = repo_path.join(".kaptaind").join("releases");
    let _ = std::fs::create_dir_all(&releases_dir);
    if let Ok(content) = serde_json::to_string_pretty(&index) {
        let _ = std::fs::write(releases_dir.join("index.json"), content);
    }
}
