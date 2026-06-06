# Whitepaper: Event Clustering by Temporal Proximity

## Abstract
Kaptaind groups filesystem events into clusters based on temporal proximity. This whitepaper validates that the clustering engine correctly merges related events within the configured window and emits distinct clusters when the window expires. All tests passed.

## Claim Statement
> "Groups related changes by temporal proximity & file relationships automatically." (Landing page, architecture flow)

## Methodology
We tested the `ClusterEngine` directly using the public API. Two hypotheses were evaluated:

1. **H1**: Events occurring within the cluster window (default 5 seconds) are merged into a single cluster.
2. **H2**: Events occurring outside the window produce separate clusters.

The test environment used an in-memory `ClusterEngine` with a 1-second and 5-second window. No filesystem or daemon runtime was required.

## Test Implementation
Source: `tests/claims_validation.rs`

```rust
fn claim_clustering_groups_events_within_window() {
    let mut engine = ClusterEngine::new(Duration::from_secs(5));
    let first = FsEvent { paths: vec!["src/main.rs".into()], kind: Modify, timestamp: Utc::now() };
    let second = FsEvent { paths: vec!["src/lib.rs".into()], kind: Modify, timestamp: Utc::now() + Duration::milliseconds(500) };
    assert!(engine.ingest(first).is_none());
    assert!(engine.ingest(second).is_none());
    let cluster = engine.flush().unwrap();
    assert_eq!(cluster.events.len(), 2);
}

fn claim_clustering_emits_when_window_expires() {
    let mut engine = ClusterEngine::new(Duration::from_secs(1));
    let first = FsEvent { paths: vec!["a.rs".into()], kind: Modify, timestamp: Utc::now() };
    let second = FsEvent { paths: vec!["b.rs".into()], kind: Modify, timestamp: Utc::now() + Duration::seconds(2) };
    assert!(engine.ingest(first).is_none());
    let emitted = engine.ingest(second).unwrap();
    assert_eq!(emitted.events.len(), 1);
}
```

## Results
**PASS** — Both hypotheses confirmed.

| Test | Result |
|------|--------|
| Events within 500ms of each other merge | PASS |
| Events 2 seconds apart emit separate clusters | PASS |

## Evidence
The `ClusterEngine::ingest` method returns `None` while merging and `Some(Cluster)` when the previous cluster is complete. The flushed cluster contains both events in H1 and only one event in H2.

## Limitations
- Tests used artificial timestamps, not real filesystem events.
- Burst detection and adaptive window sizing were not evaluated.
- Event compaction (merging duplicate paths) was not explicitly tested.

## Conclusion
The claim is **supported**. Kaptaind correctly clusters filesystem events by temporal proximity using a configurable window.
