# syntax=docker/dockerfile:1

# Build stage
FROM rust:1.82-bookworm AS builder

# Create non-root user for build
RUN useradd -m -u 1000 kaptaind

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src/main.rs src/cli/main.rs ./src/
RUN mkdir -p src && echo "fn main() {}" > src/lib.rs
# Build dependencies only (this layer caches if deps don't change)
RUN cargo build --release --bin kaptaind --bin kaptaind-cli || true

# Now copy full source
COPY . .
RUN cargo build --release --bin kaptaind --bin kaptaind-cli

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git libssl3 && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 kaptaind

COPY --from=builder --chown=kaptaind:kaptaind /build/target/release/kaptaind /usr/local/bin/kaptaind
COPY --from=builder --chown=kaptaind:kaptaind /build/target/release/kaptaind-cli /usr/local/bin/kaptaind-cli

USER kaptaind
WORKDIR /opt/kaptaind

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD kaptaind-cli status || exit 1

CMD ["kaptaind"]
