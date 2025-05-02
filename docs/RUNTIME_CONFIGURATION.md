# ICN Runtime Configuration Guide

This document describes the available configuration parameters for deploying and operating an ICN Runtime node.

## Overview

The ICN Runtime is a highly configurable system that can be tailored to different deployment scenarios. This guide covers:

1. Core configuration parameters
2. Resource limits and quotas
3. Network settings
4. Storage backend configuration
5. Federation settings
6. Security considerations
7. Example deployment configurations

## Configuration Format

The ICN Runtime configuration is specified in TOML format. The configuration file is typically named `runtime-config.toml` and must be provided at startup.

## Core Configuration

### Basic Parameters

```toml
[runtime]
# Unique identifier for this runtime node
node_id = "icn-runtime-validator-1"

# Runtime version (used for compatibility checks)
version = "1.0.0"

# Operational mode: "validator", "observer", "guardian", or "development"
mode = "validator"

# Directory for runtime data
data_dir = "/var/lib/icn-runtime"

# Maximum system memory usage (in MiB)
max_memory_mib = 2048
```

### Identity Configuration

```toml
[identity]
# DID for this node (must be unique)
did = "did:icn:node:validator1"

# Path to the node's private key file (must be secure)
key_file = "/etc/icn-runtime/node-key.pem"

# Key password (can also be provided via environment variable ICN_KEY_PASSWORD)
# key_password = "secure-password"  # NOT RECOMMENDED in config file

# Genesis identity file (for bootstrap nodes)
genesis_identity_file = "/etc/icn-runtime/genesis-identity.json"
```

## Resource Limits

```toml
[resources]
# Maximum CPU usage (percentage of available cores, 0-100)
max_cpu_percent = 80

# Maximum storage usage
max_storage_gb = 50

# Maximum concurrent VM executions
max_concurrent_vms = 8

# Memory limit per VM instance (MiB)
vm_memory_limit_mib = 128

# Maximum VM execution time (seconds)
vm_execution_timeout_sec = 30

# Rate limiting for VM execution requests (per minute)
vm_execution_rate_limit = 120

# WASM module size limit (MB)
max_wasm_module_size_mb = 10
```

## Network Configuration

```toml
[network]
# Listen address for the HTTP API
http_listen = "0.0.0.0:8080"

# Enable TLS for HTTP API
http_tls_enabled = true

# TLS certificate file
http_tls_cert_file = "/etc/icn-runtime/tls/cert.pem"

# TLS key file
http_tls_key_file = "/etc/icn-runtime/tls/key.pem"

# Federation P2P listen addresses (multiple can be specified)
p2p_listen = [
  "/ip4/0.0.0.0/tcp/4001",
  "/ip6/::/tcp/4001"
]

# External addresses for advertising to peers
external_addresses = [
  "/ip4/203.0.113.1/tcp/4001"
]

# Enable metrics endpoint
metrics_enabled = true

# Metrics listen address
metrics_listen = "127.0.0.1:9090"
```

## Storage Configuration

The ICN Runtime supports multiple storage backends.

### In-Memory Storage (Development Only)

```toml
[storage]
# Storage backend type
backend = "memory"

# Memory storage capacity limit (MB)
capacity_mb = 1024
```

### File System Storage

```toml
[storage]
# Storage backend type
backend = "filesystem"

# Base directory for file storage
base_dir = "/var/lib/icn-runtime/storage"

# Maximum storage size (GB)
max_size_gb = 100

# Blob garbage collection interval (seconds)
gc_interval_sec = 3600

# Path to SQLite metadata database
metadata_db = "/var/lib/icn-runtime/metadata.db"
```

### S3-Compatible Storage

```toml
[storage]
# Storage backend type
backend = "s3"

# S3 bucket name
bucket = "icn-runtime-data"

# S3 endpoint URL
endpoint = "https://s3.amazonaws.com"

# AWS region
region = "us-west-2"

# S3 prefix (optional)
prefix = "node1/"

# Authentication via environment variables:
# AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY
```

