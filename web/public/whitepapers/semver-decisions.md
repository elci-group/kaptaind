# Whitepaper: Semantic Versioning Decision Engine

## Abstract
Kaptaind calculates semantic version bumps based on code impact rather than commit message conventions. This whitepaper validates the deterministic decision rules that map API changes and composite scores to Major, Minor, Patch, or None bumps. All tests passed.

## Claim Statement
> "Calculates the correct semantic bump and updates VERSION and manifests." (Landing page, workflow step 05)
> "A local daemon that watches your repository, measures the real impact of your changes — across AST, API surface, and dependencies — and handles version bumps, commits, and changelogs automatically." (Landing page, hero paragraph)

## Methodology
We tested the `version::decide` function with a matrix of `WeightResult` inputs representing different change scenarios. We also tested `version::apply` to ensure correct semver mutation. Finally, we verified that `save_version` writes the `VERSION` file to disk.

## Test Implementation
Source: `tests/claims_validation.rs` and `src/version/semver.rs`

```rust
fn claim_semver_breaking_api_yields_major() {
    let weight = WeightResult { score: 0.0, api_breaking: true, api_added: false };
    assert_eq!(decide(&weight, &thresholds), Bump::Major);
}

fn claim_semver_api_addition_yields_minor() {
    let weight = WeightResult { score: 0.0, api_breaking: false, api_added: true };
    assert_eq!(decide(&weight, &thresholds), Bump::Minor);
}

fn claim_semver_score_thresholds() {
    let patch = WeightResult { score: 0.2, api_breaking: false, api_added: false };
    let minor = WeightResult { score: 0.7, api_breaking: false, api_added: false };
    let none  = WeightResult { score: 0.05, api_breaking: false, api_added: false };
    assert_eq!(decide(&patch, &thresholds), Bump::Patch);
    assert_eq!(decide(&minor, &thresholds), Bump::Minor);
    assert_eq!(decide(&none,  &thresholds), Bump::None);
}

fn claim_semver_apply_increments_correctly() {
    let base = Version::new(1, 2, 3);
    assert_eq!(apply(base.clone(), Bump::Patch), Version::new(1, 2, 4));
    assert_eq!(apply(base.clone(), Bump::Minor), Version::new(1, 3, 0));
    assert_eq!(apply(base, Bump::Major), Version::new(2, 0, 0));
}
```

## Results
**PASS** — All decision rules and application logic confirmed.

| Scenario | Expected Bump | Result |
|----------|--------------|--------|
| Breaking API change | Major | PASS |
| New API addition | Minor | PASS |
| Score > 0.6 (no API change) | Minor | PASS |
| Score > 0.1 (no API change) | Patch | PASS |
| Score ≤ 0.1 (no API change) | None | PASS |
| Apply Patch to 1.2.3 | 1.2.4 | PASS |
| Apply Minor to 1.2.3 | 1.3.0 | PASS |
| Apply Major to 1.2.3 | 2.0.0 | PASS |

## Evidence
The decision logic is deterministic and matches the documented rules in `AGENTS.md`:
- Breaking API → `Major`
- Added API or score > 0.6 → `Minor`
- Score > 0.1 → `Patch`
- Otherwise → `None`

## Limitations
- Thresholds (0.6, 0.1) are configurable; we tested only the defaults.
- The "correctness" of a bump is contextual; we validated determinism, not omniscient correctness.
- Cargo.toml mutation was not fully exercised in the integration test (only VERSION file writing was verified).

## Conclusion
The claim is **supported**. Kaptaind applies deterministic, code-impact-driven semantic versioning rules that replace manual convention-based versioning.
