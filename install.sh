#!/usr/bin/env bash
#
# kaptaind Installer — CLI-based installation for Linux, macOS, Windows/WSL
#
# Usage: curl -fsSL https://raw.githubusercontent.com/elci-group/kaptaind/main/install.sh | bash
#        or: bash ./install.sh [OPTIONS]
#
# By default the installer downloads a prebuilt, cosign keyless-signed release
# archive, verifies it against SHA256SUMS.txt (and the cosign bundle when
# `cosign` is available), and installs the binaries. Building from source is
# supported but skips artifact signing, so it is gated behind
# `--build-from-source` and prints a warning.
#
# Options:
#   --install-dir DIR       Installation directory (default: ~/.local/bin)
#   --system                Install system-wide to /usr/local/bin (requires sudo)
#   --ref TAG               Release tag to install (default: latest), e.g. v9.7.16
#   --build-from-source     Clone and build from source instead of downloading a
#                           signed release (skips artifact signature checks)
#   --no-init               Skip kaptaind-cli init after installation
#   --autostart             Enable auto-start on login (systemd/launchd/shell)
#   --build-only            (build-from-source) Build but don't install
#   --debug                 (build-from-source) Build debug binary instead of release
#   --help                  Show this help message

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTALL_DIR="${HOME}/.local/bin"
SYSTEM_INSTALL=false
RUN_INIT=true
BUILD_ONLY=false
BUILD_MODE="release"
ENABLE_AUTOSTART=false
BUILD_FROM_SOURCE=false
REF=""
GITHUB_REPO="elci-group/kaptaind"
REPO_URL="https://github.com/${GITHUB_REPO}.git"
RELEASES_API="https://api.github.com/repos/${GITHUB_REPO}/releases"
DOWNLOAD_BASE="https://github.com/${GITHUB_REPO}/releases/download"
TEMP_DIR=""
SRC_DIR=""   # directory holding the binaries to install (downloaded or built)

# TLS-restricted curl (HTTPS only, TLS >= 1.2).
CURL=(curl -fsSL --proto '=https' --tlsv1.2)

# Cleanup on exit
cleanup() {
    if [[ -n "$TEMP_DIR" && -d "$TEMP_DIR" ]]; then
        rm -rf "$TEMP_DIR"
    fi
}
trap cleanup EXIT

# Print with color
print_info() {
    echo -e "${BLUE}ℹ${NC} $*"
}

print_success() {
    echo -e "${GREEN}✓${NC} $*"
}

print_error() {
    echo -e "${RED}✗${NC} $*" >&2
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $*"
}

print_header() {
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $*${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
}

# Show help
show_help() {
    sed -n '2,28p' "$0" | sed 's/^# //'
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --install-dir)
            INSTALL_DIR="$2"
            shift 2
            ;;
        --system)
            SYSTEM_INSTALL=true
            INSTALL_DIR="/usr/local/bin"
            shift
            ;;
        --ref)
            REF="$2"
            shift 2
            ;;
        --build-from-source)
            BUILD_FROM_SOURCE=true
            shift
            ;;
        --no-init)
            RUN_INIT=false
            shift
            ;;
        --autostart)
            ENABLE_AUTOSTART=true
            shift
            ;;
        --build-only)
            BUILD_ONLY=true
            shift
            ;;
        --debug)
            BUILD_MODE="debug"
            shift
            ;;
        --help)
            show_help
            ;;
        *)
            print_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Detect OS and architecture
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "linux" ;;
        Darwin*)    echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *)          echo "unknown" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64)     echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        armv7l)     echo "armv7" ;;
        *)          echo "unknown" ;;
    esac
}

