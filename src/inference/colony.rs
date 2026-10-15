//! Distributed consensus through the Colony (`:e`) control plane
//! (`validation_mode = "colony"`).
//!
//! kaptaind is the Colony host. Each cluster becomes a work graph of
//! independent candidate units, one per distinct routed provider/model:
//! every resource has concurrency 1, so a single dispatch wave makes the
//! allocator place each candidate on a different route, ranked by fit,
//! cost, latency and quota under the configured integer budget. The host
//! runs the assigned calls concurrently, feeds the results back, and its
//! deterministic verifier (never the model) decides which candidates are
//! validated. The existing Jaccard election then picks the subject. A report
//! of the run is written to `.kaptaind/colony/<cluster-id>.json`.

use colony_core::{
    Artifact, Budget, CheckResult, Classification, ContextManifest, Demand, Error, Evidence,
    FailureKind, OntologySlice, Policy, VerificationContract, WorkGraph, WorkUnit, WorkerResult,
};
use colony_provider::{Capabilities, Registry, Resource};
use colony_runtime::{Colony, Dispatch, Mesut, State, Verifier};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

use crate::config::loader::{InferenceConfig, ModelRoute};
use crate::version::Bump;

use super::CommitContext;

const VERIFIER: &str = "kaptaind-subject-verifier";
const OUTPUT: &str = "commit_subject";
const MAX_SUBJECT_CHARS: usize = 72;
const CHECKS: [&str; 5] = [
    "single_line",
    "subject_length",
    "no_preamble",
    "breaking_marker_matches_bump",
    "artifact_integrity",
];
const ACCEPTANCE: [&str; 2] = [
    "one_subject_line_describing_the_change",
    "consistent_with_computed_bump",
];

fn parse_classification(value: &str) -> Classification {
    match value.to_ascii_lowercase().replace('-', "_").as_str() {
        "public" => Classification::Public,
        "confidential" => Classification::Confidential,
        "restricted" => Classification::Restricted,
        "local_only" => Classification::LocalOnly,
        "internal" => Classification::Internal,
        other => {
            tracing::warn!(
                classification = other,
                "unknown [inference.colony] classification; using local_only"
            );
            Classification::LocalOnly
        }
    }
}

fn default_latency_ms(provider: &str) -> u64 {
    match provider {
        "ollama" => 3_000,
        "openrouter" => 4_000,
        _ => 1_500,
    }
}

