# DumDum Project Context

Project: kaptaind

Languages observed:
- HTML: 1 files
- Rust: 180 files
- Text: 1 files

Directory shape:
- src: 9 files
- src/angler: 6 files
- src/aoc: 5 files
- src/cli: 5 files
- src/cli/commands: 24 files
- src/cluster: 2 files
- src/commit: 3 files
- src/config: 1 files
- src/daemon: 17 files
- src/diff: 6 files
- src/diff/lang: 4 files
- src/diff/lang/adapters: 39 files
- src/diff/version: 1 files
- src/git: 2 files
- src/inference: 7 files
- src/installer: 2 files
- src/notify: 2 files
- src/push: 2 files
- src/qualification: 3 files
- src/release: 11 files
- src/schedule: 2 files
- src/stability: 3 files
- src/trawler: 3 files
- src/util: 10 files
- src/vacs: 6 files
- src/version: 3 files
- src/watcher: 2 files
- src/weight: 2 files

Important file signals:
- src/angler/bait.rs (Rust, 23483 bytes): use crate::angler::config::{BaitConfig, BaitDefinition, BaitEvent, BaitType};
- src/angler/config.rs (Rust, 14025 bytes): use serde::{Deserialize, Serialize};
- src/angler/git_hooks.rs (Rust, 23924 bytes): use crate::angler::config::{GitHooksConfig, HookConfig};
- src/angler/mod.rs (Rust, 15740 bytes): pub mod bait;
- src/angler/selective.rs (Rust, 27644 bytes): use crate::angler::config::{CaptureAction, CaptureRule, ChangeType, SelectiveConfig};
- src/angler/webhooks.rs (Rust, 29095 bytes): use crate::angler::config::{RetryConfig, SignatureAlgorithm, WebhookEndpoint, WebhooksConfig};
- src/aoc/db.rs (Rust, 2776 bytes): use crate::aoc::tracer::TraceRecord;
- src/aoc/interceptor.rs (Rust, 2384 bytes): use crate::aoc::tracer::AgentEvent;
- src/aoc/mod.rs (Rust, 323 bytes): pub mod db;
- src/aoc/session.rs (Rust, 5882 bytes): use chrono::{DateTime, Utc};
- src/aoc/tracer.rs (Rust, 4026 bytes): use chrono::{DateTime, Utc};
- src/audit.rs (Rust, 14778 bytes): use chrono::{DateTime, Utc};
- src/cli/analyze.rs (Rust, 3768 bytes): use chrono::Utc;
- src/cli/autostart.rs (Rust, 628 bytes): use kaptaind::util::style::*;
- src/cli/commands/aoc.rs (Rust, 8265 bytes): use chrono::Utc;
- src/cli/commands/audit.rs (Rust, 8464 bytes): use chrono::{DateTime, Utc};
- src/cli/commands/autostart.rs (Rust, 81 bytes): pub fn handle_autostart() -> anyhow::Result<()> {
- src/cli/commands/cihint.rs (Rust, 4173 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/dashboard.rs (Rust, 8558 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/doctor.rs (Rust, 52670 bytes): use chrono::Utc;
- src/cli/commands/explain.rs (Rust, 539 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/init.rs (Rust, 6879 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/log.rs (Rust, 3419 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/logs.rs (Rust, 2414 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/mod.rs (Rust, 1062 bytes): pub mod aoc;
- src/cli/commands/monitor.rs (Rust, 1924 bytes): use kaptaind::util::style::*;
- src/cli/commands/probe.rs (Rust, 6484 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/report.rs (Rust, 17759 bytes): use chrono::Utc;
- src/cli/commands/rollback.rs (Rust, 3915 bytes): use anyhow::{anyhow, bail, Context};
- src/cli/commands/service.rs (Rust, 974 bytes): use crate::ServiceCommand;
- src/cli/commands/shark.rs (Rust, 9495 bytes): use anyhow::Context;
- src/cli/commands/ship.rs (Rust, 7630 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/status.rs (Rust, 3425 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/storage.rs (Rust, 2370 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/stress.rs (Rust, 14775 bytes): use chrono::Utc;
- src/cli/commands/trace.rs (Rust, 5361 bytes): use kaptaind::config::loader::Config;
- src/cli/commands/trawl.rs (Rust, 5806 bytes): use kaptaind::trawler::TrawlOptions;
- src/cli/commands/vacs.rs (Rust, 1251 bytes): use kaptaind::config::loader::Config;
- src/cli/main.rs (Rust, 67366 bytes): use chrono::{DateTime, Utc};
- src/cli/monitor.rs (Rust, 26154 bytes): use kaptaind::monitor::{load_registry, save_registry};
- src/cli/table.rs (Rust, 1766 bytes): pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
- src/cluster/engine.rs (Rust, 8837 bytes): use crate::watcher::FsEvent;
- src/cluster/mod.rs (Rust, 16 bytes): pub mod engine;
- src/commit/message.rs (Rust, 19839 bytes): use crate::cluster::engine::Cluster;
- src/commit/mod.rs (Rust, 70 bytes): pub mod message;
- src/commit/orchestrator.rs (Rust, 18077 bytes): use crate::config::loader::{CommitConfig, StagingConfig, StagingMode};
- src/compliance.rs (Rust, 1346 bytes): use crate::config::loader::{Config, EgressChannel};
- src/config/mod.rs (Rust, 308 bytes): pub mod loader;
- src/daemon/decisions.rs (Rust, 11195 bytes): use chrono::{DateTime, Utc};
- src/daemon/deckhand.rs (Rust, 10425 bytes): use crate::config::loader::DeckhandConfig;
- src/daemon/health.rs (Rust, 13576 bytes): use axum::{
- src/daemon/mod.rs (Rust, 272 bytes): pub mod decisions;
- src/daemon/notification.rs (Rust, 39497 bytes): use crate::config::loader::NotifyConfig;
- src/daemon/pidfile.rs (Rust, 4217 bytes): use std::path::Path;
- src/daemon/policy.rs (Rust, 27173 bytes): use crate::config::loader::PolicyTrustConfig;
- src/daemon/process.rs (Rust, 6732 bytes): use anyhow::{anyhow, Context};
- src/daemon/prune.rs (Rust, 4122 bytes): use chrono::Utc;
- src/daemon/runtime.rs (Rust, 10672 bytes): use crate::config::Config;
- src/daemon/shark.rs (Rust, 41265 bytes): use crate::config::loader::{Config, SharkMode};
- src/daemon/shutdown.rs (Rust, 1031 bytes): use tokio::sync::watch;
- src/daemon/status.rs (Rust, 3919 bytes): use chrono::{DateTime, Utc};
- src/daemon/telemetry.rs (Rust, 7035 bytes): use serde::{Deserialize, Serialize};
- src/daemon/trace.rs (Rust, 2171 bytes): use crate::aoc::tracer;
- src/daemon/web.rs (Rust, 30867 bytes): use axum::{
- src/daemon/web_ui.html (HTML, 29323 bytes): <!DOCTYPE html>
- src/diff/api.rs (Rust, 20785 bytes): use crate::cluster::engine::Cluster;
- src/diff/ast.rs (Rust, 26856 bytes): use crate::cluster::engine::Cluster;
- src/diff/bundle.rs (Rust, 6142 bytes): use crate::config::loader::BundleConfig;
- src/diff/cache.rs (Rust, 9380 bytes): use crate::diff::lang::adapter::AstRepresentation;
- src/diff/lang/adapter.rs (Rust, 4821 bytes): use serde::{Deserialize, Serialize};
- src/diff/lang/adapters/TEMPLATE.rs.txt (Text, 3160 bytes): use super::super::adapter::{
- src/diff/lang/adapters/astro.rs (Rust, 3189 bytes): use super::super::adapter::{
- src/diff/lang/adapters/c.rs (Rust, 8855 bytes): use super::super::adapter::{
- src/diff/lang/adapters/clojure.rs (Rust, 8369 bytes): use super::super::adapter::{
- src/diff/lang/adapters/common.rs (Rust, 12757 bytes): use super::super::adapter::{AstRepresentation, Symbol};
- src/diff/lang/adapters/cpp.rs (Rust, 9434 bytes): use super::super::adapter::{
- src/diff/lang/adapters/csharp.rs (Rust, 9738 bytes): use super::super::adapter::{
- src/diff/lang/adapters/dart.rs (Rust, 11353 bytes): use super::super::adapter::{
- src/diff/lang/adapters/elixir.rs (Rust, 4789 bytes): use super::super::adapter::{
- src/diff/lang/adapters/erlang.rs (Rust, 7474 bytes): use super::super::adapter::{
- src/diff/lang/adapters/fsharp.rs (Rust, 9109 bytes): use super::super::adapter::{
- src/diff/lang/adapters/go.rs (Rust, 3849 bytes): use super::super::adapter::{
- src/diff/lang/adapters/groovy.rs (Rust, 23884 bytes): use super::super::adapter::{
- src/diff/lang/adapters/haskell.rs (Rust, 9839 bytes): use super::super::adapter::{
- src/diff/lang/adapters/hcl.rs (Rust, 8490 bytes): use super::super::adapter::{
- src/diff/lang/adapters/htmlcss.rs (Rust, 2639 bytes): use super::super::adapter::{
- src/diff/lang/adapters/java.rs (Rust, 7373 bytes): use super::super::adapter::{
- src/diff/lang/adapters/javascript.rs (Rust, 3517 bytes): use super::super::adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter};
- src/diff/lang/adapters/julia.rs (Rust, 22812 bytes): use super::super::adapter::{
- src/diff/lang/adapters/kotlin.rs (Rust, 7208 bytes): use super::super::adapter::{
- src/diff/lang/adapters/lua.rs (Rust, 7990 bytes): use super::super::adapter::{
- src/diff/lang/adapters/mod.rs (Rust, 4312 bytes): pub mod astro;
- src/diff/lang/adapters/objc.rs (Rust, 15888 bytes): use super::super::adapter::{
- src/diff/lang/adapters/ocaml.rs (Rust, 8094 bytes): use super::super::adapter::{
- src/diff/lang/adapters/perl.rs (Rust, 7132 bytes): use super::super::adapter::{
- src/diff/lang/adapters/php.rs (Rust, 10689 bytes): use super::super::adapter::{
- src/diff/lang/adapters/python.rs (Rust, 4458 bytes): use super::super::adapter::{
- src/diff/lang/adapters/r.rs (Rust, 15888 bytes): use super::super::adapter::{
- src/diff/lang/adapters/ruby.rs (Rust, 6986 bytes): use super::super::adapter::{
- src/diff/lang/adapters/rust.rs (Rust, 15054 bytes): use super::super::adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter};
- src/diff/lang/adapters/scala.rs (Rust, 10893 bytes): use super::super::adapter::{
- src/diff/lang/adapters/scss.rs (Rust, 5749 bytes): use super::super::adapter::{
- src/diff/lang/adapters/solidity.rs (Rust, 20688 bytes): use super::super::adapter::{
- src/diff/lang/adapters/sql.rs (Rust, 10183 bytes): use super::super::adapter::{
- src/diff/lang/adapters/svelte.rs (Rust, 4790 bytes): use super::super::adapter::{
- src/diff/lang/adapters/swift.rs (Rust, 5314 bytes): use super::super::adapter::{
- src/diff/lang/adapters/typescript.rs (Rust, 2807 bytes): use super::super::adapter::{ApiSurface, AstDiff, AstRepresentation, Language, LanguageAdapter};
- src/diff/lang/adapters/vue.rs (Rust, 4321 bytes): use super::super::adapter::{
- src/diff/lang/adapters/zig.rs (Rust, 19696 bytes): use super::super::adapter::{
- src/diff/lang/mod.rs (Rust, 1592 bytes): pub mod adapter;
- src/diff/lang/plugin.rs (Rust, 5688 bytes): use crate::config::loader::PluginAdapterConfig;
- src/diff/lang/registry.rs (Rust, 1351 bytes): use super::adapter::LanguageAdapter;
- src/diff/mod.rs (Rust, 3817 bytes): pub mod api;
- src/diff/text.rs (Rust, 824 bytes): use crate::cluster::engine::Cluster;
- src/diff/version/mod.rs (Rust, 242 bytes): pub mod cache {
- src/dryrun.rs (Rust, 4185 bytes): use crate::config::Config;
- src/evidence.rs (Rust, 3940 bytes): use chrono::{DateTime, Utc};
- src/git/mod.rs (Rust, 14 bytes): pub mod repo;
- src/git/repo.rs (Rust, 8449 bytes): use anyhow::{anyhow, Context};
- src/icon.rs (Rust, 3488 bytes): use std::path::PathBuf;


Recent documented file:
## `src/angler/bait.rs`

**Documentation depth:** deep explanation, target 1400-2000 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why it matters to users or maintainers, in plain language that defines any technical term on first use.
- User-visible behavior or operational effect.
- How the important functions, settings, or document sections work together, with a one-line plain-English gloss for each important symbol.
- Failure modes, security concerns, and testing guidance, each explained so a newcomer understands the risk, not just the name of it.
- Worked example: a concrete, realistic example drawn only from this file's real content - a command invocation, a short code snippet, or a step-by-step call flow. Use only commands, symbols, and paths that actually appear in the file.
- Maintainer notes and review checklist.

**What it is:** This is a Rust file in `src`. Its first useful signal is: use crate::angler::config::{BaitConfig, BaitDefinition, BaitEvent, BaitType};.

**Why it matters:** Its first useful signal is: use crate::angler::config::{BaitConfig, BaitDefinition, BaitEvent, BaitType};. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**In plain terms:** Its first useful signal is: use crate::angler::config::{BaitConfig, BaitDefinition, BaitEvent, BaitType};.

**What users should know:** Users may not touch this file directly, but its behavior can still affect reliability, output, or workflow.

**How it works:** The current snapshot has 734 lines and 24 function-like definitions. Read the public functions first, then follow data flow into helpers before changing behavior.

**For example:** to see this file at work, start from `success` (function) in `src/angler/bait.rs` and follow what it calls or configures next.

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 734 lines, 24 detected function-like definitions, hash 13204195801883127724.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.


Recent documented file:
## `src/angler/config.rs`

**Documentation depth:** deep explanation, target 1400-2000 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why it matters to users or maintainers, in plain language that defines any technical term on first use.
- User-visible behavior or operational effect.
- How the important functions, settings, or document sections work together, with a one-line plain-English gloss for each important symbol.
- Failure modes, security concerns, and testing guidance, each explained so a newcomer understands the risk, not just the name of it.
- Worked example: a concrete, realistic example drawn only from this file's real content - a command invocation, a short code snippet, or a step-by-step call flow. Use only commands, symbols, and paths that actually appear in the file.
- Maintainer notes and review checklist.

**What it is:** This is a Rust file in `src`. Its first useful signal is: use serde::{Deserialize, Serialize};.

**Why it matters:** Its first useful signal is: use serde::{Deserialize, Serialize};. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**In plain terms:** Its first useful signal is: use serde::{Deserialize, Serialize};.

**What users should know:** Users may not touch this file directly, but its behavior can still affect reliability, output, or workflow.

**How it works:** The current snapshot has 553 lines and 19 function-like definitions. Read the public functions first, then follow data flow into helpers before changing behavior.

**For example:** to see this file at work, start from `default` (function) in `src/angler/config.rs` and follow what it calls or configures next.

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 553 lines, 19 detected function-like definitions, hash 14963980082032174338.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.


Recent documented file:
## `src/angler/git_hooks.rs`

**Documentation depth:** deep explanation, target 1400-2000 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why i
[trimmed]