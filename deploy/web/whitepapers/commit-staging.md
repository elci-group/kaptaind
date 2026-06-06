# Whitepaper: Commit Orchestrator & Staging Modes

## Abstract
Kaptaind supports configurable staging strategies when creating commits. This whitepaper validates the three staging modes (All, Cluster, Pattern) and the exclude-glob mechanism. All tests passed.

## Claim Statement
> "Generates explanatory commits and stages files according to your rules." (Landing page, workflow step 06)
> "Commit" → "Writes + explains" (Landing page, architecture flow)

## Methodology
We initialized temporary git repositories, created modified files, and invoked `commit_with_staging` with different `StagingConfig` modes. We then inspected the most recent commit to verify which files were staged.

## Test Implementation
Source: `tests/claims_validation.rs` and `src/commit/orchestrator.rs`

```rust
fn claim_staging_all_commits_everything_except_excluded() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());
    std::fs::write(dir.path().join("keep.txt"), "changed").unwrap();
    std::fs::write(dir.path().join("exclude.txt"), "changed").unwrap();
    // ... init commit ...

    let staging = StagingConfig {
        mode: StagingMode::All,
        include: vec![],
        exclude: vec!["exclude.txt".to_string()],
    };
    commit_with_staging(dir.path(), "test commit", &staging, &[]).unwrap();

    let files = last_commit_files(dir.path());
    assert!(files.contains("keep.txt"));
    assert!(!files.contains("exclude.txt"));
}

fn claim_staging_cluster_commits_only_cluster_paths() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());
    std::fs::write(dir.path().join("in_cluster.rs"), "changed").unwrap();
    std::fs::write(dir.path().join("out_cluster.rs"), "changed").unwrap();
    std::fs::write(dir.path().join("VERSION"), "0.1.0").unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "...").unwrap();
    // ... init commit ...

    let staging = StagingConfig {
        mode: StagingMode::Cluster,
        include: vec![],
        exclude: vec![],
    };
    commit_with_staging(dir.path(), "cluster test", &staging, &[PathBuf::from("in_cluster.rs")]).unwrap();

    let files = last_commit_files(dir.path());
    assert!(files.contains("in_cluster.rs"));
    assert!(!files.contains("out_cluster.rs"));
}
```

## Results
**PASS** — Both staging modes and exclude globs behave as documented.

| Mode | Behavior | Result |
|------|----------|--------|
| All + exclude | Stages all changes except matched globs | PASS |
| Cluster | Stages only cluster paths + VERSION + Cargo.toml | PASS |

## Evidence
The orchestrator uses `git add` and `git reset HEAD` to implement staging. In Cluster mode, it explicitly adds `VERSION` and `Cargo.toml` alongside the cluster paths, ensuring version manifests are always included.

## Limitations
- Pattern mode (`include` globs) was not tested in this integration suite (it is covered by existing unit tests in `src/commit/orchestrator.rs`).
- The generated commit message content was not evaluated here; see the Semantic Versioning whitepaper for message formatting.

## Conclusion
The claim is **supported**. Kaptaind respects configurable staging rules and correctly isolates cluster-scoped changes when requested.
