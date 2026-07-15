# Workspace Version Bumping Plan

**Status:** in progress — W0+W1+W2 landed 2026-07-14 (W0 `src/version/workspace.rs` discovery; W1 `src/version/writeback.rs` + `[versioning].workspace` = `root_only`(default)/`touched`/`lockstep` + 8 daemon regression tests in `tests/workspace_regressions.rs`; W2 `members_bumped` in decisions.jsonl + `explain` rendering, commit-subject member scope via `dominant_member`, `kaptaind-cli doctor` workspace checks (`workspace_lock_drift`, `workspace_requirement_unsatisfiable`, `workspace_root_only_deflation`), `daemon_soak_workspace_member_waves` in `tests/soak.rs`); W3 partial 2026-07-14 — this repo now dogfoods `[versioning].workspace = "touched"` and `release.yml` cuts member tags (`kaptaind-diff-vX.Y.Z`) via tag-existence detection; running a live daemon here was deferred by the owner, so the "kaptaind-diff self-bump produced by the daemon" criterion stays open · **Target:** v10.x (opt-in) → v11.0.0 (default flip) · **Date:** 2026-07-12

Companion to `AUTONOMOUS_COMMIT_SAFETY_PLAN.md`. That plan made a single-project
version triple (VERSION ⇔ Cargo.toml ⇔ Cargo.lock) coherent. This plan extends
the same coherence to Cargo **workspaces**, where one daemon decision must keep
*N* member manifests and one shared lockfile consistent.

---

## 1. Problem statement (evidence from this repository)

kaptaind's own repo is the reference case:

- Root `Cargo.toml`: `[workspace] members = [".", "crates/kaptaind-diff"]`.
- `kaptaind` is at **10.0.1**; `crates/kaptaind-diff` is at **9.6.3** and has
  not moved across the entire v9.7 → v10 arc, despite its source changing
  (Workstream B4 and the adapter-200 effort both touched it).
- The daemon watches the project root. When a cluster only contains
  `crates/kaptaind-diff/**` paths, the score is computed, the bump decided,
  and `save_version` writes the **root** VERSION/Cargo.toml/lock entry. The
  member whose code actually changed never moves. This is silent version
  **deflation** — the mirror image of finding #17 (inflation), and just as
  corrosive: a published member would never release its fixes.

Current writeback mechanics (`src/daemon/scheduler.rs:1919` `save_version`):

1. Write `VERSION` at project root.
2. Edit `[package].version` in `Cargo.toml` (and `src-tauri/Cargo.toml`) via
   `toml_edit`.
3. `sync_cargo_lock`: update the own-package `[[package]]` entry in
   `Cargo.lock`.

None of the three steps knows `[workspace]` exists. There is no member
enumeration, no `workspace.package.version` inheritance handling, and no
awareness that the lockfile lists *every* member.

### Scope of the problem in the wild

Three workspace layouts exist in practice, all must be handled:

| Layout | Example | Bump must touch |
|--------|---------|-----------------|
| Root crate + members | kaptaind (root crate + `crates/kaptaind-diff`) | root and/or touched members |
| Virtual workspace | `[workspace]` only, no root `[package]` | touched members only |
| Inherited version | `member.version.workspace = true` | workspace root `[workspace.package]` once |

---

## 2. Goals / non-goals

**Goals**

- G1. A cluster that only touches member code never moves the root version
  (and vice versa) — under the new default policy.
- G2. Every bump leaves the workspace **N-tuple consistent**: each touched
  member's `Cargo.toml` version equals its `Cargo.lock` entry, and (for the
  root crate) VERSION agrees too.
- G3. `workspace.package.version` inheritance is resolved and written
  correctly (write once at the root, never into members).
- G4. Inter-member path dependencies with version requirements
  (`member-a = { path = "../a", version = "1.2" }`) stay satisfiable after a
  bump; the lockfile never drifts.
- G5. Zero behavior change for non-workspace projects and for existing
  single-crate dogfood repos (scotia, fract, ontism) — verified by the full
  existing suite unmodified.
- G6. kaptaind's own repo adopts the new policy first (dogfood), retiring the
  kaptaind-diff drift.

**Non-goals**

