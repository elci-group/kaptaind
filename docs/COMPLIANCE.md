# Regional compliance controls

Kaptaind provides technical controls that help an operator enforce its own
regional compliance programme. They do not constitute legal advice or a
certification of compliance.

## What is enforced

`[compliance]` makes egress an explicit, fail-closed configuration decision.
When one or more profiles are selected, an enabled inference or webhook
channel must either be disabled or use `approved_only` with exact approved
hostnames. The check runs both at configuration validation and immediately
before Kaptaind sends repository-derived data to an inference provider or
webhook.

```toml
[compliance]
profiles = ["uk"]

[compliance.egress]
inference = "approved_only"
webhooks = "deny"
audit_export = "local_only"
allowed_hosts = ["lumen.internal.example"]

[inference]
enabled = true
provider = "cosine"
cosine_base_url = "https://lumen.internal.example/v1"
```

Profiles available today are `eu_eea`, `uk`, `us_california`, `canada`,
`brazil`, `india`, `japan`, `china`, and `sovereign`. They are governance
labels, not jurisdiction-specific legal determinations. `sovereign` requires
inference and webhook egress to be denied and audit export to remain local.
The UK profile requires a controlled Cosine Lumen endpoint when inference is
enabled.

## Operating boundaries

- `allowed_hosts` accepts exact DNS names only; wildcards, IP ranges, and
  implicit subdomains are rejected by design.
- This policy controls Kaptaind's own inference and webhook paths. It cannot
  govern commands, plugins, collectors, proxies, or software run by the host.
- Audit export is intentionally local JSONL. Use an existing approved log
  collector for onward SIEM delivery.
- Data residency, processor terms, retention, transfer assessments, DPIAs,
  human-review obligations, and AI-risk classification remain operator duties.

## Deployment checklist

1. Map each configured endpoint to its processor, region, retention, and data
   transfer basis.
2. Use `kaptaind-cli validate` in CI and require an approved host inventory.
3. Keep model hosting within the selected jurisdiction or documented transfer
   mechanism; the UK Cosine route is a selectable controlled deployment, not a
   residency guarantee by itself.
4. Retain `.kaptaind/audit.jsonl` and any local export under your documented
   retention and access-control policy.
5. Review profiles and endpoints whenever providers, regions, or regulations
   change.
