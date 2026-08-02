//! Provider-specific, authenticated ingress adapters.
//!
//! These adapters deliberately stop at the durable, privacy-minimised
//! envelope boundary. HTTP servers and local observers can call them after
//! obtaining a body and headers, but cannot bypass signature verification,
//! replay protection, or the common audit trail.

use super::{
    accept_delivery, authorize_action, verify_hmac_sha256, Capability, ConnectorConfig,
    EventEnvelope, Provider,
};
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use hmac::Mac;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

const SLACK_MAX_AGE_SECONDS: i64 = 300;
const SLACK_MAX_FUTURE_SECONDS: i64 = 60;

fn verify_fresh_timestamp(timestamp: &str, now: DateTime<Utc>, provider: &str) -> Result<()> {
    let timestamp = timestamp
        .parse::<i64>()
        .map(|seconds| {
            DateTime::from_timestamp(seconds, 0).ok_or_else(|| anyhow::anyhow!("invalid timestamp"))
        })
        .unwrap_or_else(|_| {
            DateTime::parse_from_rfc3339(timestamp)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|error| {
                    anyhow::anyhow!("{provider} signature timestamp is invalid: {error}")
                })
        })?;
    let age = now.signed_duration_since(timestamp).num_seconds();
    if !(-SLACK_MAX_FUTURE_SECONDS..=SLACK_MAX_AGE_SECONDS).contains(&age) {
        bail!("{provider} signature timestamp is outside the accepted freshness window");
    }
    Ok(())
}

/// Exact inbound request fields required to construct an auditable envelope.
/// The body is borrowed and is never retained by this type or its callers.
pub struct InboundEvent<'a> {
    pub repo_path: &'a Path,
    pub tenant_id: &'a str,
    pub event_id: &'a str,
    pub kind: &'a str,
    pub body: &'a [u8],
    pub signature: &'a str,
    pub secret: &'a [u8],
}

/// A provider-neutral notification request. Authentication material is
/// injected by the deployment's secret resolver at call time and is never
/// read from `kaptaind.toml` or recorded in the audit ledger.
pub struct OutboundNotification<'a> {
    pub repo_path: &'a Path,
    pub policy: &'a crate::daemon::policy::Policy,
    pub connector: &'a ConnectorConfig,
    /// Caller-generated stable key; retries of one logical notification must
    /// reuse it so providers with idempotency support can de-duplicate them.
    pub idempotency_key: &'a str,
    pub payload: &'a Value,
    /// Complete authorization header supplied by an external secret resolver,
    /// for example `Bearer …`. It is used only in the in-memory request.
    pub authorization: Option<&'a str>,
}

fn notification_destination(connector: &ConnectorConfig) -> Result<&str> {
    connector.endpoint.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "integration {} notifications require an explicit customer endpoint",
            connector.provider
        )
    })
}

fn validate_notification(request: &OutboundNotification<'_>) -> Result<String> {
    super::valid_identifier(
        "integration notification idempotency_key",
        request.idempotency_key,
    )?;
    authorize_action(
        request.repo_path,
        request.policy,
        request.connector,
        Capability::SendNotification,
    )?;
    let destination = notification_destination(request.connector)?;
    crate::util::http::validate_outbound_url(destination)?;
    crate::compliance::enforce_egress_url(
        crate::config::loader::EgressChannel::Integrations,
        destination,
    )?;
    Ok(destination.to_string())
}

