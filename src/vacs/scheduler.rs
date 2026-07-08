use crate::config::loader::VacsConfig;
use crate::vacs::asset::{Asset, AssetManager, AssetMetrics};
use crate::vacs::scoring::ScoredConcept;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

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
        queue.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(())
    }

    pub async fn run_pending(&self, asset_manager: &AssetManager) -> anyhow::Result<()> {
        if *self.jobs_this_hour.lock().unwrap() >= self.config.max_jobs_per_hour {
            return Ok(());
        }

        let to_process: Vec<ScoredConcept> = {
            let mut queue = self.queue.lock().unwrap();
            queue.drain(..).collect()
        };

        let mut jobs_this_hour = 0;

        for scored in to_process {
            let asset_type = &scored.recommended_asset;
            if !self.config.allowed_assets.contains(asset_type) {
                if asset_type == "video" && self.config.video_enabled {
                    // Proceed
                } else {
                    continue;
                }
            }

            let content = match self.generate_asset(&scored).await {
                Ok(content) => content,
                Err(e) => {
                    tracing::warn!(error = %e, concept = %scored.concept.concept_id, "asset generation failed");
                    continue;
                }
            };

            let hash = crate::util::hex::encode(Sha256::digest(content.as_bytes()));

            let asset = Asset {
                asset_id: format!("asset_{}", uuid::Uuid::new_v4()),
                concept_id: scored.concept.concept_id.clone(),
                asset_type: asset_type.clone(),
                created_at: Utc::now(),
                source_commit: scored
                    .concept
                    .source_refs
                    .commits
                    .first()
                    .cloned()
                    .unwrap_or_default(),
                hash,
                status: "active".to_string(),
                metrics: AssetMetrics { views: 0, reuse: 0 },
                content,
            };

            if let Err(e) = asset_manager.save(&asset) {
                tracing::warn!(error = %e, "failed to save asset");
                continue;
            }

            jobs_this_hour += 1;

            tracing::info!(
                concept_id = %scored.concept.concept_id,
                asset_type = %asset_type,
                score = %scored.score,
                "VACS asset generated"
            );

            if jobs_this_hour >= self.config.max_jobs_per_hour {
                break;
            }
        }

        Ok(())
    }

    async fn generate_asset(&self, scored: &ScoredConcept) -> anyhow::Result<String> {
        match scored.recommended_asset.as_str() {
            "diagram" => self.generate_diagram(scored).await,
            "video" => self.generate_video_placeholder(scored).await,
            "document" => self.generate_document(scored).await,
            _ => self.generate_diagram(scored).await,
        }
    }

    async fn generate_diagram(&self, scored: &ScoredConcept) -> anyhow::Result<String> {
        let concept_type = scored.concept.concept_type.as_str();

        let svg = match concept_type {
            "flow" => self.generate_flow_diagram(scored),
            "architecture" => self.generate_architecture_diagram(scored),
            "state" => self.generate_state_diagram(scored),
            "api" => self.generate_api_diagram(scored),
            "dependency" => self.generate_dependency_diagram(scored),
            _ => self.generate_generic_diagram(scored),
        };

        Ok(svg)
    }

    fn generate_flow_diagram(&self, scored: &ScoredConcept) -> String {
        let files = &scored.concept.source_refs.files;
        let height = 100 + files.len() * 60;

        let mut nodes = String::new();
        let mut connections = String::new();

        for (i, file) in files.iter().enumerate() {
            let y = 50 + i * 60;
            let short_name = escape_xml(file.split('/').next_back().unwrap_or(file));

            nodes += "<rect x=\"150\" y=\"";
            nodes += &y.to_string();
            nodes += "\" width=\"200\" height=\"40\" rx=\"5\" fill=\"";
            nodes += "#4a90d9";
            nodes += "\" stroke=\"";
            nodes += "#2c5aa0";
            nodes += "\" stroke-width=\"2\"/>\n";

            nodes += "<text x=\"250\" y=\"";
            nodes += &(y + 25).to_string();
            nodes +=
                "\" text-anchor=\"middle\" fill=\"white\" font-size=\"12\" font-family=\"sans-";
            nodes += "serif\">";
            nodes += &short_name;
            nodes += "</text>\n";

            if i < files.len() - 1 {
                connections += "<line x1=\"250\" y1=\"";
                connections += &(y + 40).to_string();
                connections += "\" x2=\"250\" y2=\"";
                connections += &(y + 55).to_string();
                connections += "\" stroke=\"";
                connections += "#666";
                connections += "\" stroke-width=\"2\" marker-end=\"url(#arrowhead)\"/>\n";
            }
        }

        let mut svg = String::new();
        svg += "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 500 ";
        svg += &height.to_string();
        svg += "\" width=\"500\" height=\"";
        svg += &height.to_string();
        svg += "\">\n";
        svg += "<defs>\n";
        svg += "<marker id=\"arrowhead\" markerWidth=\"10\" markerHeight=\"7\" refX=\"9\" refY=\"3.5\" orient=\"auto\">\n";
        svg += "<polygon points=\"0 0, 10 3.5, 0 7\" fill=\"";
        svg += "#666";
        svg += "\"/>\n";
        svg += "</marker>\n";
        svg += "</defs>\n";
        svg += "<rect width=\"100%\" height=\"100%\" fill=\"";
        svg += "#f8f9fa";
        svg += "\"/>\n";
        svg += "<text x=\"250\" y=\"25\" text-anchor=\"middle\" font-size=\"16\" font-weight=\"bold\" font-family=\"sans-";
        svg += "serif\" fill=\"";
        svg += "#333";
        svg += "\">";
        svg += &capitalize(&scored.concept.concept_type);
        svg += " Flow</text>\n";
        svg += &nodes;
        svg += &connections;
        svg += "</svg>";
        svg
    }

    fn generate_architecture_diagram(&self, scored: &ScoredConcept) -> String {
        let files = &scored.concept.source_refs.files;
        let center_x = 250;
        let center_y = 150;
        let radius = 100;

        let mut components = String::new();

        for (i, file) in files.iter().enumerate().take(6) {
            let angle = (i as f64 / files.len().min(6) as f64) * 2.0 * std::f64::consts::PI;
            let x = center_x as f64 + radius as f64 * angle.cos();
            let y = center_y as f64 + radius as f64 * angle.sin();
            let short_name = escape_xml(&truncate(file.split('/').next_back().unwrap_or(file), 10));

            components += "<circle cx=\"";
            components += &x.to_string();
            components += "\" cy=\"";
            components += &y.to_string();
            components += "\" r=\"35\" fill=\"";
            components += "#5cb85c";
            components += "\" stroke=\"";
            components += "#4cae4c";
            components += "\" stroke-width=\"2\"/>\n";

            components += "<text x=\"";
            components += &x.to_string();
            components += "\" y=\"";
            components += &(y + 4.0).to_string();
            components +=
                "\" text-anchor=\"middle\" fill=\"white\" font-size=\"10\" font-family=\"sans-";
            components += "serif\">";
            components += &short_name;
            components += "</text>\n";

            components += "<line x1=\"";
            components += &x.to_string();
            components += "\" y1=\"";
            components += &y.to_string();
            components += "\" x2=\"";
            components += &center_x.to_string();
            components += "\" y2=\"";
            components += &center_y.to_string();
            components += "\" stroke=\"";
            components += "#999";
            components += "\" stroke-width=\"1\" stroke-dasharray=\"5,5\"/>\n";
        }

        let mut svg = String::new();
        svg += "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 500 300\" width=\"500\" height=\"300\">\n";
        svg += "<rect width=\"100%\" height=\"100%\" fill=\"";
        svg += "#f8f9fa";
        svg += "\"/>\n";
        svg += "<text x=\"250\" y=\"25\" text-anchor=\"middle\" font-size=\"16\" font-weight=\"bold\" font-family=\"sans-";
        svg += "serif\" fill=\"";
        svg += "#333";
        svg += "\">Architecture Overview</text>\n";
        svg += "<circle cx=\"250\" cy=\"150\" r=\"40\" fill=\"";
        svg += "#337ab7";
        svg += "\" stroke=\"";
        svg += "#2e6da4";
        svg += "\" stroke-width=\"2\"/>\n";
        svg += "<text x=\"250\" y=\"154\" text-anchor=\"middle\" fill=\"white\" font-size=\"12\" font-family=\"sans-";
        svg += "serif\">Core</text>\n";
        svg += &components;
        svg += "</svg>";
        svg
    }

    fn generate_state_diagram(&self, _scored: &ScoredConcept) -> String {
        let states = ["Initial", "Processing", "Complete", "Error"];
        let width = 120;
        let gap = 40;
        let total_width = states.len() * width + (states.len() - 1) * gap;
        let start_x = (500 - total_width) / 2;

        let mut nodes = String::new();
        let mut transitions = String::new();

        for (i, state) in states.iter().enumerate() {
            let x = start_x + i * (width + gap);
            let fill = match *state {
                "Complete" => "#5cb85c",
                "Error" => "#d9534f",
                _ => "#f0ad4e",
            };

            nodes += "<rect x=\"";
            nodes += &x.to_string();
            nodes += "\" y=\"100\" width=\"";
            nodes += &width.to_string();
            nodes += "\" height=\"50\" rx=\"10\" fill=\"";
            nodes += fill;
            nodes += "\" stroke=\"";
            nodes += "#333";
            nodes += "\" stroke-width=\"2\"/>\n";

            nodes += "<text x=\"";
            nodes += &(x + width / 2).to_string();
            nodes += "\" y=\"130\" text-anchor=\"middle\" fill=\"white\" font-size=\"12\" font-family=\"sans-";
            nodes += "serif\" font-weight=\"bold\">";
            nodes += state;
            nodes += "</text>\n";

            if i < states.len() - 1 {
                transitions += "<line x1=\"";
                transitions += &(x + width).to_string();
                transitions += "\" y1=\"125\" x2=\"";
                transitions += &(x + width + gap).to_string();
                transitions += "\" y2=\"125\" stroke=\"";
                transitions += "#666";
                transitions += "\" stroke-width=\"2\" marker-end=\"url(#arrowhead)\"/>\n";
            }
        }

        let mut svg = String::new();
        svg += "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 500 200\" width=\"500\" height=\"200\">\n";
        svg += "<defs>\n";
        svg += "<marker id=\"arrowhead\" markerWidth=\"10\" markerHeight=\"7\" refX=\"9\" refY=\"3.5\" orient=\"auto\">\n";
        svg += "<polygon points=\"0 0, 10 3.5, 0 7\" fill=\"";
        svg += "#666";
        svg += "\"/>\n";
        svg += "</marker>\n";
        svg += "</defs>\n";
        svg += "<rect width=\"100%\" height=\"100%\" fill=\"";
        svg += "#f8f9fa";
        svg += "\"/>\n";
        svg += "<text x=\"250\" y=\"40\" text-anchor=\"middle\" font-size=\"16\" font-weight=\"bold\" font-family=\"sans-";
        svg += "serif\" fill=\"";
        svg += "#333";
        svg += "\">State Flow</text>\n";
        svg += &nodes;
        svg += &transitions;
        svg += "</svg>";
        svg
    }

    fn generate_api_diagram(&self, scored: &ScoredConcept) -> String {
        let files = &scored.concept.source_refs.files;
        let endpoints: Vec<_> = files
            .iter()
            .filter_map(|f| f.split('/').next_back())
            .map(|f| f.strip_suffix(".rs").unwrap_or(f))
            .collect();

        let total_height = 100 + endpoints.len() * 50;

        let mut rows = String::new();
        for (i, endpoint) in endpoints.iter().enumerate() {
            let y = 80 + i * 50;
            let escaped = escape_xml(endpoint);

            rows += "<rect x=\"50\" y=\"";
            rows += &y.to_string();
            rows += "\" width=\"400\" height=\"40\" rx=\"3\" fill=\"";
            rows += "#fff";
            rows += "\" stroke=\"";
            rows += "#ddd";
            rows += "\" stroke-width=\"1\"/>\n";

            rows += "<text x=\"70\" y=\"";
            rows += &(y + 25).to_string();
            rows += "\" font-size=\"12\" font-family=\"monospace\" fill=\"";
            rows += "#333";
            rows += "\">GET /api/";
            rows += &escaped;
            rows += "</text>\n";

            rows += "<rect x=\"350\" y=\"";
            rows += &(y + 8).to_string();
            rows += "\" width=\"80\" height=\"24\" rx=\"3\" fill=\"";
            rows += "#5cb85c";
            rows += "\"/>\n";

            rows += "<text x=\"390\" y=\"";
            rows += &(y + 22).to_string();
            rows += "\" text-anchor=\"middle\" font-size=\"10\" fill=\"white\">200 OK</text>\n";
        }

        let mut svg = String::new();
        svg += "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 500 ";
        svg += &total_height.to_string();
        svg += "\" width=\"500\" height=\"";
        svg += &total_height.to_string();
        svg += "\">\n";
        svg += "<rect width=\"100%\" height=\"100%\" fill=\"";
        svg += "#f8f9fa";
        svg += "\"/>\n";
        svg += "<text x=\"250\" y=\"40\" text-anchor=\"middle\" font-size=\"16\" font-weight=\"bold\" font-family=\"sans-";
        svg += "serif\" fill=\"";
        svg += "#333";
        svg += "\">API Endpoints</text>\n";
        svg += &rows;
        svg += "</svg>";
        svg
    }

    fn generate_dependency_diagram(&self, scored: &ScoredConcept) -> String {
        let files = &scored.concept.source_refs.files;
        let packages: Vec<_> = files.iter().filter_map(|f| f.split('/').nth(1)).collect();

        let unique_packages: std::collections::HashSet<_> = packages.iter().cloned().collect();

        let mut nodes = String::new();
        let center_x = 250;
        let center_y = 150;

        nodes += "<rect x=\"200\" y=\"125\" width=\"100\" height=\"50\" rx=\"5\" fill=\"";
        nodes += "#337ab7";
        nodes += "\" stroke=\"";
        nodes += "#2e6da4";
        nodes += "\" stroke-width=\"2\"/>\n";

        nodes += "<text x=\"250\" y=\"155\" text-anchor=\"middle\" fill=\"white\" font-size=\"12\" font-family=\"sans-";
        nodes += "serif\">Main</text>\n";

        for (i, pkg) in unique_packages.iter().enumerate().take(5) {
            let angle =
                (i as f64 / unique_packages.len().min(5) as f64) * 2.0 * std::f64::consts::PI;
            let x = center_x as f64 + 120.0 * angle.cos();
            let y = center_y as f64 + 80.0 * angle.sin();
            let pkg_escaped = escape_xml(&truncate(pkg, 10));

            nodes += "<rect x=\"";
            nodes += &(x - 40.0).to_string();
            nodes += "\" y=\"";
            nodes += &(y - 20.0).to_string();
            nodes += "\" width=\"80\" height=\"40\" rx=\"5\" fill=\"";
            nodes += "#5bc0de";
            nodes += "\" stroke=\"";
            nodes += "#46b8da";
            nodes += "\" stroke-width=\"2\"/>\n";

            nodes += "<text x=\"";
            nodes += &x.to_string();
            nodes += "\" y=\"";
            nodes += &(y + 5.0).to_string();
            nodes +=
                "\" text-anchor=\"middle\" fill=\"white\" font-size=\"10\" font-family=\"sans-";
            nodes += "serif\">";
            nodes += &pkg_escaped;
            nodes += "</text>\n";

            nodes += "<line x1=\"";
            nodes += &x.to_string();
            nodes += "\" y1=\"";
            nodes += &y.to_string();
            nodes += "\" x2=\"";
            nodes += &center_x.to_string();
            nodes += "\" y2=\"";
            nodes += &center_y.to_string();
            nodes += "\" stroke=\"";
            nodes += "#999";
            nodes += "\" stroke-width=\"1\"/>\n";
        }

        let mut svg = String::new();
        svg += "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 500 300\" width=\"500\" height=\"300\">\n";
        svg += "<rect width=\"100%\" height=\"100%\" fill=\"";
        svg += "#f8f9fa";
        svg += "\"/>\n";
        svg += "<text x=\"250\" y=\"30\" text-anchor=\"middle\" font-size=\"16\" font-weight=\"bold\" font-family=\"sans-";
        svg += "serif\" fill=\"";
        svg += "#333";
        svg += "\">Dependency Graph</text>\n";
        svg += &nodes;
        svg += "</svg>";
        svg
    }

    fn generate_generic_diagram(&self, scored: &ScoredConcept) -> String {
        let concept_type = &scored.concept.concept_type;
        let description = &scored.concept.description;

        let color = match concept_type.as_str() {
            "security" => "#d9534f",
            "performance" => "#5cb85c",
            "configuration" => "#f0ad4e",
            _ => "#337ab7",
        };

        let mut svg = String::new();
        svg += "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 500 200\" width=\"500\" height=\"200\">\n";
        svg += "<rect width=\"100%\" height=\"100%\" fill=\"";
        svg += "#f8f9fa";
        svg += "\"/>\n";
        svg += "<rect x=\"50\" y=\"50\" width=\"400\" height=\"100\" rx=\"10\" fill=\"";
        svg += color;
        svg += "\" fill-opacity=\"0.1\" stroke=\"";
        svg += color;
        svg += "\" stroke-width=\"2\"/>\n";
        svg += "<text x=\"250\" y=\"85\" text-anchor=\"middle\" font-size=\"18\" font-weight=\"bold\" font-family=\"sans-";
        svg += "serif\" fill=\"";
        svg += color;
        svg += "\">";
        svg += &capitalize(concept_type);
        svg += "</text>\n";
        svg +=
            "<text x=\"250\" y=\"115\" text-anchor=\"middle\" font-size=\"12\" font-family=\"sans-";
        svg += "serif\" fill=\"";
        svg += "#666";
        svg += "\">";
        svg += &escape_xml(&truncate(description, 50));
        svg += "</text>\n";
        svg += "<text x=\"250\" y=\"140\" text-anchor=\"middle\" font-size=\"10\" font-family=\"monospace\" fill=\"";
        svg += "#999";
        svg += "\">Score: ";
        svg += &format!("{:.2}", scored.score);
        svg += "</text>\n";
        svg += "</svg>";
        svg
    }

    async fn generate_video_placeholder(&self, scored: &ScoredConcept) -> anyhow::Result<String> {
        let concept_type = &scored.concept.concept_type;

        let mut svg = String::new();
        svg += "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 640 360\" width=\"640\" height=\"360\">\n";
        svg += "<defs>\n";
        svg += "<linearGradient id=\"grad\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"100%\">\n";
        svg += "<stop offset=\"0%\" style=\"stop-color:#667eea;stop-opacity:1\" />\n";
        svg += "<stop offset=\"100%\" style=\"stop-color:#764ba2;stop-opacity:1\" />\n";
        svg += "</linearGradient>\n";
        svg += "</defs>\n";
        svg += "<rect width=\"100%\" height=\"100%\" fill=\"url(#grad)\"/>\n";
        svg += "<circle cx=\"320\" cy=\"180\" r=\"60\" fill=\"white\" fill-opacity=\"0.3\"/>\n";
        svg += "<polygon points=\"300,150 300,210 360,180\" fill=\"white\"/>\n";
        svg +=
            "<text x=\"320\" y=\"280\" text-anchor=\"middle\" font-size=\"20\" font-family=\"sans-";
        svg += "serif\" fill=\"white\">";
        svg += &capitalize(concept_type);
        svg += " Animation</text>\n";
        svg +=
            "<text x=\"320\" y=\"310\" text-anchor=\"middle\" font-size=\"12\" font-family=\"sans-";
        svg += "serif\" fill=\"white\" fill-opacity=\"0.7\">Video generation not yet implemented</text>\n";
        svg += "</svg>";
        Ok(svg)
    }

    async fn generate_document(&self, scored: &ScoredConcept) -> anyhow::Result<String> {
        let mut doc = String::new();
        doc += "# ";
        doc += &capitalize(&scored.concept.concept_type);
        doc += " Documentation\n\n";

        doc += "## Overview\n\n";
        doc += &scored.concept.description;
        doc += "\n\n**Priority:** ";
        doc += &scored.priority;
        doc += "\n\n**Score:** ";
        doc += &format!("{:.2}", scored.score);
        doc += "/1.0\n\n";

        if !scored.concept.source_refs.files.is_empty() {
            doc += "## Affected Files\n\n";
            for file in &scored.concept.source_refs.files {
                doc += "- `";
                doc += file;
                doc += "`\n";
            }
            doc += "\n";
        }

        if !scored.concept.source_refs.symbols.is_empty() {
            doc += "## Relevant Symbols\n\n";
            for symbol in &scored.concept.source_refs.symbols {
                doc += "- `";
                doc += symbol;
                doc += "`\n";
            }
            doc += "\n";
        }

        doc += "## Features\n\n";
        let f = &scored.concept.features;
        doc += &format!("- **Complexity:** {:.2}\n", f.complexity);
        doc += &format!("- **Change Magnitude:** {:.2}\n", f.change_magnitude);
        doc += &format!("- **Explanation Gap:** {:.2}\n", f.explanation_gap);
        doc += &format!("- **Visual Affinity:** {:.2}\n", f.visual_affinity);
        doc += &format!("- **Recurrence:** {}\n", f.recurrence);

        Ok(doc)
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