# Map (os, arch) to the Rust target triple used in release artifact names.
map_target() {
    local os="$1" arch="$2"
    case "${os}:${arch}" in
        linux:x86_64)    echo "x86_64-unknown-linux-gnu" ;;
        linux:aarch64)   echo "aarch64-unknown-linux-gnu" ;;
        macos:x86_64)    echo "x86_64-apple-darwin" ;;
        macos:aarch64)   echo "aarch64-apple-darwin" ;;
        windows:x86_64)  echo "x86_64-pc-windows-msvc" ;;
        *)               echo "" ;;
    esac
}

# Check if command exists
has_cmd() {
    command -v "$1" >/dev/null 2>&1
}

# Check system requirements
check_requirements() {
    print_header "Checking System Requirements"

    local os arch
    os=$(detect_os)
    arch=$(detect_arch)

    print_info "OS: $(uname -s) ($os)"
    print_info "Architecture: $(uname -m) ($arch)"

    if [[ "$os" == "unknown" ]]; then
        print_error "Unsupported operating system"
        exit 1
    fi

    if [[ "$arch" == "unknown" ]]; then
        print_warning "Unknown architecture; proceeding anyway (may fail)"
    fi

    if [[ "$BUILD_FROM_SOURCE" == true ]]; then
        # Source builds need the full toolchain.
        if ! has_cmd cargo; then
            print_error "Rust/Cargo not found (required for --build-from-source)"
            echo "Install from https://rustup.rs/ with:"
            echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
            exit 1
        fi
        print_success "Rust $(rustc --version)"

        if ! has_cmd git; then
            print_error "Git not found"
            exit 1
        fi
        print_success "Git $(git --version | awk '{print $3}')"

        if [[ "$os" == "linux" ]]; then
            if ! has_cmd gcc && ! has_cmd clang; then
                print_warning "C compiler not found (gcc/clang)"
                echo "On Debian/Ubuntu: sudo apt-get install build-essential"
                echo "On Fedora/RHEL: sudo dnf install gcc"
            fi
        fi
    else
        # Download path needs curl and sha256sum/shasum.
        if ! has_cmd curl; then
            print_error "curl not found (required to download signed releases)"
            exit 1
        fi
        if ! has_cmd sha256sum && ! has_cmd shasum; then
            print_error "Neither sha256sum nor shasum found (required to verify checksums)"
            exit 1
        fi
        if ! has_cmd tar && [[ "$os" != "windows" ]]; then
            print_error "tar not found (required to unpack the release archive)"
            exit 1
        fi
    fi
}

# sha256 wrapper that works on GNU (sha256sum) and BSD/macOS (shasum -a 256).
sha256_check_stdin() {
    if has_cmd sha256sum; then
        sha256sum -c -
    else
        # shasum expects "HASH  filename" lines in the same format.
        shasum -a 256 -c -
    fi
}

