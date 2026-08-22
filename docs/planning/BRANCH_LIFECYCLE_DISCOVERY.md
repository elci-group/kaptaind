# Branch lifecycle discovery

This note records the pre-feature assumptions found before adding governed
branch promotion.

## Existing mechanisms

- Git repository access is rooted through `git::Repo`; most mutation helpers
  invoke the Git CLI, while status/hash operations optionally use `git2`.
- Branches previously had no semantic type. The push configuration defaulted
  to `main`, and shipping created `v<version>` tags, but neither mechanism
  distinguished development from production.
- The authoritative project version remains `VERSION`, with Cargo manifest and
  workspace writeback handled by the existing `version` module.
- `.kaptaind/state.toml` is the existing explicitly versioned semantic-state
  document. Its registry and stepwise migrator were at format 2.1; migration is
  explicit and records an append-only digest ledger.
- Completed automated releases are indexed under
  `.kaptaind/releases/index.json`; manual shipping has a separate ship index.
  These formats remain readable. A branch initialization may adopt a legacy
  release only when its indexed commit exactly matches its immutable version
  tag.
- The CLI uses Clap subcommand enums in `src/cli/main.rs` with handlers under
  `src/cli/commands/`. Tests are colocated unit tests plus real-Git CLI tests in
  `tests/cli_integration.rs`.

## Extension decisions

- Format 2.2 extends the semantic document with the canonical branch topology
  and stable/bleeding channel policy.
- Mutable candidate, validation, staging, and release-event state lives in the
  separately versioned `.kaptaind/lifecycle.json`; it does not overload daemon
  health state or rewrite older release indexes.
- Production refs are not created from an arbitrary initialization commit.
  They are created or advanced only by an issued release, or adopted from a
  verified legacy index/tag pair.
- Generic promotions are constrained to typed, fast-forward transitions and
  can never target production. Divergence remains an explicit operator action.
- Rollback creates a new release commit with the restored tree and a new
  `VERSION`; it never resets or force-updates production history.
