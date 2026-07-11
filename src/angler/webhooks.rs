//! Enhanced webhook system for Angler.
//!
//! Features:
//! - Multiple endpoint support with per-endpoint configuration
//! - Event filtering and subscription
//! - HMAC signature verification
//! - Exponential backoff retry
//! - Rate limiting
//! - Custom headers

use crate::angler::config::{RetryConfig, SignatureAlgorithm, WebhookEndpoint, WebhooksConfig};
use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::{Sha256, Sha512};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, warn};

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

/// Webhook event types.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEvent {
    /// Commit was created
    Commit {
        version: String,
        score: f32,
        message: String,
        files_changed: usize,
        cluster_id: String,
    },
    /// Push completed
    Push {
        branch: String,
        commits: usize,
        remote: String,
    },
    /// Analysis completed
    Analysis {
        cluster_id: String,
        score: f32,
        bump: String,
    },
    /// Error occurred
    Error {
        error: String,
        context: Option<String>,
    },
    /// Custom event
    Custom {
        event_type: String,
        payload: serde_json::Value,
    },
}

impl WebhookEvent {
    /// Get the event type name.
    pub fn event_type(&self) -> &str {
        match self {
            WebhookEvent::Commit { .. } => "commit",
            WebhookEvent::Push { .. } => "push",
            WebhookEvent::Analysis { .. } => "analysis",
            WebhookEvent::Error { .. } => "error",
            WebhookEvent::Custom { event_type, .. } => event_type,
        }
    }

    /// Get the event payload as JSON.
    pub fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self).context("Failed to serialize event")
    }
}

/// Webhook delivery result.
#[derive(Debug, Clone)]
pub struct DeliveryResult {
    /// Whether delivery succeeded
    pub success: bool,
    /// HTTP status code
    pub status_code: Option<u16>,
    /// Number of attempts made
    pub attempts: u32,
    /// Total duration
    pub duration_ms: u64,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Response body (if any)
    pub response_body: Option<String>,
}

/// Rate limit tracker for endpoints.
#[derive(Debug, Clone)]
struct RateLimitState {
    requests: Vec<Instant>,
}

impl RateLimitState {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
        }
    }

    fn check_rate_limit(&mut self, limit_per_min: u32) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(60);

        // Remove old requests outside the window
        self.requests.retain(|&t| now.duration_since(t) < window);

        if self.requests.len() >= limit_per_min as usize {
            false
        } else {
            self.requests.push(now);
            true
        }
    }
}

/// Enhanced webhook manager.
pub struct WebhookManager {
    config: WebhooksConfig,
    client: reqwest::Client,
    rate_limits: Arc<RwLock<HashMap<String, RateLimitState>>>,
}

