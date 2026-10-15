//! `kaptaind data` and `kaptaind dataset`.
//!
//! Census and extraction only. Nothing here trains a model or checks out history.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use kaptaind::config::loader::Config;
use kaptaind::data::{
    build_manifest, census_estate, export_corpus, extract, load_manifest, read_jsonl, render,
    validate, write_jsonl, CensusOptions, ExtractOptions, PartitionPlan,
};
use kaptaind::util::style::*;

#[derive(Debug, Subcommand)]
pub enum DataCommand {
    /// Count repositories, commits, and recoverable Kaptaind analyses.
    #[command(long_about = r#"Purpose:
    Census local checkouts and, with --org, every repository the GitHub CLI
    can see. Labelled commits, Kaptaind-era commits, and observed analyses are
    reported as three different numbers.

Usage:
    kaptaind data census [--org ORG] [--root PATH]... [--format text|json]

Examples:
    kaptaind data census
    kaptaind data census --org elci-group
    kaptaind data census --root ~/src --format json

Notes:
    Remote repositories are accounted for without being cloned. This command
    does not train a model. KDS stays n/a until validate and dedupe have run."#)]
    Census {
        /// GitHub organisation to account for, for example elci-group.
        #[arg(long)]
        org: Option<String>,
        /// Directory to scan for git checkouts. Repeatable.
        #[arg(long)]
        root: Vec<PathBuf>,
        /// How many directory levels to walk below each root.
        #[arg(long, default_value_t = 3)]
        max_depth: usize,
        /// Output format: text (default) or json.
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Reconstruct Kaptaind observation units from history and local records.
    #[command(long_about = r#"Purpose:
    Build one observation per symbol or file from Kaptaind scorecards,
    decision records, and analysis artifacts. Before/after text is read with
    git show. The worktree is not checked out.

Usage:
    kaptaind data extract [--include-weak] [--horizon N] [--limit N]

Examples:
    kaptaind data extract
    kaptaind data extract --limit 100 --format json

Notes:
    A commit label is not copied onto a symbol that disagrees with it.
    Weak conventional-commit labels are off unless --include-weak is set."#)]
    Extract {
        /// Also admit conventional-commit inferences at confidence 0.50.
        #[arg(long)]
        include_weak: bool,
        /// How many later commits to read when auditing a label.
        #[arg(long, default_value_t = 30)]
        horizon: u32,
        /// Stop after this many source commits.
        #[arg(long)]
        limit: Option<usize>,
        /// Organisation recorded on each observation.
        #[arg(long)]
        org: Option<String>,
        /// Output format: text (default) or json.
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Drop unknown labels and report confidence, disputes, and eligibility.
    Validate {
        /// Output format: text (default) or json.
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Cluster duplicate changes, then assign train/test partitions.
    Dedupe {
        /// Temporal cutoff (YYYY-MM-DD). Omit to hold out the latest fifth of clusters.
        #[arg(long)]
        cutoff: Option<String>,
        /// Fraction of repositories held out entirely. Ignored when only one repository is present.
        #[arg(long, default_value_t = 0.2)]
        repo_holdout: f64,
        /// Semantic kind to withhold, for example API_CHANGE. Repeatable.
        #[arg(long)]
        pattern_holdout: Vec<String>,
        /// Language to withhold, for example python.
        #[arg(long)]
        language_holdout: Option<String>,
        /// Output format: text (default) or json.
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Show the corpus funnel, class balance, KDS, and model-readiness gate.
    Stats {
        /// Recompute the census and include its commit counts.
        #[arg(long)]
        with_census: bool,
        /// Output format: text (default) or json.
        #[arg(long, default_value = "text")]
        format: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DatasetCommand {
    /// Write an immutable kaptaind-corpus manifest for the deduplicated set.
    Build {
        /// Corpus version. The name becomes kaptaind-corpus-<version>.
        #[arg(long, default_value = "0.1.0")]
        version: String,
        /// Date stored as the dataset cutoff.
        #[arg(long, default_value = "2026-10-10")]
        cutoff: String,
    },
    /// Print the current corpus manifest.
    Inspect,
    /// Copy the manifest and deduplicated observations to a directory.
    Export {
        /// Destination directory.
        #[arg(long)]
        out: PathBuf,
    },
}

pub fn handle_data(config: &Config, cmd: &DataCommand) -> anyhow::Result<()> {
    match cmd {
        DataCommand::Census {
            org,
            root,
            max_depth,
            format,
        } => census(config, org.clone(), root.clone(), *max_depth, format),
        DataCommand::Extract {
            include_weak,
            horizon,
            limit,
            org,
            format,
        } => extract_cmd(config, *include_weak, *horizon, *limit, org.clone(), format),
        DataCommand::Validate { format } => validate_cmd(config, format),
        DataCommand::Dedupe {
            cutoff,
            repo_holdout,
            pattern_holdout,
            language_holdout,
            format,
        } => dedupe_cmd(
            config,
            cutoff.clone(),
            *repo_holdout,
            pattern_holdout.clone(),
            language_holdout.clone(),
            format,
        ),
        DataCommand::Stats {
            with_census,
            format,
        } => stats_cmd(config, *with_census, format),
    }
}

pub fn handle_dataset(config: &Config, cmd: &DatasetCommand) -> anyhow::Result<()> {
    let dir = corpus_dir(config);
    match cmd {
        DatasetCommand::Build { version, cutoff } => {
            let manifest = build_manifest(&dir, version, cutoff)?;
            println!("{} {}", "corpus".cyan(), manifest.name.bold());
            println!("   observations     {}", manifest.deduplicated);
            println!("   high-confidence  {}", manifest.high_confidence);
            println!("   KDS              {:.2}", manifest.kds.score);
            println!("   checksum         {}", manifest.checksum_sha256);
            println!("   gate             {}", manifest.readiness.gate);
            println!(
                "   KAPTAIND-SLM/0   {}",
                if manifest.readiness.slm0_ready {
                    "READY".green()
                } else {
                    "NOT READY".yellow()
                }
            );
        }
        DatasetCommand::Inspect => {
            let manifest = load_manifest(&dir.join("manifest.json"))?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        DatasetCommand::Export { out } => {
            let dest = export_corpus(&dir, out)?;
            println!("exported {}", dest.display());
        }
    }
    Ok(())
}

fn census(
    config: &Config,
    org: Option<String>,
    roots: Vec<PathBuf>,
    max_depth: usize,
    format: &str,
) -> anyhow::Result<()> {
    let mut first_party = Vec::new();
    if let Some(org) = &org {
        first_party.push(org.clone());
    }
    if let Some(org) = &config.governance.organization_id {
        first_party.push(org.clone());
    }
    let estate = census_estate(&CensusOptions {
        roots,
        org,
        max_depth,
        first_party_orgs: first_party,
        fallback_repo: config.repo_path.clone(),
    })?;
    let dir = corpus_dir(config);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("census.json"),
        serde_json::to_string_pretty(&estate)?,
    )?;
    emit(format, &estate, &render::render_census(&estate))
}

fn extract_cmd(
    config: &Config,
    include_weak: bool,
    horizon: u32,
    limit: Option<usize>,
    org: Option<String>,
    format: &str,
) -> anyhow::Result<()> {
    let mut first_party = Vec::new();
    if let Some(org) = &org {
        first_party.push(org.clone());
    }
    if let Some(id) = &config.governance.organization_id {
        first_party.push(id.clone());
    }
    let observations = extract(&ExtractOptions {
        repo: config.repo_path.clone(),
        organisation: org,
        include_weak,
        horizon,
        limit,
        first_party_orgs: first_party,
    })?;
    let path = corpus_dir(config).join("observations.jsonl");
    write_jsonl(&path, &observations)?;
    let eligible = observations
        .iter()
        .filter(|obs| obs.training_eligible)
        .count();
    let disputed = observations.iter().filter(|obs| obs.label_disputed).count();
    let summary = serde_json::json!({
        "observations": observations.len(),
        "training_eligible": eligible,
        "disputed": disputed,
        "path": path.display().to_string(),
    });
    let text = format!(
        "KAPTAIND DATA EXTRACT\n\n{:<32}{}\n{:<32}{}\n{:<32}{}\n{:<32}{}\n",
        "Candidate observations",
        render::commas(observations.len() as u64),
        "Training-eligible",
        render::commas(eligible as u64),
        "Disputed labels",
        render::commas(disputed as u64),
        "Wrote",
        path.display(),
    );
    emit(format, &summary, &text)
}

fn validate_cmd(config: &Config, format: &str) -> anyhow::Result<()> {
    let dir = corpus_dir(config);
    let observations = read_jsonl(&dir.join("observations.jsonl"))?;
    let (accepted, report) = validate(&observations);
    write_jsonl(&dir.join("validated.jsonl"), &accepted)?;
    let text = format!(
        "KAPTAIND DATA VALIDATE\n\n{:<32}{}\n{:<32}{}\n{:<32}{}\n{:<32}{}\n{:<32}{}\n",
        "Read",
        render::commas(report.read),
        "Validated observations",
        render::commas(report.accepted),
        "Rejected",
        render::commas(report.rejected),
        "Training-eligible",
        render::commas(report.training_eligible),
        "Disputed labels",
        render::commas(report.disputed),
    );
    emit(format, &report, &text)
}

fn dedupe_cmd(
    config: &Config,
    cutoff: Option<String>,
    repo_holdout: f64,
    pattern_holdout: Vec<String>,
    language_holdout: Option<String>,
    format: &str,
) -> anyhow::Result<()> {
    let dir = corpus_dir(config);
    let mut observations = read_jsonl(&dir.join("validated.jsonl"))?;
    let plan = PartitionPlan {
        temporal_cutoff: cutoff,
        repository_holdout_ratio: repo_holdout,
        pattern_holdout,
        language_holdout,
    };
    let report = kaptaind::data::cluster_and_partition(&mut observations, &plan);
    let reps = kaptaind::data::representatives(&observations);
    write_jsonl(&dir.join("clustered.jsonl"), &observations)?;
    write_jsonl(&dir.join("deduped.jsonl"), &reps)?;
    std::fs::write(
        dir.join("dedupe.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "input": report.input,
            "clusters": report.clusters,
            "representatives": reps.len(),
            "temporal_cutoff": report.temporal_cutoff,
            "pattern_holdout": report.pattern_holdout,
            "repository_holdout": report.repository_holdout,
            "language_holdout": report.language_holdout,
        }))?,
    )?;
    let kds = kaptaind::data::score(&reps);
    let text = format!(
        "KAPTAIND DATA DEDUPE\n\n{:<32}{}\n{:<32}{}\n{:<32}{}\n{:<32}{:.2}\n",
        "Validated observations",
        render::commas(report.input),
        "Clusters",
        render::commas(report.clusters),
        "Deduplicated observations",
        render::commas(reps.len() as u64),
        "KDS",
        kds.score,
    );
    emit(
        format,
        &serde_json::json!({
            "input": report.input,
            "clusters": report.clusters,
            "deduplicated": reps.len(),
            "kds": kds,
        }),
        &text,
    )
}

fn stats_cmd(config: &Config, with_census: bool, format: &str) -> anyhow::Result<()> {
    let dir = corpus_dir(config);
    let census = if with_census {
        let mut first_party = Vec::new();
        if let Some(org) = &config.governance.organization_id {
            first_party.push(org.clone());
        }
        let estate = census_estate(&CensusOptions {
            roots: Vec::new(),
            org: None,
            max_depth: 1,
            first_party_orgs: first_party,
            fallback_repo: config.repo_path.clone(),
        })?;
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("census.json"),
            serde_json::to_string_pretty(&estate)?,
        )?;
        Some(estate)
    } else {
        std::fs::read_to_string(dir.join("census.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
    };
    let candidates = read_if(&dir.join("observations.jsonl"))?;
    let validated = read_if(&dir.join("validated.jsonl"))?;
    let deduped = read_if(&dir.join("deduped.jsonl"))?;
    let stage = deduped
        .as_ref()
        .or(validated.as_ref())
        .or(candidates.as_ref());
    let classes = stage.map(|rows| kaptaind::data::corpus::class_counts(rows));
    let kds = deduped.as_ref().map(|rows| kaptaind::data::score(rows));
    let high = deduped.as_ref().map(|rows| {
        rows.iter()
            .filter(|obs| obs.training_eligible && obs.confidence >= 0.9)
            .count() as u64
    });
    let text = render::render_funnel(
        census.as_ref().map(|estate| &estate.totals),
        candidates.as_ref().map(|rows| rows.len() as u64),
        validated.as_ref().map(|rows| rows.len() as u64),
        deduped.as_ref().map(|rows| rows.len() as u64),
        high,
        classes.as_ref(),
        kds.as_ref(),
    );
    emit(
        format,
        &serde_json::json!({
            "candidates": candidates.as_ref().map(|rows| rows.len()),
            "validated": validated.as_ref().map(|rows| rows.len()),
            "deduplicated": deduped.as_ref().map(|rows| rows.len()),
            "high_confidence": high,
            "kds": kds,
        }),
        &text,
    )
}

fn read_if(path: &Path) -> anyhow::Result<Option<Vec<kaptaind::data::KaptaindObservation>>> {
    if path.exists() {
        Ok(Some(read_jsonl(path)?))
    } else {
        Ok(None)
    }
}

fn corpus_dir(config: &Config) -> PathBuf {
    config.repo_path.join(".kaptaind").join("corpus")
}

fn emit(format: &str, value: &impl serde::Serialize, text: &str) -> anyhow::Result<()> {
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(value)?),
        "text" => print!("{text}"),
        other => anyhow::bail!("unknown format `{other}`; use text or json"),
    }
    Ok(())
}
