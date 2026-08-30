# Enterprise Padagonia Supervisor Migration Directive

Status: implementation directive  
Schema: `kaptaind.supervisor/v1`  
Control ontology: `kaptaind.control/v1`  
Decision owner: kaptaind maintainers  
Last reviewed: 2026-08-28

## 1. Directive

Kaptaind SHALL migrate from a one-shot fleet launcher to a resident supervisor
backed by Padagonia while retaining one isolated operating-system process per
active repository. The supervisor owns desired-state reconciliation, fleet
visibility, admission control, and Padagonia projection. Project workers retain
filesystem watching, tests, analysis, version writeback, Git mutation, release
execution, local recovery artifacts, and project-specific credentials.

This directive explicitly rejects an in-process multi-project scheduler until
all process-global configuration has been removed and fault-isolation gates in
section 14 pass. A single supervisor process is not authorization to combine
project execution contexts.

## 2. Business outcome

The migration SHALL provide:

- one fleet control endpoint instead of one mandatory control port per project;
- a versioned, provenance-rich record of project intent and observed worker
  state in Padagonia;
- deterministic reconciliation after supervisor or host restart;
- backward-compatible import of `~/.config/kaptaind/monitored.json`;
- independent project worker restart, containment, and upgrade;
- bounded fleet-wide worker starts and visible reconciliation outcomes;
- fail-safe local operation when Padagonia is temporarily unavailable.

## 3. Non-goals

The first production release SHALL NOT:

- run multiple repository schedulers inside one process;
- load multiple project `.env` files into the supervisor environment;
- execute project test, build, plugin, Git, push, or release commands itself;
- require Padagonia for a worker already running safely;
- treat Padagonia confidence as truth or as authorization;
- expose the supervisor or Padagonia without explicit network policy;
- delete the legacy registry during migration.

## 4. Architectural boundary

```text
operator / service manager
           |
           v
kaptaind-supervisor (one per host/user boundary)
  - desired-state reconciler
  - local API and aggregate health
  - bounded worker launcher
  - local atomic snapshot
  - Padagonia projection/client
           |
           +---- kaptaind worker: repository A
           +---- kaptaind worker: repository B
           `---- kaptaind worker: repository N

Padagonia
  - append-only control observations
  - project/run/capability provenance
  - replayable fleet history