- crates.io publishing (`cargo publish` ordering, yank handling). The plan
  stops at *publish-ready* state; a separate release workstream owns upload.
- Independent per-member *scoring* (separate bump decisions per member from
  one cluster). One cluster → one bump decision, applied to the right set of
  manifests. Per-member scoring is a possible v12 exploration, not required
  for coherence.
- Multi-*repo* version trains (the home monorepo). Out of scope: each
  kaptaind-watched project remains autonomous.

---

## 3. Design

### 3.1 Workspace discovery

New module `src/version/workspace.rs`:

- `WorkspaceLayout::discover(project_root) -> Layout` — parse root
  `Cargo.toml` with `toml_edit` (already a dependency):
  - `RootCrate { members: Vec<Member> }` — `[package]` + `[workspace]`.
  - `Virtual { members: Vec<Member> }` — `[workspace]` only.
  - `Single` — no `[workspace]` (today's path; zero-cost fallback).
- `Member { name, manifest: PathBuf, inherits_version: bool }` — resolved
  from `members` globs (support explicit lists and simple `crates/*` globs;
  exclude `exclude` entries). `inherits_version` is true when the member
  declares `version.workspace = true`.
- Discovery runs once at daemon start and on config hot reload; manifests
  changing members mid-run are picked up by the existing rescan.

### 3.2 Bump policies — `[versioning] workspace`

(Implemented under the existing `[versioning]` block, which already owns
version policy — not a new `[version]` section as first drafted.)

```toml
[versioning]
workspace = "root_only"   # today's behavior (default through v10.x)
# workspace = "touched"   # bump only members the cluster touched (v11 default)
# workspace = "lockstep"  # every bump applies to every member
```

- **root_only** — current behavior, unchanged. Keeps v10 compatibility.
- **touched** (the recommended model): one cluster → one bump decision (the
  existing scorer/weight/`decide` pipeline, unchanged). The decided bump is
  applied to **every member whose subtree contains at least one cluster
  path**, plus the root crate only if the cluster touched paths outside all
  member subtrees. A `crates/kaptaind-diff`-only cluster therefore bumps
  kaptaind-diff alone — fixing deflation without inflating the root.
- **lockstep** — the decided bump applies to every member + root. For teams
  that release the workspace as one train.

Virtual workspaces behave as `touched` minus the root clause regardless of
setting (there is no root version to move).

### 3.3 Writeback: `save_workspace_version`

`save_version` becomes the `Single` special case of:

```
save_workspace_version(layout, bump, cluster_paths, repo_root)
```

- For each target member (per policy §3.2):
  - If `inherits_version`: edit `[workspace.package].version` at the root
    **once** (dedupe across inheriting members).
  - Else: edit that member's `[package].version` in place via `toml_edit`.
- Root crate additionally writes `VERSION` (members never get VERSION files;
  `resolve_baseline` already prefers VERSION → Cargo.toml, so member baselines
  resolve from their manifests with no new code).
- `sync_cargo_lock` generalizes to a `(name, version)` map: one lockfile pass
  updates all bumped members' `[[package]]` entries.
- **Inter-member requirement check** (G4): after editing, for every path
  dependency between workspace members that carries a `version` requirement,
  verify `semver::VersionReq::matches(new_version)`. On violation: bump the
  requirement's lower bound to the new version (the only edit that keeps both
  manifests truthful and `cargo build --locked` green) and record it in the
  decision log. Never silently widen to `*`.
