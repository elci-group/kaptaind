use crate::config::loader::VacsConfig;
use crate::vacs::scoring::ScoredConcept;
use crate::vacs::asset::{AssetManager, AssetMetrics, Asset};
use std::sync::Mutex;
use chrono::Utc;

pub struct Scheduler {
    config: VacsConfig,
    queue: Mutex<Vec<ScoredConcept>>,
    jobs_this_hour: Mutex<u32>,
}

impl Scheduler {
    pub fn new(config: VacsConfig) -> Self {
        Self {
            config,
            queue: Mutex::new(Vec::new()),
            jobs_this_hour: Mutex::new(0),
        }
    }

    pub async fn schedule(&self, scored: ScoredConcept) -> anyhow::Result<()> {
        let mut queue = self.queue.lock().unwrap();
        queue.push(scored);
        // Sort by score descending
        queue.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(())
    }

    pub async fn run_pending(&self, asset_manager: &AssetManager) -> anyhow::Result<()> {
        let mut jobs_this_hour = self.jobs_this_hour.lock().unwrap();
        
        if *jobs_this_hour >= self.config.max_jobs_per_hour {
            return Ok(()); // Capacity reached
        }

        let mut queue = self.queue.lock().unwrap();
        
        while let Some(scored) = queue.pop() {
            // Simplified MVP Generation Router
            if !self.config.allowed_assets.contains(&scored.recommended_asset) {
                if scored.recommended_asset == "video" && self.config.video_enabled {
                    // Proceed
                } else {
                    continue; // Skip restricted assets
                }
            }

            // Simulate generation
            tracing::info!("VACS generating {} for concept: {}", scored.recommended_asset, scored.concept.description);
            let asset = Asset {
                asset_id: format!("asset_{}", uuid::Uuid::new_v4()),
                concept_id: scored.concept.concept_id.clone(),
                asset_type: scored.recommended_asset.clone(),
                created_at: Utc::now(),
                source_commit: scored.concept.source_refs.commits.first().cloned().unwrap_or_default(),
                hash: "dummy_hash".to_string(),
                status: "active".to_string(),
                metrics: AssetMetrics { views: 0, reuse: 0 },
                content: format!("<!-- VACS generated {} MVP -->\n<svg></svg>", scored.recommended_asset),
            };

            asset_manager.save(&asset)?;
            *jobs_this_hour += 1;

            if *jobs_this_hour >= self.config.max_jobs_per_hour {
                break;
            }
        }

        Ok(())
    }
}