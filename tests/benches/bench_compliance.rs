//! Micro-benchmark for the hot-path regional egress host check.

use kaptaind::config::loader::{
    ComplianceConfig, Config, DataEgressConfig, EgressChannel, EgressPolicy,
};
use std::collections::BTreeSet;

fn main() {
    divan::main();
}

fn approved_config() -> Config {
    Config {
        compliance: ComplianceConfig {
            profiles: BTreeSet::new(),
            egress: DataEgressConfig {
                inference: EgressPolicy::ApprovedOnly,
                webhooks: EgressPolicy::Deny,
                allowed_hosts: BTreeSet::from(["lumen.internal.example".to_string()]),
                ..DataEgressConfig::default()
            },
        },
        ..Config::default()
    }
}

#[divan::bench]
fn approved_inference_host() {
    let config = approved_config();
    divan::black_box(
        config
            .allows_egress_url(
                EgressChannel::Inference,
                "https://lumen.internal.example/v1/chat/completions",
            )
            .is_ok(),
    );
}
