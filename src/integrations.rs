//! Enterprise integration foundation.
//!
//! This module deliberately models connector capability and evidence before it
//! models provider-specific API calls. A connector never receives authority to
//! mutate a customer system merely because it was configured: writes require a
//! governed mode and a policy decision at the caller boundary. Incoming event
//! bodies are reduced to a digest-bearing envelope so audit artifacts do not
//! accidentally become copies of customer collaboration data.

pub mod adapters;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Products supported by the integration catalogue. The catalogue is stable
/// across configuration, policy, audit and operator UI surfaces.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Aws,
    GoogleCloud,
    GoogleDrive,
    Microsoft365,
    Slack,
    WhatsappBusiness,
    Docker,
    Kubernetes,
    Hetzner,
    Monday,
    Github,
    Gitlab,
    Bitbucket,
    Jira,
    ServiceNow,
    PagerDuty,
    Opsgenie,
    Okta,
    Entra,
    TerraformCloud,
    ArgoCd,
    Flux,
    Datadog,
    Splunk,
    Elastic,
}

impl Provider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aws => "aws",
            Self::GoogleCloud => "google_cloud",
            Self::GoogleDrive => "google_drive",
            Self::Microsoft365 => "microsoft_365",
            Self::Slack => "slack",
            Self::WhatsappBusiness => "whatsapp_business",
            Self::Docker => "docker",
            Self::Kubernetes => "kubernetes",
            Self::Hetzner => "hetzner",
            Self::Monday => "monday",
            Self::Github => "github",
            Self::Gitlab => "gitlab",
            Self::Bitbucket => "bitbucket",
            Self::Jira => "jira",
            Self::ServiceNow => "service_now",
            Self::PagerDuty => "pagerduty",
            Self::Opsgenie => "opsgenie",
            Self::Okta => "okta",
            Self::Entra => "entra",
            Self::TerraformCloud => "terraform_cloud",
            Self::ArgoCd => "argo_cd",
            Self::Flux => "flux",
            Self::Datadog => "datadog",
            Self::Splunk => "splunk",
            Self::Elastic => "elastic",
        }
    }

    /// The default product API origin. Customer-controlled endpoints must be
    /// supplied explicitly, particularly for sovereign/on-prem deployments.
    pub const fn default_endpoint(self) -> Option<&'static str> {
        match self {
            Self::Aws | Self::Docker | Self::Kubernetes | Self::ArgoCd | Self::Flux => None,
            Self::GoogleCloud | Self::GoogleDrive => Some("https://www.googleapis.com"),
            Self::Microsoft365 | Self::Entra => Some("https://graph.microsoft.com"),
            Self::Slack => Some("https://slack.com"),
            Self::WhatsappBusiness => Some("https://graph.facebook.com"),
            Self::Hetzner => Some("https://api.hetzner.cloud"),
            Self::Monday => Some("https://api.monday.com"),
            Self::Github => Some("https://api.github.com"),
            Self::Gitlab => Some("https://gitlab.com"),
            Self::Bitbucket => Some("https://api.bitbucket.org"),
            Self::Jira => None,
            Self::ServiceNow => None,
            Self::PagerDuty => Some("https://events.pagerduty.com"),
            Self::Opsgenie => Some("https://api.opsgenie.com"),
            Self::Okta => None,
            Self::TerraformCloud => Some("https://app.terraform.io"),
            Self::Datadog => None,
            Self::Splunk => None,
            Self::Elastic => None,
        }
    }
}

/// Stable order used by CLI/catalogue output and documentation generation.
pub const ALL_PROVIDERS: &[Provider] = &[
    Provider::Aws,
    Provider::GoogleCloud,
    Provider::GoogleDrive,
    Provider::Microsoft365,
    Provider::Slack,
    Provider::WhatsappBusiness,
    Provider::Docker,
    Provider::Kubernetes,
    Provider::Hetzner,
    Provider::Monday,
    Provider::Github,
    Provider::Gitlab,
    Provider::Bitbucket,
    Provider::Jira,
    Provider::ServiceNow,
    Provider::PagerDuty,
    Provider::Opsgenie,
    Provider::Okta,
    Provider::Entra,
    Provider::TerraformCloud,
    Provider::ArgoCd,
    Provider::Flux,
    Provider::Datadog,
    Provider::Splunk,
    Provider::Elastic,
];

