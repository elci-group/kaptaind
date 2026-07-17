# Enterprise integrations

Kaptaind's integration catalogue is intentionally capability-first. A
connector is not permitted to perform a provider write merely because it has a
credential: every connector starts `disabled`; `governed_write` is explicit
and must be paired with the existing release policy, approval and audit gates.

See [the production integration runbook](INTEGRATION_RUNBOOK.md) for the
preflight, provisioning, governance, and incident-handling sequence.

Run `kaptaind-cli integrations --format json` to inspect the catalogue and
the configured connector count for each provider.

## Configuration

Credentials are external references, not values. Resolve `credential_ref` in
the customer deployment from a secret manager, workload identity, Kubernetes
secret projection, or equivalent platform mechanism. Do not put OAuth tokens,
API keys, webhook secrets, or private keys in `kaptaind.toml`.

```toml
[integrations]

[[integrations.connectors]]
provider = "slack"
mode = "notification_only"
tenant_id = "acme-payments"
credential_ref = "vault:slack-kaptaind"
capabilities = ["receive_events", "send_notification"]

[[integrations.connectors]]
provider = "kubernetes"
mode = "read_only"
tenant_id = "acme-payments"
endpoint = "https://kubernetes.customer.example"
credential_ref = "workload:k8s-release-observer"
capabilities = ["read_state", "receive_events"]
```

Customer-hosted products require an explicit `endpoint`; provider-hosted APIs
use their catalogue origin unless an approved custom endpoint is supplied.
Enabled connectors require `capabilities.network_integrations = true`.

For regional profiles, integrations are their own egress channel:

```toml
[compliance.egress]
integrations = "approved_only"
allowed_hosts = ["slack.com", "api.hetzner.cloud", "kubernetes.customer.example"]
```

The sovereign profile requires `integrations = "deny"`. The `air_gapped`
mode disables the integration network capability.

## Provider catalogue and safe initial scope

| Provider | Initial mode | First capability set |
|---|---|---|
| AWS, Google Cloud, Hetzner | `read_only` | deployment/CI/action state |
| Docker, Kubernetes, Argo CD, Flux | `read_only` | local or in-cluster state/events |
| Slack, WhatsApp Business, PagerDuty, Opsgenie | `notification_only` | signed events and redacted notification delivery |
| Microsoft 365, Google Drive, monday.com | `read_only` | scoped evidence/change state; no tenant-wide write by default |
| GitHub, GitLab, Bitbucket | `read_only` | pull request, status and artifact evidence |
| Jira, ServiceNow | `read_only` | ticket/change evidence; no automatic closure |
| Okta, Entra | `read_only` | identity/group state; use separate OIDC/SCIM lifecycle controls |
| Terraform Cloud | `read_only` | plan/apply evidence |
| Datadog, Splunk, Elastic | `read_only` | observability and audit export state |

`governed_write` is reserved for scoped infrastructure actions, work-item
updates, and identity lifecycle tasks after an approved policy explicitly
allows the action. WhatsApp Business is notification/escalation only: it is
never an approval or evidence authority.

For example, a signed policy pack must grant the exact tenant and capability
before a Hetzner, Kubernetes, Terraform, ServiceNow, Jira, Entra, or similar
write adapter may execute:

```json
{
  "integration_grants": [{
    "provider": "kubernetes",
    "tenant_id": "acme-payments",
    "capabilities": ["write_infrastructure"]
  }]
}
```

This grant is additive to connector configuration, release approval, regional
egress and role-based access checks; it does not replace any of them.

## Event and delivery model

Inbound transports must verify provider signatures before constructing an
`EventEnvelope`. Kaptaind records provider, tenant, event ID, type, timestamp,
correlation ID and SHA-256 of the raw payload—not the raw collaboration or
customer data. Delivery IDs are durably recorded with create-once semantics;
replays are rejected before side effects.

The built-in ingress primitives verify Slack's timestamped `v0` signature and
GitHub's `sha256` webhook signature over the exact raw body. AWS EventBridge
gateways and Docker/Kubernetes local observers must terminate their upstream
IAM, mTLS, or service-account authentication in the customer boundary, then
forward a per-connector HMAC-signed body to Kaptaind. This deliberately avoids
exposing an unauthenticated cluster or cloud control-plane endpoint.

GitLab connectors use its modern Standard Webhooks signing token, with a
freshness check over `webhook-id.timestamp.raw-body`. The older
`X-Gitlab-Token` can be compared in constant time only during migration; it
cannot admit an event because it does not protect body integrity or replay.
Bitbucket Cloud connectors verify its secret-token `sha256=<HMAC>` over the
exact UTF-8 raw body. Configure a high-entropy secret and leave provider-side
TLS verification enabled.

Microsoft 365 basic change notifications are accepted only after a
tenant-bound `clientState` comparison. Rich notifications must also have every
Microsoft validation JWT checked by a dedicated OIDC/JWKS verifier before they
enter Kaptaind; a `clientState` alone is insufficient for those payloads.
Google Drive webhook channels use the caller-provided channel token and the
channel/message-number pair as the delivery identity. Treat Drive signals as
metadata-only prompts: use a separately authorized, least-privilege API read
to reconcile the resource, and renew channels before their provider expiry.

monday.com board webhooks require an `Authorization` JWT verified with the
app signing secret. Its initial `challenge` request is echoed as JSON but is
never processed as an event. Jira Cloud webhooks must be created with a secret
and verified through their raw-body `X-Hub-Signature` HMAC. ServiceNow remains
gateway-only: terminate its instance-specific OAuth or mTLS authentication in
the customer boundary and forward a separately HMAC-authenticated envelope.

Google Cloud Pub/Sub push is accepted only after an injected OIDC/JWKS verifier
has checked the JWT signature and expiry, and Kaptaind has matched its Google
issuer, configured audience, service-account email, and `email_verified`
claim. Hetzner, Argo CD, Flux, PagerDuty, and ServiceNow may use the same
customer-gateway envelope where a direct vendor signature contract is not
configured. WhatsApp Business verifies both its subscription challenge token
and each raw-body `X-Hub-Signature-256`; it remains notification-only and is
not an approval or evidence channel.

## Outbound notifications

Provider adapters send outbound notifications only through the governed
delivery primitive. It requires `send_notification` in connector
configuration, checks the signed policy decision, validates the configured
HTTPS endpoint again at send time, applies the `integrations` regional-egress
channel, and adds a caller-supplied idempotency key. Authentication headers are
resolved externally at runtime; neither them nor the notification payload is
written to Kaptaind's audit records. The audit record contains only provider,
tenant, idempotency key, response status, delivery outcome, and a payload
SHA-256 digest.

Provider adapters must acknowledge webhook requests promptly and hand work to
the durable connector queue. They must implement subscription renewal where
the provider requires it, provider-specific idempotency keys, exponential
backoff, dead-letter routing, and reconciliation from authoritative provider
state.
