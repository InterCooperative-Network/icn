# Full Stack Deployment Guide

Complete end-to-end ICN system deployment with all components wired together.

> Historical deployment recipe snapshot.
> For current API surface, verify `docs/api/openapi.yaml` and `icn/crates/icn-gateway/src/server.rs`.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     User Applications                        │
├──────────────────────┬──────────────────────────────────────┤
│   Web UI (Next.js)   │   Mobile App (React Native)          │
│   Port: 3001         │   (Expo Go / Native)                 │
└──────────┬───────────┴───────────────┬──────────────────────┘
           │                           │
           │ HTTP/WebSocket            │ HTTP/WebSocket
           │                           │
           ▼                           ▼
┌──────────────────────────────────────────────────────────────┐
│               Gateway API (icn-gateway)                       │
│         REST API (8080) + WebSocket Events (8080)            │
├───────────────────────────────────────────────────────────────┤
│                    ICN Network Layer                          │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐             │
│  │  Node 1    │  │  Node 2    │  │  Node 3    │             │
│  │  (Primary) │  │  (Peer)    │  │  (Peer)    │             │
│  │  :5001     │  │  :5002     │  │  :5003     │             │
│  └────────────┘  └────────────┘  └────────────┘             │
│         │                │                │                   │
│         └────────────────┴────────────────┘                   │
│              QUIC/TLS P2P + Gossip                           │
└───────────────────────────────────────────────────────────────┘
           │                           │
           ▼                           ▼
┌──────────────────────────────────────────────────────────────┐
│                  Monitoring Stack                             │
│  ┌─────────────────┐         ┌──────────────────┐            │
│  │  Prometheus     │────────▶│    Grafana       │            │
│  │  :9090          │         │    :3000         │            │
│  └─────────────────┘         └──────────────────┘            │
└──────────────────────────────────────────────────────────────┘
```

## Components

### 1. ICN Network (Backend)
- **Node 1**: Primary node with Gateway API enabled (ports 5001, 8080, 9100)
- **Node 2**: Peer node (ports 5002, 9101)
- **Node 3**: Peer node (ports 5003, 9102)
- **Features**: Identity, Trust Graph, Gossip, Ledger, Governance, Compute

### 2. Gateway API
- **REST API**: Standard HTTP endpoints for all operations
- **WebSocket**: Real-time event streaming for notifications
- **Authentication**: DID-based auth with session tokens
- **Events**: `notification`, `trust_update`, `ledger_update`, `proposal_update`

### 3. Web UI (pilot-ui)
- **Framework**: Next.js 14 with App Router
- **Port**: 3001
- **Features**: 
  - Member profiles and trust attestation
  - Ledger transactions
  - Proposal voting
  - Real-time notifications via WebSocket
  - Offline queue management

### 4. Mobile App (React Native)
- **Framework**: Expo + React Native
- **SDK**: `@icn/react-native-sdk`
- **Features**:
  - Trust graph visualization
  - Push notifications
  - Offline-first operation
  - Secure storage (Keychain/KeyStore)

### 5. Monitoring
- **Prometheus**: Metrics collection from all nodes (9090)
- **Grafana**: Visualization dashboards (3000)
- **Metrics**: Network health, gossip convergence, trust scores, ledger sync

## Quick Start

### Prerequisites
```bash
# Ensure Docker and Docker Compose are installed
docker --version
docker compose version

# Navigate to ICN project root
cd /home/matt/projects/icn
```

### 1. Build Everything
```bash
# Build ICN daemon
cd icn
cargo build --release
cd ..

# Build Web UI
cd web/pilot-ui
npm install
npm run build
cd ../..

# Build Mobile SDK
cd sdk/typescript
npm install
npm run build
cd ../..
```

### 2. Deploy Full Stack
```bash
# Start all services
docker compose -f docker-compose.full.yml up -d

# Check service health
docker compose -f docker-compose.full.yml ps

# View logs
docker compose -f docker-compose.full.yml logs -f

# View specific service logs
docker compose -f docker-compose.full.yml logs -f web-ui
docker compose -f docker-compose.full.yml logs -f icn-node1
```

### 3. Verify Services

**ICN Nodes:**
```bash
# Node 1 metrics
curl http://localhost:9100/metrics

# Gateway API health
curl http://localhost:8080/v1/health

# Gateway auth challenge (public endpoint)
curl -X POST http://localhost:8080/v1/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"did":"did:icn:YOUR_DID"}'
```

**Web UI:**
```bash
# Open in browser
open http://localhost:3001
```

**Monitoring:**
```bash
# Prometheus targets
open http://localhost:9090/targets

# Grafana dashboards
open http://localhost:3000
# Login: admin/admin
```

### 4. Test WebSocket Connection
```bash
# Use wscat to test WebSocket
npm install -g wscat
wscat -c ws://localhost:8080/v1/ws/YOUR_COOP_ID
```

### 5. Mobile App Development
```bash
cd sdk/react-native/example

# Update API endpoint in app.json:
# "extra": {
#   "apiUrl": "http://YOUR_LOCAL_IP:8080"
# }

# Start Expo
npm start

# Scan QR code with Expo Go app
```

## Configuration

### Gateway API Environment Variables
```bash
ICN_GATEWAY_ENABLE=true
ICN_GATEWAY_PORT=8080
ICN_GATEWAY_WS_PORT=8080
ICN_GATEWAY_CORS_ORIGINS=http://localhost:3001,http://localhost:3000
```

### Web UI Environment Variables
Create `web/pilot-ui/.env.local`:
```bash
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXT_PUBLIC_WS_URL=ws://localhost:8080
```

### Mobile App Configuration
Update `sdk/react-native/example/app.json`:
```json
{
  "expo": {
    "extra": {
      "apiUrl": "http://YOUR_LOCAL_IP:8080",
      "wsUrl": "ws://YOUR_LOCAL_IP:8080"
    }
  }
}
```

## Network Topology

The test network includes 3 nodes with trust relationships:

```
Node 1 (Primary)
  ├─ Trusts Node 2 (0.8)
  └─ Trusts Node 3 (0.7)