impl std::fmt::Display for Provider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// An action class which can be allowed independently by governance policy.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ReadState,
    ReceiveEvents,
    SendNotification,
    WriteWorkItem,
    WriteInfrastructure,
    ManageIdentity,
    ExportAudit,
}

/// Connector modes ordered from least to most authority. `GovernedWrite` is
/// intentionally opt-in; no provider starts with infrastructure write power.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Disabled,
    ReadOnly,
    NotificationOnly,
    GovernedWrite,
}

impl Mode {
    pub const fn allows(self, capability: Capability) -> bool {
        match self {
            Self::Disabled => false,
            Self::ReadOnly => matches!(
                capability,
                Capability::ReadState | Capability::ReceiveEvents
            ),
            Self::NotificationOnly => matches!(
                capability,
                Capability::ReceiveEvents | Capability::SendNotification
            ),
            Self::GovernedWrite => true,
        }
    }
}

/// Provider metadata used to generate safe configuration and permission UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Manifest {
    pub provider: Provider,
    pub capabilities: &'static [Capability],
    pub default_mode: Mode,
    pub requires_customer_endpoint: bool,
}

const READ_EVENT: &[Capability] = &[Capability::ReadState, Capability::ReceiveEvents];
const NOTIFY_EVENT: &[Capability] = &[Capability::ReceiveEvents, Capability::SendNotification];
const COLLAB: &[Capability] = &[
    Capability::ReadState,
    Capability::ReceiveEvents,
    Capability::SendNotification,
    Capability::WriteWorkItem,
];
const INFRA: &[Capability] = &[
    Capability::ReadState,
    Capability::ReceiveEvents,
    Capability::WriteInfrastructure,
];
const IDENTITY: &[Capability] = &[Capability::ReadState, Capability::ManageIdentity];

/// Return the least-authority default manifest for a catalogued provider.
pub const fn manifest(provider: Provider) -> Manifest {
    let capabilities = match provider {
        Provider::Aws
        | Provider::GoogleCloud
        | Provider::Docker
        | Provider::Kubernetes
        | Provider::Hetzner
        | Provider::TerraformCloud
        | Provider::ArgoCd
        | Provider::Flux => INFRA,
        Provider::Slack | Provider::WhatsappBusiness | Provider::PagerDuty | Provider::Opsgenie => {
            NOTIFY_EVENT
        }
        Provider::GoogleDrive
        | Provider::Microsoft365
        | Provider::Monday
        | Provider::Github
        | Provider::Gitlab
        | Provider::Bitbucket
        | Provider::Jira
        | Provider::ServiceNow => COLLAB,
        Provider::Okta | Provider::Entra => IDENTITY,
        Provider::Datadog | Provider::Splunk | Provider::Elastic => READ_EVENT,
    };
    let requires_customer_endpoint = matches!(
        provider,
        Provider::Aws
            | Provider::Docker
            | Provider::Kubernetes
            | Provider::ArgoCd
            | Provider::Flux
            | Provider::Jira
            | Provider::ServiceNow
            | Provider::Okta
            | Provider::Datadog
            | Provider::Splunk
            | Provider::Elastic
    );
    Manifest {
        provider,
        capabilities,
        default_mode: Mode::Disabled,
        requires_customer_endpoint,
    }
}

/// A non-secret connector declaration. Tokens, API keys and OAuth refresh
/// tokens are deliberately represented only by an external secret reference.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConnectorConfig {
    pub provider: Provider,
    #[serde(default)]
    pub mode: Mode,
    pub tenant_id: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub capabilities: BTreeSet<Capability>,
}

