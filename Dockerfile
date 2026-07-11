# syntax=docker/dockerfile:1
#
# kaptaind daemon container.
#
# Build context requirement: the manifest depends on `deckhand` as a pinned git
# dependency (`deckhand = { git = "...", rev = "..." }` in Cargo.toml), so the
# builder stage needs network access to fetch it from GitHub during
# `cargo build`. The supported, reproducible multi-arch release artifacts are
# produced by `.github/workflows/release.yml`; this image is a convenience for
# self-hosting the daemon.
#
# The container starts as root so the entrypoint can fix ownership of the
# `.kaptaind` data volume, then drops to the unprivileged `kaptaind` user
# (UID 1000) via `runuser` before exec'ing the daemon — see docker-entrypoint.sh.
# Base images are pinned by digest for reproducible, tamper-evident builds.

FROM rust:1.82-bookworm@sha256:d9c3c6f1264a547d84560e06ffd79ed7a799ce0bff0980b26cf10d29af888377 AS builder
WORKDIR /build

# Warm the dependency layer. Manifests first so this layer is cached unless
# dependencies change. `cargo fetch` only needs manifests, not sources, and
# must fail the build if the locked dependencies cannot be fetched.
COPY Cargo.toml Cargo.lock ./
COPY crates/kaptaind-diff/Cargo.toml crates/kaptaind-diff/Cargo.toml
RUN mkdir -p src/cli src/installer crates/kaptaind-diff/src \
    && printf 'fn main(){}\n' > src/main.rs \
    && printf 'fn main(){}\n' > src/cli/main.rs \
    && printf 'fn main(){}\n' > src/installer/gui.rs \
    && printf '' > crates/kaptaind-diff/src/lib.rs \
    && cargo fetch

# Full source, then the authoritative build.
COPY . .
RUN cargo build --release --bin kaptaind --bin kaptaind-cli

# --- runtime ---
FROM debian:bookworm-slim@sha256:60eac759739651111db372c07be67863818726f754804b8707c90979bda511df

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git libssl3 curl util-linux \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 kaptaind

COPY --from=builder --chown=kaptaind:kaptaind /build/target/release/kaptaind /usr/local/bin/kaptaind
COPY --from=builder --chown=kaptaind:kaptaind /build/target/release/kaptaind-cli /usr/local/bin/kaptaind-cli
COPY --chown=root:root docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

WORKDIR /opt/kaptaind
ENV KAPTAIND_HEALTH_PORT=9090

# Probe the daemon health server (served on KAPTAIND_HEALTH_PORT) rather than
# `kaptaind-cli status`, which fails before the daemon has written status.json.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS "http://localhost:${KAPTAIND_HEALTH_PORT}/health" >/dev/null || exit 1

ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["kaptaind"]
