# Federation Integration Guide

This guide provides detailed information on integrating with the ICN Federation system, configuring nodes, and managing federation operations.

## Table of Contents

1. [Node Setup and Configuration](#node-setup-and-configuration)
2. [TrustBundle Lifecycle](#trustbundle-lifecycle)
3. [Blob Replication](#blob-replication)
4. [Federation Health](#federation-health)
5. [Multi-Node Testing](#multi-node-testing)
6. [Troubleshooting](#troubleshooting)

## Node Setup and Configuration

### Node Roles

The federation consists of nodes with different roles, each with specific responsibilities:

- **Genesis**: The bootstrap node that initializes the federation; only one per federation
- **Validator**: Normal voting node in the federation that participates in consensus
- **Guardian**: Special node with mandate powers for constitutional enforcement
- **Observer**: Read-only node that replicates data but doesn't participate in consensus

### Configuration

Nodes are configured using environment variables and/or a configuration file (`runtime-config.toml`):

```toml
[federation]
# Federation node configuration
role = "validator"  # Possible values: genesis, validator, guardian, observer
bootstrap_peers = ["/ip4/1.2.3.4/tcp/4001"]
trust_sync_interval_sec = 60
max_peers = 25

[replication]
# Blob replication configuration
default_policy = "factor:3"  # Minimum replication factor
important_contexts = ["governance:5", "constitution:7"]  # Special context replication policies
max_blob_size = 10485760  # 10MB max blob size

[health]
# Health monitoring configuration
check_interval_sec = 30
metrics_enabled = true
```

### Environment Variables

Key environment variables include:

- `ICN_NODE_ROLE`: The role of this node (genesis, validator, guardian, observer)
- `ICN_BOOTSTRAP_PEER`: The multiaddress of a peer to bootstrap from
- `ICN_FEDERATION_TRUST_SYNC_INTERVAL`: Interval in seconds for TrustBundle synchronization
- `ICN_BLOB_MIN_REPLICATION`: Default minimum replication factor for blobs
- `ICN_HEALTH_CHECK_INTERVAL`: Interval in seconds for health checks

### Starting a Node

Nodes can be started with Docker:

```bash
docker run -d --name icn-runtime \
  -p 8080:8080 -p 4001:4001 \
  -v ./config:/etc/icn-runtime \
  -v ./data:/var/lib/icn-runtime \
  -e RUST_LOG=info \
  -e ICN_NODE_ROLE=validator \
  -e ICN_BOOTSTRAP_PEER=/ip4/bootstrap.icn.network/tcp/4001 \
  icn-runtime:latest
```

Or using Docker Compose:

```yaml
version: '3'
services:
  icn-runtime:
    image: icn-runtime:latest
    environment:
      - RUST_LOG=info
      - ICN_NODE_ROLE=validator
      - ICN_BOOTSTRAP_PEER=/ip4/bootstrap.icn.network/tcp/4001
    volumes:
      - ./config:/etc/icn-runtime
      - ./data:/var/lib/icn-runtime
    ports:
      - "8080:8080"  # API
      - "4001:4001"  # Federation/libp2p
```

## TrustBundle Lifecycle

The TrustBundle is the foundation of federation trust and synchronization, containing epoch information, DAG roots, and attestations.

### TrustBundle Generation

1. The federation epoch advances when consensus is reached on a new TrustBundle
2. The TrustBundle is created with:
   - A unique sequential epoch ID
   - Federation ID
   - DAG roots (content addresses of important DAG heads)
   - Attestations (verifiable credentials)

### TrustBundle Signing and Propagation

Signing a TrustBundle:

1. The bundle content hash is calculated with `TrustBundle::hash()`
2. Authorized Guardians sign the hash to form a quorum
3. Signatures are collected into a `QuorumProof`
4. The fully signed bundle is anchored in the DAG

Propagation happens automatically via:

- Direct request-response protocol (`/icn/trustbundle/1.0.0`)
- DAG synchronization (TrustBundles are recorded in the DAG)
- Regular synchronization intervals configured by `trust_sync_interval_sec`

### TrustBundle Verification

When a node receives a TrustBundle, it verifies:

1. The epoch ID is not outdated
2. All signers are authorized guardians 
3. No duplicate signers exist
4. Quorum has been reached
5. All signatures are cryptographically valid
6. DAG roots referenced in the bundle exist and are verified

Example verification code:

```rust
// Verify the received trust bundle
let current_epoch = federation_manager.get_latest_known_epoch().await?;
let current_time = SystemTime::now();

// Get the list of authorized guardians
let authorized_guardians = roles::get_authorized_guardians(&federation_id).await?;

// Verify the bundle
match trust_bundle.verify(&authorized_guardians, current_epoch, current_time).await {
    Ok(true) => {
        info!("TrustBundle verified successfully");
        // Store and process the bundle
        sync::store_trust_bundle(&trust_bundle, &storage).await?;
    },
    Ok(false) => {
        warn!("TrustBundle verification failed - invalid signatures");
        // Discard the bundle
    },
    Err(e) => {
        error!("TrustBundle verification error: {}", e);
        // Handle error (may be outdated, malformed, etc.)
    }
}
```

### TrustBundle Storage

Verified TrustBundles are stored with content-addressed keys:

- Key format: `trustbundle::{epoch_id}`
- Latest known epoch is tracked with key `federation::latest_epoch`
- Previous bundles are retained for history and audit

## Blob Replication

Blob storage provides content-addressed data storage with policies for replication.

### Replication Policies

Blobs can have different replication requirements:

- **Factor(n)**: Replicate to at least n nodes
- **Specific**: Replicate to specific nodes (by ID)
- **Context**: Apply context-specific policy (`governance:5` = governance context, 5 replicas)
- **None**: No replication required

Configuration in `runtime-config.toml`:

```toml
[replication]
default_policy = "factor:3"
important_contexts = [
  "governance:5",
  "constitution:7",
  "financials:5"
]
```

### Replication Process

1. A blob is stored and pinned on a node
2. Replication is triggered based on policy
3. The node identifies target peers using the policy
4. The blob is announced on the DHT
5. Replication requests are sent to target peers
6. Target peers fetch, verify, and store the blob
7. Replication status is tracked and reported

### Blob Status CLI

Check blob replication status with CLI:

```bash
# Check basic status
icn-runtime blob status bafybeihxrheji2kavf5x5q33jefctzecqbir5yunkqv2t7hewsba6uxrxq

# Show detailed replication info
icn-runtime blob status --verbose bafybeihxrheji2kavf5x5q33jefctzecqbir5yunkqv2t7hewsba6uxrxq

# Output in JSON format
icn-runtime blob status --json bafybeihxrheji2kavf5x5q33jefctzecqbir5yunkqv2t7hewsba6uxrxq
```

### De-replication on Node Removal

When a node is removed from the federation:

1. The node's role is revoked via a Guardian mandate
2. The TrustBundle is updated to exclude the node
3. Blob replication policies are recalculated
4. Under-replicated blobs are identified and re-replicated
5. The removed node's health status is tracked until blobs are migrated

## Federation Health

Comprehensive health monitoring ensures federation stability.

### Health Endpoints

- `GET /api/health`: Basic service health
- `GET /api/v1/federation/health`: Detailed federation health metrics
- `GET /api/v1/federation/status`: Federation status and role information
- `GET /api/v1/federation/diagnostic`: Comprehensive diagnostic information

Response includes:

- DAG anchor status
- TrustBundle epoch information
- Blob replication statistics
- Quorum health
- Peer connectivity status

Example health response:

```json
{
  "status": "ok",
  "federation": {
    "status": "ok",
    "epoch": 42,
    "connected_peers": 5,
    "replication_status": {
      "total_blobs": 100,
      "fully_replicated": 98,
      "in_progress": 2,
      "failed": 0,
      "completion_percentage": 98,
      "health_issues": []
    },
    "quorum_health": {
      "has_validator_quorum": true,
      "has_guardian_quorum": true,
      "validator_count": 3,
      "guardian_count": 2,
      "required_quorum": 2,
      "quorum_percentage": 100
    },
    "dag_anchor": {
      "head_cid": "bafybeiabc123...",
      "consistent_with_trust_bundle": true
    },
    "trust_bundle_status": {
      "epoch": 42,
      "created_at": "2023-06-01T12:00:00Z",
      "node_count": 5,
      "signature_count": 3
    }
  }
}
```

### Monitoring Dashboards

A Prometheus/Grafana monitoring stack is provided for federation health:

- **Prometheus**: Collects metrics from nodes
- **Grafana**: Visualizes federation health
- **Federation Dashboard**: Shows real-time status

Access the dashboards:
- Grafana: http://localhost:3001 (default: admin/admin)
- Federation Monitor: http://localhost:3002

## Multi-Node Testing

### Docker Compose Integration Environment

We provide a comprehensive Docker Compose environment for testing:

```bash
# Start the standard federation environment
docker-compose -f docker-compose.integration.yml up -d

# Start including the test node
docker-compose -f docker-compose.integration.yml --profile manual up -d

# Run integration tests
docker-compose -f docker-compose.integration.yml --profile test up
```

The environment includes:
- Genesis node
- Multiple validator nodes
- Guardian node
- Observer node
- Misconfigured node (for testing)
- Rejoin test node
- Integration test suite

### Testing Node Rejoins and Catchup

To test node rejoining:

1. Start with the rejoin node down
2. Generate content on other nodes
3. Start the rejoin node:
   ```bash
   docker-compose -f docker-compose.integration.yml up -d icn-runtime-rejoin
   ```
4. Monitor catchup progress:
   ```bash
   curl http://localhost:8086/api/v1/federation/status
   ```

### Testing Cross-Node Proposal Flow

The test environment supports tracing proposals across nodes:

1. Submit a proposal to Node A:
   ```bash
   curl -X POST http://localhost:8080/api/v1/proposal -d '{...}'
   ```
2. Verify propagation to Node B:
   ```bash
   curl http://localhost:8081/api/v1/dag/{CID}
   ```
3. Confirm TrustBundle inclusion and DAG anchoring on Node C:
   ```bash
   curl http://localhost:8082/api/v1/federation/status
   ```

## Troubleshooting

### Common Issues

1. **Node not syncing TrustBundles**
   - Check connectivity to bootstrap peer
   - Verify trust_sync_interval_sec is non-zero
   - Check logs for verification errors
   - Ensure node has proper role permissions

2. **Blob replication issues**
   - Verify blob exists on source node
   - Check replication policy configuration
   - Ensure sufficient nodes are available for policy
   - Verify network connectivity between nodes

3. **DAG inconsistency**
   - Verify DAG roots in TrustBundle
   - Check for TrustBundle verification errors
   - Initiate forced resync with newer epoch

4. **Quorum not achieved**
   - Verify enough validators are connected
   - Check Guardian authorizations
   - Ensure nodes have correct roles assigned

### Recovery Procedures

1. **Reset and resync node**:
   ```bash
   # Stop the node
   docker stop icn-runtime
   
   # Remove existing data (optional - use with caution)
   rm -rf ./data/*
   
   # Restart the node
   docker start icn-runtime
   ```

2. **Manual TrustBundle sync**:
   ```bash
   # Export from a healthy node
   curl http://healthynode:8080/api/v1/federation/trust-bundle/latest > trustbundle.json
   
   # Import to the problematic node
   curl -X POST http://problemnode:8080/api/v1/federation/trust-bundle -d @trustbundle.json
   ```

3. **Verify blob replication**:
   ```bash
   # Check replication status
   icn-runtime blob status <CID>
   
   # Force replication
   curl -X POST http://localhost:8080/api/v1/blob/<CID>/replicate
   ``` 