fn resource(route: &ModelRoute, output_tokens: u64) -> Resource {
    let local = route.provider == "ollama";
    let mut classifications = BTreeSet::from([
        Classification::Public,
        Classification::Internal,
        Classification::Confidential,
    ]);
    if local {
        classifications.extend([Classification::Restricted, Classification::LocalOnly]);
    }
    Resource {
        provider: route.provider.clone(),
        model: route.model.clone(),
        local,
        classifications,
        // Capabilities are not benchmarked for this task; allocation is
        // decided by cost, latency and quota under the shared contract.
        declared: Capabilities {
            reasoning: 0.6,
            coding: 0.6,
            research: 0.3,
            structured_output: 0.8,
            tool_use: 0.2,
        },
        benchmarked: None,
        observed: None,
        context_window: 128_000,
        max_output: output_tokens.max(1),
        latency_ms: if route.latency_ms > 0 {
            route.latency_ms
        } else {
            default_latency_ms(&route.provider)
        },
        // One call per route per cluster: a wave lands on distinct routes.
        concurrency: 1,
        available: true,
        remaining_calls: 1,
        input_micros_per_million: route.input_micros_per_million,
        output_micros_per_million: route.output_micros_per_million,
        shadow_price: 1.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn candidate(
    index: usize,
    source_sha: &str,
    input_tokens: u64,
    output_tokens: u64,
    money_micros: u64,
    classification: Classification,
    estimated_ms: u64,
) -> WorkUnit {
    WorkUnit {
        id: format!("candidate-{}", index + 1),
        objective: "Write the commit subject line for this change".to_string(),
        rationale: "independent candidate for distributed consensus".to_string(),
        dependencies: Vec::new(),
        expected_outputs: vec![OUTPUT.to_string()],
        acceptance: ACCEPTANCE.iter().map(|a| a.to_string()).collect(),
        context: ContextManifest {
            source_sha: source_sha.to_string(),
            relevant_paths: Vec::new(),
            symbols: Vec::new(),
            ontology_entities: Vec::new(),
            architectural_constraints: vec![format!(
                "one subject line, at most {MAX_SUBJECT_CHARS} characters"
            )],
            trusted_context: Vec::new(),
            retrieved_untrusted: Vec::new(),
            input_tokens,
        },
        demand: Demand {
            reasoning: 0.3,
            coding: 0.4,
            research: 0.0,
            structured_output: 0.9,
            tool_use: 0.0,
            output_tokens,
        },
        classification,
        mutation: None,
        relational_density: 0.0,
        uncertainty: 0.2,
        consequence: 0.3,
        budget: Budget {
            money_micros,
            tokens: input_tokens + output_tokens,
            calls: 1,
        },
        estimated_ms,
        verification: VerificationContract {
            deterministic_checks: CHECKS.iter().map(|c| c.to_string()).collect(),
            semantic_review: false,
        },
    }
}

fn sha256_ref(text: &str) -> String {
    format!(
        "sha256:{}",
        crate::util::hex::encode(Sha256::digest(text.as_bytes()))
    )
}

/// Collects the wave's dispatches; the host runs them after dispatch.
#[derive(Default)]
struct Wave {
    submitted: Vec<Dispatch>,
}

impl Mesut for Wave {
    fn submit(&mut self, dispatch: &Dispatch) -> colony_core::Result<String> {
        self.submitted.push(dispatch.clone());
        Ok(format!("{}#{}", dispatch.colony_id, dispatch.attempt))
    }

    fn cancel(&mut self, _handle: &str) -> colony_core::Result<()> {
        // Calls are bounded by the HTTP client timeout and never outlive the
        // transaction; nothing remains to stop.
        Ok(())
    }
}

/// Deterministic checks over the subject text the host received.
fn subject_checks(text: &str, bump: Bump, reference: Option<&str>) -> Vec<CheckResult> {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    let head = trimmed.split_once(':').map(|(head, _)| head).unwrap_or("");
    let claims_breaking =
        (head.ends_with('!') && !head.contains(' ')) || lower.contains("breaking change");
    let preamble = ["here is", "here's", "sure", "commit message", "subject:"]
        .iter()
        .any(|p| lower.starts_with(p))
        || trimmed.starts_with(['"', '\'', '`']);
    let integrity = reference == Some(sha256_ref(text).as_str());
    let digest = sha256_ref(text);
    let short = &digest[..digest.len().min(19)];
    [
        (
            "single_line",
            !trimmed.is_empty() && !trimmed.contains('\n'),
        ),
        (
            "subject_length",
            trimmed.chars().count() <= MAX_SUBJECT_CHARS,
        ),
        ("no_preamble", !preamble),
        (
            "breaking_marker_matches_bump",
            !claims_breaking || bump == Bump::Major,
        ),
        ("artifact_integrity", integrity),
    ]
    .into_iter()
    .map(|(name, passed)| CheckResult {
        name: name.to_string(),
        passed,
        evidence_ref: format!("{VERIFIER}:{name}:{short}"),
    })
    .collect()
}

struct SubjectVerifier<'a> {
    subjects: &'a BTreeMap<String, String>,
    bump: Bump,
    now: u64,
}

impl Verifier for SubjectVerifier<'_> {
    fn verify(&mut self, unit: &WorkUnit, result: &WorkerResult) -> colony_core::Result<Evidence> {
        let text = self.subjects.get(&result.work_unit).ok_or_else(|| {
            Error::new(
                FailureKind::InvalidArtifact,
                "no host-received subject for this result",
            )
        })?;
        let reference = result
            .artifacts
            .iter()
            .find(|a| a.name == OUTPUT)
            .map(|a| a.reference.as_str());
        let checks = subject_checks(text, self.bump, reference);
        let passed = checks.iter().all(|c| c.passed);
        Ok(Evidence {
            work_unit: unit.id.clone(),
            verifier: VERIFIER.to_string(),
            checks,
            acceptance: if passed {
                unit.acceptance.iter().cloned().collect()
            } else {
                BTreeSet::new()
            },
            consistent: passed,
            semantic_approved: false,
            timestamp_ms: self.now,
        })
    }
}

