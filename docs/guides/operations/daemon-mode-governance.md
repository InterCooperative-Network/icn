# ICN Daemon Mode with Governance Receipts

**Status**: Verified 2026-02-14
**Applies to**: Phase 0 pilot deployment

## Overview

The ICN daemon (`icnd`) runs with `GovernanceActor` enabled, which produces `GovernanceDecisionReceipt` on proposal closure. This is the default mode for all deployments and ensures that governance decisions generate cryptographic proof receipts.

## Architecture

```
icnd (main.rs)
  └─> Runtime::new()
      └─> Supervisor::spawn_all()
          └─> init_governance_services()
              ├─> GovernanceActor::spawn(signing_key)
              └─> KernelGovernanceExecutor::new()
```

### Key Components

| Component | Location | Purpose |
|-----------|----------|---------|
| `icnd` | `icn/bins/icnd/src/main.rs` | Daemon entry point, loads identity and services |
| `GovernanceActor` | `apps/governance` (re-exported via `icn-core`) | Proposal management, receipt generation |
| `KernelGovernanceExecutor` | `icn-core/src/supervisor/governance_executor.rs` | Effect execution for approved proposals |
| `signing_key` | Derived from keystore identity | Ed25519 key for GovernanceProofV2 signatures |

## Deployment Configuration

### K8s Deployment (`deploy/k8s/deployment.yaml`)

```yaml
containers:
- name: icnd
  image: icn:latest
  args:
    - --config
    - /etc/icn/icn.toml
  env:
  - name: ICN_PASSPHRASE
    valueFrom:
      secretKeyRef:
        name: icn-secrets
        key: passphrase
  - name: GATEWAY_BASE_URL
    valueFrom:
      configMapKeyRef:
        name: icn-config
        key: gateway_base_url
        optional: true
```

**Critical**: The `ICN_PASSPHRASE` environment variable unlocks the keystore, providing the signing key needed for GovernanceActor receipt generation.

### Configuration File (`deploy/config/icn.toml`)

```toml
[gateway]
enabled = true
bind_addr = "0.0.0.0:8080"
token_expiry_hours = 24
challenge_ttl_minutes = 5

[identity]
backend = "age"  # Software keystore (production-ready)
```

## Receipt Generation Flow

1. **Proposal Created**: `POST /v1/gov/proposals`
2. **Votes Cast**: `POST /v1/gov/proposals/{id}/vote`
3. **Proposal Closed**: `POST /v1/gov/proposals/{id}/close`
   - GovernanceActor calls `close_proposal()`
   - If approved, KernelGovernanceExecutor executes effects
   - GovernanceProofV2 generated with signing_key
   - Receipt includes: decision_hash, tally, timestamp, signature
4. **Receipt Retrieved**: `GET /v1/gov/proposals/{id}/proof`

## Verification

### Test Receipt Generation

```bash
# 1. Create a proposal (assumes authenticated session)
PROPOSAL_ID=$(curl -s -X POST http://gateway:8080/v1/gov/proposals \
  -H "Authorization: Bearer $JWT" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Test Proposal",
    "description": "Verify receipt generation",
    "payload": {"Text": {"content": "Test"}}
  }' | jq -r '.proposal_id')

# 2. Vote
curl -X POST http://gateway:8080/v1/gov/proposals/$PROPOSAL_ID/vote \
  -H "Authorization: Bearer $JWT" \
  -H "Content-Type: application/json" \
  -d '{"choice": "For"}'

# 3. Close proposal
curl -X POST http://gateway:8080/v1/gov/proposals/$PROPOSAL_ID/close \
  -H "Authorization: Bearer $JWT"

# 4. Retrieve receipt
curl -s http://gateway:8080/v1/gov/proposals/$PROPOSAL_ID/proof \
  -H "Authorization: Bearer $JWT" | jq
```

Expected output:
```json
{
  "decision_hash": "0x...",
  "proposal_id": "...",
  "outcome": "Approved",
  "tally": {"for": 1, "against": 0, "abstain": 0},
  "timestamp": 1234567890,
  "signature": "..."
}
```

### Verify in Logs

```bash
kubectl logs -n icn deployment/icn-daemon | grep -i "governance"
```

Expected log entries:
```
✓ Governance actor spawned at /data/governance
✓ Governance executor created
```

## Troubleshooting

### No Receipt Generated

**Symptom**: `GET /proposals/{id}/proof` returns 404 or empty receipt.

**Causes**:
1. **No signing key**: Keystore not unlocked (missing `ICN_PASSPHRASE`)
2. **Standalone gateway mode**: Gateway running without icnd supervisor
3. **Receipt storage not wired**: ReceiptStore not configured (fixed in P0-T09/T10)

**Fix**:
```bash
# Check keystore unlock
kubectl logs -n icn deployment/icn-daemon | grep "Identity loaded"
# Should see: "Identity loaded: did:icn:... (with DID-TLS binding)"

# Verify governance actor spawned
kubectl logs -n icn deployment/icn-daemon | grep "Governance actor"
```

### Receipt Signature Invalid

**Symptom**: Receipt exists but signature verification fails.

**Cause**: Signing key mismatch (keystore rotated but receipts reference old key).

**Fix**: Rotate governance identity (requires governance proposal for key rotation).

## Security Considerations

1. **Keystore Passphrase**: Stored in Kubernetes secret, auto-injected via env var
2. **Signing Key Exposure**: Never exported; used only within icnd process
3. **Receipt Integrity**: SHA-256 canonical hash + Ed25519 signature
4. **Replay Protection**: Timestamp + proposal_id uniqueness

## Phase 0 Status

- [x] GovernanceActor initialized with signing key
- [x] KernelGovernanceExecutor attached
- [x] Receipt generation on close_proposal
- [x] K8s deployment configured for daemon mode
- [ ] Receipt storage (P0-T09)
- [ ] Receipt chaining (P0-T10)
- [ ] Receipt Explorer UI (P0-T11)

## References

- [Governance Architecture](/home/ubuntu/projects/icn-wt-ops-security/docs/ARCHITECTURE.md#governance)
- [Receipt Chain Spec](/home/ubuntu/projects/icn-wt-ops-security/docs/plans/2026-02-14-icn-consensus-stack-a-to-b.md#receipts-infrastructure-5-tasks)
- [Deployment Guide](/home/ubuntu/projects/icn-wt-ops-security/docs/HOMELAB_DEPLOYMENT.md)
