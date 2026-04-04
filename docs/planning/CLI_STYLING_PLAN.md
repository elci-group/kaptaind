# Kaptaind CLI Stylisation & Emojification Plan

## Objective
To transform `kaptaind` and `kaptaind-cli` terminal outputs from plain text into a rich, modern, and visually intuitive experience using emojis, ANSI colors, and structured layouts (like tables and progress bars). This improves developer experience (DX) by making statuses parseable at a glance.

## 1. Color Palette & Theming (via `crossterm` or `owo-colors`)
We will adopt a consistent color semantics:
- **Blue/Cyan (`info`)**: Neutral information, idle states, directories.
- **Green (`success`)**: Successful commits, tests passed, optimal statuses.
- **Yellow (`warn`)**: Rate limiting, skipping commits, minor warnings.
- **Red (`error`)**: Test failures, git conflicts, critical daemon panics.
- **Magenta (`accent`)**: Version bumps, highlighted scores or paths.

## 2. Global Emojification Dictionary
Standardizing emojis ensures the UI feels cohesive across all subcommands.

### Daemon States
- 💤 `Idle` -> `[💤 Idle]`
- 🔍 `Clustering` -> `[🔍 Clustering]`
- 🧪 `Testing` -> `[🧪 Testing]`
- 🚢 `Committing` -> `[🚢 Committing]`
- 🛑 `Failed` -> `[🛑 Failed]`

### Versioning & Bumps
- 🚀 `Major` Bump
- ✨ `Minor` Bump
- 🩹 `Patch` Bump
- 📌 `No Bump` / `Stable`

### Analysis & Diffing
- 📊 Structural changes
- 🔌 API changes
- 📦 Dependency changes
- ⚙️ Runtime changes

## 3. Subcommand Stylisation

### `kaptaind-cli status`
*Current:* Plain text list.
*Target:* A colorful dashboard.
```text
🚢 Kaptaind Status
==================
📂 Repository:  /home/user/kaptaind
🏷️  Version:     v0.1.28
⚙️  Daemon:      🟢 Running (PID: 1234)
```

### `kaptaind-cli log`
*Current:* Standard `tabled` output.
*Target:* Emojified headers and color-coded rows based on the bump type.
- *Header:* `| 🏷️ Version | 📈 Bump | 🎯 Score | 🗂️ Paths | 🔌 API Touches | ...`
- *Row styling:* 
  - Major bumps colored in bold Red with 🚀.
  - Minor bumps colored in bold Cyan with ✨.
  - Patch bumps colored in Green with 🩹.

### `kaptaind-cli analyze`
*Current:* Simple printed list.
*Target:* A visually distinct "dry-run" receipt.
```text
🧪 Dry-run Analysis Result:
-----------------------------------
🗂️ Touched Paths:  4
💥 API Break:      No
➕ API Added:      Yes
🔌 API Score:      0.450
📦 Deps Score:     0.000
⚙️ Runtime Score:  0.000
-----------------------------------
🎯 Total Score:    0.620
📈 Projected Bump: ✨ Minor -> v0.2.0
```

### `kaptaind` (Core Daemon Flags)

#### `--dock` (Static Projects)
```text
⚓ Watched Static Projects (Dock)
--------------------------------------------------
📂 Path                               | 🚦 Status
--------------------------------------------------
/home/user/kaptaind                   | 🟢 Watched
```

#### `--radar` (Active Projects)
```text
📡 Active Projects (Radar)
--------------------------------------------------
📂 Active Project                     | ⚡ Events/hr | 🕒 Last Action
--------------------------------------------------
/home/user/kaptaind                   | 〰️ 12       | 5m ago
```

#### `--lanes` (Service Load)
```text
🛣️ Service/Model Load Breakdown (Lanes)
--------------------------------------------------
🛠️ Service/Model            | 🚥 Load   | 🚦 Status
--------------------------------------------------
📊 Semantic Diff Engine     | 🟢 Low    | ✅ Optimal
📦 Dependency Grapher       | 💤 Idle   | ✅ Ready
🎯 Version Heuristics       | 🟢 Low    | ✅ Optimal
```

## 4. Implementation Steps
1. **Add Dependencies:** Introduce a colorization crate like `owo-colors` or `colored` to `Cargo.toml`.
2. **Refactor CLI Formatters:** Update the print statements in `src/cli/main.rs` and `src/main.rs` to interpolate the emojis and apply color methods.
3. **Table Styling:** Enhance the `tabled` configuration in `handle_log` to use rounded or double-line borders and center-aligned emojified text.
4. **Daemon Logs:** Update `tracing` formatters (or custom subscribers) if we want daemon standard out to also feature these emojis (e.g., prefixing info logs with ℹ️, warnings with ⚠️).