/// A signed-policy grant for a connector capability. Grants are deliberately
/// tenant-bound: approving a Kubernetes rollout in one tenant cannot become
/// authority over a different tenant merely because both use the same
/// provider.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Grant {
    pub provider: Provider,
    pub tenant_id: String,
    pub capabilities: BTreeSet<Capability>,
}

impl Grant {
    pub fn validate(&self) -> anyhow::Result<()> {
        valid_identifier("integration grant tenant_id", &self.tenant_id)?;
        let definition = manifest(self.provider);
        if self.capabilities.is_empty() {
            anyhow::bail!("integration grant must allow at least one capability");
        }
        for capability in &self.capabilities {
            if !definition.capabilities.contains(capability) {
                anyhow::bail!(
                    "integration {} cannot grant unsupported capability {capability:?}",
                    self.provider
                );
            }
            if !is_mutating(*capability) {
                anyhow::bail!("integration grants are only valid for mutating capabilities");
            }
        }
        Ok(())
    }
}

const fn is_mutating(capability: Capability) -> bool {
    matches!(
        capability,
        Capability::WriteWorkItem | Capability::WriteInfrastructure | Capability::ManageIdentity
    )
}

impl ConnectorConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        valid_identifier("integration tenant_id", &self.tenant_id)?;
        if let Some(reference) = &self.credential_ref {
            valid_identifier("integration credential_ref", reference)?;
        }
        let definition = manifest(self.provider);
        if definition.requires_customer_endpoint
            && self.mode != Mode::Disabled
            && self.endpoint.is_none()
        {
            anyhow::bail!(
                "integration {} requires an explicit customer endpoint",
                self.provider
            );
        }
        if let Some(endpoint) = &self.endpoint {
            crate::util::http::validate_outbound_url(endpoint).map_err(|error| {
                anyhow::anyhow!("unsafe integration endpoint for {}: {error}", self.provider)
            })?;
        }
        if self.mode != Mode::Disabled && self.credential_ref.as_deref().is_none_or(str::is_empty) {
            anyhow::bail!(
                "enabled integration {} requires a non-secret credential_ref",
                self.provider
            );
        }
        for capability in &self.capabilities {
            if !definition.capabilities.contains(capability) {
                anyhow::bail!(
                    "integration {} does not support capability {capability:?}",
                    self.provider
                );
            }
            if !self.mode.allows(*capability) {
                anyhow::bail!(
                    "integration {} mode {:?} does not allow {capability:?}",
                    self.provider,
                    self.mode
                );
            }
        }
        Ok(())
    }
}

fn valid_identifier(field: &str, value: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':' | '.')
        })
    {
        anyhow::bail!("{field} must be 1-128 ASCII letters, digits, '-', '_', ':' or '.'");
    }
    Ok(())
}

/// Privacy-minimised, vendor-neutral integration event. `payload_sha256` is
/// the only retained representation of an inbound payload by this primitive.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EventEnvelope {
    pub schema_version: u8,
    pub provider: Provider,
    pub tenant_id: String,
    pub event_id: String,
    pub kind: String,
    pub occurred_at: DateTime<Utc>,
    pub payload_sha256: String,
    pub correlation_id: Option<String>,
}

impl EventEnvelope {
    pub fn from_payload(
        provider: Provider,
        tenant_id: impl Into<String>,
        event_id: impl Into<String>,
        kind: impl Into<String>,
        payload: &[u8],
    ) -> anyhow::Result<Self> {
        let tenant_id = tenant_id.into();
        let event_id = event_id.into();
        let kind = kind.into();
        valid_identifier("integration tenant_id", &tenant_id)?;
        valid_identifier("integration event_id", &event_id)?;
        valid_identifier("integration kind", &kind)?;
        Ok(Self {
            schema_version: 1,
            provider,
            tenant_id,
            event_id,
            kind,
            occurred_at: Utc::now(),
            payload_sha256: crate::util::hex::encode(Sha256::digest(payload)),
            correlation_id: None,
        })
    }
}

