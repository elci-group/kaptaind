# kaptaind phase 3 plan

## Goal

Provide a command-line interface (`kaptaind-cli` or similar) to allow users to interact with the daemon's artifacts, manage state, and manually trigger behaviors. This elevates `kaptaind` from a background-only processor to a controllable piece of developer infrastructure.

## Context

Phase 1 established a robust daemon lifecycle, configuration resolution, and scheduler flow. Phase 2 deepened the diff analysis capabilities and began persisting analysis artifacts.

Phase 3 bridges the gap to the developer by letting them query the artifacts written in Phase 2 and interact with the daemon's environment without restarting the service or manually reading SQLite/JSON files.

## Deliverables

- [ ] Scaffold a CLI binary (`kaptaind-cli` or `kpt` alias).
- [ ] Implement `log` / `history` command to read and format the persisted analysis artifacts.
- [ ] Implement `status` command to read the current daemon health and `VERSION` file state.
- [ ] Implement `analyze` command to perform a one-off analysis of the working tree without committing, useful for dry runs.
- [ ] Add integration tests for the CLI commands against a mock repository.
