# Padagonia Supervisor Qualification Benchmark

Status: **PASS**  
Date: 2026-08-28  
Directive: [`../ENTERPRISE_PADAGONIA_SUPERVISOR_DIRECTIVE.md`](../ENTERPRISE_PADAGONIA_SUPERVISOR_DIRECTIVE.md)

## Environment

- CPU: Intel Core 5 120U, 12 logical CPUs, 10 cores
- OS: Linux 6.18.7-76061807-generic, x86_64
- Rust: `rustc 1.98.0 (88d9e12ae 2026-08-18)`
- Cargo: `cargo 1.98.0 (797e8a9bc 2026-08-05)`
- Profile: optimized `bench`
- Samples: 20 per case
- Command: `cargo bench --bench bench_supervisor -- --sample-count 20`

Fixture construction is excluded from timed validation, serialization, load,
and persistence measurements. Planning performs no filesystem access, network
access, or process creation. Atomic persistence includes serialization, file
write, file `fsync`, rename, and parent-directory `fsync`.

## Results

Times are Divan median values; the maximum observed sample is included as a
conservative tail indicator because Divan's standard table does not emit p95.

| Case | Projects | Median | Maximum observed |
|---|---:|---:|---:|
| Validate desired-state snapshot | 1,000 | 901.8 us | 934 us |
| Validate desired-state snapshot | 10,000 | 12.95 ms | 21.09 ms |
| Serialize desired-state snapshot | 1,000 | 216.5 us | 247.2 us |
| Serialize desired-state snapshot | 10,000 | 2.688 ms | 3.549 ms |
| Load and validate snapshot | 1,000 | 2.512 ms | 5.292 ms |
| Load and validate snapshot | 10,000 | 17.57 ms | 31.83 ms |
| Atomic persistence | 1,000 | 2.751 ms | 4.416 ms |
| No-op reconciliation plan | 1,000 | 805.4 us | 951.6 us |
| No-op reconciliation plan | 10,000 | 10.4 ms | 12.1 ms |
| Mixed reconciliation plan | 1,000 | 812.9 us | 850.2 us |
| Mixed reconciliation plan | 10,000 | 9.818 ms | 10.52 ms |

The suite also measured 10- and 100-project cases for validation,
serialization, loading, and both planning modes. Full raw results remain
reproducible from the command above.

## Acceptance decision

| Directive budget | Worst relevant observation | Result |
|---|---:|---:|
| 1,000-project plan under 100 ms | 951.6 us | PASS |
| 10,000-project plan under 1 s | 12.1 ms | PASS |
| No network/process creation in planning | Pure in-memory benchmark | PASS |
| Bounded task creation | Planner creates no async tasks | PASS |

Both planning modes retain substantial headroom: the slowest observed
10,000-project plan used 1.21% of the one-second budget.
