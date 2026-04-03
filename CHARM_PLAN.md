# Charmbracelet Interactive UI & Logging Plan

## Objective
To elevate the `kaptaind` CLI from a standard output-only application to a rich, stylish, interactive Terminal User Interface (TUI) and logging experience, heavily inspired by (and leveraging) the [Charmbracelet](https://github.com/charmbracelet) ecosystem.

## Context: Rust vs. Go Ecosystems
Because `kaptaind` is built in Rust and the official Charm tools (`bubbletea`, `lipgloss`, `huh`, `log`) are written in Go, we have two primary architectural paths to integrate Charm's aesthetics and interactivity:

### Path A: The Side-Car / Wrapper Approach (Native Charm in Go)
We create a lightweight Go binary (`kaptaind-tui`) that acts as a wrapper around the Rust `kaptaind` daemon and CLI outputs using `os/exec` and IPC.
- **Interactivity:** We build the TUI entirely in Go using `bubbletea` and `huh` (for forms/prompts).
- **Styling:** We use `lipgloss` to render gorgeous gradient tables and status boards.
- **Logging:** We intercept the JSON tracing logs from the Rust daemon and re-render them beautifully using `charmbracelet/log`.

### Path B: The Shell Integration Approach (Using `gum`)
We ship [Charm Gum](https://github.com/charmbracelet/gum) alongside `kaptaind` and orchestrate it from Rust.
- **Interactivity:** Whenever `kaptaind-cli` needs user input (e.g., selecting a repo, confirming a configuration), Rust spawns `gum choose`, `gum input`, or `gum confirm`.
- **Status Spinners:** When the user triggers an analysis (`kaptaind-cli analyze`), we wrap it in `gum spin --title "Analyzing Diff..."`.

## Proposed Architecture (Path A: The Go TUI Client)
Building a dedicated `kaptaind-ui` Go module allows us to leverage the full, native power of Charm tools.

### 1. Interactive Elements (`charmbracelet/bubbletea` & `huh`)
- **Dashboard (`bubbletea`):** Instead of static output for `--radar` and `--lanes`, we build a live-refreshing TUI dashboard. 
  - Users can use arrow keys to navigate between the `Dock`, `Radar`, and `Lanes` views dynamically.
- **Configuration Forms (`huh`):** A new command `kaptaind init` will launch a beautiful interactive form to generate `kaptaind.toml`.
  - *Fields:* Repo Path (Input), Weights (Sliders), Required Tests (Toggle), Notify Hooks (Input).

### 2. Live Log Rendering (`charmbracelet/log`)
- Currently, `kaptaind` outputs `tracing` logs. We will format these as `json` (using `tracing-subscriber::fmt().json()`).
- The `kaptaind-tui log-stream` command will read this JSON stream and pipe it through `charmbracelet/log`.
- **Visuals:** 
  - Errors will have bold red `ERRO` badges.
  - Trace logs will be muted, keeping the console clean.
  - Key-value pairs (like `cluster_id=abc`) will be automatically color-coded and aligned by the logger.

### 3. Execution Flow
1. User types `kaptaind ui`.
2. The Go TUI boots, rendering the layout with `lipgloss`.
3. The TUI reads `.kaptaind/status.json` and `.kaptaind/telemetry.json` every 500ms to update the active dashboard visually.
4. If the user presses `a`, the TUI triggers `kaptaind-cli analyze` and displays the output in a styled markdown box using `charmbracelet/glamour`.

## Next Steps for Implementation
1. **Prepare Rust JSON Output:** Update `kaptaind` tracing to support a `--json` or `--structured` mode.
2. **Init Go Module:** Create a `tui/` directory with a Go module importing `bubbletea`, `lipgloss`, and `log`.
3. **Build the Dashboard Component:** Construct the live status screen reading from the local `status.json` file.
4. **Implement Forms:** Add the `huh` forms for dynamic configuration editing.