FROM rust:1.71-slim-bullseye as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the source code
COPY . .

# Build the ICN Runtime
RUN cargo build --release -p icn-runtime-cli

# Runtime stage
FROM debian:bullseye-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libssl1.1 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary
COPY --from=builder /app/target/release/icn-runtime /usr/local/bin/

# Create directories
RUN mkdir -p /data /config

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/icn-runtime"]

# Default command
CMD ["--help"] 