# ICN Runtime Deployment Guide

This document provides a comprehensive guide for deploying and validating the ICN Runtime in different environments.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Stress Testing](#stress-testing)
3. [Security Review](#security-review)
4. [Deployment Preparation](#deployment-preparation)
5. [Environment Configuration](#environment-configuration)
6. [Production Deployment](#production-deployment)
7. [Monitoring](#monitoring)
8. [Troubleshooting](#troubleshooting)

## Prerequisites

Before deploying the ICN Runtime, ensure you have the following:

- Rust toolchain (1.63.0+)
- OpenSSL development libraries
- libp2p dependencies
- Sufficient disk space for storage
- Linux, macOS, or Windows with WSL

### Installing Dependencies

On Ubuntu/Debian:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev curl git
```

On macOS:

```bash
brew install openssl pkg-config
```

Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Stress Testing

Before deploying to production, run the stress tests to verify the runtime's stability under load:

```bash
# Make the script executable
chmod +x run_stress_tests.sh

# Run all stress tests
./run_stress_tests.sh

# Run specific tests
./run_stress_tests.sh governance federation
```

The stress tests cover:
- Governance operations (proposals, voting)
- Federation protocol (TrustBundle synchronization)
- DAG operations
- Concurrent operations
- Resource utilization

### Interpreting Stress Test Results

The stress tests output performance metrics including:
- Average operation time
- p50, p95, and p99 percentiles
- Operations per second
- Resource utilization (CPU, memory)

Look for any anomalies or bottlenecks that might affect production performance.

## Security Review

Perform a security review using the provided checklist:

```bash
# Install security tools
cargo install cargo-audit
cargo install cargo-deny

# Run security audit
cargo audit
```

Review the [Security Review Checklist](SECURITY_REVIEW.md) and ensure all items are addressed before production deployment.

## Deployment Preparation

Use the preparation script to build, test, and configure the runtime:

```bash
# First ensure the bin directory exists
mkdir -p bin

# Make the script executable
chmod +x bin/prepare_runtime.sh

# For development environment
./bin/prepare_runtime.sh development

# For testnet environment
./bin/prepare_runtime.sh testnet ./testnet-config

# For production environment
./bin/prepare_runtime.sh production /etc/icn-runtime
```

The preparation script performs these steps:
1. Builds the runtime in release mode
2. Runs tests to ensure functionality
3. Runs a security audit
4. Creates appropriate configuration for the target environment
5. Generates cryptographic keys if needed
6. Sets up the directory structure
7. Sets appropriate file permissions
8. Creates startup scripts
9. Creates systemd service files (for production)

## Environment Configuration

### Development Environment

The development configuration:
- Uses in-memory storage
- Runs on localhost
- Enables debug logging
- Uses simplified network settings

Starting the development environment:

```bash
./config/start-development.sh
```

### Testnet Environment

The testnet configuration:
- Uses filesystem storage
- Binds to all network interfaces
- Connects to testnet bootstrap nodes
- Uses moderate resource limits

Starting the testnet environment:

```bash
./testnet-config/start-testnet.sh
```

### Production Environment

The production configuration:
- Uses robust filesystem or S3 storage
- Enforces strict resource limits
- Enables TLS for HTTP API
- Implements security hardening
- Configures proper log rotation

## Production Deployment

For production deployment, follow these additional steps:

1. **Create a dedicated user**:
   ```bash
   sudo useradd -r -s /sbin/nologin icn-runtime
   ```

2. **Install the binary**:
   ```bash
   sudo cp ./target/release/icn-runtime /usr/local/bin/
   sudo chmod 755 /usr/local/bin/icn-runtime
   ```

3. **Install configuration**:
   ```bash
   sudo mkdir -p /etc/icn-runtime
   sudo cp ./config/runtime-config-production.toml /etc/icn-runtime/
   sudo chmod 640 /etc/icn-runtime/runtime-config-production.toml
   sudo chown root:icn-runtime /etc/icn-runtime/runtime-config-production.toml
   ```

4. **Copy TLS certificates** (if using TLS):
   ```bash
   sudo mkdir -p /etc/icn-runtime/tls
   sudo cp ./cert.pem /etc/icn-runtime/tls/
   sudo cp ./key.pem /etc/icn-runtime/tls/
   sudo chmod 600 /etc/icn-runtime/tls/key.pem
   sudo chmod 644 /etc/icn-runtime/tls/cert.pem
   sudo chown -R root:icn-runtime /etc/icn-runtime/tls
   ```

5. **Create required directories**:
   ```bash
   sudo mkdir -p /var/lib/icn-runtime
   sudo mkdir -p /var/log/icn-runtime
   sudo chown -R icn-runtime:icn-runtime /var/lib/icn-runtime
   sudo chown -R icn-runtime:icn-runtime /var/log/icn-runtime
   ```

6. **Install systemd service**:
   ```bash
   sudo cp ./config/icn-runtime.service /etc/systemd/system/
   sudo systemctl daemon-reload
   sudo systemctl enable icn-runtime.service
   sudo systemctl start icn-runtime.service
   ```

7. **Verify deployment**:
   ```bash
   sudo systemctl status icn-runtime.service
   ```

### Using Docker

A `Dockerfile` is available for containerized deployment:

```bash
# Build Docker image
docker build -t icn-runtime:latest .

# Run with appropriate volume mounts
docker run -d \
  --name icn-runtime \
  -v ./config:/etc/icn-runtime \
  -v ./data:/var/lib/icn-runtime \
  -v ./logs:/var/log/icn-runtime \
  -p 8080:8080 \
  -p 4001:4001 \
  icn-runtime:latest
```

For Docker Compose deployment, use the provided `docker-compose.yml` file:

```bash
docker-compose up -d
```

## Monitoring

### Logs

Access logs based on your configuration:

```bash
# For stdout logging
journalctl -u icn-runtime.service

# For file logging
tail -f /var/log/icn-runtime/runtime.log
```

### Metrics

The runtime exposes Prometheus metrics when enabled:

1. Configure metrics in your runtime configuration:
   ```toml
   [network]
   metrics_enabled = true
   metrics_listen = "127.0.0.1:9090"
   ```

2. Set up Prometheus to scrape these metrics:
   ```yaml
   scrape_configs:
     - job_name: 'icn-runtime'
       static_configs:
         - targets: ['localhost:9090']
   ```

3. Configure Grafana dashboards to visualize the metrics

Key metrics to monitor:
- VM execution rate and timing
- Storage usage
- Network peer count
- TrustBundle synchronization frequency
- Proposal and voting rates
- Resource utilization (CPU, memory)

## Troubleshooting

### Common Issues

#### Federation Connectivity Issues

If nodes cannot connect to the federation:

1. Verify network configuration:
   ```bash
   # Check if the node is listening on the configured address
   netstat -tulpn | grep 4001
   ```

2. Verify bootstrap peer configuration:
   ```bash
   # Check config file
   grep bootstrap_peers /etc/icn-runtime/runtime-config-production.toml
   ```

3. Check firewall settings:
   ```bash
   # Ensure port 4001 is open for libp2p communication
   sudo ufw status
   ```

#### Storage Issues

If experiencing storage problems:

1. Check permissions:
   ```bash
   ls -la /var/lib/icn-runtime
   ```

2. Verify disk space:
   ```bash
   df -h /var/lib
   ```

3. Check storage backend configuration:
   ```bash
   grep -A 10 '\[storage\]' /etc/icn-runtime/runtime-config-production.toml
   ```

#### High CPU/Memory Usage

If experiencing resource issues:

1. Check resource limits in configuration:
   ```bash
   grep -A 10 '\[resources\]' /etc/icn-runtime/runtime-config-production.toml
   ```

2. Monitor resource usage:
   ```bash
   top -p $(pgrep -f icn-runtime)
   ```

3. Consider adjusting resource limits or enabling more constraints in the configuration

### Getting Support

For additional help:

- Review the documentation in the `docs/` directory
- Check the logs for specific error messages
- Create an issue in the repository with detailed information about the problem

## Further Reading

- [Runtime Configuration Guide](RUNTIME_CONFIGURATION.md)
- [Federation Protocol Documentation](FEDERATION_PROTOCOL.md)
- [Events and Credentials Documentation](EVENTS_CREDENTIALS.md)
- [Security Review Checklist](SECURITY_REVIEW.md) 