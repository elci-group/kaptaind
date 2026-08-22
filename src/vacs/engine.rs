use crate::config::loader::VacsConfig;
use crate::vacs::asset::AssetManager;
use crate::vacs::extractor::ConceptExtractor;
use crate::vacs::scheduler::Scheduler;
use crate::vacs::scoring::ScoringEngine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacsPayload {
    pub files_changed: Vec<String>,
    pub diff_summary: String,
    pub aoc_id: Option<String>,
    pub complexity_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacsEvent {
    pub event_type: String, // "commit.created", "aoc.updated", etc.
    pub timestamp: DateTime<Utc>,
    pub project_id: String,
    pub payload: VacsPayload,
}

pub struct VacsEngine {
    config: VacsConfig,
    extractor: ConceptExtractor,
    scoring: ScoringEngine,
    scheduler: Scheduler,
    asset_manager: AssetManager,
    tx: mpsc::Sender<VacsEvent>,
}

impl VacsEngine {
    pub fn new(repo_path: &Path, config: &VacsConfig) -> (Arc<Self>, mpsc::Receiver<VacsEvent>) {
        let (tx, rx) = mpsc::channel(100);
        let engine = Self {
            config: config.clone(),
            extractor: ConceptExtractor::new(),
            scoring: ScoringEngine::new(),
            scheduler: Scheduler::new(config.clone()),
            asset_manager: AssetManager::new(repo_path),
            tx,
        };
        (Arc::new(engine), rx)
    }

    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn ingest(&self, event: VacsEvent) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        self.tx.send(event).await?;
        Ok(())
    }

    // traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
    pub async fn process_queue(self: Arc<Self>, mut rx: mpsc::Receiver<VacsEvent>) {
        while let Some(event) = rx.recv().await {
            if let Err(e) = self.handle_event(event).await {
                tracing::error!(
                    component = module_path!(),
                    "VACS event handling failed: {}",
                    e
                );
            }
        }
    }

    async fn handle_event(&self, event: VacsEvent) -> anyhow::Result<()> {
        let concepts = self.extractor.extract(&event);

        for concept in concepts {
            let scored = self.scoring.score(concept);

            if scored.score >= 0.65 {
                self.scheduler.schedule(scored).await?;
            }
        }

        self.scheduler.run_pending(&self.asset_manager).await?;

        Ok(())
    }
}
