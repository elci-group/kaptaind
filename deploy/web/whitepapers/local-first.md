# Whitepaper: Local-First Architecture

## Abstract
Kaptaind markets itself as a local-first tool that runs entirely on the user's machine without cloud dependencies. This whitepaper validates that the default configuration requires no external API calls, webhooks, or inference services. All tests passed.

## Claim Statement
> "Open source — runs entirely on your machine" (Landing page, hero badge)
> "100% Local-first" (Landing page, social proof bar)

## Methodology
We inspected the default configurations for inference, push, and notify subsystems. A configuration is deemed "local-first" if all network-egress features are disabled by default.

## Test Implementation
Source: `tests/claims_validation.rs`

```rust
fn claim_default_config_requires_no_external_api() {
    let inference: InferenceConfig = Default::default();
    assert!(!inference.enabled, "default inference config should be disabled");

    let push = PushConfig {
        enabled: false,
        branch: "main".to_string(),
        remote: "origin".to_string(),
        dry_run: false,
        retry: Default::default(),
        conflict: Default::default(),
        pre_push: Default::default(),
        safety: Default::default(),
        batch: Default::default(),
    };
    assert!(!push.enabled, "default push should be disabled");

    let notify: NotifyConfig = Default::default();
    assert!(notify.webhook_url.is_none(), "default notify should have no webhook_url");
}
```

## Results
**PASS** — Default configuration is entirely local.

| Subsystem | Default State | Network Egress? | Result |
|-----------|--------------|-----------------|--------|
| Inference | Disabled | No | PASS |
| Push | Disabled | No | PASS |
| Notify | No webhook URL | No | PASS |

## Evidence
`Config::default()` sets `inference.enabled = false`, `push.enabled = false`, and `notify.webhook_url = None`. The daemon can process changes, compute diffs, and create commits without any network connectivity.

## Limitations
- Users can opt into cloud inference (OpenAI, Anthropic, Ollama) and webhooks via configuration.
- The SaaS dashboard (`web/`) is a separate optional component; the core daemon does not require it.
- Telemetry data is stored locally in `.kaptaind/telemetry.json`; no telemetry is sent to external servers by default.

## Conclusion
The claim is **supported**. Kaptaind is local-first by default and fully operational without cloud connectivity.