## Federation Configuration

```toml
[federation]
# Bootstrap period (seconds)
bootstrap_period_sec = 30

# Peer synchronization interval (seconds)
peer_sync_interval_sec = 60

# TrustBundle synchronization interval (seconds)
trust_bundle_sync_interval_sec = 300

# Maximum number of peers to maintain
max_peers = 25

# Bootstrap peers to connect to
bootstrap_peers = [
  "/ip4/203.0.113.2/tcp/4001/p2p/QmbootstrapNodeId1",
  "/ip4/203.0.113.3/tcp/4001/p2p/QmbootstrapNodeId2"
]

# Content replication factors
default_replication_factor = 3

# Default lifetime for pinned content (seconds, 0 means indefinite)
default_pin_lifetime_sec = 0
```

## Governance Configuration

```toml
[governance]
# Minimum voting period for proposals (seconds)
min_voting_period_sec = 86400  # 1 day

# Maximum voting period for proposals (seconds)
max_voting_period_sec = 604800  # 1 week

# Required quorum percentage (0-100)
quorum_percent = 66

# Approval threshold percentage (0-100)
approval_threshold_percent = 51

# Emergency proposal threshold
emergency_proposal_threshold_percent = 75

# Proposals limit per hour per identity
proposals_rate_limit = 5
```

## Logging Configuration

```toml
[logging]
# Log level: "error", "warn", "info", "debug", "trace"
level = "info"

# Log format: "text" or "json"
format = "json"

# Log output: "stdout", "file", or "both"
output = "both"

# Log file path (if output is "file" or "both")
file_path = "/var/log/icn-runtime/runtime.log"

# Maximum log file size before rotation (MB)
max_file_size_mb = 100

# Number of rotated log files to keep
max_file_count = 10
```

## Security Settings

```toml
[security]
# Enable VM sandbox 
sandbox_enabled = true

# VM sandbox type: "wasm", "native-isolated", or "docker"
sandbox_type = "wasm"

# Enable access control 
access_control_enabled = true

# Security token required for admin operations
admin_token_hash = "sha256:HASH_OF_ADMIN_TOKEN"

# Remote hosts allowed to connect to admin endpoints
admin_allowed_hosts = ["127.0.0.1", "10.0.0.1"]

# Enable WebAssembly features (comma-separated list)
wasm_features = "threads,simd,bulk-memory"
```

## Example Deployment Configurations

### Minimal Validator Node

```toml
[runtime]
node_id = "icn-validator-1"
mode = "validator"
data_dir = "/var/lib/icn-runtime"
max_memory_mib = 2048

[identity]
did = "did:icn:node:validator1"
key_file = "/etc/icn-runtime/node-key.pem"

[network]
http_listen = "0.0.0.0:8080"
p2p_listen = ["/ip4/0.0.0.0/tcp/4001"]
external_addresses = ["/ip4/203.0.113.1/tcp/4001"]

[storage]
backend = "filesystem"
base_dir = "/var/lib/icn-runtime/storage"
max_size_gb = 100

[federation]
bootstrap_peers = [
  "/ip4/203.0.113.2/tcp/4001/p2p/QmbootstrapNodeId1",
  "/ip4/203.0.113.3/tcp/4001/p2p/QmbootstrapNodeId2"
]

[logging]
level = "info"
output = "both"
file_path = "/var/log/icn-runtime/runtime.log"
```

### High-Performance Guardian Node

