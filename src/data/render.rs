//! Plain-text reports for the census and the corpus funnel.

use super::kds::{self, KdsReport};
use super::schema::{CensusTotals, ClassCounts, EstateCensus, RepositoryCensus};

pub fn render_census(estate: &EstateCensus) -> String {
    let totals = &estate.totals;
    let mut out = String::new();
    out.push_str("KAPTAIND DATA CENSUS\n\n");
    if let Some(org) = &estate.organisation {
        line(&mut out, "Organisation", org);
    }
    line(&mut out, "Repositories", &commas(totals.repositories));
    line(&mut out, "  cloned", &commas(totals.cloned));
    line(&mut out, "  remote-only", &commas(totals.remote_only));
    line(&mut out, "  inaccessible", &commas(totals.inaccessible));
    line(
        &mut out,
        "Kaptaind-era repositories",
        &commas(totals.kaptaind_era_repositories),
    );
    out.push('\n');
    line(&mut out, "Commits", &commas(totals.commits));
    line(
        &mut out,
        "Kaptaind-era commits",
        &commas(totals.kaptaind_era_commits),
    );
    line(
        &mut out,
        "Explicitly labelled commits",
        &commas(totals.explicitly_labelled_commits),
    );
    line(
        &mut out,
        "Kaptaind-observed analyses",
        &commas(totals.kaptaind_observed),
    );
    out.push('\n');
    line(
        &mut out,
        "Estimated candidate KOUs",
        &commas(totals.estimated_observations),
    );
    line(
        &mut out,
        "Estimated corpus tokens",
        &commas(totals.estimated_training_tokens),
    );
    out.push('\n');
    out.push_str("LABELLED COMMIT CLASSES\n");
    render_classes(&mut out, &totals.labelled_classes);
    out.push('\n');
    out.push_str("KDS\n");
    out.push_str("  n/a until observations are validated and deduplicated\n\n");
    out.push_str("MODEL READINESS\n");
    out.push_str(&format!("  {}\n", estate.model_readiness));
    out.push('\n');
    out.push_str(
        "Labelled commits, Kaptaind-era commits, and observed analyses are counted separately.\n",
    );
    if !estate.inaccessible.is_empty() {
        out.push('\n');
        out.push_str("INACCESSIBLE\n");
        for repo in &estate.inaccessible {
            out.push_str(&format!("  {}  {}\n", repo.name, repo.reason));
        }
    }
    if estate
        .repositories
        .iter()
        .any(|repo| repo.availability == "cloned")
    {
        out.push('\n');
        out.push_str("REPOSITORIES\n");
        for repo in estate
            .repositories
            .iter()
            .filter(|repo| repo.availability == "cloned")
        {
            out.push_str(&render_repository(repo));
        }
    }
    out
}

fn render_repository(repo: &RepositoryCensus) -> String {
    format!(
        "  {}  commits={}  era={}  labelled={}  observed={}  permission={}\n",
        repo.repository_id,
        commas(repo.total_commits),
        commas(repo.kaptaind_era_commits),
        commas(repo.explicitly_labelled_commits),
        commas(repo.kaptaind_observed),
        repo.training_permission.as_label(),
    )
}

pub fn render_funnel(
    totals: Option<&CensusTotals>,
    candidates: Option<u64>,
    validated: Option<u64>,
    deduplicated: Option<u64>,
    high_confidence: Option<u64>,
    classes: Option<&ClassCounts>,
    kds: Option<&KdsReport>,
) -> String {
    let mut out = String::new();
    out.push_str("KAPTAIND CORPUS\n\n");
    if let Some(totals) = totals {
        line(&mut out, "Raw commits", &commas(totals.commits));
        line(
            &mut out,
            "Kaptaind-era commits",
            &commas(totals.kaptaind_era_commits),
        );
        line(
            &mut out,
            "Explicitly labelled commits",
            &commas(totals.explicitly_labelled_commits),
        );
        line(
            &mut out,
            "Kaptaind-observed analyses",
            &commas(totals.kaptaind_observed),
        );
    }
    line(&mut out, "Candidate observations", &opt_commas(candidates));
    line(&mut out, "Validated observations", &opt_commas(validated));
    line(
        &mut out,
        "Deduplicated observations",
        &opt_commas(deduplicated),
    );
    line(
        &mut out,
        "High-confidence examples",
        &opt_commas(high_confidence),
    );
    out.push('\n');
    if let Some(classes) = classes {
        render_classes(&mut out, classes);
        out.push('\n');
    }
    match kds {
        Some(kds) => {
            line(&mut out, "KDS", &format!("{:.2}", kds.score));
            line(&mut out, "  label quality", &format!("{:.2}", kds.quality));
            line(&mut out, "  diversity", &format!("{:.2}", kds.diversity));
            line(&mut out, "  provenance", &format!("{:.2}", kds.provenance));
            line(
                &mut out,
                "  leakage resistance",
                &format!("{:.2}", kds.leakage),
            );
            line(
                &mut out,
                "  temporal validation",
                &format!("{:.2}", kds.temporal),
            );
        }
        None => {
            line(&mut out, "KDS", "n/a");
        }
    }
    out.push('\n');
    let ready = kds::readiness(high_confidence.unwrap_or(0));
    out.push_str("MODEL READINESS\n");
    line(&mut out, "  gate", ready.gate);
    line(
        &mut out,
        "  KAPTAIND-SLM/0",
        if ready.slm0_ready {
            "READY"
        } else {
            "NOT READY"
        },
    );
    out
}

fn render_classes(out: &mut String, classes: &ClassCounts) {
    line(out, "NO_BUMP", &commas(classes.no_bump));
    line(out, "PATCH", &commas(classes.patch));
    line(out, "MINOR", &commas(classes.minor));
    line(out, "MAJOR", &commas(classes.major));
    line(out, "AMBIGUOUS", &commas(classes.ambiguous));
}

fn line(out: &mut String, label: &str, value: &str) {
    out.push_str(&format!("{label:<32}{value}\n"));
}

fn opt_commas(value: Option<u64>) -> String {
    value.map(commas).unwrap_or_else(|| "n/a".to_string())
}

pub fn commas(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_separator() {
        assert_eq!(commas(0), "0");
        assert_eq!(commas(54321), "54,321");
        assert_eq!(commas(1000), "1,000");
    }
}
