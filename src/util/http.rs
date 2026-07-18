//! Hardened outbound HTTP client and SSRF guard.
//!
//! All outbound requests in kaptaind (webhooks, LLM/TTS providers, S3, GitHub)
//! must go through [`hardened_client`] and must call [`validate_outbound_url`]
//! on the destination first. URLs are sourced from `kaptaind.toml` / `.env` and
//! are therefore attacker-influenced (a cloned repo can ship a malicious config),
//! so we:
//!   * require TLS (`https`) except for explicit loopback dev opt-in,
//!   * refuse destinations that resolve to loopback / private / link-local /
//!     cloud-metadata ranges,
//!   * disable automatic redirects so a 30x cannot re-target a signed request to
//!     an internal host,
//!   * ignore proxy environment variables (a hijacked `HTTPS_PROXY` cannot
//!     reroute traffic) — outbound connections go direct.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect::Policy;
use reqwest::Url;

#[derive(Clone, Copy)]
struct HardenedResolver {
    allow_localhost: bool,
}

impl Resolve for HardenedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        let allow_localhost = self.allow_localhost;
        Box::pin(async move {
            let resolved = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?
                .collect::<Vec<_>>();
            validate_resolved_addrs(&host, &resolved, allow_localhost)
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { error.into() })?;
            Ok(Box::new(resolved.into_iter()) as Addrs)
        })
    }
}

/// Build a `reqwest::Client` with kaptaind's hardened defaults.
///
/// The client enforces a connect + overall request timeout, never follows
/// redirects, validates TLS certificates (rustls default), and ignores proxy
/// environment variables.
pub fn hardened_client(timeout: Duration) -> reqwest::Client {
    let allow_localhost = matches!(
        std::env::var("KAPTAIND_ALLOW_INSECURE_HTTP").as_deref(),
        Ok("1")
    );
    build_hardened_client(timeout, allow_localhost)
}

/// Hardened client for inference endpoints. Explicit localhost is permitted
/// for local model servers, while DNS names that rebind to local addresses
/// remain rejected at connection time.
pub fn hardened_inference_client(timeout: Duration) -> reqwest::Client {
    build_hardened_client(timeout, true)
}

fn build_hardened_client(timeout: Duration, allow_localhost: bool) -> reqwest::Client {
    let result = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .redirect(Policy::none())
        .no_proxy()
        .dns_resolver(HardenedResolver { allow_localhost })
        .build();
    match result {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(
                %error,
                component = "outbound_http",
                "failed to construct hardened HTTP client"
            );
            panic!("failed to construct hardened HTTP client: {error}");
        }
    }
}

fn validate_resolved_addrs(
    host: &str,
    addrs: &[std::net::SocketAddr],
    allow_localhost: bool,
) -> Result<()> {
    if addrs.is_empty() {
        bail!("DNS resolved no addresses for {host:?}");
    }
    let explicit_localhost = host.eq_ignore_ascii_case("localhost");
    for addr in addrs {
        if allow_localhost && explicit_localhost {
            if !addr.ip().is_loopback() {
                bail!(
                    "refusing connection to localhost: DNS resolved non-loopback address {}",
                    addr.ip()
                );
            }
        } else if is_disallowed_ip(&addr.ip()) {
            bail!(
                "refusing connection to {host:?}: DNS resolved disallowed address {}",
                addr.ip()
            );
        }
    }
    Ok(())
}

/// Validate that `raw` is a safe destination for an outbound request.
///
/// Returns `Ok(())` only if the URL parses, uses an acceptable scheme, and its
/// host does not resolve to a disallowed (loopback/private/link-local/metadata)
/// address. DNS failures are treated as a rejection (fail closed).
pub fn validate_outbound_url(raw: &str) -> Result<()> {
    let url = Url::parse(raw).map_err(|e| anyhow!("invalid URL {raw:?}: {e}"))?;

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("URL has no host: {raw:?}"))?;

    let scheme = url.scheme();
    let loopback_dev = matches!(
        std::env::var("KAPTAIND_ALLOW_INSECURE_HTTP").as_deref(),
        Ok("1")
    );
    match scheme {
        "https" => {}
        "http" if loopback_dev && is_loopback_host(host) => {
            // Explicit loopback-dev opt-in: accept plain HTTP to a loopback host
            // and skip the SSRF IP filter (which would otherwise reject 127.0.0.1).
            return Ok(());
        }
        "http" => bail!(
            "refusing non-TLS outbound URL {raw:?} \
             (set KAPTAIND_ALLOW_INSECURE_HTTP=1 only for loopback dev)"
        ),
        other => bail!("refusing outbound URL with unsupported scheme {other:?}: {raw:?}"),
    }

    // IP literal: classify directly, no DNS.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_disallowed_ip(&ip) {
            bail!("refusing outbound URL {raw:?}: host {ip} is a disallowed address");
        }
        return Ok(());
    }

    // Hostname: resolve and reject if *any* address is disallowed.
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|e| anyhow!("DNS resolution failed for {host:?}: {e}"))?
        .collect();
    if addrs.is_empty() {
        bail!("DNS resolved no addresses for {host:?}");
    }
    for addr in &addrs {
        if is_disallowed_ip(&addr.ip()) {
            bail!(
                "refusing outbound URL {raw:?}: {host:?} resolves to disallowed address {}",
                addr.ip()
            );
        }
    }

    Ok(())
}