- The monotonic downgrade guard (`save_version`'s existing check) applies
  per member.
- All writes happen before staging, and the SelfWriteGuard already suppresses
  the writeback paths — extended per member manifest path so multi-member
  writeback cannot cascade (A2 invariant preserved).

### 3.4 Observability

- `decisions.jsonl` gains `members_bumped: ["kaptaind", "kaptaind-diff"]`
  (additive field; older readers ignore it per the compatibility contract).
- `kaptaind-cli explain` renders the member list when present.
- Commit message (D2 format) gains the member scope when a single member
  dominates: `feat(kaptaind-diff): extend public API (…)`.
- `kaptaind-cli doctor` flags: member lockfile drift (manifest ≠ lock entry),
  unsatisfiable inter-member requirements, and `workspace = "root_only"` on a
  repo whose recent commits are member-only (the deflation signature).

### 3.5 kaptaind's own adoption (dogfood first)

1. Land the feature with `workspace = "root_only"` default — no behavior
   change anywhere.
2. Set `workspace = "touched"` in kaptaind's own `kaptaind.toml`, run a
   daemon on the repo (currently none runs here), and let kaptaind-diff earn
   its first self-bump from the next member-only cluster.
3. `release.yml` learns member tags: a member bump produces tag
   `kaptaind-diff-vX.Y.Z` alongside the root `vX.Y.Z` flow; the matrix build
   stays root-only (kaptaind-diff ships as a library, not a binary).
4. After two clean dogfood weeks: flip the default to `touched` in v11.0.0
   with a CHANGELOG migration note (same playbook as Workstream D).

---

## 4. Testing strategy

Extends the existing harness (`tests/regressions/harness.rs`):

- **Workspace fixture**: tempdir repo, root crate + two members
  (`crates/alpha`, `crates/beta`), one inheriting-version member, one
  inter-member path dep with a version requirement. Fresh health ports
  (19117+).
- Named regression tests:
  - `member_only_edit_bumps_member_not_root` — alpha-only cluster → alpha
    patch bump, root VERSION unchanged (deflation fix).
  - `root_only_edit_bumps_root_not_members`.
  - `cross_member_cluster_bumps_all_touched` — cluster spanning alpha+beta →
    both move, one commit, one decision.
  - `workspace_lock_consistent_after_every_bump` — every member's
    `Cargo.toml` == its `Cargo.lock` entry at HEAD (extends the triple
    invariant to the N-tuple).
  - `inherited_version_written_at_root_once` — `version.workspace = true`
    member: root `[workspace.package]` edited, member untouched.
  - `inter_member_requirement_stays_satisfiable` — path dep with
    `version = "x.y"` survives a bump; requirement floor advanced, lock green.
  - `lockstep_bumps_everything`.
  - `virtual_workspace_has_no_root_bump`.
- **Soak extension**: the nightly soak's workload generator adds member-subtree
  waves; invariant (b) becomes the N-tuple check. No new CI job — the existing
  `soak.yml` covers it.
- **Property test**: `version_never_moves_backwards` extends per member
  (proptest already in place for the root).

## 5. Milestones

| Milestone | Content | Gate |
|-----------|---------|------|
| **W0** | `workspace.rs` discovery + layout types + unit tests (no behavior change) | lib suite green; discovery proptest vs hand-built layouts |
| **W1** | `save_workspace_version` + `touched`/`lockstep` behind `root_only` default; 8 regression tests | N-tuple invariant green; existing suite unmodified |
| **W2** | Observability (decisions field, explain, doctor, commit scope) + soak member waves | doctor flags seeded drift; soak 2× clean |
| **W3** | Dogfood on kaptaind repo (`touched`); release.yml member tags; kaptaind-diff self-bumps | 2 weeks clean; `kaptaind-diff-v*` tag exists |
| **v11** | Default flip `root_only` → `touched`; migration guide | doctor migration check, CHANGELOG |

Single senior engineer: ~3–4 weeks elapsed, W0–W2 shippable in the first two.

## 6. Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Member glob mis-resolution bumps the wrong crate | Medium | High | Explicit-list fast path; discovery unit tests per layout; doctor reports resolved members for operator review |
| Inter-member requirement rewrite surprises users | Medium | Medium | Only ever *raises* the floor; logged in decisions.jsonl; never widens |
| Inherited-version root edit races with member edits | Low | Medium | Single write before staging; SelfWriteGuard covers root manifest already |
| `touched` policy hides root changes in mixed clusters | Low | Low | Root clause: any path outside member subtrees includes the root |
| Workspace detection breaks exotic layouts (renamed manifests, `package.workspace` members) | Medium | Low | Fall back to `Single` + warn; never guess |

## 7. Definition of Done

- All 8 workspace regression tests + soak member waves green in CI.
- N-tuple consistency holds across a 30-minute soak with member waves.
- kaptaind-diff carries a self-earned bump and tag, produced by the daemon.
- `kaptaind-cli doctor` catches seeded member drift on a fixture.
- CHANGELOG + migration note for the v11 default flip.