/// Send a governed provider notification through Kaptaind's hardened outbound
/// HTTP client. The caller owns provider-specific payload shaping and secret
/// resolution; this primitive owns policy, egress, idempotency and audit.
// traci: allow -- this async API inherits the caller span; process roots create correlation IDs.
pub async fn send_notification(request: OutboundNotification<'_>) -> Result<()> {
    let destination = validate_notification(&request)?;
    let payload_sha256 =
        crate::util::hex::encode(Sha256::digest(serde_json::to_vec(request.payload)?));
    let mut outbound = crate::util::http::hardened_client(Duration::from_secs(15))
        .post(destination)
        .header("Idempotency-Key", request.idempotency_key)
        .json(request.payload);
    if let Some(authorization) = request.authorization {
        outbound = outbound.header(reqwest::header::AUTHORIZATION, authorization);
    }
    let response = outbound.send().await;
    let success = response
        .as_ref()
        .map(|response| response.status().is_success())
        .unwrap_or(false);
    crate::audit::log_event(
        request.repo_path,
        request.connector.provider.as_str(),
        "integration_notification_delivery",
        success,
        serde_json::json!({
            "tenant_id": request.connector.tenant_id,
            "idempotency_key": request.idempotency_key,
            "payload_sha256": payload_sha256,
            // traci: allow -- optional failure is represented by None and handled by the caller.
            "status": response.as_ref().ok().map(|value| value.status().as_u16()),
        }),
    );
    match response {
        Ok(response) if response.status().is_success() => Ok(()),
        Ok(response) => bail!(
            "integration {} notification endpoint returned HTTP {}",
            request.connector.provider,
            response.status()
        ),
        Err(error) => {
            tracing::error!(
                ?error,
                operation = "send_notification",
                source_line = line!(),
                "send notification returned an error"
            );
            Err(error.into())
        }
    }
}

fn verify_prefixed_hmac(
    signed_payload: &[u8],
    signature: &str,
    prefix: &str,
    secret: &[u8],
) -> bool {
    signature
        .strip_prefix(prefix)
        .is_some_and(|digest| verify_hmac_sha256(signed_payload, digest, secret))
}

fn base64url_decode(value: &str) -> Result<Vec<u8>> {
    let mut standard = value.replace('-', "+").replace('_', "/");
    standard.extend(std::iter::repeat_n('=', (4 - standard.len() % 4) % 4));
    crate::util::base64::decode(&standard)
        .map_err(|error| anyhow::anyhow!("invalid base64url value: {error}"))
}

fn base64url_encode(value: &[u8]) -> String {
    crate::util::base64::encode(value)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

/// Verify a monday.com HS256 webhook JWT and optional standard JWT time
/// claims. The verification uses the app signing secret, never an OAuth token.
pub fn verify_monday_jwt_at(token: &str, signing_secret: &[u8], now: DateTime<Utc>) -> Result<()> {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        bail!("monday authorization token must be a compact JWT");
    };
    let header_value: Value = serde_json::from_slice(&base64url_decode(header)?)?;
    if header_value.get("alg").and_then(Value::as_str) != Some("HS256") {
        bail!("monday authorization JWT must use HS256");
    }
    let claims: Value = serde_json::from_slice(&base64url_decode(payload)?)?;
    if claims
        .get("exp")
        .and_then(Value::as_i64)
        .is_some_and(|expiry| expiry <= now.timestamp())
    {
        bail!("monday authorization JWT has expired");
    }
    if claims
        .get("iat")
        .and_then(Value::as_i64)
        .is_some_and(|issued_at| issued_at > now.timestamp() + SLACK_MAX_FUTURE_SECONDS)
    {
        bail!("monday authorization JWT has an invalid future issue time");
    }
    // JWT signatures cover the original base64url segments, not a parsed and
    // re-serialized JSON representation (whose whitespace/key order may vary).
    let signed = format!("{header}.{payload}");
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(signing_secret)
        .map_err(|error| anyhow::anyhow!("monday signing secret is invalid: {error:?}"))?;
    mac.update(signed.as_bytes());
    let expected = base64url_encode(&mac.finalize().into_bytes());
    if !crate::util::constant_time::constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        bail!("monday authorization JWT did not verify");
    }
    Ok(())
}

