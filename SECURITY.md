# Security & Safety Guarantees

Kaptaind automates critical Git operations (commits, version bumps, pushes). This document outlines how safety is maintained and what guarantees are provided.

## Diff Validation & Test Hook Gating

Before **any** commit, kaptaind:

1. **Analyzes the diff** across five dimensions (structural, API, dependencies, runtime, bundle size).
2. **Runs the configured test hook** (default: `cargo test`). If `[test].required = true` (default), a failing test **blocks the commit entirely**.
3. **Only after tests pass** does the daemon compute the version bump and create the commit.

This ensures:
- **No broken code is committed.** Test failures = no commit.
- **Version bumps are data-driven.** Each bump is justified by the analysis and visible in `.kaptaind/analysis/`.

### Optional Test Hooks

If your test suite is slow, make the hook optional:

```toml
[test]
command = "npm test"
required = false  # Failures logged but don't block commit
```

This can be useful if tests fail intermittently or for non-critical branches, but **required hooks are recommended for production branches**.

## Branch Safety

Kaptaind operates on a configurable branch:

```toml
[push]
branch = "main"  # Commits are created on the currently checked-out branch
```

By default, `[push].enabled = false`, so kaptaind commits locally but never pushes. To safely enable push:

1. Ensure `branch` matches your production branch (`main`, `master`, `develop`, etc.).
2. Verify no other automated systems are pushing to the same branch (avoid conflicts).
3. Test with `[push].enabled = false` first, then observe behavior before enabling.

## Secret & Sensitive File Protection

Kaptaind respects `.kaptainignore` and supports configurable file exclusion:

```toml
[staging]
exclude = ["*.log", ".env*", "secrets/*", "*.pem", "*.key", "*.p12"]
```

The `init` command generates a default `.kaptainignore` that includes common sensitive paths:
- `.env`, `.env.local`, `.env.*.local`
- `secrets/`, `private/`
- `.kaptaind/` (daemon artifacts)

