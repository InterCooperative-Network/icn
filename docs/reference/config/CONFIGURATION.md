# ICN Configuration Management Guide

**Version**: 1.0  
**Last Updated**: 2025-12-16  
**Status**: Complete  

---

## Overview

ICN uses TOML configuration files for all node settings. This guide covers configuration management, validation, and best practices.

---

## Configuration Files

### Default Locations

1. **Command-line specified**: `icnd --config /path/to/icn.toml`
2. **Home directory**: `~/.icn/icn.toml`
3. **System-wide**: `/etc/icn/icn.toml`
4. **Current directory**: `./icn.toml`

ICN searches in this order and uses the first file found.

### Example Configuration

See `config/icn.toml.example` for a fully documented example with all available options.

---

## Configuration Validation

### JSON Schema

A JSON Schema is provided for validation: `config/icn-config.schema.json`

### Validation Tool

Use the provided Python script to validate configurations:

```bash
# Install dependencies
pip install jsonschema toml

# Validate configuration
python3 scripts/validate-config.py config/icn.toml

# Validate with verbose output
python3 scripts/validate-config.py --verbose config/icn.toml

# Validate JSON configuration
python3 scripts/validate-config.py --format json config/icn.json
```

### IDE Integration

Configure your IDE to validate TOML files:

**VS Code** (`settings.json`):
```json
{
  "evenBetterToml.schema.associations": {
    "icn.toml": "file:///path/to/icn/config/icn-config.schema.json"
  }
}
```

**IntelliJ/PyCharm**:
1. Settings → Languages & Frameworks → Schemas and DTDs
2. Add JSON Schema → File
3. Select `config/icn-config.schema.json`
4. Add file pattern: `**/icn*.toml`

---

## Configuration Sections

### 1. Node Configuration

```toml
# Data directory for all node data
data_dir = "/var/lib/icn"
```

**Best Practices**:
- Use absolute paths
- Ensure write permissions
- Consider disk space (ledger growth)

### 2. Network Configuration

```toml
[network]
listen_addr = "0.0.0.0:7777"
rpc_port = 5601
mdns_enabled = true
bootstrap_peers = []
min_trust_threshold = 0.1
```

**Security Considerations**:
- Set `min_trust_threshold >= 0.1` for public networks
- Disable mDNS (`mdns_enabled = false`) in production
- Use firewall rules to restrict `listen_addr`

**Bootstrap Peers**:
```toml
bootstrap_peers = [
    "icn://did:icn:z6Mk...@203.0.113.50:7777",
    "icn://did:icn:z6Mk...@203.0.113.51:7777"
]
```

### 3. Observability

```toml
[observability]
metrics_port = 9100
health_port = 8080
log_level = "info"
```

**Log Levels**:
- `trace`: Very verbose (development only)
- `debug`: Detailed debugging information
- `info`: Normal operations (recommended)
- `warn`: Warning messages only
- `error`: Errors only (not recommended)

### 4. Rate Limiting

```toml
[rate_limiting]
enabled = true
refill_interval_ms = 100

[rate_limiting.isolated]
max_messages_per_second = 10
burst_capacity = 2

[rate_limiting.known]
max_messages_per_second = 50
burst_capacity = 10

[rate_limiting.partner]
max_messages_per_second = 100
burst_capacity = 20

[rate_limiting.federated]
max_messages_per_second = 200
burst_capacity = 50
```

**Tuning Guidelines**:
- **Isolated**: Strict limits for untrusted peers
- **Known**: Moderate limits for basic trust
- **Partner**: Generous limits for collaborators
- **Federated**: High limits for trusted members

### 5. Topology (Optional)

```toml
[topology]
region = "us-west"
cluster_id = "sfo-1"
role = "edge"

[topology.neighbor_limits]
max_local_cluster = 50
max_regional = 30
max_backbone = 20
max_trusted = 100

[topology.fanout]
local_cluster = 8
regional = 6
global = 4
```

**When to Use**:
- Large networks (100+ nodes)
- Geographic distribution
- Bandwidth optimization

**When to Skip**:
- Small cooperatives (<50 nodes)
- Single geographic location
- Simple deployments

