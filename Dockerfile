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

# Copy workspace files (build context is already ./icn from docker-compose.yml)
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY bins ./bins

# Build release binaries
RUN cargo build --release --bins

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
COPY --from=builder /app/target/release/icnd /usr/local/bin/
COPY --from=builder /app/target/release/icnctl /usr/local/bin/

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
