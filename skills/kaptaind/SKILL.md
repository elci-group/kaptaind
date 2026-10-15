---
name: kaptaind
description: >-
  Use `kaptaind` (10.4+) for semantic-versioned git commits, change analysis,
  and release automation. Reach for it whenever the user asks to "commit via
  kaptaind", "commit and push via kaptaind", "let kaptaind version this",
  bump VERSION from the actual diff, check why kaptaind did not commit,
  inspect the daemon, or run an Aim-of-Change session. Use `kaptaind ready`
  before mutating and `kaptaind commit --push --json` for a one-shot,
  daemon-equivalent commit. `kaptaind-cli` is deprecated: always call
  `kaptaind <cmd>`.
---

# kaptaind

`kaptaind` watches a repo, clusters changes, scores the diff, decides a
semantic bump, writes `VERSION`, commits, and optionally pushes. Since 10.4.0
every former `kaptaind-cli` subcommand is a `kaptaind` subcommand, and
`-r, --repo <PATH>` is a global flag accepted by all of them.

## Recipe: "commit and push via kaptaind"

```bash
# 1. One read-only pass over every gate. Fix each `block` with its printed fix.
kaptaind ready --for push          # or: kaptaind ready --for commit (default)

# 2. One transaction through the daemon pipeline, then exit.
kaptaind commit --push --json
```

Exit codes of `kaptaind commit`:

| Code | Meaning | What to do |
|------|---------|------------|
| 0 | committed (and any requested push succeeded) | report `version_after` and `commit` |
| 1 | error (commit/version write/push failed) | read `reason`, `kaptaind logs errors` |
| 3 | nothing to commit (clean tree, `no_bump`, nothing staged) | tell the user; not a failure |
| 4 | refused by a gate | diagnose, see below |

JSON shape (one object on stdout):

```json
{"outcome":"commit","version_before":"1.2.0","version_after":"1.3.0",
 "commit":"<sha>","push":{"status":"ok|disabled|refused|failed|skipped","detail":"..."},
 "reason":"committed","cluster_id":"<uuid>","paths":["src/lib.rs"],"skipped_tests":null}
```

`push.status = "skipped"` means no commit was created. `outcome` is the
decision code from `.kaptaind/decisions.jsonl` (or `daemon_running`,
`commit_in_progress`, `untrusted`, `unauthorized`, `ha_enabled`, `suspended`
for pre-flight refusals).

### 3. On exit 4

```bash
kaptaind explain            # last decisions, with the threshold/gate that stopped it
kaptaind status --json      # alive, last_outcome, last_error, suspended, version (HEAD), worktree_version
kaptaind ready --for push   # re-check after fixing
```

- Never background, restart, or kill the daemon to get a commit through.
- Never silently fall back to plain `git commit`/`git push`. If kaptaind
  cannot commit, say why and ask the user before using git directly.
- `--skip-tests "<reason>"` only when the user explicitly accepts the risk.
  The reason is mandatory, lands in the decision log, and becomes a
  `Kaptaind-Skipped-Tests: <reason>` commit trailer.
- Do not edit `kaptaind.toml` gates (mode, trust, push) without the user's
  consent: they are deliberate per-repo opt-ins.

## Gates (`kaptaind ready`)

States: `pass`, `block`, `warn`, `info`, `n/a`. Exit 0 when nothing required
by `--for analyze|commit|push` blocks, 4 otherwise. `--json` emits
`{"for","ready","gates":[{"id","state","detail","fix"}]}`.

| Gate id | Blocks | Fix (kaptaind.toml key or command) |
|---------|--------|------------------------------------|
| `operation_mode` | commit, push | `[operation] mode = "actuate"` (default is `observe`) |
| `execution_trust` | commit, push | review commands, then `[trust] execution = "trusted"` |
| `test_hook` | commit, push (if required) | install the program, fix `[test] command`, or `[test] required = false` |
| `push_enabled` | push | `[push] enabled = true` |
| `network_push` | push | `[capabilities] network_push = true` (not with `air_gapped`) |
| `protected_branch` | push | switch to a feature branch, or edit `[push.safety] protect_branches` |
| `startup_guard` | daemon start only (info/warn) | commit/stash, `kaptaind --force`, or `[daemon] startup_guard = false` |
| `version_baseline` | commit, push (if unresolvable) | `echo 0.1.0 > VERSION`, or `[versioning] bootstrap` / `initial_version` |
| `kaptaind_dir_writable` | commit, push | make `.kaptaind/` writable, or `--repo` a writable checkout |
| `nested_git_root` | all, if not a git repo | `git init`, or `--repo <git root>`; a monorepo subproject only warns |
| `suspended` | commit, push | `kaptaind resume` |
| `daemon_running` | commit, push | let the daemon commit, or `commit --force` |

## Scope a commit to your own files

```bash
kaptaind commit --paths src/parser.rs docs/ --push --json
```

`--paths` restricts the cluster AND staging to those files/directories (plus
version files). Other dirty files stay unstaged. Use it when the user or
other agents have unrelated uncommitted work in the tree.

## Daemon vs one-shot

- If `kaptaind status --json` shows `"alive": true`, a daemon owns the repo:
  let it commit (wait for the cluster window, then `kaptaind explain`).
  `kaptaind commit` refuses (exit 4, `daemon_running`) while a live daemon
  holds `.kaptaind/daemon.pid`; `--force` races it, so avoid it.