### 6. Federation

```toml
[federation]
enabled = true
network_name = "icn-mainnet"
bootstrap_peer_trust = 0.3
auto_accept_invites = false
min_invite_trust = 0.5
max_federations = 10
announce_public_addr = true

[federation.retry]
max_retries = 5
initial_delay_secs = 1
max_delay_secs = 60
reconnect_interval_secs = 30
```

**Security**:
- Set `auto_accept_invites = false` initially
- Use `min_invite_trust >= 0.5` if enabling auto-accept
- Monitor `max_federations` to prevent exhaustion

### 7. Gateway API

```toml
[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
token_expiry_hours = 24
challenge_ttl_minutes = 5
jwt_secret = ""  # Use environment variable!

[gateway.rate_limiting]
capacity = 100
refill_rate = 10
cost_per_request = 1
```

**Security Critical**:
- ⚠️ **NEVER** commit `jwt_secret` to version control
- Use environment variable: `ICN_GATEWAY_JWT_SECRET`
- Bind to `127.0.0.1` and use reverse proxy
- Enable TLS in reverse proxy (nginx/Apache)

**Generate JWT Secret**:
```bash
openssl rand -base64 32
```

### 8. Privacy (Optional)

```toml
[privacy]
enabled = true
onion_routing_enabled = true
onion_hops = 2
min_relay_trust = 0.3
topic_encryption_enabled = false
traffic_obfuscation_enabled = false
padding_target = 0
max_delay_ms = 0
cover_traffic_rate = 0
```

**Performance Impact**:
- Onion routing: +100-500ms latency per hop
- Traffic obfuscation: +10-50ms latency
- Cover traffic: Bandwidth cost

### 9. Cooperative

```toml
[cooperative]
treasury_did = "did:icn:z6Mk..."
name = "My Cooperative"
description = "A community cooperative"
```

---

## Environment Variables

Environment variables override configuration file settings:

| Variable | Configuration | Example |
|----------|---------------|---------|
| `ICN_DATA_DIR` | `data_dir` | `/var/lib/icn` |
| `ICN_NETWORK_LISTEN_ADDR` | `network.listen_addr` | `0.0.0.0:7777` |
| `ICN_GATEWAY_JWT_SECRET` | `gateway.jwt_secret` | `<secret>` |
| `ICN_LOG_LEVEL` | `observability.log_level` | `info` |

**Precedence** (highest to lowest):
1. Command-line arguments
2. Environment variables
3. Configuration file
4. Default values

---

## Configuration Templates

### Development

```toml
data_dir = "./.icn-dev"

[network]
listen_addr = "127.0.0.1:7777"
mdns_enabled = true
min_trust_threshold = 0.0

[observability]
log_level = "debug"

[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
jwt_secret = "dev-secret-change-in-production"
```

### Production (Small Cooperative)

```toml
data_dir = "/var/lib/icn"

[network]
listen_addr = "0.0.0.0:7777"
mdns_enabled = false
bootstrap_peers = ["icn://..."]
min_trust_threshold = 0.1

[observability]
metrics_port = 9100
health_port = 8080
log_level = "info"

[rate_limiting]
enabled = true

[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
# jwt_secret loaded from environment

[cooperative]
name = "My Coop"
```

### Production (Federated Network)

```toml
data_dir = "/var/lib/icn"

[network]
listen_addr = "0.0.0.0:7777"
mdns_enabled = false
bootstrap_peers = ["icn://..."]
min_trust_threshold = 0.3

[observability]
metrics_port = 9100
log_level = "info"

[topology]
region = "us-west"
cluster_id = "sfo-1"
role = "edge"

[federation]
enabled = true
network_name = "icn-mainnet"
auto_accept_invites = false
min_invite_trust = 0.6

[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
token_expiry_hours = 12
```

---

## Secrets Management

### DO NOT Store Secrets in Config Files

❌ **Never do this**:
```toml
[gateway]
jwt_secret = "my-secret-key"  # INSECURE!
```

✅ **Always do this**:
```bash
# Set environment variable
export ICN_GATEWAY_JWT_SECRET=$(openssl rand -base64 32)

# Or use systemd EnvironmentFile
# /etc/icn/icnd.env:
ICN_GATEWAY_JWT_SECRET=<generated-secret>
```

