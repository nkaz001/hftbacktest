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
COPY factor-engine ./factor-engine
COPY strategies ./strategies

# Build all components in release mode
# Note: This takes 30-45 minutes on first build due to LTO
RUN cargo build --release \
    -p connector --features recording \
    -p collector \
    -p order-recorder \
    -p hft-supervisor \
    -p hft-server \
    -p template_strategy && \
    cargo build --release -p obi-mm --features live

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

# Copy binaries from builder
COPY --from=builder /build/target/release/connector /usr/local/bin/
COPY --from=builder /build/target/release/collector /usr/local/bin/
COPY --from=builder /build/target/release/order-recorder /usr/local/bin/
COPY --from=builder /build/target/release/hft-supervisor /usr/local/bin/
COPY --from=builder /build/target/release/hft-server /usr/local/bin/
COPY --from=builder /build/target/release/hftctl /usr/local/bin/
COPY --from=builder /build/target/release/obi_mm_live /usr/local/bin/
COPY --from=builder /build/target/release/template_strategy /usr/local/bin/

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