/// Return monday.com's mandatory verification response, if this is a setup
/// challenge rather than an event. Challenges are never admitted as events.
pub fn monday_challenge_response(body: &[u8]) -> Result<Option<Value>> {
    let value: Value = serde_json::from_slice(body)?;
    let Some(challenge) = value.get("challenge").and_then(Value::as_str) else {
        return Ok(None);
    };
    if challenge.is_empty() || challenge.len() > 1024 {
        bail!("monday webhook challenge has invalid length");
    }
    Ok(Some(serde_json::json!({ "challenge": challenge })))
}

/// Verify Slack's `v0` request signature and freshness window. The caller
/// must pass the exact raw request body; JSON reserialization invalidates the
/// signature and must never be attempted.
pub fn verify_slack_signature_at(
    body: &[u8],
    signature: &str,
    timestamp: &str,
    secret: &[u8],
    now: DateTime<Utc>,
) -> Result<()> {
    verify_fresh_timestamp(timestamp, now, "Slack")?;
    let signed_payload = format!("v0:{timestamp}:").into_bytes();
    let mut signed_payload = signed_payload;
    signed_payload.extend_from_slice(body);
    if !verify_prefixed_hmac(&signed_payload, signature, "v0=", secret) {
        bail!("Slack request signature did not verify");
    }
    Ok(())
}

