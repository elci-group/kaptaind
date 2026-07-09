# Kaptaind — Stable-Release Assessment & Roadmap

**Date:** 2026-07-09
**Scope:** Readiness assessment of `kaptaind` against three release lenses — **enterprise-grade**, **open-source**, and **home-user** — and a sequenced roadmap to an **upcoming stable build** (treated here as the first semver-stable, community-consumable release).
**Method:** Inspection of `README.md`, `SECURITY.md`, `CHANGELOG.md`, `Cargo.toml`, `docs/**`, `.github/**`, `Dockerfile`, `docker-compose.yml`, `web/**`, `prisma/schema.prisma`, plus live verification (`cargo test`, `cargo clippy --all-targets -- -D warnings`).

---

## 0. Snapshot (verified)

| Dimension | Observed state |
|---|---|
| Core build | `cargo test` ✅ (incl. `claims_validation`, `cli_integration`, `check_csharp_adapter`); `cargo clippy --all-targets -- -D warnings` ✅ clean. |
| Versioning | `VERSION` = `9.7.16`; `Cargo.toml` = `9.7.16`; **zero git tags**; `CHANGELOG.md` stops at `v0.1.44` + `Unreleased`. README examples cite `9.2.587`. → **Severe version/changelog drift.** |
| License | MIT (`LICENSE`), copyright `elci-group`. Remote `github.com/elci-group/kaptaind`. |
| Platforms (CI) | Release workflow builds **only** `ubuntu-latest` x86_64. No macOS/Windows binaries, no arm64. |
| Web/SaaS | Prisma now `postgresql` (`init_postgres` migration present); billing/api-auth/middleware/rate-limit/debug routes exist (launch-blocker plan largely executed). |
| Web deploy | `release.yml` still performs the **static-export hack** (moves `app/api` + `app/dashboard` out to build); `docker-compose.yml` serves `web` via **nginx static files**. API routes cannot run under either path. |
| Community files | `CONTRIBUTING.md`, `SECURITY.md`, PR template, Dependabot present. **Missing:** `CODE_OF_CONDUCT.md`, `CODEOWNERS`, issue templates. |

**Bottom line:** The Rust core is genuinely strong (tests + clippy green, deep feature set). The blockers to a *credible stable* are not core correctness — they are **release engineering, documentation/version integrity, the web deployment contradiction, and OSS/home-user packaging**. The existing `ENTERPRISE_STRATEGY.md` self-grades "S"; that grade reflects feature breadth, not release readiness. Re-grade below is release-oriented and deliberately stricter.

> **Execution status — 2026-07-09 (v9.7.16 cut).** Phases 0–4 below have been **substantially executed in-tree**; what remains runs in CI on the tag (matrix build, SBOM, keyless signing) or is tracked as a fast-follow. Honest split:
>
> **Done in-tree (verified this cut):** version/changelog/tag integrity reconciled (`VERSION` = `Cargo.toml` = `9.7.16`, CHANGELOG carries the version-line note + a `9.7.16` entry; first annotated tag created with this release — unsigned, no local GPG); `SECURITY.md`/`README.md`/`AGENTS.md` updated to implemented behavior; container `HEALTHCHECK` fixed and verified `healthy`; OSS community files added (`CODE_OF_CONDUCT.md`, `.github/CODEOWNERS`, `.github/ISSUE_TEMPLATE/`); `cargo-deny` (`deny.toml`) + `rust-toolchain.toml` pinned, with `cargo audit` + `cargo deny check` both green (git2 0.19 + optional-feature advisories allow-listed by ID and documented); validation tooling shipped (`tests/benches/` divan micro-benches, `kaptaind-cli doctor/stress/report/logs/audit/probe`) with a generated qualification report (`docs/releases/qualification/9.7.16.md`).
>
> **Deferred to CI on the tag:** multi-arch matrix (`linux/macOS/windows`, x86_64+arm64), `SHA256SUMS`, SPDX SBOM, and Sigstore keyless signatures/SLSA — wired in `.github/workflows/release.yml` but only executed there; the in-session build is host-only `x86_64-unknown-linux-gnu`.
>
> **Open / fast-follow (not release-blocking for v9.7.16, tracked):** the Next.js `standalone` migration + removal of the `release.yml` static-export hack (P0-2); Homebrew/`.deb`/`winget` publishing (P1-11, depends on CI artifacts); `kaptaind-cli rollback` (P1-12); 24 h containerised soak and the full distro matrix (CI/scheduled harness). These are named explicitly in the qualification report's PASS-WITH-NOTES sections rather than claimed as done.

---

## 1. Assessment against release criteria

