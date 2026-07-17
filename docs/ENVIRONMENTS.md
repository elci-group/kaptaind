# Environment intelligence

Kaptaind is the release-metadata observer for local, development, QA, staging,
canary, and production environments. It does not invoke deployment APIs,
Terraform, Kubernetes, or CI runners.

External delivery systems record completed facts after their own deployment:

```bash
kaptaind-cli environment record staging --version 2.4.0 \
  --health healthy --rollout-percent 100 --config-sha256 <digest>
kaptaind-cli environment promote staging production --version 2.4.0 --adr ADR-42
kaptaind-cli environment record production --version 2.4.0 \
  --health healthy --rollout-percent 100 --config-sha256 <digest>
```

Read-only review commands:

```bash
kaptaind-cli environment status
kaptaind-cli environment history production
kaptaind-cli environment diff staging production
kaptaind-cli environment risk
```

`rollback` records an approved rollback decision, including an optional ADR;
the external deployment system remains responsible for performing it. The
timeline is stored at `.kaptaind/environments/timeline.jsonl` and each append
also produces a digest-only governance audit event.

`status` always shows the standard registry (`local`, `dev`, `qa`, `staging`,
`canary`, `production`) and marks environments with no evidence as unknown.
`risk` is deterministic and explains each signal: latest production rollback or
unhealthy health is high risk; incomplete rollout, missing production evidence,
or a staging/production version or configuration-digest difference is medium
risk. It is advisory evidence, never a deployment authorization.
