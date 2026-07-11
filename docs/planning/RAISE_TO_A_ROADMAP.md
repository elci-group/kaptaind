# kaptaind — Roadmap to A / A+ across release domains

**Date:** 2026-07-09 · **Baseline:** `v9.7.16` tagged at `9aadae56`, from-source green, first stable shipped.
**Companions:** `STABLE_RELEASE_ROADMAP.md` (release engineering, executed), `FIRST_STABLE_VALIDATION_ROADMAP.md` (validation, executed for v9.7.16), `ENTERPRISE_STRATEGY.md` (feature rubric).
**Purpose:** lift every release domain that is currently **sub‑A** to **A** (solid, usable, minor gaps) or **A+** (production‑ready: automated, observable, auditable, documented). This is the *release‑readiness* lens (stricter than feature breadth) used in `STABLE_RELEASE_ROADMAP.md`, not the feature self‑grade in `ENTERPRISE_STRATEGY.md`.

---

## 0. Current grades, targets, and what "A / A+" means here

| Domain | Current | Target | A means | A+ means |
|---|---|---|---|---|
| Enterprise‑grade | B | **A+** | Signed, multi‑arch, SBOM'd releases produced by CI and attached to the tag; no un‑ignored high/critical advisories; web deploy coherent; rollback exists | All of A + keyless (Sigstore) SLSA provenance verified on every artifact; container distro matrix green; full observability parity (daemon **and** web); 24 h soak green on the reference tier |
| Open‑source | B | **A+** | Community standards green and *verified*; changelog/tag discipline automated; version line resolved and trusted; docs/links all resolve | All of A + provenance/SBOM published for public audit; reproducible build from a clean clone documented and CI‑enforced; clear MSRV + support policy |
| Home‑user | C+ | **A** | One‑command, **offline‑capable** install on each major OS via a package manager; first run succeeds offline; container first‑run works without hand‑holding | A + signed desktop (Tauri) app and an in‑app updater/migration path |
| **Overall** | **B‑** | **A+** | Every domain ≥ A | Every domain A+, with the evidence to prove it |

Two principles govern every track:
- **Evidence over assertion.** An A claim is only valid if an artifact or CI run proves it (checksum, signature, SBOM, soak log, matrix result). "Wired" ≠ "produced."
- **No guessed numbers/specs.** Hardware tiers, latencies, and package support are published only after measurement on the reference host (see `FIRST_STABLE_VALIDATION_ROADMAP.md` §6/§10).

---

## 1. P0 — Precondition: stabilize the working tree and establish CI truth

*Nothing below is achievable on a tree that is being rewritten by concurrent sessions. This is the gate for the whole plan.*

**Current state (2026‑07‑09):** 23 uncommitted tracked modifications and climbing; multiple long‑running `kimi` sessions on the shared host are mutating `main`'s working tree; a `touch src/lib.rs && cargo build` loop originating in `/home/sal/scotia` contends for the cargo lock.

Steps:
1. **Quiesce writers.** Identify and stop the runaway/scotia build loop and any agent writing to this checkout; confirm with `ps` and a stable `git status` over ≥10 min.
2. **Clean the tree.** Stash or commit the legitimate concurrent changes into their own branch; restore `main` to the tag (`git status` empty, `HEAD == v9.7.16` until the next planned cut).
3. **Protect `main`.** Enable branch protection (require PR + the `rust.yml`/`web.yml`/`security-audit.yml` checks) so no session can push or mutate `main` directly; tags created only by CI or a named maintainer.
4. **Establish CI truth for v9.7.16.** `release.yml` currently triggers on `push: branches:` and `workflow_dispatch` — **not** on tag push. Confirm whether the `main` push at `9aadae56` produced and attached the multi‑arch/signed artifacts; if not, add a `push: tags: ['v*']` trigger (or a `workflow_dispatch` on the tag) and re‑run, capturing the run URL as evidence.

**Exit (A):** `git status` clean and stable; `main` protected; CI run for `v9.7.16` green with artifacts attached (or a documented decision that the first multi‑arch drop is `v9.7.17`).

---

## 2. Track E — Enterprise‑grade: B → A+