/// Verify a conventional SHA-256 HMAC webhook signature without retaining the
/// raw body. Provider-specific adapters are responsible for parsing headers.
pub fn verify_hmac_sha256(payload: &[u8], signature: &str, secret: &[u8]) -> bool {
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(payload);
    let expected = crate::util::hex::encode(mac.finalize().into_bytes());
    crate::util::constant_time::constant_time_eq(signature.as_bytes(), expected.as_bytes())
}

/// Atomically record a provider delivery ID. A second delivery with the same
/// provider/tenant/event ID is rejected before it can trigger a duplicate
/// side effect. The small ledger intentionally stores envelope metadata only.
pub fn record_delivery(repo_path: &Path, event: &EventEnvelope) -> anyhow::Result<()> {
    let provider = event.provider.as_str();
    let directory = repo_path
        .join(".kaptaind")
        .join("integrations")
        .join("deliveries")
        .join(provider)
        .join(&event.tenant_id);
    std::fs::create_dir_all(&directory)?;
    let file_name = crate::util::hex::encode(Sha256::digest(event.event_id.as_bytes()));
    let path = directory.join(file_name);
    let serialized = serde_json::to_vec(event)?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(&serialized)?;
            file.sync_all()?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            anyhow::bail!(
                "duplicate integration delivery for {provider}/{}",
                event.event_id
            )
        }
        Err(error) => Err(error.into()),
    }
}

/// Accept an already-authenticated inbound event, deduplicate it durably, and
/// append a privacy-minimised integration audit event. Adapters must call this
/// before dispatching provider work.
pub fn accept_delivery(repo_path: &Path, event: &EventEnvelope) -> anyhow::Result<()> {
    record_delivery(repo_path, event)?;
    crate::audit::log_event(
        repo_path,
        event.provider.as_str(),
        "integration_delivery_accepted",
        true,
        serde_json::json!({
            "provider": event.provider.as_str(),
            "tenant_id": event.tenant_id,
            "event_id": event.event_id,
            "kind": event.kind,
            "payload_sha256": event.payload_sha256,
            "correlation_id": event.correlation_id,
        }),
    );
    Ok(())
}

/// Check a signed policy grant and write an authorization decision before an
/// adapter executes a connector side effect. The actual transport remains the
/// adapter's responsibility, but it cannot skip the common governance trail.
pub fn authorize_action(
    repo_path: &Path,
    policy: &crate::daemon::policy::Policy,
    connector: &ConnectorConfig,
    capability: Capability,
) -> anyhow::Result<()> {
    policy.authorize_integration(connector, capability)?;
    crate::audit::log_event(
        repo_path,
        "integration-policy",
        "integration_action_authorized",
        true,
        serde_json::json!({
            "provider": connector.provider.as_str(),
            "tenant_id": connector.tenant_id,
            "capability": format!("{capability:?}").to_ascii_lowercase(),
        }),
    );
    Ok(())
}

/// Resolve a connector origin without allowing a configuration default to hide
/// a customer-controlled endpoint requirement.
pub fn endpoint(config: &ConnectorConfig) -> anyhow::Result<&str> {
    config
        .endpoint
        .as_deref()
        .or_else(|| config.provider.default_endpoint())
        .ok_or_else(|| anyhow::anyhow!("integration {} has no endpoint", config.provider))
}

