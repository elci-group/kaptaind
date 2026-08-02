# DumDum Project Context

Project: kaptaind

Languages observed:
- Rust: 6 files
- TOML: 1 files

Directory shape:
- crates/kaptaind-diff: 1 files
- crates/kaptaind-diff/src: 1 files
- crates/kaptaind-diff/src/diff_version: 3 files
- crates/kaptaind-diff/src/version: 2 files

Important file signals:
- crates/kaptaind-diff/Cargo.toml (TOML, 278 bytes): [package]
- crates/kaptaind-diff/src/diff_version/cache.rs (Rust, 3014 bytes): use crate::diff_version::detector::LanguageVersion;
- crates/kaptaind-diff/src/diff_version/detector.rs (Rust, 15193 bytes): use crate::diff_version::cache::VersionCache;
- crates/kaptaind-diff/src/diff_version/mod.rs (Rust, 127 bytes): pub mod cache;
- crates/kaptaind-diff/src/lib.rs (Rust, 39 bytes): pub mod diff_version;
- crates/kaptaind-diff/src/version/mod.rs (Rust, 72 bytes): pub mod semver;
- crates/kaptaind-diff/src/version/semver.rs (Rust, 2016 bytes): use semver::Version;


Recent documented file:
## `crates/kaptaind-diff/Cargo.toml`

**Documentation depth:** brief explanation, target 400-600 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why it matters to users or maintainers, in plain language that defines any technical term on first use.
- User-visible behavior or operational effect.
- Worked example: a concrete, realistic example drawn only from this file's real content - a command invocation, a short code snippet, or a step-by-step call flow. Use only commands, symbols, and paths that actually appear in the file.
- Maintainer notes and review checklist.

**What it is:** This is a TOML file in `crates`. It configures tooling or runtime behavior rather than directly serving end-user screens.

**Why it matters:** It configures tooling or runtime behavior rather than directly serving end-user screens. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**In plain terms:** It configures tooling or runtime behavior rather than directly serving end-user screens.

**What users should know:** Changing this can alter the binary name, version, Rust edition, or external crates needed to build.

**How it works:** The first meaningful line and surrounding directory are the strongest signals for this file. If that signal is weak, inspect imports, callers, or links before treating the explanation as complete.

**For example:** open `crates/kaptaind-diff/Cargo.toml` and read its first meaningful line - it is the shortest accurate summary of everything that follows.

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 15 lines, 0 detected function-like definitions, hash 6351175972609509642.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.


Recent documented file:
## `crates/kaptaind-diff/src/diff_version/cache.rs`

**Documentation depth:** standard explanation, target 800-1100 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why it matters to users or maintainers, in plain language that defines any technical term on first use.
- User-visible behavior or operational effect.
- How the important functions, settings, or document sections work together, with a one-line plain-English gloss for each important symbol.
- Worked example: a concrete, realistic example drawn only from this file's real content - a command invocation, a short code snippet, or a step-by-step call flow. Use only commands, symbols, and paths that actually appear in the file.
- Maintainer notes and review checklist.

**What it is:** This is a Rust file in `crates`. Its first useful signal is: use crate::diff_version::detector::LanguageVersion;.

**Why it matters:** Its first useful signal is: use crate::diff_version::detector::LanguageVersion;. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**In plain terms:** Its first useful signal is: use crate::diff_version::detector::LanguageVersion;.

**What users should know:** Users may not touch this file directly, but its behavior can still affect reliability, output, or workflow.

**How it works:** The current snapshot has 100 lines and 6 function-like definitions. Read the public functions first, then follow data flow into helpers before changing behavior.

**Key symbols:** const CACHE_FILE, const TTL_SECS, struct VersionCache, struct CacheEntry, impl VersionCache, function load, function save, function get, function put, module tests, function roundtrip_through_disk, function missing_entry_returns_none

**For example:** to see this file at work, start from `CACHE_FILE` (const) in `crates/kaptaind-diff/src/diff_version/cache.rs` and follow what it calls or configures next.

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 100 lines, 6 detected function-like definitions, hash 17048064189112946790.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.


Recent documented file:
## `crates/kaptaind-diff/src/diff_version/detector.rs`

**Documentation depth:** deep explanation, target 1400-2000 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why it matters to users or maintainers, in plain language that defines any technical term on first use.
- User-visible behavior or operational effect.
- How the important functions, settings, or document sections work together, with a one-line plain-English gloss for each important symbol.
- Failure modes, security concerns, and testing guidance, each explained so a newcomer understands the risk, not just the name of it.
- Worked example: a concrete, realistic example drawn only from this file's real content - a command invocation, a short code snippet, or a step-by-step call flow. Use only commands, symbols, and paths that actually appear in the file.
- Maintainer notes and review checklist.

**What it is:** This is a Rust file in `crates`. Its first useful signal is: use crate::diff_version::cache::VersionCache;.

