# Kaptaind Installation Guide

This document covers all methods for installing kaptaind on your system.

## Table of Contents

1. [Quick Install (CLI)](#quick-install-cli)
2. [GUI Installer](#gui-installer)
3. [Manual Installation](#manual-installation)
4. [Man Pages](#man-pages)
5. [Uninstall](#uninstall)
6. [Troubleshooting](#troubleshooting)

---

## Quick Install (CLI)

The simplest way to install kaptaind is with the one-liner CLI installer.

### Linux, macOS, or WSL

```bash
curl -fsSL https://raw.githubusercontent.com/elci-group/kaptaind/main/install.sh | bash
```

Or clone and run locally:

```bash
git clone https://github.com/elci-group/kaptaind.git
cd kaptaind
bash install.sh
```

### Installation Options

```bash
# Install to custom directory
bash install.sh --install-dir /opt/kaptaind/bin

# System-wide installation (requires sudo)
bash install.sh --system

# Enable auto-start on login (systemd/launchd/shell)
bash install.sh --autostart

# Build debug binary instead of release
bash install.sh --debug

# Build but don't install
bash install.sh --build-only

# Skip running kaptaind-cli init after install
bash install.sh --no-init

# View all options
bash install.sh --help
```

### What the CLI Installer Does

1. ✓ Detects OS and architecture
2. ✓ Checks prerequisites (Rust, Cargo, Git)
3. ✓ Clones the repository
4. ✓ Builds release binaries
5. ✓ Installs to `~/.local/bin` (or custom location)
6. ✓ Creates `~/.kaptaind` configuration directory
7. ✓ Verifies the installation
8. ✓ Provides next steps

### Auto-Start Setup

To enable kaptaind to start automatically on login or boot:

**During Installation:**
```bash
bash install.sh --autostart
```

**After Installation:**
```bash
kaptaind-cli enable-autostart
```

**To Disable Auto-Start:**
```bash
kaptaind-cli disable-autostart
```

**How Auto-Start Works:**
- **Linux (systemd):** Creates a user systemd service at `~/.config/systemd/user/kaptaind.service`
- **macOS (launchd):** Creates a plist at `~/.Library/LaunchAgents/com.elcigroup.kaptaind.plist`
- **Other systems:** Adds startup code to `~/.bashrc` and `~/.zshrc`

---

## GUI Installer

For a graphical installation experience, use the GUI installer.

### Building the GUI Installer

The GUI installer requires the `gui` feature. Build it with:

```bash
cargo build --release --features gui --bin kaptaind-installer
```

The binary will be at `target/release/kaptaind-installer`.

### Running the GUI Installer

```bash
./target/release/kaptaind-installer
```

Or after installation:

```bash
kaptaind-installer
```

### GUI Installer Screens

1. **Welcome Screen**
   - Displays system information (OS, architecture, Rust version)
   - Shows what will be installed
   - Prerequisites check overview

2. **Dependencies Screen**
   - Detailed dependency status (Rust, Cargo, Git)
   - Instructions if dependencies are missing
   - Proceed to next step only if all dependencies are found

3. **Options Screen**
   - Confirms installation path (`~/.local/bin`)
   - Build mode selection (Release/Debug)
   - Configuration directory information

4. **Progress Screen**
   - Real-time installation status
   - Clone, build, and install progress

5. **Complete Screen**
   - Installation summary
   - Next steps with shell integration instructions
   - Links to documentation

---

## Manual Installation

If you prefer to install manually, follow these steps.

### Prerequisites

- **Rust & Cargo** (install from https://rustup.rs/)
- **Git** (for cloning and for Kaptaind runtime repository operations)
- **C Compiler** (gcc/clang for building)

### Step-by-Step

1. Clone the repository:

```bash
git clone https://github.com/elci-group/kaptaind.git
cd kaptaind
```

2. Build release binaries:

```bash
cargo build --release
```

3. Install binaries:

```bash
mkdir -p ~/.local/bin
cp target/release/kaptaind ~/.local/bin/
cp target/release/kaptaind-cli ~/.local/bin/
chmod +x ~/.local/bin/kaptaind*
```

4. Create configuration directory:

```bash
mkdir -p ~/.kaptaind
```

5. Add to PATH (if not already):

Add this to `~/.bashrc`, `~/.zshrc`, or your shell config:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

6. Verify installation:

```bash
kaptaind --version
kaptaind-cli --version
```

7. Initialize a project:

```bash
cd /path/to/your/repo
kaptaind-cli init
```

---

## Post-Installation

### Shell Integration

After installation, add the following to your shell configuration file (`~/.bashrc`, `~/.zshrc`, etc.):

```bash
# Kaptaind
export PATH="$HOME/.local/bin:$PATH"

# Optional: shell aliases for convenience
alias kd='kaptaind'
alias kdcli='kaptaind-cli'
```

Then reload your shell:

```bash
source ~/.bashrc   # or ~/.zshrc
```

### First Run

1. Navigate to your project:

```bash
cd /path/to/your/project
```

2. Initialize kaptaind:

```bash
kaptaind-cli init
```

This creates a `kaptaind.toml` configuration file tailored to your project type.

3. Start the daemon:

```bash
kaptaind --daemon
```

4. Check status:

```bash
kaptaind-cli status
```

5. **Note:** the `kaptaind.toml` from step 2 defaults to observe-only — the
   daemon will analyze and score your changes but won't commit or push
   anything yet. `kaptaind-cli status` and `kaptaind-cli validate` won't tell
   you this either. Add the following once you're ready for it to actually
   commit:

```toml
[operation]
mode = "actuate"
```

   Pushing needs `[push] enabled = true` and
   `[capabilities] network_push = true` on top of that. See
   [Repository mutation: observe vs. actuate](README.md#repository-mutation-observe-vs-actuate)
   in the README for the full gate list.

---

## Man Pages

Man-page sources are included in the repository as Markdown. You can install them with or without **pandoc**.

### With pandoc

Render the man pages for **kaptaind** and **kaptaind-cli**:

```bash
pandoc man/kaptaind.1.md -s -t man -o /usr/local/share/man/man1/kaptaind.1
pandoc man/kaptaind-cli.1.md -s -t man -o /usr/local/share/man/man1/kaptaind-cli.1
```

After installing, refresh the man database:

```bash
mandb   # Linux
# or
man -k kaptaind   # macOS, to verify indexing
```

### Without pandoc

If you do not have **pandoc** installed, you can copy the Markdown sources directly to the man directory as reference documents:

```bash
cp man/kaptaind.1.md /usr/local/share/man/man1/kaptaind.1.md
cp man/kaptaind-cli.1.md /usr/local/share/man/man1/kaptaind-cli.1.md
```

### Using the Makefile

A minimal Makefile target is provided that renders with pandoc when available and falls back to copying the Markdown sources otherwise:

```bash
make install-man
```

You can change the destination with the `MANDIR` variable:

```bash
make install-man MANDIR=~/.local/share/man
```

---

## Uninstall

### Remove Binaries

```bash
rm ~/.local/bin/kaptaind
rm ~/.local/bin/kaptaind-cli
```

Or if installed system-wide:

```bash
sudo rm /usr/local/bin/kaptaind
sudo rm /usr/local/bin/kaptaind-cli
```

### Remove Configuration (Optional)

To completely remove all kaptaind data:

```bash
rm -rf ~/.kaptaind
```

This will delete:
- `.kaptaind/status.json` — daemon status
- `.kaptaind/analysis/` — analysis artifacts
- `.kaptaind/stability.json` — stability scores
- `.kaptaind/aoc/` — Aim of Change sessions
- All other cached and telemetry files

**Note:** This does NOT affect your source code or version history.

### Cleanup Shell Config

Remove the kaptaind entries from your `~/.bashrc`, `~/.zshrc`, etc.

---

## Troubleshooting

### "command not found: kaptaind"

**Solution:** Add `~/.local/bin` to your PATH:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Add this permanently to `~/.bashrc` or `~/.zshrc`.

### "Rust not found"

**Solution:** Install Rust from https://rustup.rs/:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then reload your shell:

```bash
source $HOME/.cargo/env
```

### "Git not found"

**Solution:** Install Git:

- **Debian/Ubuntu:** `sudo apt-get install git`
- **Fedora/RHEL:** `sudo dnf install git`
- **macOS:** `brew install git`
- **Windows:** Download from https://git-scm.com/

### "Build failed"

**Solution:** Ensure you have a C compiler:

- **Debian/Ubuntu:** `sudo apt-get install build-essential`
- **Fedora/RHEL:** `sudo dnf install gcc`
- **macOS:** `xcode-select --install`

### "Permission denied" when installing to `/usr/local/bin`

**Solution:** Use `sudo` or install to `~/.local/bin` instead:

```bash
bash install.sh --system   # Uses sudo automatically
# or
bash install.sh --install-dir ~/.local/bin
```

### GUI installer not building

**Solution:** Build with the `gui` feature:

```bash
cargo build --release --features gui --bin kaptaind-installer
```

The GUI feature is optional and not built by default due to its dependencies.

---

## Platform-Specific Notes

### macOS

- Install Xcode Command Line Tools: `xcode-select --install`
- Rust can be installed via Homebrew: `brew install rust`

### Windows/WSL

- Use WSL 2 for best compatibility
- Install Rust from https://rustup.rs/
- Use the installer script in WSL:

```bash
curl -fsSL https://raw.githubusercontent.com/elci-group/kaptaind/main/install.sh | bash
```

### Docker

For containerized development, see `Dockerfile` in the repository.

---

## Getting Help

- Documentation: https://github.com/elci-group/kaptaind/blob/main/README.md
- Issues: https://github.com/elci-group/kaptaind/issues
- Discussions: https://github.com/elci-group/kaptaind/discussions
