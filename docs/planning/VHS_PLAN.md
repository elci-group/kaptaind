# VHS Demo Integration Plan

## Objective
To provide a compelling visual demonstration of Kaptaind's core capabilities in the `README.md` using Charmbracelet's `vhs`. This will show users the terminal output exactly as it appears, complete with emojis, colors, and layout.

## Demo Flow & Timing Cues
The VHS tape (`demo.tape`) will follow this narrative:

1. **Setup (Hidden):** 
   - Run `cargo install --path .` to ensure `kaptaind` and `kaptaind-cli` are available in the PATH.
   - Clear the screen.
2. **Status Check (0-5s):** 
   - Run `kaptaind-cli status` to show the daemon is currently stopped.
3. **Daemonization (5-10s):**
   - Run `kaptaind --daemon` to start the process in the background.
   - Re-run `kaptaind-cli status` to confirm it is running (Green PID status).
4. **Visual Interfaces (10-15s):**
   - Run `kaptaind --lanes` to show the engine load dashboard.
5. **Simulated File Modification (15-20s):**
   - Inject a new file: `echo "pub fn demo_api() {}" > src/demo.rs`.
   - Run `kaptaind-cli analyze` immediately to show the "dry-run" API-Added detection (before the 5s clustering window closes).
6. **Background Commit & Verification (20-30s):**
   - Add a sleep to wait for the 5-second `[cluster].window` to close and the background test hook to execute.
   - Run `kaptaind-cli log` to view the rich table output, highlighting the newly generated automated semantic bump.

## Integration Steps
1. Create `demo.tape` with the script described above.
2. Execute `vhs demo.tape` to generate `demo.gif`.
3. Update `README.md` to display the GIF prominently below the "Features" section.
4. Add `demo.gif` to `.gitignore` if we prefer not to track binary blobs, OR commit it directly to the repository so it renders on GitHub. (We will commit it directly for GitHub rendering).
5. Branch Maintenance: Move the `master` branch to `main`.