# Multi-stage Dockerfile for ICN Runtime (icn-covm-v3)

# ================ BUILD STAGE ================
FROM rust:1.81-slim-bullseye as builder

WORKDIR /usr/src/covm

# Install dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Copy manifests and source code
COPY . .

# Build the application with release profile
RUN cargo build --release

# ================ RUNTIME STAGE ================
FROM debian:bullseye-slim

ARG APP=/usr/local/bin/icn-covm-v3

RUN apt-get update && \
    apt-get install -y ca-certificates tzdata libssl1.1 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /usr/src/covm/target/release/icn-covm-v3 ${APP}

# Set the working directory
WORKDIR /usr/local/bin

# Create a non-root user and switch to it
RUN groupadd -r covm && useradd -r -g covm covm
RUN chown -R covm:covm ${APP}
USER covm

# Set environment variables
ENV TZ=Etc/UTC \
    RUST_LOG=info

# Expose the API port
EXPOSE 3000

# Command to run the application
CMD ["icn-covm-v3"] 