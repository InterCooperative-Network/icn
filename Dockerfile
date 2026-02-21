# ICN Docker Image for Testing
# Multi-stage build for minimal image size

# Stage 1: Builder
FROM rust:slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy workspace files from repo root build context.
# Some crates in icn/ depend on top-level apps/* via relative paths.
COPY icn/Cargo.toml icn/Cargo.lock ./icn/
COPY icn/crates ./icn/crates
COPY icn/bins ./icn/bins
COPY icn/apps ./icn/apps
COPY apps ./apps

WORKDIR /app/icn

# Build-time provenance args
ARG GIT_SHA=unknown
ARG BUILD_TIME=unknown

# Build release binaries
# RUST_MIN_STACK: Increase rustc stack size to prevent SIGSEGV on complex crates
# -j 2: Limit parallelism to reduce peak memory usage
ENV RUST_MIN_STACK=33554432
ENV GIT_SHA=${GIT_SHA}
ENV BUILD_TIME=${BUILD_TIME}
RUN cargo build --release --bins -j 2

# Stage 2: Runtime (must match builder's glibc version)
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create icn user
RUN useradd -m -u 1000 -s /bin/bash icn

# Create data directory
RUN mkdir -p /data && chown icn:icn /data

# Copy binaries from builder
COPY --from=builder /app/icn/target/release/icnd /usr/local/bin/
COPY --from=builder /app/icn/target/release/icnctl /usr/local/bin/

# Set permissions
RUN chmod +x /usr/local/bin/icnd /usr/local/bin/icnctl

# Switch to icn user
USER icn

# Set working directory
WORKDIR /data

# Expose ports
# 5000-5010: P2P (configurable via config file)
# 9100: Prometheus metrics (icnd default)
# 8080: Gateway API (if enabled)
EXPOSE 5000-5010 9100 8080

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:9100/metrics || exit 1

# Default command
ENTRYPOINT ["icnd"]
CMD []