### E1. Signed, multi‑arch, SBOM'd releases — *produced and attached*, not just wired
**Gap:** `v9.7.16` shipped a host‑only (`x86_64-unknown-linux-gnu`), unsigned tarball. The multi‑arch matrix, `SHA256SUMS`, SPDX SBOM and Sigstore/SLSA provenance are wired in `.github/workflows/release.yml` and the in‑tree `src/release/{sbom,provenance}.rs`, but not verified as produced for the tag.

Steps:
1. Verify/repair the `release.yml` trigger (P0‑4) so a tag produces the full matrix: `linux x86_64/arm64`, `macOS x86_64/arm64`, `windows x86_64`.
2. Attach per‑artifact `.sha256`, a consolidated `SHA256SUMS` (+ detached signature), an SPDX 2.3 SBOM (from `Cargo.lock`/`web/package-lock.json`), and SLSA v1.0 provenance.
3. **Sigstore keyless** signing (cosign) of every artifact + the SBOM, with a `cosign verify` step in the workflow and instructions in `SECURITY.md`/release notes.
4. Add a release verification job that downloads its own artifacts and checks checksum + signature + SBOM well‑formedness before publishing (fail closed).
5. Document the verification one‑liner in the release notes template.

**Exit (A):** a tag push yields ≥5 target binaries, each with checksum + SBOM + signature, attached to the GitHub release; the verify job is green.
**Stretch (A+):** keyless signatures verify end‑to‑end from the release notes; provenance is in‑toto/SLSA and reproducible by a third party from the tag.

### E2. Supply chain: retire the git2 accepted‑risk
**Gap:** `git2 0.19.0` (RUSTSEC‑2026‑0008/0183/0184) and a few optional‑feature advisories are ID‑allow‑listed in `deny.toml`/`.cargo/audit.toml`.

Steps:
1. Upgrade `git2` → `0.20` / `libgit2` ≥ 1.9; fix API breaks in `src/git/**`, `src/commit/**`, `src/push/**`; re‑run the full gate + a real commit/push integration test.
2. Drop the three git2 IDs from the allow‑lists; confirm `cargo audit`/`cargo deny` are green **with zero** git2 ignores.
3. Re‑assess the optional‑feature advisories (`ttf-parser`/fltk, `quick‑xml`/notify‑rust, `quinn‑proto`/reqwest‑http3): upgrade where a compatible bump exists; keep only ID‑scoped, justified, re‑eval‑dated entries.
4. Add a CI gate that fails on any *new* advisory (default features) and on any allow‑list entry older than N days (stale‑ignore watchdog).

**Exit (A):** no git2 ignores; `cargo audit`/`cargo deny` green on default features without the v9.7.16 exceptions.
**Stretch (A+):** all‑features deny clean; stale‑ignore watchdog enforced in CI.

### E3. Web/deployment coherence (the P0‑2 contradiction)
**Gap:** `release.yml` still does the static‑export `mv app/api` hack; `docker-compose.yml` serves `web` as nginx static files, so `/api/*` and `/dashboard/*` cannot run. `SECURITY.md`/`next.config.ts` headers need reconciliation.

Steps:
1. Switch `web/` to Next.js `output: 'standalone'`; delete the `mv app/api`/`mv app/dashboard` steps from `release.yml`; build the Node server image.
2. Rewrite `docker-compose.yml` `web` as the Node server behind nginx (reverse proxy), or explicitly scope compose to the static marketing site and document the split.
3. Add HSTS + a real CSP (Stripe `script`/`frame`/`connect` allowlist) + `Permissions‑Policy` in `next.config.ts`; align with nginx to avoid duplication; scan with a header checker in CI.
4. Fail‑fast env validation at boot (`NEXTAUTH_SECRET`, `STRIPE_WEBHOOK_SECRET`, `DATABASE_URL`); gate `prisma/seed.ts` test user behind `NODE_ENV==='development'` with a random password.
5. Smoke test in CI: `docker compose up` → `/api/health` reachable → no default creds.