```toml
[runtime]
node_id = "icn-guardian-1"
mode = "guardian"
data_dir = "/data/icn-runtime"
max_memory_mib = 8192

[identity]
did = "did:icn:node:guardian1"
key_file = "/secrets/icn-runtime/node-key.pem"

[resources]
max_cpu_percent = 90
max_storage_gb = 500
max_concurrent_vms = 32
vm_memory_limit_mib = 512

[network]
http_listen = "0.0.0.0:8080"
http_tls_enabled = true
http_tls_cert_file = "/secrets/icn-runtime/tls/cert.pem"
http_tls_key_file = "/secrets/icn-runtime/tls/key.pem"
p2p_listen = [
  "/ip4/0.0.0.0/tcp/4001",
  "/ip6/::/tcp/4001"
]
external_addresses = [
  "/ip4/203.0.113.10/tcp/4001",
  "/dns4/guardian1.icn-example.org/tcp/4001"
]
metrics_enabled = true
metrics_listen = "0.0.0.0:9090"

[storage]
backend = "s3"
bucket = "icn-guardian-data"
endpoint = "https://s3.amazonaws.com"
region = "us-west-2"

[federation]
max_peers = 50
default_replication_factor = 5

[logging]
level = "info"
format = "json"
output = "both"
file_path = "/logs/icn-runtime/runtime.log"
max_file_size_mb = 500
max_file_count = 30

[security]
sandbox_enabled = true
sandbox_type = "wasm"
access_control_enabled = true
```

### Development/Testing Node

```toml
[runtime]
node_id = "icn-dev-1"
mode = "development"
data_dir = "./data"
max_memory_mib = 1024

[identity]
did = "did:icn:node:dev1"
key_file = "./keys/node-key.pem"

[network]
http_listen = "127.0.0.1:8080"
p2p_listen = ["/ip4/127.0.0.1/tcp/4001"]

[storage]
backend = "memory"
capacity_mb = 1024

[federation]
bootstrap_period_sec = 5
peer_sync_interval_sec = 10
trust_bundle_sync_interval_sec = 30

[logging]
level = "debug"
output = "stdout"
```

## Docker Deployment Example

This section outlines how to configure the ICN Runtime within a Docker environment:

```dockerfile
FROM icn-runtime:latest

# Copy configuration
COPY runtime-config.toml /etc/icn-runtime/config.toml

# Set environment variables for secrets
ENV ICN_KEY_PASSWORD=your-secure-password
ENV AWS_ACCESS_KEY_ID=your-access-key
ENV AWS_SECRET_ACCESS_KEY=your-secret-key

# Expose ports
EXPOSE 8080 4001 9090

# Set up volumes
VOLUME ["/var/lib/icn-runtime", "/var/log/icn-runtime"]

# Start the runtime
CMD ["icn-runtime", "--config", "/etc/icn-runtime/config.toml"]
```

Docker Compose example:

```yaml
version: '3'
services:
  icn-runtime:
    image: icn-runtime:latest
    volumes:
      - ./config:/etc/icn-runtime
      - ./data:/var/lib/icn-runtime
      - ./logs:/var/log/icn-runtime
    ports:
      - "8080:8080"
      - "4001:4001"
      - "9090:9090"
    environment:
      - ICN_KEY_PASSWORD=your-secure-password
      - AWS_ACCESS_KEY_ID=your-access-key
      - AWS_SECRET_ACCESS_KEY=your-secret-key
    restart: unless-stopped
```

## Environment Variables

The following environment variables can be used to override configuration file settings:

| Variable | Description |
|----------|-------------|
| `ICN_CONFIG_FILE` | Path to configuration file |
| `ICN_LOG_LEVEL` | Logging level |
| `ICN_DATA_DIR` | Data directory |
| `ICN_KEY_PASSWORD` | Password for node key file |
| `ICN_ADMIN_TOKEN` | Admin token for privileged operations |
| `ICN_HTTP_PORT` | HTTP API port number |
| `ICN_MAX_MEMORY` | Maximum memory usage in MiB |
| `AWS_ACCESS_KEY_ID` | AWS access key for S3 storage |
| `AWS_SECRET_ACCESS_KEY` | AWS secret key for S3 storage |
| `AWS_REGION` | AWS region for S3 storage |

## See Also

- [Core VM Documentation](./CORE_VM.md)
- [DAG System](./DAG_SYSTEM.md)
- [Federation Protocol](./FEDERATION_PROTOCOL.md)
- [Governance Kernel](./GOVERNANCE_KERNEL.md) 