Grades use the repo's own rubric (`S` best-in-class → `C/D` unsafe for unsupervised use), applied to **what a user installing a stable release actually experiences**, not to feature existence on a branch.

### 1.1 Enterprise-grade lens

| Criterion | Grade | Evidence / gap |
|---|---|---|
| Correctness & test coverage | **A** | Core tests + clippy green; good claim-coverage tests. Web/TS paths and the ship/release end-to-end path are not under equivalent automated test. |
| Reproducible, signed releases | **C** | `release.yml` uploads unsigned tarballs; no checksums file, no SBOM, no cosign/Sigstore on the GitHub release. In-app SLSA/SBOM exist behind `[ship]` config but are **not wired into CI publishing**. |
| Multi-platform delivery | **D** | Single-target Linux x86_64 only. README claims cross-platform (inotify/FSEvents/Windows) but ships no artifacts for macOS/Windows. |
| Supply-chain posture | **B+** | `cargo audit` + `npm audit` weekly; Dependabot. No `cargo-deny` license/advisory bans; no pinned-toolchain (`rust-toolchain.toml`); no lockfile-freeze policy. |
| Security doc accuracy | **C** | `SECURITY.md` "Known Limitations" still states **"No commit signing"** and **"No branch protection"**, but GPG-signed commits (`src/commit/orchestrator.rs`, `git commit -S`) and `[push.protection]` are implemented. Doc actively misleads operators. |
| Observability | **B+** | Daemon exposes `/metrics/prometheus`, SSE, health. Next.js app has no equivalent metrics/tracing. |
| Data integrity (web) | **B** | `AuditLog.projectId` is `onDelete: SetNull` (good — prior cascade risk fixed). No partitioning/retention automation for `AuditLog`/`MeteredUsageEvent` yet. |
| Secret/config handling | **C+** | `.env` supported; good secret-exclusion defaults. `docker-compose.yml` ships **default weak DB password** (`kaptaind/kaptaind`) and binds `5432` to the host — fine for dev, must not be the released example. |

### 1.2 Open-source lens

| Criterion | Grade | Evidence / gap |
|---|---|---|
| License clarity | **A** | MIT, present and consistent. |
| Community scaffolding | **C** | No Code of Conduct, no `CODEOWNERS`, no issue templates. These are table stakes for an OSS stable and for GitHub's "community standards" checklist. |
| Changelog integrity | **D** | CHANGELOG documents ≤ `v0.1.44` while shipped `VERSION` is `9.7.16`. A stable release cannot ship with a changelog that omits ~9 major versions of history. |
| Version discipline | **D** | No tags, ever; semver at `9.x` for a self-described pre-stable product signals runaway auto-versioning on the project's own repo. Undermines trust in the core feature (auto-semver). |
| Doc/reference integrity | **C** | README links `./tutorial_inference_routing.md`, `./tutorial_commit_validation.md`, `scripts/release.sh`, and `man/` pages — several referenced artifacts are absent or stale relative to current behavior. |
| Build-from-source | **B** | `cargo build` works; `install.sh` present. GUI installer gated behind heavy `fltk` feature and not shipped. |

### 1.3 Home-user lens

| Criterion | Grade | Evidence / gap |
|---|---|---|
| One-command install | **B+** | `install.sh` one-liner exists and is reasonable. |
| Sane, safe defaults | **B** | Push off by default, test hook required, secret excludes — good. But inference enabled-by-default language and "auto" provider can surprise an offline home user; needs clearer offline-first default messaging. |
| Installable artifacts | **D** | No Homebrew formula, no `.deb`/`.rpm`, no `winget`/scoop, no signed macOS/Windows builds, no desktop-app (Tauri) release pipeline despite `apps/desktop` existing. |
| Desktop experience | **C** | Embedded `--web` WebUI + Tauri shell exist in tree, but neither is packaged for end users. |
| First-run success | **C** | `Dockerfile` `HEALTHCHECK` runs `kaptaind-cli status`, which fails on a fresh container (no running daemon/status file) → container reports perpetually unhealthy. `docker-compose` `web` cannot serve API routes. A new user following the compose file hits a broken stack. |
| Recovery/undo UX | **B** | Documented `git revert` path; no first-class `kaptaind undo`/`rollback` command. |

**Overall release-readiness grade: C+.** Strong engine, immature release surface. The path to stable is short in *code* and long in *packaging, integrity, and deployment correctness*.

---

## 2. Gap register (prioritized)

**P0 — must fix before tagging stable (release-blocking).**

