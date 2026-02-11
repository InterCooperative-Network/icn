# Trust Threshold Configuration

## Overview

The `min_trust_threshold` setting controls trust-gated TLS verification at the network transport layer. This setting determines the minimum trust score required for a peer to establish a QUIC/TLS connection.

## Configuration

Add to your `icn.toml` configuration file under the `[network]` section:

```toml
[network]
listen_addr = "0.0.0.0:7777"
rpc_port = 5601
mdns_enabled = true
bootstrap_peers = []
min_trust_threshold = 0.1  # Default: 0.1
```

## Trust Score Thresholds

| Threshold | Trust Class | Description |
|-----------|-------------|-------------|
| 0.0 | None (disabled) | Accept all authenticated DIDs. Trust-gated TLS is disabled but DID-TLS binding still provides cryptographic authentication. |
| 0.1 | Known | Require at least one trust edge. Peer must have at least one trust relationship. |
| 0.4 | Partner | Require "Partner" trust class. Peer is a known collaborator. |
| 0.7 | Federated | Require "Federated" trust class. Peer is highly trusted. |

## Use Cases

### Local Development (min_trust_threshold = 0.0)

For local testing with multiple nodes that don't have pre-established trust relationships:

```toml
[network]
min_trust_threshold = 0.0
```

**Benefits:**
- Nodes can connect immediately without trust setup
- Simplifies local development and testing
- Still maintains DID-TLS binding for authentication

**Security Note:** This is only recommended for controlled environments where all nodes are pre-authorized.

### Cooperative Deployments (min_trust_threshold = 0.0)

For closed cooperative networks where all nodes are known and trusted:

```toml
[network]
min_trust_threshold = 0.0
```

**Use When:**
- All nodes in the deployment are pre-authorized
- Network is isolated or behind firewall
- Nodes are managed by the same organization/cooperative
- Trust relationships are managed out-of-band

### Public Networks (min_trust_threshold = 0.1 or higher)

For networks that accept connections from external peers:

```toml
[network]
min_trust_threshold = 0.1  # or higher
```

**Benefits:**
- Protects against untrusted connections
- Enforces trust-based access control
- Only peers with established trust relationships can connect

**Recommended for:**
- Public-facing nodes
- Federation endpoints
- Nodes accepting external connections

## Deployment Examples

### Docker/Kubernetes Cooperative Deployment

The `deploy-coop.sh` script automatically sets `min_trust_threshold = 0.0` for cooperative deployments:

```bash
./deploy/k8s/multi-node/scripts/deploy-coop.sh alpha "Alpha Cooperative"
```

This allows cooperative nodes to connect without pre-established trust relationships.

### Manual Configuration

Edit `/etc/icn/icn.toml` or your config file:

```toml
[network]
listen_addr = "0.0.0.0:7777"
rpc_port = 5601
mdns_enabled = true
bootstrap_peers = []
min_trust_threshold = 0.0  # For cooperative deployment
```

Then restart the daemon:

```bash
sudo systemctl restart icnd
# or
kubectl rollout restart deployment/icn-alpha -n icn-coop-alpha
```

## Security Considerations

### What min_trust_threshold = 0.0 Still Provides

Even with trust-gated TLS disabled, you still get:

1. **DID-TLS Binding**: Every connection is authenticated to a specific DID using Ed25519 signatures
2. **Transport Encryption**: All connections use TLS 1.3 with QUIC
3. **Message Signing**: All messages are cryptographically signed by the sender
4. **Replay Protection**: Nonce-based replay guards prevent message replay attacks

### What Changes

With `min_trust_threshold = 0.0`:
- Any authenticated DID can establish a connection
- Trust scores are still computed and used for rate limiting
- Access control is enforced at the application layer (gossip topics, contracts, etc.)

### Best Practices

1. **Controlled Environments**: Use 0.0 only in trusted, controlled deployments
2. **Monitor Connections**: Even with 0.0, monitor who connects to your node
3. **Application-Layer Security**: Trust-gated gossip topics and contracts still enforce trust requirements
4. **Review Periodically**: Re-evaluate trust threshold as your network grows

## Troubleshooting

### Issue: Nodes reject each other with "insufficient trust"

**Symptoms:**
```
🔒 Connection rejected: DID did:icn:z... has insufficient trust (score: 0.000, required: 0.100)
```

**Solution:**
Set `min_trust_threshold = 0.0` in the network configuration for cooperative deployments.

### Issue: Want to re-enable trust gating

**Solution:**
1. Establish trust relationships between nodes:
   ```bash
   icnctl trust add <peer-did> --score 0.5
   ```

2. Update configuration to re-enable trust threshold:
   ```toml
   [network]
   min_trust_threshold = 0.1
   ```

3. Restart the daemon

## Related Documentation

- [ARCHITECTURE.md](../../ARCHITECTURE.md) - System architecture and trust model
- [production-hardening.md](../../security/production-hardening.md) - Security hardening measures
- [DEPLOYMENT_GUIDE.md](../../operations/deployment/deployment-guide.md) - Kubernetes deployment guide

## Version History

- **v0.1.0**: Trust threshold is hardcoded to 0.1
- **v0.2.0**: Trust threshold is configurable via `network.min_trust_threshold`