/// Validate an inference endpoint URL. Behaves like [`validate_outbound_url`]
/// but additionally permits plaintext `http` to a loopback host (local model
/// servers such as Ollama) without requiring the global dev opt-in. Remote
/// hosts still require `https` and must pass the SSRF guard.
pub fn validate_inference_url(raw: &str) -> Result<()> {
    let url = Url::parse(raw).map_err(|e| anyhow!("invalid URL {raw:?}: {e}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("URL has no host: {raw:?}"))?;
    if url.scheme() == "http" && is_loopback_host(host) {
        return Ok(());
    }
    validate_outbound_url(raw)
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn is_disallowed_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_disallowed(*v4),
        IpAddr::V6(v6) => v6_disallowed(v6),
    }
}

fn v4_disallowed(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    if ip.is_loopback() || ip.is_unspecified() || ip.is_broadcast() {
        return true;
    }
    // RFC 1918.
    let rfc1918 = a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168);
    // 169.254.0.0/16 link-local (includes 169.254.169.254 metadata).
    let link_local = a == 169 && b == 254;
    // Carrier-grade NAT 100.64.0.0/10.
    let cgnat = a == 100 && (64..=127).contains(&b);
    // Multicast / reserved / future-use 224.0.0.0+.
    let multicast_reserved = a >= 224;
    rfc1918 || link_local || cgnat || multicast_reserved
}

fn v6_disallowed(ip: &Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_disallowed(v4);
    }
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }
    let seg0 = ip.segments()[0];
    let link_local = seg0 & 0xffc0 == 0xfe80; // fe80::/10
    let unique_local = seg0 & 0xfe00 == 0xfc00; // fc00::/7
    let multicast = seg0 & 0xff00 == 0xff00; // ff00::/8
    link_local || unique_local || multicast
}

#[cfg(test)]
mod tests {
    use super::*;

    // `validate_outbound_url` reads the `KAPTAIND_ALLOW_INSECURE_HTTP` env var,
    // which is process-global. Serialize every test that depends on its value so
    // parallel test threads cannot race one another's set/remove_var.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn rejects_metadata_endpoint() {
        assert!(validate_outbound_url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(validate_outbound_url("https://169.254.169.254/").is_err());
    }

    #[test]
    fn rejects_loopback_and_private_over_tls() {
        assert!(validate_outbound_url("https://127.0.0.1/").is_err());
        assert!(validate_outbound_url("https://10.0.0.1/").is_err());
        assert!(validate_outbound_url("https://192.168.1.1/").is_err());
        assert!(validate_outbound_url("https://172.16.0.1/").is_err());
        assert!(validate_outbound_url("https://100.64.0.1/").is_err());
    }

    #[test]
    fn rejects_non_tls_http_by_default() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("KAPTAIND_ALLOW_INSECURE_HTTP");
        assert!(validate_outbound_url("http://93.184.216.34/").is_err());
    }

    #[test]
    fn allows_public_https_ip_literal() {
        // Public IP literal: no DNS required, deterministic offline.
        assert!(validate_outbound_url("https://93.184.216.34/").is_ok());
    }

    #[test]
    fn rejects_unsupported_scheme() {
        assert!(validate_outbound_url("ftp://example.com/").is_err());
        assert!(validate_outbound_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn inference_url_allows_loopback_http_blocks_private_remote() {
        assert!(validate_inference_url("http://localhost:11434/api/chat").is_ok());
        assert!(validate_inference_url("http://127.0.0.1:11434/").is_ok());
        assert!(validate_inference_url("http://10.0.0.1/").is_err());
        assert!(validate_inference_url("https://93.184.216.34/").is_ok());
    }

    #[test]
    fn loopback_http_allowed_only_with_explicit_opt_in() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("KAPTAIND_ALLOW_INSECURE_HTTP");
        assert!(validate_outbound_url("http://127.0.0.1:8080/health").is_err());
        std::env::set_var("KAPTAIND_ALLOW_INSECURE_HTTP", "1");
        assert!(validate_outbound_url("http://127.0.0.1:8080/health").is_ok());
        std::env::remove_var("KAPTAIND_ALLOW_INSECURE_HTTP");
    }

    #[test]
    fn connection_time_dns_policy_rejects_rebinding() {
        let public = ["93.184.216.34:0".parse().unwrap()];
        let private = ["127.0.0.1:0".parse().unwrap()];
        assert!(validate_resolved_addrs("example.com", &public, false).is_ok());
        assert!(validate_resolved_addrs("attacker.example", &private, true).is_err());
        assert!(validate_resolved_addrs("localhost", &private, false).is_err());
        assert!(validate_resolved_addrs("localhost", &private, true).is_ok());
        assert!(validate_resolved_addrs("localhost", &public, true).is_err());
    }
}