### Secrets Management Tools

**Kubernetes**:
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: icn-secrets
stringData:
  jwt-secret: <base64-encoded-secret>
---
apiVersion: v1
kind: Pod
spec:
  containers:
  - name: icnd
    env:
    - name: ICN_GATEWAY_JWT_SECRET
      valueFrom:
        secretKeyRef:
          name: icn-secrets
          key: jwt-secret
```

**HashiCorp Vault**:
```bash
# Store secret
vault kv put secret/icn/jwt-secret value="$(openssl rand -base64 32)"

# Retrieve and use
export ICN_GATEWAY_JWT_SECRET=$(vault kv get -field=value secret/icn/jwt-secret)
```

**AWS Secrets Manager**:
```bash
# Store secret
aws secretsmanager create-secret \
  --name icn/jwt-secret \
  --secret-string "$(openssl rand -base64 32)"

# Retrieve in application
aws secretsmanager get-secret-value \
  --secret-id icn/jwt-secret \
  --query SecretString \
  --output text
```

---

## Configuration Migration

### Version Compatibility

ICN follows semantic versioning. Configuration changes:

| Version Change | Compatibility |
|---------------|---------------|
| Patch (0.1.0 → 0.1.1) | Fully compatible |
| Minor (0.1.x → 0.2.0) | Backward compatible |
| Major (0.x.y → 1.0.0) | May require migration |

### Migration Checklist

When upgrading ICN:

1. **Backup configuration**: `cp icn.toml icn.toml.backup`
2. **Review CHANGELOG**: Check for config changes
3. **Validate**: `python3 scripts/validate-config.py icn.toml`
4. **Test**: Run with `--dry-run` if available
5. **Monitor**: Watch logs after upgrade

---

## Troubleshooting

### Configuration Not Loading

**Symptom**: ICN uses default values instead of config

**Solutions**:
1. Check file path: `icnd --config /path/to/icn.toml`
2. Verify file permissions: `ls -l icn.toml`
3. Enable debug logging: `ICN_LOG_LEVEL=debug icnd`

### Validation Fails

**Symptom**: `validate-config.py` reports errors

**Solutions**:
1. Check for typos in section names
2. Verify data types (string vs. integer)
3. Check required fields are present
4. Review error message carefully

### Gateway Won't Start

**Symptom**: Gateway enabled but not responding

**Common Causes**:
1. Missing `jwt_secret`: Set via environment variable
2. Port already in use: Check with `netstat -tlnp | grep 8080`
3. Firewall blocking: `sudo ufw allow 8080/tcp`

### Network Connectivity Issues

**Symptom**: No peers connecting

**Solutions**:
1. Check firewall: `sudo ufw status`
2. Verify `listen_addr` is correct
3. Test bootstrap peers: `telnet peer.example.com 7777`
4. Lower `min_trust_threshold` temporarily

---

## Best Practices

### 1. Configuration as Code

- Store configurations in version control
- Use separate configs for dev/staging/prod
- Remove secrets before committing
- Document custom settings

### 2. Regular Validation

```bash
# Add to CI pipeline
python3 scripts/validate-config.py config/*.toml
```

### 3. Monitoring

Monitor configuration drift:
- Alert on unexpected changes
- Track configuration versions
- Audit configuration access

### 4. Documentation

Document custom settings:
```toml
# Custom rate limit for our high-traffic network
# Increased after load testing on 2025-12-01
[rate_limiting.partner]
max_messages_per_second = 150  # Was: 100
```

### 5. Testing

Test configuration changes:
1. Validate with schema
2. Test in staging environment
3. Monitor metrics after deployment
4. Have rollback plan ready

---

## Appendix: Full Configuration Reference

See `config/icn.toml.example` for complete reference with all options documented.

---

## Support

- **Configuration Issues**: https://github.com/InterCooperative-Network/icn/issues
- **Security**: security@icn.coop
- **Documentation**: https://icn.coop/docs

---

**Document Version**: 1.0  
**Last Updated**: 2025-12-16
