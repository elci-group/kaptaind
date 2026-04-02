# kaptaind phase 2 plan

## Goal

Strengthen semantic change analysis so version decisions are driven by real API and dependency signals instead of path-only heuristics.

## Deliverables

- [x] Feed repository root into diff analysis.
- [x] Add lightweight exported-signature detection for source files.
- [x] Parse dependency manifests (`Cargo.toml`, `package.json`, `requirements.txt`) into a graph-backed score.
- [x] Add unit tests for semantic API and dependency scoring.
- [x] Integrate commit message details from semantic analysis.
- [x] Persist analysis artifacts for later inspection.