**Exit (A):** `docker compose up` yields a working daemon **and** web with API routes reachable; headers pass a scanner; no default/weak creds in the shipped example.
**Stretch (A+):** web exposes `/metrics` + structured logs to match the daemon; OTel spans end‑to‑end.

### E4. First‑class rollback
**Gap:** no `kaptaind-cli rollback`/`undo`; recovery is documented `git revert` only.

Steps:
1. Add `src/cli/commands/rollback.rs`: locate the last kaptaind commit (via `.kaptaind/analysis` + audit), `git revert` it (no‑commit), optionally restore the prior `VERSION` and the cluster's pre‑image, and record an audit entry.
2. `--last`, `--cluster <id>`, `--dry-run`, and a confirmation prompt unless `--yes`; refuse on a dirty tree unless `--force`.
3. Tests: revert a synthetic kaptaind commit and assert tree + `VERSION` restored.

**Exit (A):** `kaptaind-cli rollback --last` reverts a real dogfood commit and restores `VERSION`; covered by tests.

### E5. Reference‑tier soak (evidence for A+)
**Gap:** 24 h soak is CI/scheduled, never run end‑to‑end with published results.

Steps:
1. Run `kaptaind-cli soak --duration 24h --rate 20/s --fixtures mixed` in a container capped at the T1 ceiling; sample RSS/fd/error rate; assert bounded growth and zero data loss.
2. Publish the `.kaptaind/soak/<id>.json` and the verdict in the qualification report for the next cut.

**Exit (A+):** a 24 h soak artifact exists showing stable RSS/fd and zero data loss on the reference tier.

---

## 3. Track O — Open‑source: B → A+

### O1. Resolve the version line (trust)
**Gap:** semver sits at `9.7.16` for a self‑described *first* stable, with no tags before it; the jump from `0.1.44` is documented but still signals prior runaway auto‑versioning.

Steps (choose one, record the decision in `CHANGELOG.md`):
- **Option A (keep 9.x):** publish a short "versioning policy" note explaining the 9.x line is intentional and frozen for stable, and that the daemon no longer self‑versions this repo (already true). Lowest churn.
- **Option B (reset optics):** tag the next stable as `1.0.0` and keep `9.x` as a legacy/internal line; document the mapping. Higher trust, more churn (Cargo.toml/VERSION/tags).

**Exit (A):** a one‑paragraph versioning policy is committed and linked from `README.md`; `VERSION`/Cargo.toml/CHANGELOG/tag agree and are provably CI‑driven.

### O2. Automated changelog on tag
**Gap:** changelog was reconciled by hand; it will drift again.

Steps:
1. Adopt "Keep a Changelog"; add a CI job that, on tag, generates the `[Unreleased] → [X.Y.Z]` section from conventional commits + `.kaptaind/analysis` and opens/updates the release notes.
2. Lint PRs for a changelog entry (or a `skip-changelog` label).

**Exit (A):** tagging `vX.Y.Z` auto‑produces the CHANGELOG section and the GitHub release notes; no hand edits.

### O3. Community standards green *and verified*
**Gap:** CoC/CODEOWNERS/issue templates exist (v9.7.16); the `SECURITY.md` reporting contact and CODEOWNERS coverage are unverified.

Steps:
1. Confirm the `SECURITY.md` reporting channel resolves to a monitored inbox (send a test, record the response path).
2. Verify CODEOWNERS covers `src/**`, `web/**`, `.github/**`, `packaging/**`; GitHub "community standards" checklist shows 100%.
3. Add a `SUPPORT.md`/support policy (response expectations, supported versions, MSRV).

**Exit (A):** community standards checklist 100%; security contact round‑trip verified; support/MSRV policy published.

### O4. Doc & link integrity
**Gap:** stale `tutorial_*.md`, `scripts/release.sh`, and `man/` references; `LANGUAGE_MATRIX.md` cross‑checks.

Steps:
1. Link‑check `README.md`/`man/**`/`docs/**` in CI (e.g., `lychee`); repair or remove every dangling reference.
2. Regenerate `LANGUAGE_MATRIX.md` from the adapter registry (28 adapters) so it can't drift.
3. Sync `man/kaptaind.1.md` and `man/kaptaind-cli.1.md` to current flags (the new `doctor/stress/report/logs/audit/probe/rollback` commands).