```

Workers remain the authority for repository-local execution state. The
supervisor is the authority for desired fleet state. Padagonia is the durable
enterprise projection and recovery source; the supervisor's atomic local
snapshot is the continuity cache and bootstrap fallback.

## 5. Control ontology

Every Padagonia control record SHALL use namespace `kaptaind` by default and
carry `schema_version = "kaptaind.control/v1"`.

### 5.1 Node labels

| Label | Meaning | Stable identity |
|---|---|---|
| `KaptaindProjectControl` | Immutable revision of desired project state | project digest + revision |
| `KaptaindWorkerObservation` | One observed worker state transition | project digest + observation ID |
| `KaptaindReconcileRun` | One supervisor reconciliation summary | run ID |
| `KaptaindCapability` | Declared worker capability snapshot | project digest + config digest |

### 5.2 Required project properties

- canonical repository path;
- canonical configuration path;
- desired state: `enabled` or `disabled`;
- health port retained for worker compatibility;
- monotonically increasing revision;
- controller instance ID;
- timestamp in Unix milliseconds;
- source: `legacy_import`, `operator`, `recovery`, or `reconcile`;
- schema version.

### 5.3 Provenance

Records SHALL attribute `agent = "kaptaind-supervisor"`, the running kaptaind
version as the model/version field, confidence `1.0` for direct observations,
zero cost, and evidence containing only non-secret local identifiers. Tokens,
environment contents, webhook secrets, and command output SHALL NOT be stored.

### 5.4 Current-state projection

Padagonia is append-only. Current desired state SHALL be calculated by choosing
the highest valid revision for each canonical project identity. Ties SHALL be
resolved deterministically by timestamp and external ID. Malformed or unknown
schema records SHALL be ignored with a structured warning; they SHALL NOT crash
the supervisor or override a valid known record.

## 6. Trust and security requirements

1. The supervisor SHALL bind to loopback unless an independently reviewed
   transport boundary is configured.
2. Padagonia plaintext HTTP SHALL be accepted only for loopback hosts.
3. Remote Padagonia endpoints SHALL require HTTPS.
4. The Padagonia bearer token SHALL be read from a named environment variable,
   never stored in the supervisor TOML or local snapshot.
5. Project paths and config paths SHALL be canonicalized and validated before
   process launch.
6. Worker launch SHALL use argv construction, never a shell.
7. The worker binary path SHALL be explicit or resolved through the current
   executable directory/PATH; it SHALL NOT be derived from project contents.
8. The supervisor SHALL not read project `.env` files.
9. PID validation SHALL reject empty, malformed, stale, and self-referential
   entries and SHALL never signal a process merely because a PID file exists.
10. Padagonia failure SHALL degrade projection and recovery; it SHALL NOT
    silently broaden execution authority.
11. The supervisor SHALL run workers under its own effective user and group;
    it SHALL contain no privilege-elevation path and SHALL reject set-user-ID
    or set-group-ID worker binaries on Unix.
12. The initial integration SHALL use kaptaind's existing HTTP dependency and
    Padagonia's versioned HTTP contract. It SHALL NOT introduce a direct
    in-process Padagonia database dependency or couple worker availability to
    Padagonia library ABI changes.

## 7. Availability and failure semantics

### 7.1 Local snapshot

The supervisor SHALL persist `~/.config/kaptaind/supervisor-state.json` by
write-to-temporary-file, file sync, atomic rename, and parent-directory sync on
Unix. A corrupt snapshot SHALL be quarantined logically by returning a clear
startup error; it SHALL never be overwritten automatically without a valid
replacement source.

### 7.2 Padagonia unavailable

- Existing workers continue running.
- Reconciliation uses the last valid local desired state.
- New local operator changes are committed to the atomic snapshot first.
- Failed Padagonia projections remain retryable and visible in status.
- `padagonia.required = true` makes supervisor readiness false but does not kill
  healthy workers.

### 7.3 Worker failure

Each project has an independent restart budget and exponential backoff. A
failing project SHALL NOT consume every start slot or prevent other projects
from reconciling. The initial implementation MAY expose the budget and backoff
state before automatically restarting repeatedly; it MUST never spin.

### 7.4 Supervisor failure

Workers are detached project processes and SHALL survive supervisor failure.
On restart, the supervisor reconstructs actual state from validated PID files
and desired state from the local snapshot, then Padagonia when available.

## 8. Reconciliation contract

For every project, one reconciliation cycle produces exactly one action:

| Desired | Observed | Action |
|---|---|---|
| enabled | running | retain |
| enabled | stopped | start worker |
| enabled | invalid configuration | block project and report |
| disabled | stopped | retain |
| disabled | running | report `disable_pending`; do not kill implicitly |

Disabling a project changes future-start intent. Terminating an existing worker
is a distinct destructive control and is outside this migration unless the
operator explicitly requests it.

Reconciliation SHALL be deterministic for a given snapshot and liveness view.
Project order SHALL be canonical-path order. Start admission SHALL be bounded by
`max_parallel_starts`, with a default of four.

## 9. API contract

The supervisor SHALL expose a loopback HTTP API with:

- `GET /health` — process liveness;
- `GET /ready` — snapshot loaded and, when required, Padagonia reachable;
- `GET /api/v1/projects` — desired and observed project state;
- `POST /api/v1/reconcile` — request one reconciliation cycle;
- `GET /api/v1/status` — fleet totals, last run, and projection health.

Mutating endpoints SHALL require a bearer token when enabled. The first release
MAY keep operator mutation in the local CLI and expose reconciliation only on a
loopback-only endpoint, but it SHALL preserve the versioned route shape.

## 10. Migration phases and gates

### Phase 0 — baseline and acceptance contract

Deliverables:

- document current registry size, active worker count, duplicate ports, and
  process-global isolation hazards;
- record Traci complexity and Amber dependency baselines;
- define deterministic deliverables and commands before implementation.

Exit gate: baseline evidence and this directive exist.

Rollback: none; read-only.

### Phase 1 — versioned domain model and mirror

Deliverables:

- `kaptaind.supervisor/v1` snapshot model;
- `kaptaind.control/v1` Padagonia record model;
- atomic JSON store;
- legacy registry import with collision detection;
- Padagonia client with loopback/HTTPS validation, idempotency, pagination, and
  optional/required failure modes.

Exit gate: round-trip, corruption, migration, endpoint-validation, duplicate
port, and Padagonia wire-contract tests pass.

Rollback: continue reading `monitored.json`; no legacy file is removed.

### Phase 2 — resident supervisor and isolated workers

Deliverables:

- deterministic reconciler;
- validated PID liveness;
- argv-only worker launch;
- bounded start admission;
- structured tracing with project and reconciliation correlation fields;
- supervisor binary and graceful shutdown.

Exit gate: one project failure cannot prevent another project action; workers
are separate processes; supervisor shutdown does not terminate workers.

Rollback: invoke existing `kaptaind-cli monitor resume`.

### Phase 3 — CLI and service integration

Deliverables:

- supervisor CLI commands for run, once, status, import, and plan;
- service templates launch the resident supervisor rather than one-shot resume;
- existing monitor add/remove/enable/disable writes the new snapshot and legacy
  registry during the compatibility window;
- aggregate status makes port and PID conflicts visible.

Exit gate: existing monitor tests pass and compatibility tests show no loss of
registered projects.

Rollback: disable the supervisor service and resume legacy launch behavior.

### Phase 4 — benchmark and production qualification

Required benchmark cases:

- load and validate 10, 100, 1,000, and 10,000 project records;
- plan a no-op reconciliation at those sizes;
- plan a mixed running/stopped reconciliation;
- serialize and atomically persist representative snapshots;
- report throughput and p50/p95 where repeated measurement is practical.

Initial acceptance budgets on the reference development host:

- 1,000-project in-memory plan under 100 ms;
- 10,000-project in-memory plan under 1 second;
- no network or process creation in planning benchmarks;
- linear memory growth with project count and no unbounded task creation.

Exit gate: benchmark report records host/toolchain, command, sample count,
results, and whether budgets passed.

Rollback: performance work must not change control semantics.

## 11. Compatibility policy

During at least one major-version window:

- `MonitorEntry` and `monitored.json` remain readable;
- imports are idempotent;
- health ports remain in project records for worker compatibility;
- the supervisor snapshot receives a monotonically increasing generation;
- writes use a dual-write adapter, local snapshot first and legacy registry
  second, until migration telemetry shows no legacy-only consumers;
- failed secondary writes are surfaced and never reported as full success.

## 12. Observability

Every reconciliation SHALL emit:

- reconciliation ID;
- supervisor instance ID;
- project identity digest, not secrets;
- desired and observed state;
- selected action and outcome;
- elapsed milliseconds;
- Padagonia projection result;
- start admission and rejection counts.

Errors crossing async task or process boundaries SHALL retain the reconciliation
span. Swallowed process-start and persistence errors are prohibited.

## 13. Test strategy

The implementation SHALL include:

- unit tests for models, validation, revision selection, endpoint policy, PID
  parsing, action planning, and atomic snapshot round trips;
- integration tests with fake worker launchers and a local fake Padagonia API;
- recovery tests for stale PID files, unavailable Padagonia, corrupt snapshots,
  duplicate ports, and repeated idempotency keys;
- security tests for remote plaintext endpoints, token redaction, shell-free
  launch, and path validation;
- regression tests for legacy monitor JSON.

No test SHALL start the real kaptaind daemon against this repository.

## 14. Conditions for future in-process workers

An in-process multi-project scheduler remains prohibited until all of these are
demonstrated:

- dotenv and provider credentials are immutable per-project values rather than
  process environment mutations;
- compliance, audit, governance, notification, TTS, tracing, and shutdown state
  are instance-scoped;
- every subprocess has an explicit working directory and environment allowlist;
- per-project memory, CPU, task, file-descriptor, and network budgets exist;
- panic and cancellation fault injection proves containment;
- one project cannot throttle another project's notification or release path;
- a measured operational benefit exceeds the isolation and migration cost.

## 15. Final acceptance

The migration is complete only when:

1. the directive is reviewed and all accepted findings are incorporated;
2. supervisor models, store, Padagonia client, reconciler, API, and binary are
   integrated into the kaptaind workspace;
3. the legacy registry imports without project loss and port collisions are
   surfaced;
4. workers are demonstrably separate processes;
5. formatting, clippy with warnings denied, unit/integration tests, Traci, and
   deterministic delivery checks pass;
6. benchmarks meet or explicitly disposition every acceptance budget;
7. a final evidence-chain audit supports every completion claim.

## 16. Rollout controls

Rollout SHALL begin opt-in with `kaptaind-supervisor once --dry-run`, followed
by a loopback-only resident canary. Automatic service migration SHALL NOT occur
merely because the new binary is installed. Operators must explicitly enable
the supervisor service after reviewing its plan. Existing running workers are
adopted, not restarted, on first reconciliation.

## 17. Review record

The directive was challenged through four contradictory frames before code was
written. The optimistic and minimal-change frames supported the supervisor plus
isolated-worker boundary. The skeptic requested explicit rollback gates; these
are specified per phase and retain `monitor resume` throughout the compatibility
window. The security frame raised dependency and privilege-escalation risks;
the refined directive therefore prohibits privilege elevation, rejects special
permission bits on worker executables, and uses the existing HTTP client stack
instead of adding an in-process Padagonia dependency. The review returned a
conflict rather than artificial consensus, so these controls are acceptance
requirements rather than advisory notes.
