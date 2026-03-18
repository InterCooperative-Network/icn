# Multi-Node ICN Deployment Status

## Current Deployment

**Deployed**: 2025-12-04  
**Status**: ✅ Operational - 3 coop nodes running

## Deployed Coops

| Coop | Display Name | Namespace | DID | Status | Ports |
|------|-------------|-----------|-----|--------|-------|
| **alpha** | Alpha Cooperative | `icn-coop-alpha` | `did:icn:z76a6CKeGxKSek9EUkhmy9NN37XTT4Ev5X7RYsyfWc98a` | ✅ Running | QUIC: 7827, RPC: 5651 |
| **beta** | Beta Timebank | `icn-coop-beta` | `did:icn:z13gWxvgVUP7XFgzk5Z3vkuKQEdauiBhqtok3RYHWGBRk` | ✅ Running | QUIC: 7834, RPC: 5658 |
| **gamma** | Gamma Cooperative | `icn-coop-gamma` | `did:icn:zGabvrXN5uT99V2EoBhSXGYaxjqCaFLxAUqA73pnzERF6` | ✅ Running | QUIC: 7825, RPC: 5649 |

## Quick Commands

### List All Coops
```bash
cd deploy/k8s/multi-node/scripts
./list-coops.sh
```

### Check Coop Status
```bash
kubectl -n icn-coop-alpha get pods
kubectl -n icn-coop-beta get pods
kubectl -n icn-coop-gamma get pods
```

### View Logs
```bash
kubectl -n icn-coop-alpha logs -f deployment/icn-alpha
kubectl -n icn-coop-beta logs -f deployment/icn-beta
kubectl -n icn-coop-gamma logs -f deployment/icn-gamma
```

### View Identity
```bash
kubectl -n icn-coop-alpha exec deployment/icn-alpha -- icnctl id show
kubectl -n icn-coop-beta exec deployment/icn-beta -- icnctl id show
kubectl -n icn-coop-gamma exec deployment/icn-gamma -- icnctl id show
```

## Network Discovery

All nodes have mDNS enabled and should automatically discover each other on the cluster network. They're configured to use:
- **mDNS**: Enabled for automatic peer discovery
- **Bootstrap peers**: Empty (using mDNS only)
- **Network**: Same cluster network (10.42.0.0/16)

## Port Allocation

Each coop gets unique ports automatically:

| Coop | QUIC (UDP) | RPC (TCP) | Metrics | NodePort QUIC | NodePort RPC |
|------|------------|-----------|---------|---------------|--------------|
| alpha | 7827 | 5651 | 9150 | 30827 | 30651 |
| beta | 7834 | 5658 | 9157 | 30834 | 30658 |
| gamma | 7825 | 5649 | 9148 | 30825 | 30649 |

Ports are automatically allocated based on coop name hash to avoid conflicts.

## Storage

Each coop has its own 10Gi PVC on the `atlas-nfs` storage class:
- `icn-coop-alpha-data`
- `icn-coop-beta-data`
- `icn-coop-gamma-data`

## Next Steps

### Testing Multi-Coop Scenarios

1. **Peer Discovery**: Check if nodes discover each other via mDNS
2. **Network Communication**: Test P2P messaging between coops
3. **Ledger Transactions**: Test mutual credit between different coops
4. **Trust Graph**: Test trust relationships across coops

### Adding More Coops

```bash
cd deploy/k8s/multi-node/scripts
./deploy-coop.sh delta "Delta Cooperative"
```

### Multi-Device Testing (Future)

To test multiple devices with the same identity:
1. Export identity from an existing coop
2. Import into a new device deployment
3. Both devices will share the same DID but have separate storage

See [README.md](README.md) for more details.

## Notes

- All coops are in separate namespaces for isolation
- Each coop has its own identity (unique DID)
- All coops share the same cluster network for mDNS discovery
- Original single-node deployment is still running in `icn` namespace

## Original Single-Node Deployment

The original deployment is still running:
- **Namespace**: `icn`
- **DID**: `did:icn:z3TE1ei6B4L5j6Jp29RmJKt1FYonGaQAXQoYHJL3GULR3`
- **Ports**: QUIC: 7777, RPC: 5601

This can be kept for comparison or removed if not needed.

