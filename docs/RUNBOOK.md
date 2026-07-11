# kaptaind Operator Runbook

Day-to-day operation of the autonomous-commit daemon: start, stop, recover,
and explain. Everything here applies from v9.8.0 onward.

## Start

```sh
kaptaind --daemon          # double-forks into the background
kaptaind                   # foreground (useful under systemd or for debugging)
```

On start the daemon:

1. Validates `.kaptaind/daemon.pid`. A stale pid file (process gone, or
   unparseable) is removed with a log line; a live pid means another daemon
   owns this project.
2. Writes `.kaptaind/status.json` as `Idle` **before** any cluster is
   processed, so a crashed previous run never shows a frozen mid-state.
3. Reconciles changes made while it was down (`[watch] rescan_on_start`,
   default `true`): pending in-project changes form a single catch-up cluster
   that goes through the normal scored/tested/gated pipeline.
4. Installs git hooks into the **real** git hooks directory
   (`git rev-parse --git-path hooks`). If the watched path is a subdirectory
   of a larger repo and `[angler.git_hooks] hooks_dir` is not set, hook
   installation is skipped with a warning — it never fabricates a `.git`
   inside a subproject.

## Stop

```sh
kill <pid>                 # SIGTERM — preferred
kill -INT <pid>            # SIGINT — same path
```

Graceful shutdown sequence: stop ingesting events, set status `Stopping`,
drain in-flight tasks within `[daemon] shutdown_grace_secs` (default **10s**),
abort the remainder, write final status `Stopped`, remove
`.kaptaind/daemon.pid`. The runtime waits grace + 5s before forcing exit.

`kill -9` is safe but skips cleanup: the next start's stale-pid validation
and atomic status writes are the recovery path.

## Recover

| Symptom | Cause | Action |
|---|---|---|
| Daemon won't start, "pid in use" | live daemon already running | `kaptaind-cli status` to find it; stop it first |
| Stale `daemon.pid` after crash | kill -9 / power loss | nothing — startup removes it automatically |
| Frozen `status.json` ("Testing" for hours) | crashed mid-cluster | nothing — startup overwrites with `Idle` before processing |
| Missed commits while daemon was down | offline edits | automatic: startup catch-up cluster (disable with `rescan_on_start = false`) |
| Commits blocked repeatedly | red test suite | see `.kaptaind/decisions.jsonl` for `test_failed`; at ≥3 consecutive failures the daemon logs and broadcasts a warning |

## Explain

Every cluster decision — commit **or skip** — is one JSON line in
`.kaptaind/decisions.jsonl` with scores, thresholds in effect, bump, reason,
and paths.

```sh
kaptaind-cli explain            # last 10 decisions, human form
kaptaind-cli explain --last 50
kaptaind --dry-run              # full pipeline minus staging/commit:
                                # prints bump, next version, exact message
```

Skips name the exact unmet threshold, e.g.
`skip: no_bump — score 0.042 below patch threshold 0.100`.

Outcome values: `commit`, `no_bump`, `test_failed`, `blocked`,
`version_write_failed`, `baseline_unresolvable`, `rate_limited`,
`clean_tree`, `pre_commit_hook_failed`, `commit_failed`, `error`.

## Reconfigure without restart

Editing `kaptaind.toml` or the ignore file hot-reloads **thresholds,
weights, rate limits, and the ignore matcher** within one cluster window.
The config files themselves never cluster. An invalid TOML edit keeps the
previous config and logs a warning — the daemon stays up.

Other sections (watch path, cluster window, test command, push) require a
restart.

## Version invariants

After every auto-commit, `VERSION`, `Cargo.toml`, and `Cargo.lock`'s
own-package entry agree, and all three are in the commit. The baseline is
resolved from `VERSION`, then `Cargo.toml [package].version`, and never
guessed; a computed downgrade is refused (commit fails with
`version_write_failed` in the decisions log).

## Monorepo notes

The daemon distinguishes the **git root** (where git commands anchor) from
the **project root** (where `kaptaind.toml`/`VERSION` live). All staging is
scoped to the project subtree: `all` mode runs `git add -A -- <project>`,
never across the whole worktree, and meta files are resolved against the
project root. `kaptaind analyze` and `--dry-run` likewise report only
in-project paths.
