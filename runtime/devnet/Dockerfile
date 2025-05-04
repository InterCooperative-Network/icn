# ---- Build stage ----
    FROM rust:latest as builder

    WORKDIR /app
    
    # Install build dependencies including cmake
    RUN apt-get update && \
        apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        git \
        cmake \
        ca-certificates \
        && rm -rf /var/lib/apt/lists/*
    
    # Copy the source code into the image
    COPY . .
    
    # Build the ICN Runtime
    RUN cargo build --release -p icn-covm
    
    # ---- Runtime stage ----
    FROM debian:bullseye-slim
    
    WORKDIR /app
    
    # Install runtime dependencies
    RUN apt-get update && \
        apt-get install -y --no-install-recommends \
        libssl1.1 \
        ca-certificates \
        curl \
        && rm -rf /var/lib/apt/lists/*
    
    # Copy the built binary from the builder stage
    COPY --from=builder /app/target/release/covm /usr/local/bin/icn-runtime
    
    # Prepare runtime directories
    RUN mkdir -p /data /config
    
    # Set the entrypoint
    ENTRYPOINT ["/usr/local/bin/icn-runtime"]
    
    # Default command
    CMD ["--help"]
    