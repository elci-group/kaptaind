//! Runtime enforcement for regional data-egress controls.
//!
//! Configuration validation proves the declared endpoints are permitted at
//! startup. This module repeats that decision immediately before transport so
//! a configuration reload or a direct provider call cannot bypass the policy.

use crate::config::loader::{Config, EgressChannel};
use std::sync::{OnceLock, RwLock};

static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();

/// Install the normalized configuration used by outbound transport guards.
///
/// An unset guard preserves the historical permissive behaviour for library
/// consumers that do not go through Kaptaind's daemon or CLI entry points.
pub fn configure(config: Config) {
    let slot = CONFIG.get_or_init(|| RwLock::new(config.clone()));
    let mut guard = slot
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = config;
}

/// Refuse an outbound repository-data transfer that is not allowed by the
/// active regional profile. Call this directly before the network request.
pub fn enforce_egress_url(channel: EgressChannel, url: &str) -> anyhow::Result<()> {
    if let Some(config) = CONFIG.get() {
        config
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .allows_egress_url(channel, url)?;
    }
    Ok(())
}
