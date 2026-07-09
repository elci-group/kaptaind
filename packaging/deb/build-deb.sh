#!/usr/bin/env bash
# TEMPLATE — release-time scaffold, NOT self-publishing.
#
# Builds a .deb from a staged tree using dpkg-deb. Run after the release
# workflow has produced the Linux tarballs; point KAPTAIND_BIN and
# KAPTAIND_CLI_BIN at the unpacked binaries for the target architecture.
#
# The resulting .deb is published manually to an APT repository or PPA
# (see packaging/README.md).
#
# Usage:
#   VERSION=9.7.16 ARCH=amd64 \
#   KAPTAIND_BIN=/path/to/kaptaind KAPTAIND_CLI_BIN=/path/to/kaptaind-cli \
#   ./build-deb.sh
set -euo pipefail

VERSION="${VERSION:-<VERSION>}"          # without the leading "v", e.g. 9.7.16
ARCH="${ARCH:-amd64}"                    # amd64 | arm64
STAGE="${STAGE:-dist/deb}"               # staged tree root
OUT="${OUT:-dist}"                       # output directory for the .deb

if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "error: dpkg-deb not found (install the 'dpkg' package)" >&2
  exit 1
fi

: "${KAPTAIND_BIN:?set KAPTAIND_BIN to the path of the kaptaind binary}"
: "${KAPTAIND_CLI_BIN:?set KAPTAIND_CLI_BIN to the path of the kaptaind-cli binary}"

rm -rf "$STAGE"
mkdir -p "$STAGE/DEBIAN" "$STAGE/usr/bin" "$OUT"

install -m 0755 "$KAPTAIND_BIN" "$STAGE/usr/bin/kaptaind"
install -m 0755 "$KAPTAIND_CLI_BIN" "$STAGE/usr/bin/kaptaind-cli"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: kaptaind
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: elci-group <noreply@example.com>
Depends: git, libssl3
Description: Repository change-watcher that ships semantic releases
 kaptaind watches a repository for filesystem changes, clusters them,
 scores the change set, computes a semantic-version bump, and creates the
 release commit. Ships the kaptaind daemon and the kaptaind-cli tool.
EOF

dpkg-deb --build "$STAGE" "${OUT}/kaptaind_${VERSION}_${ARCH}.deb"
echo "built ${OUT}/kaptaind_${VERSION}_${ARCH}.deb"
