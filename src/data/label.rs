//! Label authority. A later, weaker inference must not overwrite a stronger one,
//! and unknown history is excluded rather than forced into a bump class.

use super::schema::{LabelSource, SemverClass};

pub fn confidence(source: LabelSource) -> f32 {
    match source {
        LabelSource::HumanReviewedRelease => 1.0,
        LabelSource::ReleaseTagWithDeterministic => 0.95,
        LabelSource::MatureKaptaindDecision => 0.90,
        LabelSource::EarlierKaptaindGeneration => 0.80,
        LabelSource::CommitMessageInference => 0.60,
        LabelSource::ConventionalCommitInference => 0.50,
        LabelSource::HeuristicReconstruction => 0.30,
        LabelSource::Unknown => 0.0,
    }
}

pub fn admitted(source: LabelSource) -> bool {
    source != LabelSource::Unknown
}

/// Keep the higher-authority label when two sources describe one change.
pub fn prefer(current: LabelSource, incoming: LabelSource) -> LabelSource {
    if confidence(incoming) > confidence(current) {
        incoming
    } else {
        current
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scorecard {
    pub class: SemverClass,
    pub version: Option<String>,
    pub paths: u64,
    pub api_touches: u32,
    pub deps: u32,
    pub runtime: u32,
    pub score: f32,
    pub cluster_id: Option<String>,
    pub api_breaking: bool,
    pub api_added: bool,
}

pub fn parse_scorecard(message: &str) -> Option<Scorecard> {
    let line = message.lines().find(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("kaptaind:")
    })?;
    let rest = line.trim().trim_start_matches("kaptaind:").trim();
    let (head, bracket) = rest.split_once('[')?;
    let bracket = bracket.split(']').next().unwrap_or(bracket);
    let head = head.trim();
    let (class_token, version) = if let Some(idx) = head.find("->") {
        let class_token = head[..idx].trim();
        let version = head[idx + 2..]
            .trim()
            .trim_start_matches('v')
            .trim()
            .to_string();
        let version = (!version.is_empty()).then_some(version);
        (class_token, version)
    } else {
        (head, None)
    };
    let class = match class_token {
        "Major" => SemverClass::Major,
        "Minor" => SemverClass::Minor,
        "Patch" => SemverClass::Patch,
        "None" | "no-bump" => SemverClass::NoBump,
        _ => return None,
    };

    let mut paths = 1;
    let mut api_touches = 0;
    let mut deps = 0;
    let mut runtime = 0;
    let mut score = 0.0;
    let mut cluster_id = None;
    let mut api_breaking = false;
    let mut api_added = false;
    for part in bracket.split(';') {
        let part = part.trim();
        if part == "breaking-api" {
            api_breaking = true;
        } else if part == "api-added" {
            api_added = true;
        }
        if let Some(value) = part.strip_prefix("paths=") {
            paths = value.trim().parse().unwrap_or(1);
        } else if let Some(value) = part.strip_prefix("api_touches=") {
            api_touches = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = part.strip_prefix("deps=") {
            deps = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = part.strip_prefix("runtime=") {
            runtime = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = part.strip_prefix("score=") {
            score = value.trim().parse().unwrap_or(0.0);
        } else if let Some(value) = part.strip_prefix("cluster=") {
            let id = value.trim();
            if !id.is_empty() {
                cluster_id = Some(id.to_string());
            }
        }
    }

    Some(Scorecard {
        class,
        version,
        paths,
        api_touches,
        deps,
        runtime,
        score,
        cluster_id,
        api_breaking,
        api_added,
    })
}

pub fn has_kaptaind_trailer(message: &str) -> bool {
    message.lines().any(|line| {
        let line = line.trim();
        line.starts_with("Kaptaind-") && line.contains(':')
    })
}

/// Weak conventional-commit signal. Callers must keep this behind an explicit
/// opt-in so it cannot drown scorecards.
pub fn conventional_class(message: &str) -> Option<SemverClass> {
    let subject = message.lines().next()?.trim();
    let lower = subject.to_ascii_lowercase();
    if message.contains("BREAKING CHANGE")
        || lower.contains("!:")
        || lower.starts_with("fix!")
        || lower.starts_with("feat!")
    {
        return Some(SemverClass::Major);
    }
    let kind = lower.split([':', '(']).next().unwrap_or("").trim();
    match kind {
        "feat" => Some(SemverClass::Minor),
        "fix" | "perf" => Some(SemverClass::Patch),
        "chore" | "docs" | "test" | "style" | "ci" | "refactor" | "build" => {
            Some(SemverClass::NoBump)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_follows_the_authority_table() {
        assert_eq!(confidence(LabelSource::HumanReviewedRelease), 1.0);
        assert_eq!(confidence(LabelSource::ReleaseTagWithDeterministic), 0.95);
        assert_eq!(confidence(LabelSource::MatureKaptaindDecision), 0.90);
        assert_eq!(confidence(LabelSource::CommitMessageInference), 0.60);
        assert_eq!(confidence(LabelSource::ConventionalCommitInference), 0.50);
        assert_eq!(confidence(LabelSource::HeuristicReconstruction), 0.30);
        assert!(!admitted(LabelSource::Unknown));
        assert_eq!(
            prefer(
                LabelSource::ConventionalCommitInference,
                LabelSource::MatureKaptaindDecision
            ),
            LabelSource::MatureKaptaindDecision
        );
        assert_eq!(
            prefer(
                LabelSource::MatureKaptaindDecision,
                LabelSource::HeuristicReconstruction
            ),
            LabelSource::MatureKaptaindDecision
        );
    }

    #[test]
    fn scorecard_parses_bump_and_no_bump_bodies() {
        let major = "feat(api)!: extend connect\n\nkaptaind: Major -> v2.0.0 [breaking-api; paths=2; api_touches=1; deps=0; runtime=0; score=0.910; cluster=abc-1]\n";
        let card = parse_scorecard(major).expect("scorecard");
        assert_eq!(card.class, SemverClass::Major);
        assert_eq!(card.version.as_deref(), Some("2.0.0"));
        assert_eq!(card.paths, 2);
        assert!(card.api_breaking);
        assert_eq!(card.cluster_id.as_deref(), Some("abc-1"));

        let none = "chore: apply changes\n\nkaptaind: no-bump [api-stable; paths=1; api_touches=0; deps=0; runtime=0; score=0.042; cluster=xyz]\n";
        let card = parse_scorecard(none).expect("no-bump");
        assert_eq!(card.class, SemverClass::NoBump);
        assert_eq!(card.score, 0.042);
    }

    #[test]
    fn unrelated_kaptaind_mention_is_not_a_scorecard() {
        assert!(parse_scorecard("docs: mention kaptaind in the readme\n").is_none());
    }
}
