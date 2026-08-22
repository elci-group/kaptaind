# PADAGONIA Integration Roadmap

See `/home/sal/padagonia/docs/enterprise-integration-directives.md`.

## Modules

- `change_event_adapter`: map diffs, analyzed intent, semantic version, commit,
  push, and release artifacts.
- `release_writer`: persist why/how evidence with hashes and actor provenance.
- `history_reader`: retrieve similar changes and prior release outcomes.
- `rollback_evidence`: link failed releases, reversions, and restored versions.

## Acceptance gates

Version decisions are reproducible, graph failure never blocks local commits,
and release evidence is exportable without secrets.