impl WebhookManager {
    /// Create a new webhook manager.
    pub fn new(config: &WebhooksConfig) -> Result<Self> {
        let client = crate::util::http::hardened_client(Duration::from_secs(30));

        Ok(Self {
            config: config.clone(),
            client,
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Send an event to all subscribed endpoints.
    pub async fn broadcast_event(
        &self,
        event: &WebhookEvent,
        file_changes: &[std::path::PathBuf],
    ) -> Vec<(String, DeliveryResult)> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut results = Vec::new();

        for endpoint in &self.config.endpoints {
            if !self.should_send_to_endpoint(endpoint, event, file_changes) {
                continue;
            }

            let result = self.send_to_endpoint(endpoint, event).await;
            results.push((endpoint.id.clone(), result));
        }

        results
    }

    /// Send an event to a specific endpoint.
    pub async fn send_to_endpoint(
        &self,
        endpoint: &WebhookEndpoint,
        event: &WebhookEvent,
    ) -> DeliveryResult {
        let start = Instant::now();

        // Check rate limit
        if let Some(limit) = endpoint.rate_limit_per_min {
            let mut limits = self.rate_limits.write().await;
            let state = limits
                .entry(endpoint.id.clone())
                .or_insert_with(RateLimitState::new);

            if !state.check_rate_limit(limit) {
                return DeliveryResult {
                    success: false,
                    status_code: None,
                    attempts: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some("Rate limit exceeded".to_string()),
                    response_body: None,
                };
            }
        }

        // Re-validate at send time so a private/metadata target cannot be
        // reached even if config changed since validation or was never validated.
        if let Err(err) = crate::util::http::validate_outbound_url(&endpoint.url) {
            return DeliveryResult {
                success: false,
                status_code: None,
                attempts: 0,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("unsafe webhook URL: {err}")),
                response_body: None,
            };
        }

        // Build payload
        let payload = match self.build_payload(event) {
            Ok(p) => p,
            Err(e) => {
                return DeliveryResult {
                    success: false,
                    status_code: None,
                    attempts: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("Payload build error: {}", e)),
                    response_body: None,
                };
            }
        };

        // Get retry configuration
        let retry_config = endpoint
            .retry
            .as_ref()
            .unwrap_or(&self.config.default_retry);

        // Attempt delivery with retries
        let mut last_error = None;
        let mut last_status = None;
        let mut attempts = 0;

        for attempt in 0..retry_config.max_attempts {
            attempts = attempt + 1;

            match self.attempt_delivery(endpoint, &payload).await {
                Ok((status, body)) => {
                    if status.is_success() {
                        return DeliveryResult {
                            success: true,
                            status_code: Some(status.as_u16()),
                            attempts,
                            duration_ms: start.elapsed().as_millis() as u64,
                            error: None,
                            response_body: Some(body),
                        };
                    } else if status.is_server_error() || is_retriable_status(status) {
                        // Server error or retriable client error - retry
                        last_status = Some(status);
                        warn!(
                            "Webhook {} returned {}, retrying ({}/{})",
                            endpoint.id,
                            status,
                            attempt + 1,
                            retry_config.max_attempts
                        );
                    } else {
                        // Client error - don't retry
                        return DeliveryResult {
                            success: false,
                            status_code: Some(status.as_u16()),
                            attempts,
                            duration_ms: start.elapsed().as_millis() as u64,
                            error: Some(format!("HTTP error: {}", status)),
                            response_body: Some(body),
                        };
                    }
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                    warn!(
                        "Webhook {} delivery error: {}, retrying ({}/{})",
                        endpoint.id,
                        e,
                        attempt + 1,
                        retry_config.max_attempts
                    );
                }
            }

            // Calculate backoff delay
            if attempt < retry_config.max_attempts - 1 {
                let delay_ms = calculate_backoff(attempt, retry_config);
                sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        // All retries exhausted
        DeliveryResult {
            success: false,
            status_code: last_status.map(|s| s.as_u16()),
            attempts,
            duration_ms: start.elapsed().as_millis() as u64,
            error: last_error.or_else(|| Some("All retry attempts exhausted".to_string())),
            response_body: None,
        }
    }

    /// Send a test ping to an endpoint.
    pub async fn send_test_ping(&self, endpoint: &WebhookEndpoint) -> DeliveryResult {
        let ping = WebhookEvent::Custom {
            event_type: "ping".to_string(),
            payload: serde_json::json!({
                "message": "Kaptaind webhook test ping",
                "timestamp": SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            }),
        };

        self.send_to_endpoint(endpoint, &ping).await
    }

    /// Validate webhook configuration.
    pub fn validate_endpoint(&self, endpoint: &WebhookEndpoint) -> Vec<String> {
        let mut errors = Vec::new();

        // Validate URL (scheme, host reachability, and SSRF safety).
        if endpoint.url.is_empty() {
            errors.push("URL is required".to_string());
        } else if let Err(err) = crate::util::http::validate_outbound_url(&endpoint.url) {
            errors.push(format!("unsafe webhook URL: {err}"));
        }

        // Validate ID
        if endpoint.id.is_empty() {
            errors.push("ID is required".to_string());
        }

        // Validate secret if signature verification is enabled
        if endpoint.verify_signature && endpoint.secret.is_none() {
            errors.push("Secret is required when signature verification is enabled".to_string());
        }

        // Validate rate limit
        if let Some(limit) = endpoint.rate_limit_per_min {
            if limit == 0 {
                errors.push("Rate limit must be greater than 0".to_string());
            }
        }

        errors
    }

    // =============================================================================
    // Internal Methods
    // =============================================================================

    fn should_send_to_endpoint(
        &self,
        endpoint: &WebhookEndpoint,
        event: &WebhookEvent,
        file_changes: &[std::path::PathBuf],
    ) -> bool {
        // Check if endpoint is subscribed to this event type
        if !endpoint.events.is_empty() && !endpoint.events.contains(&event.event_type().to_string())
        {
            return false;
        }

        // Check file filters
        if !endpoint.file_filters.is_empty() && !file_changes.is_empty() {
            let matches = file_changes.iter().any(|file| {
                endpoint.file_filters.iter().any(|pattern| {
                    glob::Pattern::new(pattern)
                        .map(|p| {
                            let path_str = file.to_string_lossy();
                            p.matches(&path_str)
                        })
                        .unwrap_or(false)
                })
            });

            if !matches {
                return false;
            }
        }

        true
    }

    fn build_payload(&self, event: &WebhookEvent) -> Result<serde_json::Value> {
        let mut payload = event.to_json()?;

        // Add common fields
        if let serde_json::Value::Object(ref mut map) = payload {
            map.insert(
                "timestamp".to_string(),
                serde_json::json!(SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs()),
            );
            map.insert(
                "event_id".to_string(),
                serde_json::json!(uuid::Uuid::new_v4().to_string()),
            );
        }

        Ok(payload)
    }

    async fn attempt_delivery(
        &self,
        endpoint: &WebhookEndpoint,
        payload: &serde_json::Value,
    ) -> Result<(reqwest::StatusCode, String), reqwest::Error> {
        let body = serde_json::to_string(payload).unwrap_or_default();

        let mut request = self
            .client
            .post(&endpoint.url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "kaptaind/1.0")
            .header("X-Kaptaind-Event", endpoint.id.clone());

        // Add custom headers
        for (key, value) in &endpoint.headers {
            request = request.header(key, value);
        }

        // Add signature if enabled
        if endpoint.verify_signature {
            if let Some(ref secret) = endpoint.secret {
                let signature = self.sign_payload(&body, secret);
                request = request.header(&self.config.signature.header_name, signature);
            }
        }

        // Add timestamp if enabled
        if self.config.signature.include_timestamp {
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            request = request.header("X-Kaptaind-Timestamp", timestamp.to_string());
        }

        let response = request.body(body).send().await?;
        let status = response.status();

        // Read response body for logging (capped + control-stripped to avoid
        // log injection and unbounded payload logging).
        let body_text = response.text().await.unwrap_or_default();
        let mut body_preview: String = body_text
            .chars()
            .take(200)
            .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
            .collect();
        if body_text.chars().count() > 200 {
            body_preview.push('…');
        }

        debug!(
            "Webhook {} response: {} - body: {}",
            endpoint.id, status, body_preview
        );

        Ok((status, body_text))
    }

    fn sign_payload(&self, payload: &str, secret: &str) -> String {
        let timestamp = if self.config.signature.include_timestamp {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string()
        } else {
            String::new()
        };

        let data = format!("{}.{}", timestamp, payload);

        match self.config.signature.algorithm {
            SignatureAlgorithm::HmacSha256 => {
                let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                    .expect("HMAC can take key of any size");
                mac.update(data.as_bytes());
                let result = mac.finalize();
                let bytes = result.into_bytes();
                format!("sha256={}", crate::util::hex::encode(bytes))
            }
            SignatureAlgorithm::HmacSha512 => {
                let mut mac = HmacSha512::new_from_slice(secret.as_bytes())
                    .expect("HMAC can take key of any size");
                mac.update(data.as_bytes());
                let result = mac.finalize();
                let bytes = result.into_bytes();
                format!("sha512={}", crate::util::hex::encode(bytes))
            }
            SignatureAlgorithm::Ed25519 => {
                // Ed25519 signing would require additional dependencies
                // For now, fall back to HMAC-SHA256
                warn!("Ed25519 signing not implemented, using HMAC-SHA256");
                let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                    .expect("HMAC can take key of any size");
                mac.update(data.as_bytes());
                let result = mac.finalize();
                let bytes = result.into_bytes();
                format!("sha256={}", crate::util::hex::encode(bytes))
            }
        }
    }
}

/// Calculate backoff delay with exponential backoff and jitter.
fn calculate_backoff(attempt: u32, config: &RetryConfig) -> u64 {
    let base_delay = config.initial_delay_ms as f64;
    let multiplier = config.backoff_multiplier.powi(attempt as i32);
    let delay = base_delay * multiplier;

    // Add jitter (±25%)
    let jitter = delay * 0.25;
    let jittered = delay + (jitter * (rand::random::<f64>() - 0.5));

    // Cap at max delay
    let capped = jittered.min(config.max_delay_ms as f64);

    capped as u64
}

/// Check if an HTTP status code warrants a retry.
fn is_retriable_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status.as_u16(),
        408 | // Request Timeout
        429 | // Too Many Requests
        502 | // Bad Gateway
        503 | // Service Unavailable
        504 // Gateway Timeout
    )
}