1. **Version/changelog/tag integrity.** Reconcile `VERSION`, `Cargo.toml`, `CHANGELOG.md`, and git history. Decide the stable version line (e.g., reset messaging to a `1.0.0` stable, or document the jump to `9.x` deliberately). Create the first annotated tag via CI, not the daemon dogfooding itself.
2. **Web deployment contradiction.** Choose Next.js `standalone` (Node server) for anything that includes `/api/*` and `/dashboard/*`; remove the static-export `mv app/api` hack from `release.yml`; fix `docker-compose.yml` `web` to run the Node server (or explicitly scope compose to the static marketing site only).
3. **SECURITY.md correctness.** Update "Known Limitations" to reflect implemented GPG signing, branch protection, SBOM/provenance, RBAC. Operators reading stale limits will misconfigure.
4. **Multi-arch release binaries.** Build and publish at minimum `linux x86_64`, `linux arm64`, `macOS x86_64+arm64`, `Windows x86_64`. Attach SHA256 checksums. (Single-arch is not a stable.)
5. **Signed/attested artifacts in CI.** Move SBOM + checksums (+ cosign/Sigstore keyless, or GPG) into the GitHub release workflow so published artifacts carry the guarantees the code already knows how to produce.
6. **Container first-run correctness.** Fix `HEALTHCHECK` (hit the daemon `/health` endpoint, not `kaptaind-cli status`), remove default DB credentials from the shipped compose example, ensure `.kaptaind` volume is writable by the non-root UID.

**P1 — required for a credible, supportable stable.**

7. OSS community files: `CODE_OF_CONDUCT.md`, `.github/CODEOWNERS`, issue templates (bug/feature), and a `SECURITY.md` reporting channel that resolves to a real inbox.
8. Dangling/stale docs: repair or remove `tutorial_*.md`, `scripts/release.sh`, and `man/` references; regenerate `LANGUAGE_MATRIX.md` cross-checks; ensure every README link resolves.
9. `cargo-deny` (licenses/advisories/bans) + `rust-toolchain.toml` pin + MSRV policy documented.
10. Web security headers: add HSTS + a real CSP (Stripe allowlist) and `Permissions-Policy` in `next.config.ts`; reconcile with any nginx headers to avoid duplication.
11. Distribution packages for home users: Homebrew formula and at least one Linux package (`.deb`) and one Windows path (`winget` or signed zip). Wire to the multi-arch artifacts from P0-4.
12. First-class rollback: `kaptaind-cli rollback` / `undo` wrapping `git revert` of the last kaptaind commit with artifact lookup.
13. `NEXTAUTH_SECRET` / `STRIPE_WEBHOOK_SECRET` / `DATABASE_URL` fail-fast validation at web boot; gate `prisma/seed.ts` test user behind `NODE_ENV === "development"` with a random password.

**P2 — polish / fast-follow after stable tag.**

14. Sigstore keyless SLSA (upgrade from GPG), OTel spans for audit, deterministic diff benchmarks, web-app metrics endpoint.
15. Desktop (Tauri) signed release pipeline for `apps/desktop`.
16. Usage/webhook hardening items from the launch critique that remain open (Stripe `subscription.updated` handling, usage-record reporting to Stripe, queue-backed webhook processing, audit-log partitioning/retention).
17. Editor/onboarding UX: clearer offline-first defaults for inference, `init` onboarding tour, richer `dashboard`/`status` recovery hints.

---

## 3. Roadmap to stable

Sequenced to respect real dependencies (release mechanics before distribution; docs/version integrity before public announcement). Effort estimates assume one focused engineer; parallelizable across the daemon/web/docs tracks.

### Phase 0 — Integrity reset (≈2–3 days) — *gate everything else*
- Decide stable version line and **freeze auto-bump on the kaptaind repo itself** during release (run dogfood in dry-run so it stops rewriting `VERSION`).
- Rebuild `CHANGELOG.md` for the gap (`v0.1.44 → 9.7.16`) using `git log` + `.kaptaind/analysis/` history; adopt a "Keep a Changelog" + automated-changelog-on-tag rule going forward.
- Cut the **first annotated/signed tag** via CI; make tag-on-`VERSION`-push the single source of release truth.
- Update `SECURITY.md`, `README.md`, `AGENTS.md` to match implemented behavior (GPG, branch protection, SBOM/provenance, RBAC). Remove/repair dangling links.
- **Exit criteria:** `VERSION`/`Cargo.toml`/CHANGELOG/tag agree; `SECURITY.md` has no false limitations; `README` links resolve.

