//! Connector catalogue and configuration inspection.

use kaptaind::{config::loader::Config, integrations};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ConnectorStatus {
    provider: String,
    configured: usize,
    default_mode: String,
    requires_customer_endpoint: bool,
    capabilities: Vec<String>,
}

pub fn handle_integrations(config: &Config, format: &str) -> anyhow::Result<()> {
    config.validate()?;
    let statuses: Vec<_> = integrations::ALL_PROVIDERS
        .iter()
        .map(|provider| {
            let manifest = integrations::manifest(*provider);
            ConnectorStatus {
                provider: provider.as_str().to_string(),
                configured: config
                    .integrations
                    .connectors
                    .iter()
                    .filter(|connector| connector.provider == *provider)
                    .count(),
                default_mode: format!("{:?}", manifest.default_mode).to_ascii_lowercase(),
                requires_customer_endpoint: manifest.requires_customer_endpoint,
                capabilities: manifest
                    .capabilities
                    .iter()
                    .map(|capability| format!("{:?}", capability).to_ascii_lowercase())
                    .collect(),
            }
        })
        .collect();
    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&statuses)?);
    } else {
        for status in statuses {
            println!(
                "{}: configured={} default={} endpoint={} capabilities={}",
                status.provider,
                status.configured,
                status.default_mode,
                if status.requires_customer_endpoint {
                    "customer"
                } else {
                    "provider"
                },
                status.capabilities.join(",")
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_covers_requested_enterprise_products() {
        let providers: Vec<_> = integrations::ALL_PROVIDERS
            .iter()
            .map(|provider| provider.as_str())
            .collect();
        for required in [
            "aws",
            "google_cloud",
            "google_drive",
            "microsoft_365",
            "slack",
            "whatsapp_business",
            "docker",
            "kubernetes",
            "hetzner",
            "monday",
        ] {
            assert!(providers.contains(&required), "missing {required}");
        }
    }
}
