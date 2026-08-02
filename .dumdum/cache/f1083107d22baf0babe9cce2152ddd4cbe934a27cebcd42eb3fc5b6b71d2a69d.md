## `src/icon.rs`

**Documentation depth:** standard explanation, target 520-760 words.

**Planned coverage:**
- What it is and where it sits in the project.
- Why it matters to users or maintainers.
- User-visible behavior or operational effect.
- How the important functions, settings, or document sections work together.
- Maintainer notes and review checklist.

**What it is:** This is a Rust file in `kaptaind`. Its first useful signal is: use std::path::PathBuf;.

**Why it matters:** Its first useful signal is: use std::path::PathBuf;. DumDum treats this file as part of the project's working contract, so the explanation should connect the file to behavior, operations, or future maintenance rather than only restating its filename.

**What users should know:** Users may not touch this file directly, but its behavior can still affect reliability, output, or workflow.

**How it works:** The current snapshot has 102 lines and 7 function-like definitions. Read the public functions first, then follow data flow into helpers before changing behavior.

**Key symbols:** const NOTIFICATION_LOGO_PNG, function cache_dir, function cached_notification_icon_path, function ensure_cached_notification_icon, function install_icon, function refresh_icon_cache, module tests, function embedded_logo_is_png, function ensure_cached_creates_file

**Media and demos:** No inline GIF, image, or VHS recording references were detected in this snapshot.

**Maintainer notes:** Keep the generated explanation aligned when this file changes. Current snapshot: 102 lines, 7 detected function-like definitions, hash 1479796205407167971.

**Review checklist:**
- Confirm the explanation still matches the file after major edits.
- Check whether linked commands, images, GIFs, or VHS tapes still exist.
- Re-run DumDum after the file has rested so generated sections stay aligned.