- With no daemon, use `kaptaind commit`. It needs actuate + trusted like the
  daemon (observe mode records `observed` and exits 4), runs the first-run
  VERSION bootstrap, ignores `startup_guard` and the commit rate limit, and
  pushes even without `--push` when `[push] enabled = true` (never twice).
- Preview without side effects: `kaptaind analyze` or `kaptaind --dry-run`.
- Pause/unpause the daemon: `kaptaind suspend --reason "..."`, `kaptaind resume`.

## Status and logs

```bash
kaptaind status --json     # alive vs last_outcome (committed|skipped|failed|suspended|null)
kaptaind explain --last 25 # decisions incl. test exit_code, output_tail, missing_program
kaptaind logs              # last 50 lines (same as: kaptaind logs tail)
kaptaind logs tail 80      # also: logs tail -n 80, logs --tail 80, logs --lines 80
kaptaind logs errors       # ERROR/WARN lines only
kaptaind logs grep "push"  # regex filter; add --format json for JSON
kaptaind doctor            # host profile + migration findings (binary_shadowing, ...)
```

`alive` (pid file present and process running) is independent of
`last_outcome`: a dead daemon can have a `committed` last result, and a live
one can have `failed`.

## Aim of Change (AoC) sessions

```bash
kaptaind aoc start "feature: auth flow"   # suspends the daemon by default
kaptaind aoc status
kaptaind aoc ship                         # end + ship the session; resumes the daemon
kaptaind aoc cancel                       # abandon; resumes the daemon
kaptaind aoc log
```

There is no `aoc stop`. Auto suspend/resume is governed by
`[daemon] auto_suspend_on_aoc_start` / `auto_resume_on_aoc_end`.

## First-run version bootstrap

With no `VERSION` and no root `Cargo.toml [package].version`, startup (daemon
and `kaptaind commit`) establishes a baseline per `[versioning] bootstrap`:

- `"initialize"` (default): seed `VERSION` from the highest semver git tag,
  else `[versioning] initial_version` (default `"0.1.0"`); written only in
  actuate mode.
- `"refuse"`: refuse until you create `VERSION` (`echo 0.1.0 > VERSION`).

## Common failures → fix

| Symptom | Cause | Fix |
|---------|-------|-----|
| exit 4, `outcome: "observed"` | observe mode (the default) | ask the user, then `[operation] mode = "actuate"` |
| `untrusted configuration cannot enable program execution` | untrusted config | review `[test] command` etc., then `[trust] execution = "trusted"` |
| `push.status: "disabled"` | push off | `[push] enabled = true` + `[capabilities] network_push = true` |
| `push.status: "refused"`, protected branch | pushing `main` etc. | feature branch, or edit `[push.safety] protect_branches` |
| `baseline_unresolvable` | no/invalid VERSION | `echo 0.1.0 > VERSION`, or `[versioning] bootstrap = "initialize"` |
| `test_hook_not_found` (exit 127) | test program not on PATH | install it or fix `[test] command`; it still blocks a required hook |
| `test_failed` | tests fail | fix tests (`kaptaind explain` shows `output_tail`); `--skip-tests` only with user consent |
| `daemon_running` | live daemon owns the repo | let it commit, or `commit --force` |
| `commit_in_progress` | another `kaptaind commit` holds `.kaptaind/commit.lock` | wait for it to finish, then retry |
| `kaptaind-cli is deprecated` / old behaviour, missing subcommands | stale `kaptaind-cli` on PATH | `kaptaind doctor` → `binary_shadowing`; `type -a kaptaind kaptaind-cli`, remove stale copies, re-run install.sh |

## `kaptaind-cli` is deprecated

`kaptaind-cli` is now an install.sh shim that prints a deprecation line to
stderr and execs `kaptaind` with the same arguments; it is planned for
removal in 11.0. Old docs that say `kaptaind-cli analyze` mean
`kaptaind analyze`. If a `kaptaind-cli` behaves differently from `kaptaind`,
it is a stale standalone binary: see `binary_shadowing` above.

## Observation corpus

Census and extraction only. Do not treat this as a trained model, and do not
collapse labelled commits, Kaptaind-era commits, and observed analyses into
one number.

```bash
kaptaind data census
kaptaind data census --org elci-group
kaptaind data extract
kaptaind data validate
kaptaind data dedupe
kaptaind data stats
kaptaind dataset build --version 0.1.0
kaptaind dataset inspect
```

`kaptaind data extract` reads `git show` / `git diff-tree` and does not check
out old commits. A symbol whose structural class disagrees with the commit
label is kept, marked ineligible, and is not taught as ground truth. Root
`VERSION`, paths under `.kaptaind/`, and build trees (`target/`, `dist/`,
`node_modules/`) are bookkeeping and are not training-eligible.

## Configuration pointers

`kaptaind.toml` in the repo root (or `--config <path>`); an optional `.env`
is loaded for provider keys. Key sections: `[operation]`, `[trust]`, `[test]`,
`[push]` / `[push.safety]`, `[capabilities]`, `[versioning]`, `[staging]`,
`[daemon]`, `[cluster]`, `[ratelimit]`, `[weights]`, `[version_thresholds]`,
`[notify]`. Validate syntax with `kaptaind validate`; check gates with
`kaptaind ready`. Generate a profile with `kaptaind init` (observe-only).
