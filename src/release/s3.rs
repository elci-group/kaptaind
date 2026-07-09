//! S3 distribution implementation using AWS Signature Version 4.
//!
//! Supports both AWS S3 and S3-compatible services (MinIO, Wasabi, etc.)
//! via custom endpoint configuration.

use crate::config::loader::S3DistConfig;
use crate::release::packager::PackageResult;
use anyhow::{anyhow, Context};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::SystemTime;

/// S3 distributor that uploads artifacts to S3 buckets.
pub struct S3Distributor {
    config: S3DistConfig,
    endpoint: Option<String>,
    access_key: String,
    secret_key: String,
}

impl S3Distributor {
    /// Create a new S3 distributor from config.
    /// Credentials are read from environment:
    /// - `AWS_ACCESS_KEY_ID` or `S3_ACCESS_KEY`
    /// - `AWS_SECRET_ACCESS_KEY` or `S3_SECRET_KEY`
    /// - `S3_ENDPOINT` (optional, for MinIO/compatible services)
    pub fn new(config: S3DistConfig) -> anyhow::Result<Self> {
        let access_key = std::env::var("AWS_ACCESS_KEY_ID")
            .or_else(|_| std::env::var("S3_ACCESS_KEY"))
            .map_err(|_| {
                anyhow!("S3 access key not found. Set AWS_ACCESS_KEY_ID or S3_ACCESS_KEY")
            })?;

        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .or_else(|_| std::env::var("S3_SECRET_KEY"))
            .map_err(|_| {
                anyhow!("S3 secret key not found. Set AWS_SECRET_ACCESS_KEY or S3_SECRET_KEY")
            })?;

        let endpoint = std::env::var("S3_ENDPOINT").ok();

        Ok(Self {
            config,
            endpoint,
            access_key,
            secret_key,
        })
    }

    /// Upload a package to S3.
    pub async fn distribute(&self, pkg: &PackageResult) -> anyhow::Result<()> {
        let tarball_name = pkg
            .tarball
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("tarball has no filename"))?;

        let manifest_name = format!("{}-manifest.json", pkg.manifest.version);

        // Upload tarball
        self.upload_file(&pkg.tarball, tarball_name).await?;

        // Upload manifest if it exists
        let manifest_path = pkg
            .tarball
            .parent()
            .unwrap_or(Path::new("."))
            .join(&manifest_name);
        if manifest_path.exists() {
            self.upload_file(&manifest_path, &manifest_name).await?;
        }

        tracing::info!(
            version = pkg.manifest.version,
            bucket = self.config.bucket,
            region = self.config.region,
            "artifacts distributed to S3"
        );

        Ok(())
    }

    async fn upload_file(&self, local_path: &Path, key: &str) -> anyhow::Result<()> {
        let content = tokio::fs::read(local_path)
            .await
            .with_context(|| format!("failed to read file: {}", local_path.display()))?;

        let content_hash = crate::util::hex::encode(Sha256::digest(&content));
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let date = chrono::DateTime::from_timestamp(timestamp as i64, 0)
            .unwrap()
            .format("%Y%m%d")
            .to_string();
        let datetime = chrono::DateTime::from_timestamp(timestamp as i64, 0)
            .unwrap()
            .format("%Y%m%dT%H%M%SZ")
            .to_string();

        let host = self.endpoint.as_ref().map_or_else(
            || {
                format!(
                    "{}.s3.{}.amazonaws.com",
                    self.config.bucket, self.config.region
                )
            },
            |e| {
                let endpoint = e.trim_end_matches('/');
                format!(
                    "{}.{}",
                    self.config.bucket,
                    endpoint.strip_prefix("https://").unwrap_or(endpoint)
                )
            },
        );

        // Build canonical request
        let canonical_uri = format!("/{}", key);
        let canonical_querystring = "";
        let content_type = "application/gzip";
        let headers = format!(
            "content-type:{content_type}\nhost:{host}\nx-amz-content-sha256:{content_hash}\nx-amz-date:{datetime}\n"
        );
        let signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "PUT\n{}\n{}\n{}\n{}\n{}",
            canonical_uri, canonical_querystring, headers, signed_headers, content_hash
        );

        // Create string to sign
        let credential_scope = format!("{}/{}/s3/aws4_request", date, self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            datetime,
            credential_scope,
            crate::util::hex::encode(Sha256::digest(canonical_request.as_bytes()))
        );

        // Calculate signature
        let signing_key = self.get_signing_key(&date)?;
        let signature =
            crate::util::hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

        // Build authorization header
        let auth_header = format!(
            "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
            self.access_key, credential_scope, signed_headers, signature
        );

        // Build URL
        let url = if let Some(endpoint) = &self.endpoint {
            format!(
                "{}/{}/{}",
                endpoint.trim_end_matches('/'),
                self.config.bucket,
                key
            )
        } else {
            format!("https://{}/{}", host, key)
        };

        // Validate the final destination: require TLS and block
        // private/loopback/link-local/cloud-metadata targets.
        crate::util::http::validate_outbound_url(&url)?;

        // Send request
        let client = crate::util::http::hardened_client(std::time::Duration::from_secs(60));
        let response = client
            .put(&url)
            .header("Content-Type", content_type)
            .header("Host", host)
            .header("x-amz-content-sha256", &content_hash)
            .header("x-amz-date", datetime)
            .header("Authorization", auth_header)
            .body(content)
            .send()
            .await
            .context("failed to send S3 PUT request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("S3 upload failed: {} - {}", status, body));
        }

        Ok(())
    }

    fn get_signing_key(&self, date: &str) -> anyhow::Result<Vec<u8>> {
        let k_secret = format!("AWS4{}", self.secret_key);
        let k_date = hmac_sha256(k_secret.as_bytes(), date.as_bytes());
        let k_region = hmac_sha256(&k_date, self.config.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"s3");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");
        Ok(k_signing)
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}