/// Verify webhook signature.
pub fn verify_signature(
    payload: &str,
    signature: &str,
    secret: &str,
    algorithm: SignatureAlgorithm,
) -> bool {
    let expected = match algorithm {
        SignatureAlgorithm::HmacSha256 => {
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(payload.as_bytes());
            let result = mac.finalize();
            format!("sha256={}", crate::util::hex::encode(result.into_bytes()))
        }
        SignatureAlgorithm::HmacSha512 => {
            let mut mac = HmacSha512::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(payload.as_bytes());
            let result = mac.finalize();
            format!("sha512={}", crate::util::hex::encode(result.into_bytes()))
        }
        SignatureAlgorithm::Ed25519 => {
            // Not implemented
            return false;
        }
    };

    // Constant-time comparison to prevent timing attacks
    crate::util::constant_time::constant_time_eq(signature.as_bytes(), expected.as_bytes())
}

/// Verify a webhook signature that embeds a Unix timestamp, rejecting replays
/// whose timestamp skew exceeds `max_skew_secs`.
///
/// `timestamped_payload` must be in the `"<unix_ts>.<payload>"` form produced
/// by [`WebhookSender::sign_payload`] when timestamps are enabled. The HMAC is
/// computed over the *entire* string (timestamp + "." + payload), so the
/// timestamp is covered by the signature and cannot be stripped or altered.
///
/// Returns `false` when the timestamp is missing, unparseable, outside the
/// allowed skew window, or the signature does not match.
///
/// Note: full replay protection also requires a nonce/jti cache on the
/// receiving side. That is deferred until kaptaind ships an inbound webhook
/// listener; callers without a receiver should use [`verify_signature`].
pub fn verify_signature_with_timestamp(
    timestamped_payload: &str,
    signature: &str,
    secret: &str,
    algorithm: SignatureAlgorithm,
    max_skew_secs: u64,
) -> bool {
    // Split on the first '.' only — the payload itself may contain dots.
    let (ts_str, payload) = match timestamped_payload.split_once('.') {
        Some(pair) => pair,
        None => return false,
    };

    let ts: u64 = match ts_str.parse() {
        Ok(v) => v,
        Err(_) => return false,
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let skew = now.abs_diff(ts);
    if skew > max_skew_secs {
        return false;
    }

    // Recompute over the canonical "<ts>.<payload>" form (not the rejoined
    // value, to guarantee the signed bytes match the producer exactly).
    let data = format!("{}.{}", ts_str, payload);

    let expected = match algorithm {
        SignatureAlgorithm::HmacSha256 => {
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(data.as_bytes());
            format!(
                "sha256={}",
                crate::util::hex::encode(mac.finalize().into_bytes())
            )
        }
        SignatureAlgorithm::HmacSha512 => {
            let mut mac = HmacSha512::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(data.as_bytes());
            format!(
                "sha512={}",
                crate::util::hex::encode(mac.finalize().into_bytes())
            )
        }
        SignatureAlgorithm::Ed25519 => return false,
    };

    crate::util::constant_time::constant_time_eq(signature.as_bytes(), expected.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_event_types() {
        let commit = WebhookEvent::Commit {
            version: "1.0.0".to_string(),
            score: 0.5,
            message: "test".to_string(),
            files_changed: 3,
            cluster_id: "abc".to_string(),
        };
        assert_eq!(commit.event_type(), "commit");

        let error = WebhookEvent::Error {
            error: "test".to_string(),
            context: None,
        };
        assert_eq!(error.event_type(), "error");
    }

    #[test]
    fn test_is_retriable_status() {
        assert!(is_retriable_status(reqwest::StatusCode::REQUEST_TIMEOUT));
        assert!(is_retriable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retriable_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(!is_retriable_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!is_retriable_status(reqwest::StatusCode::NOT_FOUND));
    }

    #[test]
    fn test_rate_limit() {
        let mut state = RateLimitState::new();
        assert!(state.check_rate_limit(3));
        assert!(state.check_rate_limit(3));
        assert!(state.check_rate_limit(3));
        assert!(!state.check_rate_limit(3));
    }

    #[test]
    fn test_calculate_backoff() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        };

        let d0 = calculate_backoff(0, &config);
        assert!((750..=1250).contains(&d0)); // ~1000ms with jitter

        let d1 = calculate_backoff(1, &config);
        assert!((1500..=2500).contains(&d1)); // ~2000ms with jitter

        let d2 = calculate_backoff(2, &config);
        assert!((3000..=5000).contains(&d2)); // ~4000ms with jitter
    }

    #[test]
    fn test_signature_verification() {
        let payload = r#"{"test": "data"}"#;
        let secret = "my_secret";

        // Create a signature
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let signature = format!(
            "sha256={}",
            crate::util::hex::encode(mac.finalize().into_bytes())
        );

        // Verify it
        assert!(verify_signature(
            payload,
            &signature,
            secret,
            SignatureAlgorithm::HmacSha256
        ));

        // Wrong secret should fail
        assert!(!verify_signature(
            payload,
            &signature,
            "wrong_secret",
            SignatureAlgorithm::HmacSha256
        ));
    }

    fn sign_timestamped(ts: u64, payload: &str, secret: &str) -> String {
        let data = format!("{}.{}", ts, payload);
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(data.as_bytes());
        format!(
            "sha256={}",
            crate::util::hex::encode(mac.finalize().into_bytes())
        )
    }

    #[test]
    fn test_verify_signature_with_timestamp_accepts_fresh() {
        let secret = "ts_secret";
        let payload = r#"{"commit":"abc"}"#;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let signed = format!("{}.{}", now, payload);
        let sig = sign_timestamped(now, payload, secret);

        assert!(verify_signature_with_timestamp(
            &signed,
            &sig,
            secret,
            SignatureAlgorithm::HmacSha256,
            300,
        ));
    }

    #[test]
    fn test_verify_signature_with_timestamp_rejects_stale() {
        let secret = "ts_secret";
        let payload = "body";
        let old_ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 10_000;
        let signed = format!("{}.{}", old_ts, payload);
        let sig = sign_timestamped(old_ts, payload, secret);

        assert!(!verify_signature_with_timestamp(
            &signed,
            &sig,
            secret,
            SignatureAlgorithm::HmacSha256,
            300,
        ));
    }

    #[test]
    fn test_verify_signature_with_timestamp_rejects_tampered_timestamp() {
        let secret = "ts_secret";
        let payload = "body.with.dots";
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let sig = sign_timestamped(now, payload, secret);
        // Attacker rewinds the timestamp but keeps the old signature.
        let tampered = format!("{}.{}", now - 10_000, payload);

        assert!(!verify_signature_with_timestamp(
            &tampered,
            &sig,
            secret,
            SignatureAlgorithm::HmacSha256,
            300,
        ));
    }

    #[test]
    fn test_delivery_result() {
        let result = DeliveryResult {
            success: true,
            status_code: Some(200),
            attempts: 1,
            duration_ms: 100,
            error: None,
            response_body: Some("OK".to_string()),
        };

        assert!(result.success);
        assert_eq!(result.status_code, Some(200));
    }

    #[tokio::test]
    async fn test_webhook_manager_creation() {
        let config = WebhooksConfig::default();
        let manager = WebhookManager::new(&config).unwrap();
        assert!(!manager.config.enabled);
    }

    #[test]
    fn test_validate_endpoint() {
        let config = WebhooksConfig::default();
        let manager = WebhookManager::new(&config).unwrap();

        let valid = WebhookEndpoint {
            id: "test".to_string(),
            url: "https://example.com/webhook".to_string(),
            events: vec![],
            headers: HashMap::new(),
            retry: None,
            verify_signature: false,
            secret: None,
            file_filters: vec![],
            rate_limit_per_min: Some(60),
        };

        let errors = manager.validate_endpoint(&valid);
        assert!(errors.is_empty());

        let invalid = WebhookEndpoint {
            id: "".to_string(),
            url: "ftp://example.com".to_string(),
            events: vec![],
            headers: HashMap::new(),
            retry: None,
            verify_signature: true,
            secret: None,
            file_filters: vec![],
            rate_limit_per_min: Some(0),
        };

        let errors = manager.validate_endpoint(&invalid);
        assert_eq!(errors.len(), 4);
    }
}