/// Verify, minimise, audit, and deduplicate a Slack event before dispatch.
pub fn accept_slack_event(request: InboundEvent<'_>, timestamp: &str) -> Result<EventEnvelope> {
    verify_slack_signature_at(
        request.body,
        request.signature,
        timestamp,
        request.secret,
        Utc::now(),
    )?;
    let event = EventEnvelope::from_payload(
        Provider::Slack,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Verify GitHub's `X-Hub-Signature-256` header. GitHub signs the exact raw
/// request body with `sha256=<hex digest>`.
pub fn verify_github_signature(body: &[u8], signature: &str, secret: &[u8]) -> Result<()> {
    if !verify_prefixed_hmac(body, signature, "sha256=", secret) {
        bail!("GitHub request signature did not verify");
    }
    Ok(())
}

/// Verify, minimise, audit, and deduplicate a GitHub webhook event.
pub fn accept_github_event(request: InboundEvent<'_>) -> Result<EventEnvelope> {
    verify_github_signature(request.body, request.signature, request.secret)?;
    let event = EventEnvelope::from_payload(
        Provider::Github,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Verify GitLab's Standard Webhooks signing-token format. GitLab signs
/// `{webhook-id}.{webhook-timestamp}.{raw-body}` with a base64-encoded key;
/// all supplied `v1,<base64>` signatures are checked in constant time.
pub fn verify_gitlab_signature_at(
    body: &[u8],
    webhook_id: &str,
    timestamp: &str,
    signatures: &str,
    signing_token: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    super::valid_identifier("GitLab webhook-id", webhook_id)?;
    verify_fresh_timestamp(timestamp, now, "GitLab")?;
    let token = signing_token
        .strip_prefix("whsec_")
        .unwrap_or(signing_token);
    let key = crate::util::base64::decode(token)
        .map_err(|error| anyhow::anyhow!("GitLab signing token is not valid base64: {error}"))?;
    if key.is_empty() {
        bail!("GitLab signing token must not be empty");
    }
    let signed_payload = format!("{webhook_id}.{timestamp}.").into_bytes();
    let mut signed_payload = signed_payload;
    signed_payload.extend_from_slice(body);
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(&key)
        .map_err(|error| anyhow::anyhow!("GitLab signing key is invalid: {error:?}"))?;
    mac.update(&signed_payload);
    let expected = format!(
        "v1,{}",
        crate::util::base64::encode(&mac.finalize().into_bytes())
    );
    if !signatures.split_ascii_whitespace().any(|candidate| {
        crate::util::constant_time::constant_time_eq(candidate.as_bytes(), expected.as_bytes())
    }) {
        bail!("GitLab request signature did not verify");
    }
    Ok(())
}

/// Verify GitLab's deprecated static header token in constant time. Do not use
/// this to accept an event: it provides neither integrity nor replay defense.
pub fn verify_gitlab_legacy_token(received: &str, secret: &[u8]) -> bool {
    crate::util::constant_time::constant_time_eq(received.as_bytes(), secret)
}

/// Verify, minimise, audit, and deduplicate a modern GitLab signed webhook.
pub fn accept_gitlab_event(
    request: InboundEvent<'_>,
    webhook_id: &str,
    timestamp: &str,
    signing_token: &str,
) -> Result<EventEnvelope> {
    verify_gitlab_signature_at(
        request.body,
        webhook_id,
        timestamp,
        request.signature,
        signing_token,
        Utc::now(),
    )?;
    let event = EventEnvelope::from_payload(
        Provider::Gitlab,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Verify Bitbucket Cloud's `X-Hub-Signature` HMAC header. Bitbucket uses the
/// same `sha256=<hex>` raw-body convention as GitHub, but has a distinct
/// provider envelope and delivery identity.
pub fn accept_bitbucket_event(request: InboundEvent<'_>) -> Result<EventEnvelope> {
    if !verify_prefixed_hmac(request.body, request.signature, "sha256=", request.secret) {
        bail!("Bitbucket request signature did not verify");
    }
    let event = EventEnvelope::from_payload(
        Provider::Bitbucket,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Verify, minimise, audit, and deduplicate a monday.com signed board event.
pub fn accept_monday_event(
    request: InboundEvent<'_>,
    signing_secret: &[u8],
) -> Result<EventEnvelope> {
    if monday_challenge_response(request.body)?.is_some() {
        bail!("monday webhook challenge must be answered, not processed as an event");
    }
    verify_monday_jwt_at(request.signature, signing_secret, Utc::now())?;
    let event = EventEnvelope::from_payload(
        Provider::Monday,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Verify, minimise, audit, and deduplicate a Jira Cloud secret-signed event.
pub fn accept_jira_event(request: InboundEvent<'_>) -> Result<EventEnvelope> {
    if !verify_prefixed_hmac(request.body, request.signature, "sha256=", request.secret) {
        bail!("Jira request signature did not verify");
    }
    let event = EventEnvelope::from_payload(
        Provider::Jira,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Accept a Microsoft Graph *basic* change notification after verifying the
/// tenant-scoped `clientState` configured with the subscription. Rich Graph
/// notifications additionally require JWT validation against Microsoft JWKS;
/// callers must complete that validation before using this basic primitive.
pub fn accept_microsoft365_basic_event(
    request: InboundEvent<'_>,
    subscription_id: &str,
    client_state: &str,
) -> Result<EventEnvelope> {
    super::valid_identifier("Microsoft Graph subscription_id", subscription_id)?;
    if !crate::util::constant_time::constant_time_eq(
        request.signature.as_bytes(),
        client_state.as_bytes(),
    ) {
        bail!("Microsoft Graph clientState did not verify");
    }
    let event = EventEnvelope::from_payload(
        Provider::Microsoft365,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Accept a Drive API webhook-channel notification. Drive echoes the opaque
/// channel token selected at subscription creation; it is compared in constant
/// time and the channel/message number becomes the durable delivery identity.
/// The event body may be empty because Drive change notifications are usually
/// header-only signals that require an authorized follow-up read.
pub struct GoogleDriveChannelEvent<'a> {
    pub repo_path: &'a Path,
    pub tenant_id: &'a str,
    pub channel_id: &'a str,
    pub channel_token: &'a str,
    pub expected_channel_token: &'a str,
    pub message_number: u64,
    pub resource_state: &'a str,
    pub body: &'a [u8],
}

pub fn accept_google_drive_channel_event(
    request: GoogleDriveChannelEvent<'_>,
) -> Result<EventEnvelope> {
    super::valid_identifier("Google Drive channel_id", request.channel_id)?;
    if request.message_number == 0 {
        bail!("Google Drive channel message number must be positive");
    }
    if !crate::util::constant_time::constant_time_eq(
        request.channel_token.as_bytes(),
        request.expected_channel_token.as_bytes(),
    ) {
        bail!("Google Drive channel token did not verify");
    }
    let event_id = format!("{}-{}", request.channel_id, request.message_number);
    let event = EventEnvelope::from_payload(
        Provider::GoogleDrive,
        request.tenant_id,
        event_id,
        request.resource_state,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Claims returned only after a deployment-provided OIDC verifier has checked
/// the JWT signature and expiration against the provider's JWKS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedOidcClaims {
    pub issuer: String,
    pub audience: String,
    pub email: String,
    pub email_verified: bool,
}

/// Boundary for an OIDC/JWKS verifier. Keeping it injected avoids accepting
/// decoded-but-unverified JWTs and lets enterprise deployments select their
/// approved trust store, cache, and outbound-proxy policy.
pub trait OidcVerifier {
    fn verify(&self, bearer_token: &str, expected_audience: &str) -> Result<VerifiedOidcClaims>;
}

/// Accept a Google Cloud Pub/Sub push after a real OIDC verifier has validated
/// the bearer token. Pub/Sub requires the configured audience, service-account
/// email, `email_verified`, and a Google Accounts issuer to all match.
pub fn accept_google_cloud_pubsub_event(
    request: InboundEvent<'_>,
    verifier: &impl OidcVerifier,
    expected_audience: &str,
    expected_service_account: &str,
) -> Result<EventEnvelope> {
    let token = request.signature.strip_prefix("Bearer ").ok_or_else(|| {
        anyhow::anyhow!("Google Cloud Pub/Sub authorization header must be Bearer JWT")
    })?;
    let claims = verifier.verify(token, expected_audience)?;
    if !matches!(
        claims.issuer.as_str(),
        "accounts.google.com" | "https://accounts.google.com"
    ) || claims.audience != expected_audience
        || claims.email != expected_service_account
        || !claims.email_verified
    {
        bail!("Google Cloud Pub/Sub OIDC claims did not match the configured subscription");
    }
    let event = EventEnvelope::from_payload(
        Provider::GoogleCloud,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Verify a WhatsApp Business / Meta `X-Hub-Signature-256` delivery and admit
/// it through the common privacy-minimised, replay-safe event path.
pub fn accept_whatsapp_business_event(request: InboundEvent<'_>) -> Result<EventEnvelope> {
    if !verify_prefixed_hmac(request.body, request.signature, "sha256=", request.secret) {
        bail!("WhatsApp Business request signature did not verify");
    }
    let event = EventEnvelope::from_payload(
        Provider::WhatsappBusiness,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

/// Return a WhatsApp webhook verification challenge only when its configured
/// verification token matches in constant time.
pub fn whatsapp_challenge_response(
    mode: &str,
    received_token: &str,
    expected_token: &str,
    challenge: &str,
) -> Result<Option<String>> {
    if mode != "subscribe" {
        return Ok(None);
    }
    if challenge.is_empty() || challenge.len() > 1024 {
        bail!("WhatsApp webhook challenge has invalid length");
    }
    if !crate::util::constant_time::constant_time_eq(
        received_token.as_bytes(),
        expected_token.as_bytes(),
    ) {
        bail!("WhatsApp webhook verification token did not verify");
    }
    Ok(Some(challenge.to_string()))
}

/// Admit an event from a customer-managed, authenticated ingress gateway.
/// This is used for cloud/ITSM gateways and local infrastructure observers:
/// their upstream IAM, mTLS, or service-account checks terminate at the
/// gateway, while this function verifies a separate per-connector HMAC before
/// accepting the event into Kaptaind.
pub fn accept_gateway_event(
    provider: Provider,
    request: InboundEvent<'_>,
) -> Result<EventEnvelope> {
    if !matches!(
        provider,
        Provider::Aws
            | Provider::GoogleCloud
            | Provider::Docker
            | Provider::Kubernetes
            | Provider::Hetzner
            | Provider::ArgoCd
            | Provider::Flux
            | Provider::ServiceNow
            | Provider::PagerDuty
    ) {
        bail!("gateway ingress is only supported for approved cloud, GitOps, ITSM, and incident providers");
    }
    if !verify_hmac_sha256(request.body, request.signature, request.secret) {
        bail!("integration gateway signature did not verify");
    }
    let event = EventEnvelope::from_payload(
        provider,
        request.tenant_id,
        request.event_id,
        request.kind,
        request.body,
    )?;
    accept_delivery(request.repo_path, &event)?;
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    fn hmac(payload: &[u8], secret: &[u8]) -> String {
        crate::util::hex::encode(hmac_bytes(payload, secret))
    }

    fn hmac_bytes(payload: &[u8], secret: &[u8]) -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(payload);
        mac.finalize().into_bytes().to_vec()
    }

    fn notification_connector() -> ConnectorConfig {
        let mut capabilities = std::collections::BTreeSet::new();
        capabilities.insert(Capability::SendNotification);
        ConnectorConfig {
            provider: Provider::Slack,
            mode: super::super::Mode::NotificationOnly,
            tenant_id: "acme".to_string(),
            endpoint: Some("https://93.184.216.34/hooks/kaptaind".to_string()),
            credential_ref: Some("vault:slack".to_string()),
            capabilities,
        }
    }

    #[test]
    fn slack_signature_requires_exact_body_and_fresh_timestamp() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let timestamp = "1700000000";
        let body = br#"{\"type\":\"event_callback\"}"#;
        let signed = format!("v0:{timestamp}:").into_bytes();
        let mut signed = signed;
        signed.extend_from_slice(body);
        let signature = format!("v0={}", hmac(&signed, b"slack-secret"));
        assert!(
            verify_slack_signature_at(body, &signature, timestamp, b"slack-secret", now).is_ok()
        );
        assert!(
            verify_slack_signature_at(b"altered", &signature, timestamp, b"slack-secret", now)
                .is_err()
        );
        assert!(
            verify_slack_signature_at(body, &signature, "1699999000", b"slack-secret", now)
                .is_err()
        );
    }

    #[test]
    fn github_event_is_authenticated_and_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let body = br#"{\"action\":\"opened\"}"#;
        let signature = format!("sha256={}", hmac(body, b"github-secret"));
        let request = || InboundEvent {
            repo_path: dir.path(),
            tenant_id: "acme",
            event_id: "delivery-1",
            kind: "pull_request",
            body,
            signature: &signature,
            secret: b"github-secret",
        };
        accept_github_event(request()).unwrap();
        assert!(accept_github_event(request()).is_err());
        let audit = std::fs::read_to_string(dir.path().join(".kaptaind/audit.jsonl")).unwrap();
        assert!(audit.contains("integration_delivery_accepted"));
        assert!(!audit.contains("opened"));
    }

    #[test]
    fn gitlab_standard_webhook_signature_is_fresh_and_integrity_protected() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let body = br#"{\"object_kind\":\"merge_request\"}"#;
        let webhook_id = "msg-42";
        let timestamp = "1700000000";
        let key = b"gitlab-signing-key";
        let token = format!("whsec_{}", crate::util::base64::encode(key));
        let mut signed = format!("{webhook_id}.{timestamp}.").into_bytes();
        signed.extend_from_slice(body);
        let signature = format!(
            "v1,{}",
            crate::util::base64::encode(&hmac_bytes(&signed, key))
        );
        assert!(
            verify_gitlab_signature_at(body, webhook_id, timestamp, &signature, &token, now)
                .is_ok()
        );
        assert!(verify_gitlab_signature_at(
            b"altered", webhook_id, timestamp, &signature, &token, now
        )
        .is_err());
        assert!(verify_gitlab_legacy_token("secret", b"secret"));
        assert!(!verify_gitlab_legacy_token("secret", b"other"));
    }

    #[test]
    fn bitbucket_signature_is_provider_bound_and_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let body = br#"{\"push\":{}}"#;
        let signature = format!("sha256={}", hmac(body, b"bitbucket-secret"));
        let request = || InboundEvent {
            repo_path: dir.path(),
            tenant_id: "acme",
            event_id: "delivery-2",
            kind: "repo_push",
            body,
            signature: &signature,
            secret: b"bitbucket-secret",
        };
        accept_bitbucket_event(request()).unwrap();
        assert!(accept_bitbucket_event(request()).is_err());
    }

    #[test]
    fn microsoft_graph_basic_notifications_require_client_state() {
        let dir = tempfile::tempdir().unwrap();
        let request = InboundEvent {
            repo_path: dir.path(),
            tenant_id: "acme",
            event_id: "graph-1",
            kind: "drive_item_changed",
            body: b"private graph notification",
            signature: "state-secret",
            secret: b"unused",
        };
        let bad = InboundEvent {
            repo_path: dir.path(),
            tenant_id: "acme",
            event_id: "graph-2",
            kind: "drive_item_changed",
            body: b"private graph notification",
            signature: "wrong",
            secret: b"unused",
        };
        assert!(accept_microsoft365_basic_event(request, "subscription-1", "state-secret").is_ok());
        assert!(accept_microsoft365_basic_event(bad, "subscription-1", "state-secret").is_err());
    }

    #[test]
    fn google_drive_channel_notifications_use_token_and_message_identity() {
        let dir = tempfile::tempdir().unwrap();
        let accepted = || {
            accept_google_drive_channel_event(GoogleDriveChannelEvent {
                repo_path: dir.path(),
                tenant_id: "acme",
                channel_id: "channel-1",
                channel_token: "token-secret",
                expected_channel_token: "token-secret",
                message_number: 2,
                resource_state: "change",
                body: b"",
            })
        };
        assert!(accepted().is_ok());
        assert!(accepted().is_err());
        assert!(accept_google_drive_channel_event(GoogleDriveChannelEvent {
            repo_path: dir.path(),
            tenant_id: "acme",
            channel_id: "channel-2",
            channel_token: "wrong",
            expected_channel_token: "token-secret",
            message_number: 1,
            resource_state: "sync",
            body: b"",
        })
        .is_err());
    }

    #[test]
    fn monday_jwt_and_challenge_are_verified_without_retaining_payloads() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let header = base64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = base64url_encode(br#"{"exp":1700000300,"iat":1699999900}"#);
        let signed = format!("{header}.{payload}");
        let jwt = format!(
            "{signed}.{}",
            base64url_encode(&hmac_bytes(signed.as_bytes(), b"monday-secret"))
        );
        let verified = verify_monday_jwt_at(&jwt, b"monday-secret", now);
        assert!(verified.is_ok(), "{verified:?}");
        assert!(verify_monday_jwt_at(&jwt, b"other", now).is_err());
        assert_eq!(
            monday_challenge_response(br#"{"challenge":"verify-me"}"#).unwrap(),
            Some(serde_json::json!({"challenge": "verify-me"}))
        );
    }

    #[test]
    fn jira_signature_is_required_and_deliveries_are_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let body = br#"{\"issue\":{\"id\":\"1\"}}"#;
        let signature = format!("sha256={}", hmac(body, b"jira-secret"));
        let request = || InboundEvent {
            repo_path: dir.path(),
            tenant_id: "acme",
            event_id: "jira-1",
            kind: "issue_updated",
            body,
            signature: &signature,
            secret: b"jira-secret",
        };
        assert!(accept_jira_event(request()).is_ok());
        assert!(accept_jira_event(request()).is_err());
    }

    struct TestOidcVerifier(VerifiedOidcClaims);

    impl OidcVerifier for TestOidcVerifier {
        fn verify(
            &self,
            _bearer_token: &str,
            _expected_audience: &str,
        ) -> Result<VerifiedOidcClaims> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn google_cloud_pubsub_requires_verified_matching_oidc_claims() {
        let dir = tempfile::tempdir().unwrap();
        let request = || InboundEvent {
            repo_path: dir.path(),
            tenant_id: "acme",
            event_id: "pubsub-1",
            kind: "deployment_changed",
            body: b"private cloud event",
            signature: "Bearer verified-token",
            secret: b"unused",
        };
        let valid = TestOidcVerifier(VerifiedOidcClaims {
            issuer: "https://accounts.google.com".to_string(),
            audience: "https://kaptaind.example/events".to_string(),
            email: "kaptaind-push@project.iam.gserviceaccount.com".to_string(),
            email_verified: true,
        });
        assert!(accept_google_cloud_pubsub_event(
            request(),
            &valid,
            "https://kaptaind.example/events",
            "kaptaind-push@project.iam.gserviceaccount.com",
        )
        .is_ok());
        let invalid = TestOidcVerifier(VerifiedOidcClaims {
            email_verified: false,
            ..valid.0.clone()
        });
        let retry = InboundEvent {
            event_id: "pubsub-2",
            ..request()
        };
        assert!(accept_google_cloud_pubsub_event(
            retry,
            &invalid,
            "https://kaptaind.example/events",
            "kaptaind-push@project.iam.gserviceaccount.com",
        )
        .is_err());
    }

    #[test]
    fn whatsapp_signature_and_subscription_challenge_are_required() {
        let dir = tempfile::tempdir().unwrap();
        let body = br#"{\"entry\":[]}"#;
        let signature = format!("sha256={}", hmac(body, b"whatsapp-secret"));
        let request = || InboundEvent {
            repo_path: dir.path(),
            tenant_id: "acme",
            event_id: "wa-1",
            kind: "messages",
            body,
            signature: &signature,
            secret: b"whatsapp-secret",
        };
        assert!(accept_whatsapp_business_event(request()).is_ok());
        assert_eq!(
            whatsapp_challenge_response("subscribe", "verify", "verify", "challenge-1").unwrap(),
            Some("challenge-1".to_string())
        );
        assert!(whatsapp_challenge_response("subscribe", "bad", "verify", "challenge-1").is_err());
    }

    #[test]
    fn gateway_ingress_is_limited_to_infrastructure_observers() {
        let dir = tempfile::tempdir().unwrap();
        let body = b"private cluster state";
        let signature = hmac(body, b"observer-secret");
        assert!(accept_gateway_event(
            Provider::Kubernetes,
            InboundEvent {
                repo_path: dir.path(),
                tenant_id: "acme",
                event_id: "pod-1",
                kind: "pod_updated",
                body,
                signature: &signature,
                secret: b"observer-secret",
            }
        )
        .is_ok());
        assert!(accept_gateway_event(
            Provider::Slack,
            InboundEvent {
                repo_path: dir.path(),
                tenant_id: "acme",
                event_id: "evt-2",
                kind: "message",
                body,
                signature: &signature,
                secret: b"observer-secret",
            }
        )
        .is_err());
    }

    #[test]
    fn notification_delivery_requires_configured_capability_and_policy() {
        let dir = tempfile::tempdir().unwrap();
        let connector = notification_connector();
        let policy = crate::daemon::policy::Policy::default();
        let payload = serde_json::json!({"text": "private release update"});
        let request = OutboundNotification {
            repo_path: dir.path(),
            policy: &policy,
            connector: &connector,
            idempotency_key: "release-42",
            payload: &payload,
            authorization: None,
        };
        assert!(validate_notification(&request).is_ok());
        let mut disabled = connector.clone();
        disabled.capabilities.clear();
        let blocked = OutboundNotification {
            connector: &disabled,
            ..request
        };
        assert!(validate_notification(&blocked).is_err());
    }
}