**Important**: If a sensitive file is modified, it will still be **detected** in the analysis (you'll see it in the API/diff metrics) but will **not be staged or committed**.

### Recommended Exclusions

Add these to `exclude` in `[staging]` if you're handling credentials:

```toml
exclude = [
  "*.pem",
  "*.key", 
  "*.p12",
  "*.pfx",
  "config/secrets.*",
  ".ssh/*"
]
```

## Audit Trail & Traceability

Every kaptaind-triggered commit is recorded with full evidence:

### Commit Message
```
kaptaind: Minor -> v0.2.0 [api-added; paths=4; api_touches=2; deps=0; runtime=0; score=0.62; cluster=abc123]
```

This includes:
- Version bump reason (Major/Minor/Patch)
- New version number
- Detailed metrics (API touches, path count, score)
- Cluster UUID (links to analysis artifact)

### Analysis Artifacts
`.kaptaind/analysis/<cluster-uuid>.json` contains:

```json
{
  "cluster_id": "abc123...",
  "timestamp": "2025-04-04T12:34:56Z",
  "structural": 0.35,
  "api": 0.25,
  "deps": 0.0,
  "runtime": 0.0,
  "bundle": 0.0,
  "score": 0.62,
  "api_breaking": false,
  "api_added": true,
  "touched_paths": 4,
  "api_touches": 2
}
```

You can reconstruct **exactly why** each version bump happened by inspecting these artifacts.

### AoC Session Traces
If using Aim of Change sessions:

```bash
kaptaind-cli aoc start "feature: auth"
# ... make changes ...
kaptaind-cli aoc ship
```

Session traces are stored in `.kaptaind/aoc/manifests/<id>.json` and link all commits to the declared intent. This is useful for:
- **Regulatory compliance**: Prove what was changed and when.
- **Release notes**: Map commits to features.
- **Debugging**: Correlate commits with deploy incidents.

## Agent Interception & Observability

For enhanced auditability, use agent interception:

```bash
kaptaind-cli aoc intercept \
  --model claude-3-5-sonnet \
  --intent "refactor auth middleware" \
  -- npm test
```

This:
1. Runs `npm test`.
2. Captures stdout/stderr.
3. Stores the output alongside AoC traces.
4. Adds the agent model and intent to the commit message.

Useful for:
- **Audit trails in regulated environments** (financial, healthcare, etc.).
- **Linking commits to human intent** (why was this change made?).
- **Debugging** (what was the test output that justified this commit?).

## Daemon Security

### Process Isolation
The daemon runs as a background process under your user account (daemonized via the `daemonize` crate on Unix). It has the same file permissions as your user—no privilege escalation.

### Log Files
Logs are written to `.kaptaind/daemon.out` and `.kaptaind/daemon.err`:

```bash
# View daemon logs
tail -100 .kaptaind/daemon.err
tail -100 .kaptaind/daemon.out
```

Ensure `.kaptaind/` is not world-readable if you're in a shared environment:

```bash
chmod 700 .kaptaind/
```

### Shutdown
To stop the daemon safely:

```bash
kill $(cat .kaptaind/daemon.pid)
```

Or use your system's process manager.

## Version Bump Decision Logic

Kaptaind's versioning is deterministic and rule-based:

- **Major**: Only on breaking API removals (public symbols deleted).
- **Minor**: On new API additions OR when overall diff score > 0.6.
- **Patch**: On structural churn (score > 0.1) or dependency changes.
- **None**: On trivial changes (score < 0.1).

This means:
- **No surprises**. You can predict bumps based on metrics.
- **Transparency**. Each bump is justified by the analysis.
- **Reversibility**. You can inspect `.kaptaind/analysis/` to understand any bump.

## Rollback

If kaptaind commits something problematic:

1. **Inspect the commit**: `git log -1 --stat`
2. **Review the analysis**: `cat .kaptaind/analysis/<cluster-uuid>.json`
3. **Revert if needed**: `git revert HEAD`
4. **Disable temporarily**: Stop the daemon (`kill <pid>`), investigate, fix root cause.

## Known Limitations

1. **Test hook failures are blocking**: If `[test].required = true` and tests fail intermittently, kaptaind will repeatedly fail to commit. Use `required = false` or fix the test flakiness.
2. **No conflict resolution**: If kaptaind attempts to commit on a stale branch, the commit may fail with a merge conflict. The daemon logs the error and retries.
3. **Push requires explicit opt-in and credentials**: Pushes are disabled by default. When enabled, kaptaind pushes `refs/heads/<branch>` to `origin`; configure authentication (credential helper / SSH) separately.

> Note: GPG-signed commits (`[commit] sign = true`) and branch-protection / required-CI enforcement (`[push.protection]`) are implemented and configurable — see "GPG-Signed Commits" and "Branch Protection / Required CI" above. Earlier revisions of this document listed them as unsupported; that is no longer accurate.

## Best Practices

1. **Start with `[push].enabled = false`**. Commit locally, observe behavior, then enable push.
2. **Use required test hooks** for production branches. Optional hooks are fine for experimental branches.
3. **Configure staging carefully**. Use `cluster` mode for monorepos, `all` for standard repos.
4. **Monitor logs regularly**. Check `.kaptaind/daemon.err` for errors.
5. **Use AoC sessions** for significant feature work. Provides traceability and makes release notes easy.
6. **Review analysis artifacts**. Understand why each bump happened by inspecting JSONs in `.kaptaind/analysis/`.
7. **Pin dependencies**. If using `[bundle]` scoring, ensure your build is deterministic (lock files committed).

## Reporting Security Issues

If you discover a security vulnerability in kaptaind:

1. **Do not open a public issue.**
2. **Email security@kaptaind.dev** with details (or contact the maintainers privately).
3. **Include**: reproduction steps, impact, and suggested fix if known.

We take security seriously and will respond promptly.
