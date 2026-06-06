# Kaptaind 100-Category Review Scorecard

Scored after the June 4, 2026 remediation pass. Scale: 100 is excellent, 70 is serviceable with visible gaps, 50 is risky or immature, below 50 needs structural work.

| # | Category | Score | Rationale |
|---|---|---:|---|
| 1 | Product clarity | 86 | The daemon/CLI purpose is clear: automatic semantic versioning, commit creation, and release hints. Some features feel over-broad for one product surface. |
| 2 | Core problem fit | 82 | Automated versioning and semantic diff scoring address a real workflow. The risk is that automatic commits require very strong safety rails. |
| 3 | Architecture cohesion | 78 | Modules are separated by watcher, cluster, diff, version, push, release, and dashboard concerns. Scheduler remains a large orchestration hotspot. |
| 4 | Rust module boundaries | 80 | Most subsystems have clear directories and public module exports. Some files, especially CLI and scheduler, are too large. |
| 5 | Web module boundaries | 72 | Next.js route and library separation is understandable. Authorization logic is now centralized in `resolveRepoPath`, but project APIs repeat request handling. |
| 6 | Daemon lifecycle | 79 | Startup, signal handling, watcher join, and shutdown timeout are present. PID/log handling is basic and could better handle existing daemon instances. |
| 7 | Filesystem watching | 76 | Uses `notify` and has ignore matching. It does not deeply address event storms, symlink loops, or editor-specific save behavior beyond clustering. |
| 8 | Change clustering | 84 | Temporal clustering and adaptive configuration are useful and tested. More black-box integration tests around real watcher events would improve confidence. |
| 9 | Git safety | 77 | The dangerous index-lock deletion is fixed, and staging modes exist. Remaining risk comes from automatic staging and committing user work. |
| 10 | Staging model | 73 | All, cluster, and pattern modes are flexible. Default all-staging can capture unrelated work unless users configure it carefully. |
| 11 | Commit correctness | 78 | Uses libgit2 and includes version/artifact changes. More tests should prove blocked hooks leave no generated mutations. |
| 12 | Push safety | 72 | Push now respects configured remote, dry-run, and protected branches. Retry, conflict, upstream, and batch config are still not fully implemented. |
| 13 | Config expressiveness | 85 | The TOML config covers watch, weights, tests, push, inference, release, plugins, VACS, and Angler. Breadth is high. |
| 14 | Config enforcement | 68 | Some advertised config remains partially enforced, especially advanced push behavior. This lowers trust in knobs. |
| 15 | Config defaults | 76 | Defaults are usable for local projects. `test.required = true` and push disabled are safe defaults; staging all is more aggressive. |
| 16 | Path handling | 80 | Relative paths are finalized against repo root and tested. Further hardening around symlinks and path traversal would help. |
| 17 | Error handling | 75 | Most critical paths log and set status. Some errors are intentionally swallowed, which helps liveness but can hide failures. |
| 18 | Status reporting | 78 | `.kaptaind/status.json` gives useful daemon state. Dashboard and CLI consume this, but status detail is still fairly coarse. |
| 19 | Observability | 76 | Tracing, telemetry, status files, and artifacts exist. There is no unified structured logging policy or metrics export. |
| 20 | Telemetry discipline | 70 | Cost/token tracking exists. Privacy and retention controls are not documented deeply enough for sensitive repos. |
| 21 | Test coverage breadth | 84 | Rust has 195 library tests plus 6 integration tests passing. Coverage spans diff, versioning, trawler, Angler, VACS, and CLI basics. |
| 22 | Test reliability | 76 | Tests now pass, including the fixed VACS panic. Full doctests remain blocked by the local rustdoc/libLLVM environment. |
| 23 | Integration testing | 70 | CLI integration tests exist. Daemon end-to-end tests with real git repos, hooks, push dry-runs, and watcher events are limited. |
| 24 | Web testing | 58 | Lint and production build pass. There are no visible unit, component, or API authorization tests. |
| 25 | CI readiness | 65 | Commands are straightforward, but rustdoc environment failure and web build workspace-root warning would need CI cleanup. |
| 26 | Dependency hygiene | 66 | Lockfiles exist. `npm ci` reports 6 vulnerabilities, including 1 high, requiring audit/upgrade work. |
| 27 | Rust dependency choices | 80 | Uses standard crates: tokio, notify, git2, serde, clap, reqwest, rusqlite. The set is reasonable for the domain. |
| 28 | Frontend dependency choices | 72 | Next, React, Prisma, NextAuth, and Stripe are conventional. Next 16/Turbopack introduces some build sensitivity. |
| 29 | Build reproducibility | 72 | Rust and web lockfiles help. Removed remote font dependency improves web build reproducibility. |
| 30 | Documentation depth | 83 | README and tutorials are extensive. Some claims exceed implemented safeguards, especially push safety and advanced automation. |
| 31 | Install experience | 76 | Installer scripts and manual instructions exist. The GUI installer is optional, but dependency and platform expectations could be tighter. |
| 32 | CLI usability | 78 | CLI has many commands and dashboards. Some views still show mock/static metrics, reducing operational credibility. |
| 33 | CLI maintainability | 58 | `src/cli/main.rs` is around 1,951 lines, making review and change isolation harder. |
| 34 | Daemon maintainability | 62 | `src/daemon/scheduler.rs` is around 1,247 lines and handles many responsibilities. It needs decomposition. |
| 35 | Diff analysis breadth | 87 | Language and framework detection is broad, with many tests. This is one of the strongest subsystems. |
| 36 | Diff analysis precision | 76 | AST parsing helps, but many languages necessarily use heuristics. Confidence metadata is a good mitigation. |
| 37 | API detection | 81 | Public Rust APIs, TS exports, routes, design tokens, and framework surfaces are covered. Complex breaking changes remain hard. |
| 38 | Dependency detection | 79 | Cargo, package, requirements, lockfiles, and platform configs are recognized. Transitive semantic impact is still heuristic. |
| 39 | Runtime impact scoring | 77 | Deployment and web/mobile config touches are scored. Runtime impact is broad and may over-score benign config edits. |
| 40 | Bundle scoring | 72 | Optional build-size scoring exists. It depends on user commands and may be expensive or flaky without strong timeouts. |
| 41 | Semantic versioning logic | 82 | Major/minor/patch decisions are tested and configurable. Real-world API compatibility still needs human override paths. |
| 42 | Version file handling | 80 | VERSION and Cargo.toml updates are implemented. Mutation now happens after blockers, reducing dirty-worktree failures. |
| 43 | Release pipeline | 71 | Build, package, stability, and distribution structures exist. Production release hardening is still early. |
| 44 | Packaging | 78 | Tarball and checksum manifest are straightforward and useful. Artifact path handling could be made repo-root relative explicitly. |
| 45 | Distribution | 63 | Local, S3, registry abstractions exist. External distribution needs more end-to-end validation. |
| 46 | Stability scoring | 78 | Stability models account for tests, churn, runtime, and parser confidence. Calibration against real repos is unknown. |
| 47 | Qualification policy | 78 | Release gates for streak, stability, cooldown, and diff spikes are sensible. More operator controls would help. |
| 48 | Inference routing | 78 | Multiple providers and fallback behavior are implemented. Prompt/data privacy controls need stronger docs and defaults. |
| 49 | AI commit quality controls | 73 | Deterministic fallback and validation modes exist. There is limited automated evaluation of generated message quality. |
| 50 | Local AI fallback | 80 | Ollama fallback reduces cloud dependency. Runtime availability and model performance remain external risks. |
| 51 | Cloud AI safety | 64 | Provider routing is convenient, but sending repository-derived context can leak sensitive information without explicit policy gates. |
| 52 | Web auth | 74 | NextAuth credentials/GitHub flow exists; noisy debug logs were removed. Signup is now disabled unless `ALLOW_SIGNUP=true`. |
| 53 | Web authorization | 76 | Project APIs now enforce owner or membership access through `resolveRepoPath`. Dedicated tests are still missing. |
| 54 | Multi-tenancy | 66 | Data model supports owners and memberships. Enforcement is improved, but team roles are underused. |
| 55 | Secrets handling | 67 | Secrets are environment-based. More redaction and guidance is needed around logs, telemetry, and inference prompts. |
| 56 | Web API design | 71 | API routes are simple and readable. Repeated session/project handling could become middleware or helper wrappers. |
| 57 | Web UI design | 72 | Dashboard components exist and build cleanly. The UI appears serviceable but not deeply validated by interaction tests. |
| 58 | Frontend accessibility | 66 | Uses standard React/HTML components. No explicit accessibility tests or audits were found. |
| 59 | Frontend state management | 70 | Simple providers and server routes keep state manageable. Theme provider was fixed for lint/runtime correctness. |
| 60 | Production web build | 80 | Build now succeeds after removing remote font dependency. Workspace-root warning remains. |
| 61 | Lint cleanliness | 82 | `npm run lint` passes. Rust still has many compiler warnings. |
| 62 | Rust warning hygiene | 59 | `cargo test` emits many unused/dead-code warnings, which weakens signal and should be cleaned. |
| 63 | Panic safety | 75 | The discovered VACS slicing panic is fixed. More fuzz/property tests would help string/path-heavy code. |
| 64 | Concurrency safety | 73 | Tokio and channels are used clearly. Shared mutable history via mutex is simple; scheduler task management is broad. |
| 65 | Shutdown behavior | 78 | Signals, timeout, task draining, and watcher join are handled. Running external hooks may still complicate shutdown. |
| 66 | External command execution | 67 | Hooks and shell commands are configurable and useful. They need stronger quoting, timeout, environment, and audit policies. |
| 67 | Hook system | 79 | Angler hooks, baits, webhooks, and selective capture are extensive. The surface area is large and security-sensitive. |
| 68 | Selective capture | 76 | Blocking/tagging rules exist and now run before generated version mutations. More tests should cover failed-path worktree cleanliness. |
| 69 | Webhooks | 79 | HMAC, retry, filtering, and rate-limit concepts are present. Delivery observability and secret lifecycle need depth. |
| 70 | Plugin architecture | 74 | JSON stdio adapters are simple and extensible. Plugin execution is a security boundary and needs sandbox guidance. |
| 71 | Trawler project discovery | 82 | Many project types and confidence scoring are tested. False positives/negatives in large home directories remain likely. |
| 72 | VACS concept extraction | 70 | Interesting subsystem with tests; the fixed panic shows it still needs hardening. |
| 73 | Data persistence | 74 | JSON artifacts and SQLite traces are practical. Atomicity and migrations are limited. |
| 74 | Artifact retention | 72 | Pruning exists for analysis/traces. Retention policy is simple and may not match compliance needs. |
| 75 | Database schema | 73 | Prisma schema covers users, projects, teams, subscriptions, and AI cache. Authorization semantics need more role detail. |
| 76 | Subscription integration | 65 | Stripe scaffolding exists. Production billing paths need heavier validation and tests. |
| 77 | Security posture | 72 | Major review issues were fixed. Remaining risks include command execution, AI data egress, dependency vulnerabilities, and missing web auth tests. |
| 78 | Privacy posture | 62 | Local artifacts can contain rich repo context, and inference can externalize summaries. Needs explicit privacy controls and docs. |
| 79 | Supply-chain posture | 63 | Lockfiles help, but npm audit reports vulnerabilities and no SBOM/signing policy is visible. |
| 80 | Operational safety | 73 | Push disabled by default, tests required, and lock deletion fixed. Automatic commits still need careful rollout controls. |
| 81 | Failure recovery | 70 | Status and notifications report failures. Rollback behavior for generated files and partial release tasks is limited. |
| 82 | Idempotency | 67 | Repeated daemon events are clustered/rate-limited. Some generated artifacts and commits may duplicate under edge cases. |
| 83 | Rate limiting | 73 | Commit rate limiting and webhook rate limiting exist. Inference and release task rate control is less comprehensive. |
| 84 | Performance | 74 | AST cache and async architecture help. Large repo behavior depends on watcher volume, parser cost, and command hooks. |
| 85 | Scalability | 69 | Designed for individual repos and maybe trawled sets. Multi-repo daemon scaling is not deeply proven. |
| 86 | Resource management | 70 | Task draining, pruning, and cache metrics exist. Build/test hooks can still consume unbounded project resources if configured poorly. |
| 87 | Cross-platform support | 70 | Linux/macOS/WSL install claims and notify abstraction help. Daemonization and shell assumptions are Unix-heavy. |
| 88 | Windows readiness | 55 | Rust can compile cross-platform, but daemon, shell, and path behavior appear Unix-oriented. |
| 89 | Developer ergonomics | 78 | The repo is easy to inspect, tests are runnable, docs are substantial. Large files reduce ergonomics. |
| 90 | Code style consistency | 72 | Rust style is readable; web style is conventional. Warnings and oversized modules hurt consistency. |
| 91 | API stability | 68 | Internal APIs are still evolving, and many public-looking features are young. Version is high, but maturity is mixed. |
| 92 | Backward compatibility | 67 | Artifact formats have some backward-compatible tests. Config evolution and migration strategy need more explicit handling. |
| 93 | Documentation accuracy | 70 | Docs are ambitious and useful. Some claims describe planned or partially implemented safeguards. |
| 94 | User onboarding | 78 | README, install docs, init command, and tutorials help. Risky automation needs a safer first-run wizard/checklist. |
| 95 | Maintainer onboarding | 69 | Module layout helps, but very large scheduler/CLI files and broad feature set slow new maintainers. |
| 96 | Release maturity | 66 | Versioning and release machinery exist, but CI, signing, distribution, and dependency audit gaps remain. |
| 97 | Compliance readiness | 55 | Audit trails exist through AoC/traces, but privacy, access control tests, retention, and egress policies are incomplete. |
| 98 | Innovation | 89 | Semantic daemon, AoC, VACS, Angler, and multi-provider inference are distinctive and ambitious. |
| 99 | Practicality | 73 | Core value is practical, but scope sprawl increases setup and trust burden. Best used incrementally. |
| 100 | Overall project health | 76 | After fixes, Kaptaind is a strong experimental-to-early-production project: rich features and passing tests, with remaining risks in safety, warnings, web tests, dependency audit, and oversized orchestration code. |