**Exit (A):** CI link‑check green; `LANGUAGE_MATRIX.md` and man pages match the running binary.

### O5 (A+). Public auditability
Publish SBOM + provenance + a "reproducible build from a clean clone" doc, and enforce the clean‑clone build in CI (the v9.7.16 from‑source fix made this possible; keep it true).

---

## 4. Track H — Home‑user: C+ → A

### H1. Package‑manager install paths
**Gap:** no Homebrew/`.deb`/`winget` published; they depend on the multi‑arch CI artifacts (E1).

Steps:
1. Homebrew tap `elci-group/homebrew-tap` with a `kaptaind` formula publishing the macOS arm64/x86_64 bottles from E1.
2. `.deb` (and a lightweight apt repo or per‑release asset) built from the Linux binaries; `cargo-deb` metadata in `packaging/deb/`.
3. `winget` manifest (+ a signed zip) for Windows from E1.
4. An install‑matrix CI job that installs each package on a clean OS image and runs `kaptaind-cli --version` + `init` offline.

**Exit (A):** on each major OS, a one‑command install from the published package manager works offline and reports the right version.

### H2. Offline‑first first run
**Gap:** inference "auto" provider and network‑enabled language can surprise an offline home user; the daemon's default test hook (`cargo test`) and push settings need clearer offline defaults.

Steps:
1. Default inference to **offline‑only** unless a provider key is present; make `[capabilities]` network flags explicit and documented; `kaptaind init` prints the effective offline/online mode.
2. Ensure `init` → a synthetic change → a commit works fully offline in a fresh repo (CI smoke), with push **off** by default.
3. First‑run hints in `status`/`doctor` that explain the mode and how to go online.

**Exit (A):** a clean offline machine completes `init` → first commit with no network calls; mode is obvious to the user.

### H3. Container first‑run without hand‑holding
**Gap:** bare `docker run` exits because `/opt/kaptaind` is empty (v9.7.16 finding); healthy only with a mounted repo.

Steps:
1. `docker-entrypoint.sh`: if `/opt/kaptaind` is not a git repo, `git init` it, write a minimal `kaptaind.toml`, and print the mount instruction; keep the non‑root drop and writable `.kaptaind` volume.
2. Document the mount vs. auto‑init modes; CI smoke that runs the bare image and asserts `Health=healthy`.

**Exit (A):** bare `docker run` of the released image reaches `healthy` and explains itself; mounted‑repo mode still works.

### H4 (A+). Signed desktop app
Package `apps/desktop` (Tauri) for macOS/Windows/Linux with code signing and an in‑app updater/migration prompt; out of scope for A, required for A+.

---

## 5. Cross‑cutting — validation evidence that *backs* the A claims

Ties to `FIRST_STABLE_VALIDATION_ROADMAP.md` phases V3–V6; required so "A/A+" is evidenced, not asserted.

1. **Macro‑bench `kaptaind-cli bench`** (A2) + LKG baselines in `tests/bench-baselines/`; regression gate (>15% latency / >10% throughput fails) running on `main` + `workflow_dispatch`.
2. **Container distro matrix** (F1) — Ubuntu/Debian/Fedora/Alpine + per‑tier resource caps — green on `main` and on each RC.
3. **Determinism:** `trace verify`/`audit verify` pass on a sampled set; benches pin seeds and report variance.
4. **Soak** (E5) published for the RC.

**Exit (A):** bench‑compare + distro matrix + determinism checks green on `main`.
**Exit (A+):** + 24 h soak artifact attached to the RC qualification report.

---

## 6. Sequencing backbone

Dependencies first (CI truth + integrity before distribution; distribution before package publishing). Parallelizable across Rust‑tooling / CI / web / packaging tracks.

