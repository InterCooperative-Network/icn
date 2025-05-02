# Multi-stage Dockerfile for ICN Runtime (icn-covm-v3)

# ================ BUILD STAGE ================
FROM rust:1.75-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /usr/src/app

# Copy the entire project
COPY . .

# Build the release binary for the CLI target
RUN cargo build --release --bin covm

# ================ RUNTIME STAGE ================
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    openssl \
    && rm -rf /var/lib/apt/lists/*

# Create app directories
RUN mkdir -p /app/logs /app/data /app/scripts

# Set working directory
WORKDIR /app

# Copy binary from build stage
COPY --from=builder /usr/src/app/target/release/covm /usr/local/bin/

# Copy scripts and make them executable
COPY run_integration_node.sh monitor_integration.sh /app/scripts/
RUN chmod +x /app/scripts/*.sh

# Copy config directory
COPY config/ /app/config/

# Expose necessary ports
# 8080 - HTTP API
# 8090 - WebSocket
# 4001 - libp2p
EXPOSE 8080 8090 4001

# Set entrypoint to run the integration node script
ENTRYPOINT ["/app/scripts/run_integration_node.sh"] 