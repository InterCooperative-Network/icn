# Multi-stage Dockerfile for ICN (Intercooperative Network)
#
# This Dockerfile builds both icnd (daemon) and icnctl (CLI tool) in an
# optimized multi-stage build process.
#
# Build:
#   docker build -f docker/Dockerfile -t icn:latest .
#
# Run:
#   docker run -p 4433:4433/udp -p 5050:5050 -p 9100:9100 \
#     -v ~/.icn:/data icn:latest

# ============================================================================
# Stage 1: Build environment
# ============================================================================
FROM rust:1.75-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /build

# Copy workspace manifests first (for layer caching)
COPY icn/Cargo.toml icn/Cargo.lock ./

# Copy all crate manifests (for dependency caching)
COPY icn/crates ./crates
COPY icn/bins ./bins

# Copy source code
COPY icn/crates ./crates
COPY icn/bins ./bins

# Build release binaries
RUN cargo build --release --locked \
    && strip target/release/icnd \
    && strip target/release/icnctl

# ============================================================================
# Stage 2: Runtime environment
# ============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash icn

# Create data directory
RUN mkdir -p /data && chown icn:icn /data

# Copy binaries from builder
COPY --from=builder /build/target/release/icnd /usr/local/bin/
COPY --from=builder /build/target/release/icnctl /usr/local/bin/

# Switch to non-root user
USER icn
WORKDIR /home/icn

# Set data directory
ENV ICN_DATA_DIR=/data

# Expose ports
# 4433/udp - QUIC peer transport
# 5050/tcp - RPC API
# 9100/tcp - Prometheus metrics
# 8080/tcp - Health checks
EXPOSE 4433/udp 5050/tcp 9100/tcp 8080/tcp

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Default command
ENTRYPOINT ["icnd"]
CMD ["--data-dir", "/data", "--log-level", "info"]
