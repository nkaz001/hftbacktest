# HFT System Docker Image
# Multi-stage build for minimal runtime image

# =============================================================================
# Stage 1: Dashboard Builder (Node.js)
# =============================================================================
FROM node:20-slim AS dashboard-builder

WORKDIR /dashboard
COPY server/dashboard/package*.json ./
RUN npm ci
COPY server/dashboard/ ./
RUN npm run build

# =============================================================================
# Stage 2: Rust Builder
# =============================================================================
FROM rust:latest AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    libssl-dev \
    pkg-config \
    clang \
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY hftbacktest ./hftbacktest
COPY hftbacktest-derive ./hftbacktest-derive
COPY py-hftbacktest ./py-hftbacktest
COPY connector ./connector
COPY collector ./collector
COPY supervisor ./supervisor
COPY server ./server
COPY order-recorder ./order-recorder
COPY submodules/factor-engine ./submodules/factor-engine
COPY submodules/hft-strategies ./submodules/hft-strategies

# Build main workspace components in release mode
# Uses BuildKit cache mounts to persist cargo registry and target dir across builds.
# Only changed crates are recompiled (~1-2 min instead of 5-10 min).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --release \
    -p connector --features recording \
    -p collector \
    -p order-recorder \
    -p hft-supervisor \
    -p hft-server && \
    cp target/release/connector target/release/collector \
       target/release/order-recorder target/release/hft-supervisor \
       target/release/hft-server target/release/hftctl \
       /tmp/

# Build strategies (separate workspace)
# Replace SSH git deps with local path deps to avoid needing SSH in Docker build.
# The .cargo/config.toml [patch] isn't enough - Cargo still tries to fetch the original URL.
# Path deps are relative to the Cargo.toml location (obi_mm_rust/)
# ../../.. = /build, so ../../../hftbacktest = /build/hftbacktest
RUN cd submodules/hft-strategies/obi_mm_rust && \
    sed -i \
      -e 's|git = "ssh://git@github.com/bookdisco/hftbacktest", default-features = false, features = \["backtest"\]|path = "../../../hftbacktest", default-features = false, features = ["backtest"]|' \
      -e 's|git = "ssh://git@github.com/bookdisco/factor-engine"|path = "../../factor-engine"|' \
      -e 's|git = "ssh://git@github.com/bookdisco/hftbacktest", optional = true|path = "../../../order-recorder", optional = true|' \
      Cargo.toml && \
    cat Cargo.toml

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/submodules/hft-strategies/target \
    cd submodules/hft-strategies/obi_mm_rust && \
    cargo build --release --features live && \
    cd /build/submodules/hft-strategies && \
    cargo build --release -p template_strategy && \
    cp target/release/obi_mm_live target/release/template_strategy \
       /tmp/

# =============================================================================
# Stage 3: Runtime
# =============================================================================
# Use debian:trixie-slim which has GLIBC 2.39 matching the rust:latest build environment
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    gosu \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -r -s /bin/false -d /var/lib/hft hft

# Create required directories
RUN mkdir -p /var/lib/hft/data /var/lib/hft/logs/bots /var/lib/hft/logs/infra /var/lib/hft/dashboard /etc/hft \
    && chown -R hft:hft /var/lib/hft

# Copy binaries from builder (staged to /tmp/ because target/ is a cache mount)
COPY --from=builder /tmp/connector /usr/local/bin/
COPY --from=builder /tmp/collector /usr/local/bin/
COPY --from=builder /tmp/order-recorder /usr/local/bin/
COPY --from=builder /tmp/hft-supervisor /usr/local/bin/
COPY --from=builder /tmp/hft-server /usr/local/bin/
COPY --from=builder /tmp/hftctl /usr/local/bin/
COPY --from=builder /tmp/obi_mm_live /usr/local/bin/
COPY --from=builder /tmp/template_strategy /usr/local/bin/

# Copy dashboard from dashboard-builder
COPY --from=dashboard-builder /dashboard/dist /var/lib/hft/dashboard

# Copy entrypoint script
COPY deploy/docker/entrypoint.sh /entrypoint.sh
RUN chmod 755 /entrypoint.sh

# Set working directory
WORKDIR /var/lib/hft

# Expose ports
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:3000/api/health || exit 1

# Entrypoint runs as root initially, then drops privileges to hft user
ENTRYPOINT ["/entrypoint.sh"]
