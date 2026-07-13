# v10 Dogfood Report

Window: **2026-07-12 → 2026-07-26** (2 weeks, per the safety plan's rollout gate).
Fleet: scotia (health :9091), fract (:9090), ontism (:9092) — all on v10.0.1.
Nightly chaos soak: `.github/workflows/soak.yml` (30 min, cron `17 3 * * *`).

Exit criteria (from AUTONOMOUS_COMMIT_SAFETY_PLAN.md §6 exit D): clean dogfood
reports — no unexpected cascades, version N-tuple consistent after every
auto-commit, no daemon ERRORs beyond known/expected ones, soak green nightly.

## Baseline (2026-07-12)

- All three daemons on 10.0.1 with dedicated health ports; a v10.0.0→10.0.1
  upgrade fixed the hook-artifact phantom cascade before it could bite the
  fleet (ontism's config has an active cargo test hook).
- v10 behavior live: `require_bump = false` default → below-threshold work is
  captured as `chore:` commits; conventional-commit subjects; hot config reload.
- Known expected noise: config edits (kaptaind.toml) cluster once per restart
  and chore-commit under the new default — by design.

## Daily log

| Date | Commits (bump/chore) | Errors | Soak CI | Notes |
|------|---------------------|--------|---------|-------|
| 2026-07-12 | baseline | none | n/a (first run tonight) | fleet upgraded to 10.0.1; ontism health restored on :9092 after silent-bind dead since 12:46 |
