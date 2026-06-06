# Whitepaper: Test Hook Compliance

## Abstract
Kaptaind can execute test commands before committing and optionally block the release pipeline on failure. This whitepaper validates that required hooks block commits and optional hooks do not. All tests passed.

## Claim Statement
> "Runs your test hooks (e.g. cargo test, npm test) and records compliance." (Landing page, workflow step 04)

## Methodology
We invoked `run_test_hook_for_config` with a failing shell command (`exit 1`) in both required and optional configurations. We asserted the returned `TestOutcome` and verified the blocking semantics through `should_block_commit`.

## Test Implementation
Source: `tests/claims_validation.rs` and `src/daemon/scheduler.rs`

```rust
#[tokio::test]
async fn claim_test_hook_blocks_commit_when_required_and_failing() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());
    let config = TestConfig { command: Some("exit 1".to_string()), required: true };
    let outcome = run_test_hook_for_config(&config, dir.path()).await;
    assert!(matches!(outcome, TestOutcome::Failed { .. }));
    assert!(should_block_commit(&config, &outcome), "required failing hook should block");
}

#[tokio::test]
async fn claim_test_hook_does_not_block_when_optional_and_failing() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());
    let config = TestConfig { command: Some("exit 1".to_string()), required: false };
    let outcome = run_test_hook_for_config(&config, dir.path()).await;
    assert!(matches!(outcome, TestOutcome::Failed { .. }));
    assert!(!should_block_commit(&config, &outcome), "optional failing hook should not block");
}
```

## Results
**PASS** — Blocking semantics match configuration.

| Configuration | Hook Result | Blocks Commit? | Result |
|---------------|-------------|----------------|--------|
| Required | Fails | Yes | PASS |
| Optional | Fails | No | PASS |

## Evidence
The scheduler calls `should_block_commit` after `run_test_hook`. Required failures abort `process_cluster` before versioning or committing. Optional failures log a warning but allow the pipeline to continue.

## Limitations
- Only shell exit codes were tested; real test suites (cargo test, npm test) were not exercised.
- Timeout behavior of test hooks was not evaluated.
- The runtime weight adjustment (passing hooks reduce runtime weight to 0.1) was not directly measured.

## Conclusion
The claim is **supported**. Kaptaind correctly enforces test hook compliance according to the `required` flag.