/// Enforce the local configuration and signed-policy boundary before a
/// connector performs a side effect. Read/event/notification operations are
/// bounded by the connector mode; every mutating capability additionally
/// needs a matching policy grant.
pub fn authorize(
    connector: &ConnectorConfig,
    capability: Capability,
    grants: &[Grant],
) -> anyhow::Result<()> {
    connector.validate()?;
    if !connector.capabilities.contains(&capability) || !connector.mode.allows(capability) {
        anyhow::bail!(
            "integration {} is not configured to perform {capability:?}",
            connector.provider
        );
    }
    if is_mutating(capability)
        && !grants.iter().any(|grant| {
            grant.provider == connector.provider
                && grant.tenant_id == connector.tenant_id
                && grant.capabilities.contains(&capability)
        })
    {
        anyhow::bail!(
            "integration {} lacks a signed-policy grant for tenant {:?} capability {capability:?}",
            connector.provider,
            connector.tenant_id
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connector(provider: Provider, mode: Mode) -> ConnectorConfig {
        ConnectorConfig {
            provider,
            mode,
            tenant_id: "acme-payments".to_string(),
            endpoint: Some("https://93.184.216.34/api".to_string()),
            credential_ref: Some("vault:integration-token".to_string()),
            capabilities: BTreeSet::new(),
        }
    }

    #[test]
    fn infrastructure_connectors_are_read_only_until_governed() {
        let mut config = connector(Provider::Kubernetes, Mode::ReadOnly);
        config.capabilities.insert(Capability::ReadState);
        assert!(config.validate().is_ok());
        config.capabilities.insert(Capability::WriteInfrastructure);
        assert!(config.validate().is_err());
        config.mode = Mode::GovernedWrite;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn customer_managed_connectors_require_an_explicit_endpoint() {
        let mut config = connector(Provider::Kubernetes, Mode::ReadOnly);
        config.endpoint = None;
        assert!(config.validate().is_err());
        let mut google = connector(Provider::GoogleDrive, Mode::ReadOnly);
        google.endpoint = None;
        assert!(google.validate().is_ok());
    }

    #[test]
    fn envelope_is_privacy_minimised_and_delivery_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let event = EventEnvelope::from_payload(
            Provider::Slack,
            "acme",
            "evt-42",
            "message_created",
            b"confidential customer message",
        )
        .unwrap();
        assert_eq!(event.payload_sha256.len(), 64);
        record_delivery(dir.path(), &event).unwrap();
        assert!(record_delivery(dir.path(), &event).is_err());
        let stored = std::fs::read_to_string(
            dir.path()
                .join(".kaptaind/integrations/deliveries/slack/acme/")
                .join(crate::util::hex::encode(Sha256::digest(b"evt-42"))),
        )
        .unwrap();
        assert!(!stored.contains("confidential customer message"));
    }

    #[test]
    fn accepted_delivery_is_written_to_the_governance_audit() {
        let dir = tempfile::tempdir().unwrap();
        let event = EventEnvelope::from_payload(
            Provider::Monday,
            "acme",
            "delivery-7",
            "item_changed",
            b"private board update",
        )
        .unwrap();
        accept_delivery(dir.path(), &event).unwrap();
        let audit = std::fs::read_to_string(dir.path().join(".kaptaind/audit.jsonl")).unwrap();
        assert!(audit.contains("integration_delivery_accepted"));
        assert!(!audit.contains("private board update"));
    }

    #[test]
    fn hmac_verification_rejects_tampering() {
        let body = b"payload";
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(body);
        let signature = crate::util::hex::encode(mac.finalize().into_bytes());
        assert!(verify_hmac_sha256(body, &signature, b"secret"));
        assert!(!verify_hmac_sha256(b"tampered", &signature, b"secret"));
    }

    #[test]
    fn governed_writes_require_a_tenant_bound_policy_grant() {
        let mut config = connector(Provider::Kubernetes, Mode::GovernedWrite);
        config.capabilities.insert(Capability::WriteInfrastructure);
        assert!(authorize(&config, Capability::WriteInfrastructure, &[]).is_err());
        let grant = Grant {
            provider: Provider::Kubernetes,
            tenant_id: "acme-payments".to_string(),
            capabilities: [Capability::WriteInfrastructure].into_iter().collect(),
        };
        assert!(authorize(&config, Capability::WriteInfrastructure, &[grant]).is_ok());
    }
}