# Resolve the release tag to install (latest when --ref not given).
resolve_ref() {
    if [[ -n "$REF" ]]; then
        # Normalize to a leading-v tag.
        [[ "$REF" == v* ]] || REF="v${REF}"
        print_info "Installing requested release: $REF"
        return
    fi

    print_info "Resolving latest release tag..."
    local tag
    tag=$("${CURL[@]}" "${RELEASES_API}/latest" \
        | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' \
        | head -n1)
    if [[ -z "$tag" ]]; then
        print_error "Could not resolve latest release tag from ${RELEASES_API}/latest"
        print_error "Pass an explicit tag with --ref vX.Y.Z"
        exit 1
    fi
    REF="$tag"
    print_success "Latest release: $REF"
}

# Download, verify, and unpack a signed release archive into SRC_DIR.
download_release() {
    print_header "Downloading Signed Release"

    local os arch target version asset ext url
    os=$(detect_os)
    arch=$(detect_arch)
    target=$(map_target "$os" "$arch")

    if [[ -z "$target" ]]; then
        print_error "No prebuilt release for ${os}/${arch}."
        print_error "Rebuild from source with: $0 --build-from-source"
        exit 1
    fi

    version="${REF#v}"
    if [[ "$os" == "windows" ]]; then
        ext="zip"
    else
        ext="tar.gz"
    fi
    asset="kaptaind-${version}-${target}.${ext}"
    url="${DOWNLOAD_BASE}/${REF}"

    TEMP_DIR=$(mktemp -d)
    cd "$TEMP_DIR"

    print_info "Release: ${REF}  target: ${target}"
    print_info "Fetching ${asset} and SHA256SUMS.txt..."
    "${CURL[@]}" -O "${url}/${asset}"
    "${CURL[@]}" -O "${url}/SHA256SUMS.txt"
    # Signature bundle is optional (keyless); fetch if present, continue if not.
    "${CURL[@]}" -O "${url}/SHA256SUMS.txt.sig" 2>/dev/null || true
    "${CURL[@]}" -O "${url}/SHA256SUMS.txt.cert" 2>/dev/null || true

    # 1) Verify the archive against the signed checksum manifest.
    print_info "Verifying SHA-256 checksum..."
    local line
    line=$(grep -F "  ${asset}" SHA256SUMS.txt || grep -F " ${asset}" SHA256SUMS.txt || true)
    if [[ -z "$line" ]]; then
        print_error "${asset} not listed in SHA256SUMS.txt"
        exit 1
    fi
    if ! printf '%s\n' "$line" | sha256_check_stdin; then
        print_error "Checksum verification FAILED for ${asset}"
        exit 1
    fi
    print_success "Checksum verified"

    # 2) Verify the cosign keyless signature over SHA256SUMS.txt when cosign exists.
    if has_cmd cosign && [[ -f SHA256SUMS.txt.sig && -f SHA256SUMS.txt.cert ]]; then
        print_info "Verifying cosign keyless signature over SHA256SUMS.txt..."
        if cosign verify-blob \
            --signature SHA256SUMS.txt.sig \
            --certificate SHA256SUMS.txt.cert \
            --certificate-identity-regexp "^https://github.com/${GITHUB_REPO}/.github/workflows/.+" \
            --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
            SHA256SUMS.txt; then
            print_success "cosign signature verified (GitHub Actions keyless)"
        else
            print_error "cosign signature verification FAILED"
            exit 1
        fi
    else
        print_warning "cosign not available (or signature bundle missing): verified checksum only, not the signature."
        print_warning "Install cosign (https://github.com/sigstore/cosign) to verify release signatures."
    fi

    # 3) Unpack.
    print_info "Unpacking ${asset}..."
    mkdir -p extract
    if [[ "$ext" == "zip" ]]; then
        if ! has_cmd unzip; then
            print_error "unzip not found (required to unpack ${asset})"
            exit 1
        fi
        unzip -q "$asset" -d extract
    else
        tar -xzf "$asset" -C extract
    fi
    SRC_DIR="$TEMP_DIR/extract"
    print_success "Release unpacked"
}

# Clone or update repository (build-from-source path only).
setup_repo() {
    print_header "Setting Up Repository (build from source)"

    TEMP_DIR=$(mktemp -d)
    print_info "Cloning into $TEMP_DIR"

    local clone_args=(--depth 1)
    if [[ -n "$REF" ]]; then
        clone_args+=(--branch "$REF")
    fi

    if git clone "${clone_args[@]}" "$REPO_URL" "$TEMP_DIR" 2>&1 | grep -v "^Cloning"; then
        print_success "Repository cloned"
    else
        print_error "Failed to clone repository"
        exit 1
    fi

    cd "$TEMP_DIR"
}

# Build binaries (build-from-source path only).
build_binaries() {
    print_header "Building Kaptaind"

    local mode_flag=""
    [[ "$BUILD_MODE" == "release" ]] && mode_flag="--release"

    print_info "Building mode: $BUILD_MODE"
    print_info "This may take 1-3 minutes..."

    if cargo build $mode_flag 2>&1 | tail -20; then
        print_success "Build completed"
    else
        print_error "Build failed"
        exit 1
    fi

    SRC_DIR="$TEMP_DIR/target/$([[ "$BUILD_MODE" == "release" ]] && echo "release" || echo "debug")"
}

# Install binaries from SRC_DIR.
install_binaries() {
    print_header "Installing Binaries"

    if [[ -z "$SRC_DIR" || ! -d "$SRC_DIR" ]]; then
        print_error "Internal error: no binaries available to install"
        exit 1
    fi

    # Create install directory
    if [[ "$SYSTEM_INSTALL" == true ]]; then
        if ! sudo mkdir -p "$INSTALL_DIR"; then
            print_error "Failed to create $INSTALL_DIR (requires sudo)"
            exit 1
        fi
        print_info "Installing to system location: $INSTALL_DIR"
    else
        mkdir -p "$INSTALL_DIR"
        print_info "Installing to: $INSTALL_DIR"
    fi

    # Copy binaries
    for binary in kaptaind kaptaind-cli; do
        local src="$SRC_DIR/$binary"
        [[ "$(detect_os)" == "windows" ]] && src="$SRC_DIR/${binary}.exe"
        if [[ -f "$src" ]]; then
            if [[ "$SYSTEM_INSTALL" == true ]]; then
                sudo cp "$src" "$INSTALL_DIR/$binary"
                sudo chmod +x "$INSTALL_DIR/$binary"
            else
                cp "$src" "$INSTALL_DIR/$binary"
                chmod +x "$INSTALL_DIR/$binary"
            fi
            print_success "Installed $binary"
        else
            print_error "Binary $binary not found in $SRC_DIR"
            exit 1
        fi
    done
}

# Verify installation
verify_installation() {
    print_header "Verifying Installation"

    # Check if binaries are in PATH
    if ! has_cmd kaptaind; then
        print_warning "kaptaind not in PATH"
        if [[ "$SYSTEM_INSTALL" == false ]]; then
            echo "Add this to your ~/.bashrc or ~/.zshrc:"
            echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
            return 0
        fi
    fi

    # Test execution
    if kaptaind --version >/dev/null 2>&1; then
        print_success "kaptaind executable verified"
    else
        print_warning "Could not verify kaptaind (may need PATH update)"
    fi

    if kaptaind-cli --version >/dev/null 2>&1; then
        print_success "kaptaind-cli executable verified"
    else
        print_warning "Could not verify kaptaind-cli (may need PATH update)"
    fi
}

# Setup shell integration
setup_shell_integration() {
    print_header "Shell Integration"

    local shells=()
    [[ -f "$HOME/.bashrc" ]] && shells+=("$HOME/.bashrc")
    [[ -f "$HOME/.zshrc" ]] && shells+=("$HOME/.zshrc")

    if [[ ${#shells[@]} -gt 0 ]]; then
        echo "Add this to your shell config for better integration:"
        echo ""
        echo "  # kaptaind shell integration"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
    fi
}

# Setup auto-start
setup_autostart() {
    if [[ "$ENABLE_AUTOSTART" == false ]]; then
        return
    fi

    print_header "Setting Up Auto-Start"

    # Check if kaptaind binary exists
    if [[ ! -f "$INSTALL_DIR/kaptaind" ]]; then
        print_warning "kaptaind not found at $INSTALL_DIR/kaptaind, skipping auto-start"
        return
    fi

    # Detect OS and setup accordingly
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        setup_autostart_systemd
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        setup_autostart_launchd
    else
        setup_autostart_shell
    fi
}

setup_autostart_systemd() {
    local systemd_dir="$HOME/.config/systemd/user"
    mkdir -p "$systemd_dir"

    cat > "$systemd_dir/kaptaind.service" << EOF
[Unit]
Description=Kaptaind - Automated Semantic Versioning Daemon
Documentation=https://github.com/elci-group/kaptaind
After=network.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=$INSTALL_DIR/kaptaind-cli autostart
StandardOutput=journal
StandardError=journal
SyslogIdentifier=kaptaind
Environment="RUST_LOG=info"
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload 2>/dev/null || true
    systemctl --user enable kaptaind.service 2>/dev/null || true

    print_success "Auto-start enabled via systemd user service"
    echo "  Service file: $systemd_dir/kaptaind.service"
    echo "  Start now with: systemctl --user start kaptaind"
}

setup_autostart_launchd() {
    local launchd_dir="$HOME/.Library/LaunchAgents"
    mkdir -p "$launchd_dir"

    cat > "$launchd_dir/com.elcigroup.kaptaind.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.elcigroup.kaptaind</string>
  <key>ProgramArguments</key>
  <array>
    <string>$INSTALL_DIR/kaptaind-cli</string>
    <string>autostart</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>$HOME/.kaptaind/daemon.out</string>
  <key>StandardErrorPath</key>
  <string>$HOME/.kaptaind/daemon.err</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>RUST_LOG</key>
    <string>info</string>
  </dict>
</dict>
</plist>
EOF

    launchctl load "$launchd_dir/com.elcigroup.kaptaind.plist" 2>/dev/null || true

    print_success "Auto-start enabled via launchd plist"
    echo "  Plist file: $launchd_dir/com.elcigroup.kaptaind.plist"
}

setup_autostart_shell() {
    local autostart_line="$INSTALL_DIR/kaptaind-cli autostart > /dev/null 2>&1"

    for rc_file in "$HOME/.bashrc" "$HOME/.zshrc"; do
        [[ ! -f "$rc_file" ]] && continue

        if ! grep -q "Auto-start kaptaind" "$rc_file"; then
            {
                echo ""
                echo "# Auto-start kaptaind"
                echo "export PATH=\"\$HOME/.local/bin:\$PATH\""
                echo "$autostart_line"
            } >> "$rc_file"
        fi
    done

    print_success "Auto-start enabled via shell initialization"
    echo "  Added to ~/.bashrc and ~/.zshrc"
}

# Create .kaptaind directory
setup_kaptaind_dir() {
    print_header "Setting Up Configuration Directory"

    mkdir -p "$HOME/.kaptaind"
    print_success "Created $HOME/.kaptaind"
}

# Run kaptaind-cli init (optional)
run_init() {
    if [[ "$RUN_INIT" == false ]]; then
        return
    fi

    print_header "Initialize Project (Optional)"

    echo "Would you like to initialize kaptaind for a project now?"
    echo "Run: kaptaind-cli init"
    echo ""
}

# Main flow
main() {
    print_header "Kaptaind Installer"
    echo "  Repository: $REPO_URL"
    echo ""

    check_requirements

    if [[ "$BUILD_FROM_SOURCE" == true ]]; then
        print_warning "--build-from-source: building from source SKIPS release-artifact signature"
        print_warning "verification. Prefer the default download path for trusted, signed binaries."
        setup_repo
        build_binaries
        if [[ "$BUILD_ONLY" == true ]]; then
            print_success "Build completed at $TEMP_DIR"
            exit 0
        fi
    else
        resolve_ref
        download_release
    fi

    install_binaries
    setup_kaptaind_dir
    verify_installation
    setup_shell_integration
    setup_autostart
    run_init

    print_header "Installation Complete ✓"
    echo "Next steps:"
    echo "  1. Update your PATH (if needed)"
    echo "  2. Run: kaptaind-cli init"
    if [[ "$ENABLE_AUTOSTART" == false ]]; then
        echo "  3. Run: kaptaind --daemon"
        echo ""
        echo "To enable auto-start: kaptaind-cli enable-autostart"
    else
        echo "  3. kaptaind will start automatically on next login"
    fi
    echo ""
    echo "For help: kaptaind --help"
    echo ""
}

main "$@"