Node 2 (Peer)
  ├─ Trusts Node 1 (0.9)
  └─ Trusts Node 3 (0.6)

Node 3 (Peer)
  ├─ Trusts Node 1 (0.8)
  └─ Trusts Node 2 (0.7)
```

DIDs are configured in `config/node{1,2,3}.toml`

## Common Tasks

### View Real-time Notifications
```bash
# Terminal 1: Start stack
docker compose -f docker-compose.full.yml up

# Terminal 2: Connect to WebSocket
wscat -c ws://localhost:8080/v1/ws/YOUR_COOP_ID

# Terminal 3: Create a transaction (triggers notification)
curl -X POST http://localhost:8080/v1/ledger/YOUR_COOP_ID/payment \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "from": "did:icn:...",
    "to": "did:icn:...",
    "amount": 100,
    "currency": "USD",
    "memo": "Test transaction"
  }'
```

### Test Trust Attestation
```bash
# Trust routes vary by deployment/configuration and may not be present in OpenAPI.
# Use your running gateway OpenAPI to confirm available trust paths:
curl -s http://localhost:8080/api-docs/openapi.json | jq '.paths | keys[]' | grep trust
```

### Create and Vote on Proposals
```bash
# Create proposal
curl -X POST http://localhost:8080/v1/gov/proposals \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "domain_id": "membership",
    "title": "Add new member",
    "description": "Proposal to add Alice",
    "payload": {"type": "text", "body": "Proposal text body"}
  }'

# Vote on proposal
curl -X POST http://localhost:8080/v1/gov/proposals/{id}/vote \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "choice": "for",
    "comment": "Strong trust relationship"
  }'
```

## Monitoring

### Key Metrics to Watch

**Network Health:**
- `icn_network_peers_connected`: Number of connected peers
- `icn_network_messages_sent_total`: Total messages sent
- `icn_network_messages_received_total`: Total messages received

**Gossip Protocol:**
- `icn_gossip_messages_total`: Gossip messages processed
- `icn_gossip_topics`: Active gossip topics
- `icn_gossip_convergence_time_seconds`: Time to convergence

**Trust Graph:**
- `icn_trust_attestations_total`: Total attestations recorded
- `icn_trust_score_updates_total`: Trust score calculations

**Ledger:**
- `icn_ledger_entries_total`: Total ledger entries
- `icn_ledger_sync_operations_total`: Sync operations
- `icn_ledger_balance`: Account balances

**Gateway:**
- `icn_gateway_http_requests_total`: API requests
- `icn_gateway_ws_connections`: Active WebSocket connections
- `icn_gateway_events_sent_total`: Events pushed to clients

### Grafana Dashboards

1. **ICN Overview**: System-wide health metrics
2. **Network Topology**: Peer connections and gossip
3. **Trust Graph**: Trust attestations and scores
4. **Ledger Activity**: Transaction volume and balances
5. **Gateway Performance**: API latency and WebSocket events

## Troubleshooting

### Services Won't Start
```bash
# Check Docker logs
docker compose -f docker-compose.full.yml logs

# Check for port conflicts
lsof -i :3001
lsof -i :8080
lsof -i :9090
```

### WebSocket Connection Failed
```bash
# Verify Gateway is running
curl http://localhost:8080/v1/health

# Check CORS configuration
# Ensure NEXT_PUBLIC_API_URL matches your client origin
```

### Mobile App Can't Connect
```bash
# Use local IP instead of localhost
ifconfig | grep "inet "

# Update app.json with your local IP:
# "apiUrl": "http://192.168.1.100:8080"

# Ensure phone and computer are on same network
```

### Nodes Not Discovering Each Other
```bash
# Check mDNS is working
docker compose -f docker-compose.full.yml logs icn-node1 | grep mdns

# Verify bootstrap peers in config files
cat config/node1.toml | grep bootstrap
```

## Shutdown

```bash
# Stop all services
docker compose -f docker-compose.full.yml down

# Stop and remove volumes (resets all data)
docker compose -f docker-compose.full.yml down -v

# Stop and remove images
docker compose -f docker-compose.full.yml down --rmi all
```

## Production Deployment

For production deployment:

1. **Security**: Replace test passphrases with secure secrets
2. **TLS**: Enable HTTPS for Gateway and Web UI
3. **Scaling**: Use Kubernetes (see `deploy/k8s/`)
4. **Monitoring**: Set up alerting rules in Prometheus
5. **Backup**: Configure regular backups of node data
6. **DNS**: Set up proper domain names
7. **CDN**: Use CDN for Web UI static assets

See `DEPLOYMENT_GUIDE.md` for production best practices.

## Next Steps

- [ ] Set up CI/CD pipeline
- [ ] Add end-to-end integration tests
- [ ] Configure production secrets management
- [ ] Set up SSL certificates
- [ ] Deploy to staging environment
- [ ] Performance testing and optimization
- [ ] Security audit
- [ ] Production deployment

## Resources

- **Architecture**: `docs/ARCHITECTURE.md`
- **API Reference**: `docs/api-reference.md`
- **Mobile SDK**: `sdk/react-native/README.md`
- **Web UI**: `web/pilot-ui/README.md`
- **Deployment**: `DEPLOYMENT_GUIDE.md`
- **Troubleshooting**: `docs/troubleshooting.md`
