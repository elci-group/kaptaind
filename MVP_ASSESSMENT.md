# kaptaind MVP assessment

## Current state

The scaffold already has the right module layout, event clustering, semantic version bumping, commit orchestration, push control, and basic ignore support.

## MVP blockers

1. Watcher startup is not validated before the runtime proceeds.
2. Runtime does not supervise long-lived tasks cleanly.
3. `.kaptainignore` matching is not consistently rooted at the repository.
4. The test hook from the directive is not implemented, so runtime risk is not part of versioning and failed test runs do not block automation.
5. Version file handling is not explicitly rooted at the repository.
6. MVP behaviors are under-tested.

## Path to MVP

- [x] Validate watcher initialization before daemon startup completes.
- [x] Supervise runtime startup/shutdown so the daemon exits cleanly on Ctrl+C.
- [x] Resolve `.kaptainignore` and `VERSION` paths relative to the configured repository root.
- [x] Add a configurable test hook and feed its result into runtime weighting.
- [x] Block automated commits when the configured test hook is required and fails.
- [x] Add focused tests for clustering, ignore matching, semver policy, and runtime gating helpers.
- [x] Finish with `cargo test` and `cargo clippy --all-targets --all-features`.
