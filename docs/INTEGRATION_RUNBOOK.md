# Enterprise integration runbook

This runbook is the production hand-off for Kaptaind connectors. It treats a
connector as a governed data boundary, not as a generic API token.

## Preflight

Run this from the repository root before enabling any connector:

```bash
kaptaind-cli integrations --format json
cargo test --test integration_connectors
```

The CLI calls the same configuration validation used at startup. It rejects
unsafe endpoints, unsupported capabilities, missing non-secret credential
references, duplicated provider/tenant declarations, disabled integration
network capability, and regional egress violations.

## Provisioning sequence

1. Add a least-privilege service identity in the provider.
2. Store its credential and webhook secrets in the deployment secret manager.
   Put only an opaque `credential_ref` in `kaptaind.toml`.
3. Start the connector in `read_only` or `notification_only`; specify the
   smallest supported capability set.
4. Put the exact provider hostname in
   `[compliance.egress].allowed_hosts` when `integrations = "approved_only"`.
5. Register the provider callback using its current signed-delivery method.
6. Run the preflight command and send a synthetic provider event.
7. Check `.kaptaind/audit.jsonl` for `integration_delivery_accepted`; confirm
   it contains a digest, never the raw customer payload.

## Governing writes

`governed_write` alone does not authorize a mutation. Add a tenant-scoped,
signed `integration_grants` entry to the release policy, obtain the normal
approval/evidence decision, and retain the resulting audit record. Rotate the
provider credential and webhook signing secret independently of policy grants.

## Provider onboarding notes

| Provider group | Admission control |
|---|---|
| Slack, GitHub, Bitbucket, Jira, WhatsApp | Raw-body HMAC verification and durable delivery ID deduplication. |
| GitLab | Standard Webhooks signing token plus timestamp freshness. |
| Microsoft 365 | Basic notification `clientState`; rich notifications require external OIDC/JWKS validation. |
| Google Drive | Channel token and channel/message delivery identity; reconcile through authorized reads. |
| monday.com | HS256 JWT using app signing secret; return challenges but never process them as events. |
| Google Cloud Pub/Sub | External OIDC/JWKS verification plus expected issuer, audience, service-account email, and email verification. |
| AWS, Docker, Kubernetes, Hetzner, Argo CD, Flux, ServiceNow, PagerDuty | Customer gateway terminates provider auth (IAM, mTLS, or OAuth) then supplies a per-connector HMAC envelope. |

## Failure handling

- Reject failed signature, claim, token, or timestamp validation with a non-2xx
  response so the provider can retry according to its contract.
- Do not disable validation to debug delivery. Inspect the digest-only audit
  record and provider delivery log instead.
- A duplicate delivery ID is safe to acknowledge without repeating a side
  effect. Reconcile from the provider’s authoritative API after outages.
- Renew expiring subscriptions/channels before expiry. For Microsoft lifecycle
  signals and Google Drive channel expiry, treat reconciliation as mandatory.