**Why it matters:** Its first useful signal is: use crate::diff_version::cache::VersionCache;. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**In plain terms:** Its first useful signal is: use crate::diff_version::cache::VersionCache;.

**What users should know:** Users may not touch this file directly, but its behavior can still affect reliability, output, or workflow.

**How it works:** The current snapshot has 436 lines and 26 function-like definitions. Read the public functions first, then follow data flow into helpers before changing behavior.

**For example:** to see this file at work, start from `unknown` (function) in `crates/kaptaind-diff/src/diff_version/detector.rs` and follow what it calls or configures next.

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 436 lines, 26 detected function-like definitions, hash 9033046263647144555.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.


Recent documented file:
## `crates/kaptaind-diff/src/diff_version/mod.rs`

**Documentation depth:** standard explanation, target 800-1100 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why it matters to users or maintainers, in plain language that defines any technical term on first use.
- User-visible behavior or operational effect.
- How the important functions, settings, or document sections work together, with a one-line plain-English gloss for each important symbol.
- Worked example: a concrete, realistic example drawn only from this file's real content - a command invocation, a short code snippet, or a step-by-step call flow. Use only commands, symbols, and paths that actually appear in the file.
- Maintainer notes and review checklist.

**What it is:** This is a Rust file in `crates`. Its first useful signal is: pub mod cache;.

**Why it matters:** Its first useful signal is: pub mod cache;. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**In plain terms:** Its first useful signal is: pub mod cache;.

**What users should know:** Users may not touch this file directly, but its behavior can still affect reliability, output, or workflow.

**How it works:** The current snapshot has 5 lines and 0 function-like definitions. Read the public functions first, then follow data flow into helpers before changing behavior.

**Key symbols:** module cache, module detector

**For example:** to see this file at work, start from `cache` (module) in `crates/kaptaind-diff/src/diff_version/mod.rs` and follow what it calls or configures next.

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 5 lines, 0 detected function-like definitions, hash 43844543748337553.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.


Recent documented file:
## `crates/kaptaind-diff/src/lib.rs`

**Documentation depth:** standard explanation, target 800-1100 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why it matters to users or maintainers, in plain language that defines any technical term on first use.
- User-visible behavior or operational effect.
- How the important functions, settings, or document sections work together, with a one-line plain-English gloss for each important symbol.
- Worked example: a concrete, realistic example drawn only from this file's real content - a command invocation, a short code snippet, or a step-by-step call flow. Use only commands, symbols, and paths that actually appear in the file.
- Maintainer notes and review checklist.

**What it is:** This is a Rust file in `crates`. Its first useful signal is: pub mod diff_version;.

**Why it matters:** Its first useful signal is: pub mod diff_version;. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**In plain terms:** Its first useful signal is: pub mod diff_version;.

**What users should know:** Users may not touch this file directly, but its behavior can still affect reliability, output, or workflow.

**How it works:** The current snapshot has 2 lines and 0 function-like definitions. Read the public functions first, then follow data flow into helpers before changing behavior.

**Key symbols:** module diff_version, module version

**For example:** to see this file at work, start from `diff_version` (module) in `crates/kaptaind-diff/src/lib.rs` and follow what it calls or configures next.

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 2 lines, 0 detected function-like definitions, hash 1740875778841729917.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.


Recent documented file:
## `crates/kaptaind-diff/src/version/mod.rs`

**Documentation depth:** standard explanation, target 800-1100 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would understand (what this file is like in ordinary life), then say what it is and where it sits in the project.
- Why it matters to users or maintainers, in plain language that defines any technical term on first use.
- User-visible behavior or operational effect.
- How the important functions, settings, or document sections work together, with a one-line plain-English gloss for each important symbol.
- Worked example: a concrete, realistic example drawn only from this file's real content - a command invocation, a short code snippet, or a step-by-step call flow. Use only commands, symbols, and paths that actually appear in the file.
- Maintainer notes and review checklist.

**What it is:** This is a Rust file in `crates`. Its first useful signal is: pub mod semver;.

**Why it matters:** Its first useful signal is: pub mod semver;. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**In plain terms:** Its first useful signal is: pub mod semver;.

**What users should know:** Users may not touch this file directly, but its behavior can still affect reliability, output, or workflow.

**How it works:** The current snapshot has 3 lines and 0 function-like definitions. Read the public functions first, then follow data flow into helpers before changing behavior.

**Key symbols:** module semver

**For example:** to see this file at work, start from `semver` (module) in `crates/kaptaind-diff/src/version/mod.rs` and follow what it calls or configures next.

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 3 lines, 0 detected function-like definitions, hash 5026127270911010738.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.


Recent documented file:
## `crates/kaptaind-diff/src/version/semver.rs`

**Documentation depth:** standard explanation, target 800-1100 words.

**Planned coverage:**
- In plain terms: open with one everyday analogy a non-programmer would unde
[trimmed]