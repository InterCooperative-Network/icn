# SDIS Quick Start Guide

## Overview

The **SDIS (Secure Distributed Identity System)** is now deployed and accessible on your K3s cluster.

## Access URLs

- **Gateway API**: `http://10.8.10.40:30080`
- **Pilot UI**: `http://10.8.10.40:30030`
- **SDIS Base**: `http://10.8.10.40:30080/v1/sdis`

## Available Endpoints

### Health & Status

```bash
# SDIS health check
curl http://10.8.10.40:30080/v1/sdis/health
```

### Enrollment

```bash
# Start enrollment ceremony
curl -X POST http://10.8.10.40:30080/v1/sdis/enrollment/start \
  -H "Content-Type: application/json" \
  -d '{
    "identity_name": "Alice",
    "pathway": {
      "type": "government_id",
      "country": "US",
      "doc_type": "passport",
      "doc_hash": "abc123..."
    },
    "proof_data": {},
    "initial_keybundle": {
      "signing_key": "...",
      "encryption_key": "...",
      "agreement_key": "..."
    }
  }'

# Get enrollment status
curl http://10.8.10.40:30080/v1/sdis/enrollment/{ceremony_id}

# Finalize enrollment
curl -X POST http://10.8.10.40:30080/v1/sdis/enrollment/{ceremony_id}/finalize
```

### Recovery

```bash
# Start recovery ceremony
curl -X POST http://10.8.10.40:30080/v1/sdis/recovery/start \
  -H "Content-Type: application/json" \
  -d '{
    "anchor_id": "...",
    "recovery_pathway": {...}
  }'

# Complete recovery
curl -X POST http://10.8.10.40:30080/v1/sdis/recovery/{ceremony_id}/complete
```

### Anchor Management

```bash
# Get anchor details
curl http://10.8.10.40:30080/v1/sdis/anchor/{anchor_id}

# Rotate keys
curl -X POST http://10.8.10.40:30080/v1/sdis/anchor/rotate-keys \
  -H "Content-Type: application/json" \
  -d '{
    "anchor_id": "...",
    "new_keybundle": {...},
    "signature": "..."
  }'

# Get rotation history
curl http://10.8.10.40:30080/v1/sdis/anchor/{anchor_id}/history

# Add device
curl -X POST http://10.8.10.40:30080/v1/sdis/anchor/devices/add \
  -H "Content-Type: application/json" \
  -d '{
    "anchor_id": "...",
    "device_keybundle": {...},
    "device_name": "iPhone 15"
  }'

# List devices
curl http://10.8.10.40:30080/v1/sdis/anchor/{anchor_id}/devices
```

### Verification

```bash
# Level 1: QR-only verification (offline-capable)
curl -X POST http://10.8.10.40:30080/v1/sdis/verify/level1 \
  -H "Content-Type: application/json" \
  -d '{
    "qr_data": "base64-encoded-qr-data"
  }'

# Level 2: With binding verification (hybrid)
curl -X POST http://10.8.10.40:30080/v1/sdis/verify/level2 \
  -H "Content-Type: application/json" \
  -d '{
    "qr_data": "base64-encoded-qr-data",
    "binding": "base64-encoded-binding-data"
  }'
```

## Deployment Management

### Check Status

```bash
# Check pods
ssh ubuntu@10.8.10.40 "sudo kubectl get pods -n icn"

# Check services
ssh ubuntu@10.8.10.40 "sudo kubectl get svc -n icn"

# View logs
ssh ubuntu@10.8.10.40 "sudo kubectl logs -n icn -l component=daemon --tail=100"
```

### Rebuild and Deploy

```bash
cd ~/projects/icn/deploy/k8s

# Build images
make build

# Sync to all nodes
make sync

# Update deployment
ssh ubuntu@10.8.10.40 "sudo kubectl set image deployment/icn-daemon icnd=icn:latest -n icn"
```

### Troubleshooting

If the gateway isn't responding:

1. **Check pod status**: `sudo kubectl get pods -n icn`
2. **Check logs**: `sudo kubectl logs -n icn -l component=daemon`
3. **Verify image is on all nodes**:
   ```bash
   # On each node (40, 41, 42):
   ssh ubuntu@10.8.10.{40,41,42} "sudo ctr -n k8s.io images ls | grep icn"
   ```
4. **Restart deployment**:
   ```bash
   ssh ubuntu@10.8.10.40 "sudo kubectl rollout restart deployment/icn-daemon -n icn"
   ```

## Architecture

### SDIS Components

1. **Anchor**: Permanent identity root that survives key rotation
2. **KeyBundle**: Current cryptographic keys (signing, encryption, agreement)
3. **Enrollment Ceremony**: Multi-steward verification process
4. **Recovery Ceremony**: Account recovery with social/biometric proofs
5. **Ephemeral Proofs**: Zero-knowledge credentials for verification

### Trust Model

- **Stewards**: Trusted community members who verify enrollments
- **Threshold**: Minimum number of steward approvals required
- **Pathways**: Different verification methods (gov ID, org sponsorship, biometrics)
- **Decentralized**: No single point of failure or authority

### Security

- **Ed25519**: Signing keys
- **X25519**: Encryption and key agreement
- **ChaCha20-Poly1305**: Authenticated encryption
- **STARK Proofs**: Zero-knowledge credential verification (Level 3)
- **Binding Protocol**: DID-TLS style channel binding

## Next Steps

1. **Deploy Pilot UI updates**: The UI now has SDIS enrollment wizard
2. **Configure stewards**: Add trusted stewards to approve enrollments
3. **Test enrollment flow**: Walk through the full enrollment process
4. **Integrate with mobile app**: CoopWallet mobile app integration
5. **Add monitoring**: Set up Prometheus/Grafana for SDIS metrics

## Documentation

- **Architecture**: `docs/SDIS_ARCHITECTURE.md`
- **API Guide**: `SDIS_API_GUIDE.md`
- **Steward Roadmap**: `SDIS_STEWARD_ROADMAP.md`
- **Build Plan**: `SDIS_BUILD_PLAN.md`

## Support

If you encounter issues:
1. Check logs: `sudo kubectl logs -n icn -l component=daemon`
2. Verify network connectivity from your laptop to 10.8.10.40:30080
3. Ensure WireGuard VPN is connected
4. Review recent commits for changes