#[derive(serde::Serialize)]
struct AssignmentRecord<'a> {
    work_unit: &'a str,
    provider: &'a str,
    model: &'a str,
    subject: Option<&'a str>,
    state: Option<State>,
    candidates: &'a [colony_allocator::Candidate],
}

#[derive(serde::Serialize)]
struct Report<'a> {
    cluster_id: String,
    routes: Vec<String>,
    assignments: Vec<AssignmentRecord<'a>>,
    snapshot: &'a colony_runtime::Snapshot,
    evidence: &'a BTreeMap<String, Evidence>,
    elected: Option<&'a str>,
}

fn write_report(repo_path: &Path, cluster_id: &str, report: &Report<'_>) {
    let dir = repo_path.join(".kaptaind").join("colony");
    let path = dir.join(format!("{cluster_id}.json"));
    let result = std::fs::create_dir_all(&dir).and_then(|()| {
        let json = serde_json::to_vec_pretty(report).map_err(std::io::Error::other)?;
        std::fs::write(&path, json)
    });
    if let Err(error) = result {
        tracing::warn!(%error, path = %path.display(), "failed to write colony report");
    }
}

/// Run one colony over `routes` and elect a subject line. `None` when no
/// candidate is validated or no consensus is reached (deterministic message
/// is used).
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn generate(
    config: &InferenceConfig,
    ctx: &CommitContext<'_>,
    routes: &[ModelRoute],
) -> Option<String> {
    if routes.is_empty() {
        tracing::warn!("colony: no routes available; using deterministic message");
        return None;
    }
    let started = Instant::now();
    let now = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let limits = &config.colony;

    let user_prompt = super::ollama::build_user_prompt(ctx);
    let input_tokens = u64::try_from(super::compat::SYSTEM_PROMPT.len() + user_prompt.len())
        .unwrap_or(u64::MAX / 4)
        .div_ceil(4)
        + 16;
    let output_tokens = limits.output_tokens.max(1);
    // The prompt is the immutable input every candidate sees.
    let source_sha = crate::util::hex::encode(Sha256::digest(user_prompt.as_bytes()));
    let classification = parse_classification(&limits.classification);

    let registry = Registry {
        resources: routes.iter().map(|r| resource(r, output_tokens)).collect(),
    };
    let count = limits.max_candidates.clamp(1, registry.resources.len());
    let count_u64 = count as u64;
    let money = limits.budget_micros / count_u64;
    let timeout_ms = config.timeout_secs.max(1) * 1_000;
    let nodes: Vec<WorkUnit> = (0..count)
        .map(|i| {
            candidate(
                i,
                &source_sha,
                input_tokens,
                output_tokens,
                money,
                classification,
                timeout_ms,
            )
        })
        .collect();
    let graph = WorkGraph {
        colony_id: format!("kaptaind-{}", ctx.cluster.id),
        objective: "Elect one commit subject line for the cluster".to_string(),
        nodes,
        policy: Policy {
            allowed_providers: routes.iter().map(|r| r.provider.clone()).collect(),
            max_classification: classification,
            allow_mutation: false,
            require_human_approval: false,
            max_workers: count,
            max_depth: 1,
            deadline_ms: timeout_ms * 2 + 5_000,
            budget: Budget {
                money_micros: money * count_u64,
                tokens: (input_tokens + output_tokens) * count_u64,
                calls: count_u64,
            },
        },
    };
    let mut colony = match colony_planner::plan(graph, OntologySlice::default())
        .and_then(|plan| Colony::new(plan, registry.clone()))
    {
        Ok(colony) => colony,
        Err(error) => {
            tracing::warn!(%error, "colony: work graph rejected; using deterministic message");
            return None;
        }
    };

    let mut wave = Wave::default();
    for id in colony.ready() {
        if let Err(error) = colony.dispatch(&id, now(), count, false, &mut wave) {
            tracing::info!(unit = %id, %error, "colony: candidate not dispatched");
        }
    }
    if wave.submitted.is_empty() {
        tracing::warn!("colony: no candidate could be placed within budget and policy");
        return None;
    }

    // Run every assigned call concurrently.
    let calls = wave.submitted.iter().map(|dispatch| {
        let assigned = routes.iter().find(|r| {
            r.provider == dispatch.assignment.provider && r.model == dispatch.assignment.model
        });
        async move {
            match assigned {
                Some(route) => {
                    super::generate_with_route(
                        config,
                        ctx,
                        route,
                        u32::try_from(output_tokens).unwrap_or(u32::MAX),
                    )
                    .await
                }
                None => None,
            }
        }
    });
    let outputs = futures_util::future::join_all(calls).await;

    let mut subjects: BTreeMap<String, String> = BTreeMap::new();
    for (dispatch, output) in wave.submitted.iter().zip(outputs) {
        let id = dispatch.unit.id.clone();
        let (provider, model) = (&dispatch.assignment.provider, &dispatch.assignment.model);
        match output {
            Some(text) => {
                let cost = registry
                    .get(provider, model)
                    .and_then(|r| r.cost(&dispatch.unit))
                    .unwrap_or(0);
                let usage = Budget {
                    money_micros: cost,
                    tokens: input_tokens
                        + u64::try_from(text.len()).unwrap_or(u64::MAX).div_ceil(4),
                    calls: 1,
                };
                let result = WorkerResult {
                    work_unit: id.clone(),
                    provider: provider.clone(),
                    model: model.clone(),
                    artifacts: vec![Artifact {
                        name: OUTPUT.to_string(),
                        reference: sha256_ref(&text),
                        source_sha: source_sha.clone(),
                        result_sha: None,
                        changed_paths: Vec::new(),
                    }],
                    usage,
                };
                subjects.insert(id.clone(), text);
                if let Err(error) = colony.receive(dispatch.attempt, result, now()) {
                    tracing::info!(unit = %id, %provider, %model, %error, "colony: result rejected");
                }
            }
            None => {
                let error = Error::new(FailureKind::ProviderUnavailable, "no subject returned");
                if let Err(error) = colony.fail(&id, dispatch.attempt, error, now()) {
                    tracing::warn!(unit = %id, %error, "colony: failed to record worker failure");
                }
            }
        }
    }

    let mut verifier = SubjectVerifier {
        subjects: &subjects,
        bump: ctx.bump,
        now: 0,
    };
    let awaiting: Vec<String> = colony
        .snapshot()
        .states
        .into_iter()
        .filter(|(_, state)| *state == State::AwaitingVerification)
        .map(|(id, _)| id)
        .collect();
    for id in awaiting {
        verifier.now = now();
        if let Err(error) = colony.verify(&id, verifier.now, &mut verifier) {
            tracing::info!(unit = %id, %error, "colony: candidate failed verification");
        }
    }

    let snapshot = colony.snapshot();
    let validated: Vec<String> = snapshot
        .states
        .iter()
        .filter(|(_, state)| **state == State::Validated)
        .filter_map(|(id, _)| subjects.get(id).cloned())
        .collect();
    tracing::info!(
        dispatched = wave.submitted.len(),
        validated = validated.len(),
        reserved_micros = snapshot.reserved.money_micros,
        "colony: candidates verified"
    );
    // A verified single candidate is accepted when only one could run.
    let quorum = config
        .consensus_min_agreement
        .clamp(1, wave.submitted.len());
    let elected = super::consensus::elect(&validated, config.consensus_threshold, quorum);

    let report = Report {
        cluster_id: ctx.cluster.id.to_string(),
        routes: routes
            .iter()
            .map(|r| format!("{}/{}", r.provider, r.model))
            .collect(),
        assignments: wave
            .submitted
            .iter()
            .map(|d| AssignmentRecord {
                work_unit: &d.unit.id,
                provider: &d.assignment.provider,
                model: &d.assignment.model,
                subject: subjects.get(&d.unit.id).map(String::as_str),
                state: snapshot.states.get(&d.unit.id).copied(),
                candidates: &d.assignment.candidates,
            })
            .collect(),
        snapshot: &snapshot,
        evidence: colony.evidence(),
        elected: elected.as_deref(),
    };
    write_report(ctx.repo_path, &report.cluster_id, &report);
    elected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passed(checks: &[CheckResult]) -> BTreeMap<String, bool> {
        checks.iter().map(|c| (c.name.clone(), c.passed)).collect()
    }

    #[test]
    fn checks_accept_a_clean_subject() {
        let text = "feat(inference): route clusters through colony";
        let checks = subject_checks(text, Bump::Minor, Some(&sha256_ref(text)));
        assert!(checks.iter().all(|c| c.passed), "{checks:?}");
        assert!(checks.iter().all(|c| !c.evidence_ref.is_empty()));
    }

    #[test]
    fn checks_reject_breaking_marker_without_major_bump() {
        let text = "feat(api)!: drop v1 endpoints";
        let minor = passed(&subject_checks(text, Bump::Minor, Some(&sha256_ref(text))));
        assert!(!minor["breaking_marker_matches_bump"]);
        let major = passed(&subject_checks(text, Bump::Major, Some(&sha256_ref(text))));
        assert!(major["breaking_marker_matches_bump"]);
    }

    #[test]
    fn checks_reject_preamble_length_and_tampering() {
        let text = "Here is the commit message: fix things";
        assert!(
            !passed(&subject_checks(text, Bump::Patch, Some(&sha256_ref(text))))["no_preamble"]
        );
        let long = "fix: ".to_string() + &"x".repeat(80);
        assert!(
            !passed(&subject_checks(
                &long,
                Bump::Patch,
                Some(&sha256_ref(&long))
            ))["subject_length"]
        );
        let text = "fix: correct overflow";
        let tampered = passed(&subject_checks(
            text,
            Bump::Patch,
            Some(&sha256_ref("other")),
        ));
        assert!(!tampered["artifact_integrity"]);
    }

    #[test]
    fn wave_places_each_candidate_on_a_distinct_route_within_budget() {
        let routes = [
            ModelRoute {
                provider: "openai".into(),
                model: "cheap".into(),
                input_micros_per_million: 100_000,
                output_micros_per_million: 500_000,
                latency_ms: 800,
                reasoning_effort: None,
            },
            ModelRoute {
                provider: "gemini".into(),
                model: "flash".into(),
                input_micros_per_million: 750_000,
                output_micros_per_million: 3_750_000,
                latency_ms: 1_000,
                reasoning_effort: None,
            },
            ModelRoute {
                provider: "anthropic".into(),
                model: "pricey".into(),
                input_micros_per_million: 900_000_000,
                output_micros_per_million: 900_000_000,
                latency_ms: 800,
                reasoning_effort: None,
            },
        ];
        let registry = Registry {
            resources: routes.iter().map(|r| resource(r, 512)).collect(),
        };
        let sha = "a".repeat(64);
        let nodes: Vec<WorkUnit> = (0..3)
            .map(|i| candidate(i, &sha, 400, 512, 6_000, Classification::Internal, 15_000))
            .collect();
        let graph = WorkGraph {
            colony_id: "test".into(),
            objective: "elect".into(),
            nodes,
            policy: Policy {
                allowed_providers: routes.iter().map(|r| r.provider.clone()).collect(),
                max_classification: Classification::Internal,
                allow_mutation: false,
                require_human_approval: false,
                max_workers: 3,
                max_depth: 1,
                deadline_ms: 60_000,
                budget: Budget {
                    money_micros: 18_000,
                    tokens: 3 * 912,
                    calls: 3,
                },
            },
        };
        let plan = colony_planner::plan(graph, OntologySlice::default()).unwrap();
        let mut colony = Colony::new(plan, registry).unwrap();
        let mut wave = Wave::default();
        let mut placed = 0;
        for id in colony.ready() {
            if colony.dispatch(&id, 1, 3, false, &mut wave).is_ok() {
                placed += 1;
            }
        }
        // The over-budget route is never eligible; the other two each get one.
        assert_eq!(placed, 2);
        let used: BTreeSet<_> = wave
            .submitted
            .iter()
            .map(|d| d.assignment.model.clone())
            .collect();
        assert_eq!(
            used,
            BTreeSet::from(["cheap".to_string(), "flash".to_string()])
        );
    }

    #[test]
    fn local_only_classification_restricts_to_local_routes() {
        let local = resource(
            &ModelRoute {
                provider: "ollama".into(),
                model: "m".into(),
                input_micros_per_million: 0,
                output_micros_per_million: 0,
                latency_ms: 0,
                reasoning_effort: None,
            },
            64,
        );
        let remote = resource(
            &ModelRoute {
                provider: "openai".into(),
                model: "m".into(),
                input_micros_per_million: 0,
                output_micros_per_million: 0,
                latency_ms: 0,
                reasoning_effort: None,
            },
            64,
        );
        assert!(local.classifications.contains(&Classification::LocalOnly));
        assert!(!remote.classifications.contains(&Classification::LocalOnly));
        assert_eq!(
            parse_classification("local-only"),
            Classification::LocalOnly
        );
        assert_eq!(parse_classification("bogus"), Classification::LocalOnly);
    }

    /// Serve `/api/chat` like Ollama, answering per requested model.
    async fn fake_ollama(answers: BTreeMap<&'static str, &'static str>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let answers = answers.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    // Read headers, then the declared body length.
                    let body = loop {
                        let n = socket.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        let text = String::from_utf8_lossy(&buf).to_string();
                        if let Some((head, body)) = text.split_once("\r\n\r\n") {
                            let len = head
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                                })
                                .unwrap_or(0);
                            if body.len() >= len {
                                break body.to_string();
                            }
                        }
                    };
                    let request: serde_json::Value = serde_json::from_str(&body).unwrap();
                    let model = request["model"].as_str().unwrap_or_default().to_string();
                    let content = answers.get(model.as_str()).copied().unwrap_or("");
                    let reply =
                        serde_json::json!({"message": {"role": "assistant", "content": content}})
                            .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                        reply.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn colony_run_verifies_elects_and_reports() {
        let base = fake_ollama(BTreeMap::from([
            ("alpha", "feat(inference): route commits through colony"),
            ("beta", "feat(inference): route commits through the colony"),
            ("gamma", "Here is the commit message: stuff"),
        ]))
        .await;
        let repo = tempfile::tempdir().unwrap();
        let mut config = InferenceConfig {
            ollama_base_url: base,
            timeout_secs: 10,
            ..InferenceConfig::default()
        };
        config.colony.max_candidates = 3;
        let local = |model: &str| ModelRoute {
            provider: "ollama".into(),
            model: model.into(),
            input_micros_per_million: 0,
            output_micros_per_million: 0,
            latency_ms: 0,
            reasoning_effort: None,
        };
        let routes = [local("alpha"), local("beta"), local("gamma")];

        let cluster = crate::cluster::engine::Cluster {
            id: uuid::Uuid::new_v4(),
            started_at: chrono::Utc::now(),
            ended_at: chrono::Utc::now(),
            events: Vec::new(),
        };
        let diff = crate::diff::DiffAnalysis::default();
        let weight = crate::weight::WeightResult {
            score: 0.7,
            api_breaking: false,
            api_added: true,
        };
        let previous = semver::Version::new(1, 0, 0);
        let next = semver::Version::new(1, 1, 0);
        let paths = vec![repo.path().join("src/lib.rs")];
        let ctx = CommitContext {
            cluster: &cluster,
            diff: &diff,
            weight: &weight,
            bump: Bump::Minor,
            previous: &previous,
            next: &next,
            cluster_paths: &paths,
            repo_path: repo.path(),
        };

        let elected = generate(&config, &ctx, &routes).await;

        let report_path = repo
            .path()
            .join(".kaptaind/colony")
            .join(format!("{}.json", cluster.id));
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_path).unwrap()).unwrap();
        let states: BTreeMap<String, String> = report["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                (
                    a["model"].as_str().unwrap().to_string(),
                    a["state"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            states.len(),
            3,
            "each candidate on its own route: {report:#}"
        );
        assert_eq!(states["alpha"], "validated");
        assert_eq!(states["beta"], "validated");
        assert_eq!(states["gamma"], "rejected", "preamble fails verification");
        let elected =
            elected.unwrap_or_else(|| panic!("two verified candidates agree: {report:#}"));
        assert!(
            elected.starts_with("feat(inference): route commits through"),
            "{elected}"
        );
        assert_eq!(report["elected"].as_str(), Some(elected.as_str()));
        assert_eq!(report["evidence"].as_object().map(|e| e.len()), Some(2));
    }
}
