use kaptaind::{
    config::loader::Config,
    integrations::{self, Capability, ConnectorConfig, Mode, Provider},
};
use std::collections::BTreeSet;

#[test]
fn connector_catalogue_has_safe_defaults_and_validates_a_governed_preflight() {
    for provider in integrations::ALL_PROVIDERS {
        let manifest = integrations::manifest(*provider);
        assert_eq!(manifest.default_mode, Mode::Disabled);
        assert!(!manifest.capabilities.is_empty());
    }

    let mut capabilities = BTreeSet::new();
    capabilities.insert(Capability::SendNotification);
    let mut config = Config::default();
    config.integrations.connectors = vec![ConnectorConfig {
        provider: Provider::Slack,
        mode: Mode::NotificationOnly,
        tenant_id: "acme-payments".to_string(),
        endpoint: Some("https://93.184.216.34/kaptaind/hooks".to_string()),
        credential_ref: Some("vault:slack-kaptaind".to_string()),
        capabilities,
    }];
    assert!(config.validate().is_ok());
}