| Phase | Tracks | Output | ~Effort |
|---|---|---|---|
| **A0 — Stabilize & prove CI** | P0 (working tree, branch protection, release.yml trigger/verify) | Clean tree; v9.7.16 (or .17) CI run green with artifacts attached | 2–3 d |
| **A1 — Enterprise core** | E1 (signed/multi‑arch/SBOM), E2 (git2 0.20) | A‑grade enterprise release; git2 allow‑list removed | 5–8 d |
| **A2 — Web coherence** | E3 (standalone, compose, headers, env) | `docker compose up` works; headers clean | 4–6 d |
| **A3 — Distribution** | H1 (Homebrew/.deb/winget), O4 (docs) | One‑command install per OS; docs/links green | 5–8 d |
| **A4 — Home‑user first run** | H2 (offline‑first), H3 (container first‑run), E4 (rollback) | Offline first run + rollback verified | 3–5 d |
| **A5 — Open‑source polish** | O1 (version policy), O2 (auto‑changelog), O3 (community verified) | Trusted version line; automated notes; standards 100% | 3–4 d |
| **A6 — A+ evidence** | §5 (macro‑bench, matrix, soak), E5, O5 | A+ claims backed by artifacts; reproducible clean‑clone build in CI | 5–8 d |

**Estimated total: ~4–6 focused weeks** (parallelizable). Domains reach **A** after A0–A5; **A+** after A6.

---

## 7. A / A+ Definition of Done (measurable)

A domain is **A** only when all its boxes are ticked with an artifact or CI link; **A+** when its stretch boxes are too.

**Enterprise**
- [ ] A: tag push produces ≥5 target binaries + `.sha256` + `SHA256SUMS` + SPDX SBOM + signature, attached; verify job green (link to run).
- [ ] A: zero git2 allow‑list entries; `cargo audit`/`cargo deny` green on default features.
- [ ] A: `docker compose up` → daemon + web API reachable; headers pass a scanner; no default creds.
- [ ] A: `kaptaind-cli rollback --last` reverts a real commit + restores `VERSION` (test).
- [ ] A+: keyless signatures verify from the release notes; distro matrix green; web `/metrics` parity; 24 h soak artifact.

**Open‑source**
- [ ] A: versioning policy committed; VERSION/Cargo.toml/CHANGELOG/tag agree and CI‑driven.
- [ ] A: tag auto‑generates CHANGELOG + release notes; PR changelog lint.
- [ ] A: community standards 100%; security contact round‑trip verified; SUPPORT/MSRV published.
- [ ] A: CI link‑check green; `LANGUAGE_MATRIX.md` (28 adapters) + man pages match the binary.
- [ ] A+: SBOM/provenance published; reproducible clean‑clone build enforced in CI.

**Home‑user**
- [ ] A: Homebrew + `.deb` + `winget` install offline on clean OS images; `kaptaind-cli --version` correct.
- [ ] A: offline `init` → first commit with no network; mode obvious to the user.
- [ ] A: bare `docker run` reaches `healthy` and self‑documents; mounted‑repo mode works.
- [ ] A+: signed Tauri desktop app with updater/migration.

**Overall A+:** every box above ticked, each backed by a link/artifact, and the qualification report for the cut shows **overall PASS** (no PASS‑WITH‑NOTES for any A‑gated section).

---

## 8. Risks & notes

- **Working tree first.** Attempting A1+ on the current drifting tree will corrupt work and invalidate evidence. P0 is non‑negotiable.
- **Don't claim "signed" before it's verified.** E1 must produce *and* verify; a wired‑but‑unrun pipeline is the B‑grade state we're leaving.
- **git2 0.20 blast radius.** It's a major bump touching commit/push/diff; do it in A1 with full integration tests, not during a release window.
- **Package managers gate on E1.** Homebrew/.deb/winget cannot ship until multi‑arch artifacts exist; sequence A1 before A3.
- **Version reset churn (O1‑B).** If `1.0.0` is chosen, coordinate tags/Cargo.toml/VERSION/CHANGELOG in one cut and communicate clearly; otherwise keep 9.x and document.
- **Scope discipline.** This plan lifts release domains to A/A+; it does not add product features. Keep `ENTERPRISE_STRATEGY.md` "Remaining Incremental Work" (OTel spans, web dashboard, deterministic benches) as A+ stretch, not A blockers — except deterministic benches, which are A‑grade evidence (§5).