### Phase 1 — Releasable artifacts (≈3–5 days)
- Matrix build in `release.yml`: `linux x86_64/arm64`, `macOS x86_64/arm64`, `windows x86_64` (use `cross` where needed).
- Emit `SHA256SUMS` (+ `.asc`), SPDX SBOM, and (keyless) cosign signatures/SLSA provenance; attach all to the GitHub release.
- Container: fix `HEALTHCHECK` → `/health`; scrub compose defaults; non-root volume ownership verified in CI.
- **Exit criteria:** A `workflow_dispatch` release produces signed, multi-arch artifacts + SBOM that verify; published image reports healthy.

### Phase 2 — Web/deployment correctness (≈4–6 days)
- Switch web to Next.js `standalone`; delete the static-export hack; rewrite compose `web` as a Node service behind nginx (reverse proxy) or document compose as marketing-site-only.
- Add HSTS + real CSP (Stripe `script/frame/connect` allowlist) + `Permissions-Policy`; align nginx vs. Next headers.
- Fail-fast env validation at boot; safe seeding; remaining launch-critique items (`subscription.updated`, usage→Stripe reporting, idempotency by `stripeEventId`, timestamp tolerance).
- **Exit criteria:** `docker compose up` yields a working daemon + web with API routes reachable; no default creds in shipped example; headers pass a scanner.

### Phase 3 — OSS & home-user readiness (≈3–4 days)
- Add `CODE_OF_CONDUCT.md`, `.github/CODEOWNERS`, issue templates; verify `SECURITY.md` contact works.
- `cargo-deny` config + `rust-toolchain.toml` + documented MSRV; gate CI on deny.
- Homebrew tap + `.deb` (and winget/signed-zip) publishing off the Phase-1 artifacts.
- `kaptaind-cli rollback`; offline-first inference default messaging; `init` onboarding improvements.
- **Exit criteria:** GitHub "community standards" checklist green; a home user can install via a package manager on their OS and complete first run offline.

### Phase 4 — Stable cut & hardening (≈2–3 days)
- Full `cargo fmt && clippy -D warnings && test` on all targets; web `lint`/`test:e2e`; security-audit clean.
- Release-candidate tag (`vX.0.0-rc.1`), soak on a few real repos (incl. this one, *without* dogfood self-versioning), then GA tag.
- Publish release notes generated from the reconciled changelog; announcement post; lock branch protection on `main` for release tags.
- **Exit criteria (Definition of Done for stable):** see §4.

**Estimated total: ~3–4 focused weeks** (parallelizable). This is dominated by release engineering and docs, not new features — consistent with a codebase whose engine is already mature.

---

## 4. Stable release — Definition of Done (checklist)

A build is "stable" only when **all** are true:

- [ ] `VERSION`, `Cargo.toml`, `CHANGELOG.md`, and the git tag all agree; tag is annotated and signed, created by CI.
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` green on every release target.
- [ ] `cargo audit` + `cargo deny` + `npm audit` clean (no high/critical, no banned licenses).
- [ ] GitHub release contains **linux/macOS/Windows** binaries (x86_64 + arm64 where applicable), `SHA256SUMS(+sig)`, SPDX SBOM, and a verifiable signature/provenance.
- [ ] At least one package-manager path per major OS installs and runs offline (Homebrew; `.deb` or repo; `winget` or signed zip).
- [ ] `docker compose up` from the shipped file starts a healthy daemon and a working web (API routes reachable); image `HEALTHCHECK` passes; no default/weak credentials in the example.
- [ ] `SECURITY.md` accurately describes implemented guarantees (GPG commits, branch protection, SBOM, provenance, RBAC); reporting contact is live.
- [ ] OSS community checklist green (CoC, CODEOWNERS, issue/PR templates, contributing, license).
- [ ] No dangling README/man links; `LANGUAGE_MATRIX.md` and tutorials current.
- [ ] `kaptaind-cli rollback` works against a real dogfood commit; first-run offline path documented and tested.
- [ ] Release notes generated from changelog and attached to the tag.

---

## 5. Risks & notes

- **Self-dogfooding hazard:** the project's own auto-versioner inflated `VERSION` to `9.7.16` with no tags. Keep the daemon in dry-run on this repo through the release window, or the version line will keep drifting under the release.
- **Static-export muscle memory:** the release workflow's `mv app/api` hack will silently keep breaking the web product every release until Phase 2 lands — treat it as a latent outage, not a quirk.
- **Feature breadth vs. release surface:** most P0/P1 work is *plumbing and integrity*, not new capability. Resist adding features during Phases 0–2; freeze to a stable cut first, then resume the incremental items in `ENTERPRISE_STRATEGY.md` §"Remaining Incremental Work".
- **Cross-platform claims:** README advertises macOS/Windows watcher support; until binaries ship and are CI-tested on those OSes, qualify those claims as "supported, community-verified" rather than "shipped".
