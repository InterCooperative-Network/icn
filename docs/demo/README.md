# ICN Demo Guide

**ICN** is a P2P coordination layer for cooperatives, communities, and federations to coordinate without central servers.

## What You'll See

A cooperative is created, members join, they govern democratically (propose/vote/close with cryptographic proof), exchange mutual credit, register trust attestations, deploy WASM modules, and discover services -- all through a single daemon with a REST API.

## Prerequisites

```bash
# Build the daemon (from repo root)
cd icn && cargo build --release --features wasm
```

## Single-Node Demo (5 minutes)

Start a daemon with an empty data directory:

```bash
# Initialize identity + config
./target/release/icnd --init --data-dir /tmp/icn-demo

# Start daemon with gateway
ICN_GATEWAY_JWT_SECRET="demo-secret-at-least-32-bytes!!" \
  ./target/release/icnd \
    --config /tmp/icn-demo/config.toml \
    --gateway-enable &

# Run the comprehensive demo script (exercises 10 subsystems)
ICN_GATEWAY=http://localhost:8000 bash ../scripts/demo-single-node.sh
```

The demo script exercises: health, identity, cooperative lifecycle, governance (full propose/vote/close with proof), mutual credit ledger, treasury, service discovery, WASM upload, trust attestations, and entity management.

Add `--json` for machine-readable output, or `--skip wasm,trust` to skip specific sections.

## Devnet: 3-Node Cluster (10 minutes)

```bash
cd deploy/devnet

# Build Docker images and start 3 nodes
make build
make up

# Check all nodes are healthy
make status

# Run demo against node-a
make demo
```

Node ports: **node-a** localhost:8000, **node-b** localhost:8001, **node-c** localhost:8002.

Nodes bootstrap via explicit peer URLs (`icn://node-a:9000`). DIDs are learned from the QUIC/TLS handshake.

## The Flagship Demo: Governance Proof

This is the money shot -- a proposal that produces a cryptographic proof of its outcome:

```bash
GW=http://localhost:8000

# 1. Create a cooperative
COOP=$(curl -s -X POST $GW/v1/coops \
  -H 'Content-Type: application/json' \
  -d '{"name":"Demo Coop"}' | jq -r '.coop_id // .id')

# 2. Create a governance domain
DOMAIN=$(curl -s -X POST $GW/v1/gov/domains \
  -H 'Content-Type: application/json' \
  -d "{\"name\":\"policy\",\"coop_id\":\"$COOP\"}" | jq -r '.domain_id // .id')

# 3. Propose a budget allocation
PROP=$(curl -s -X POST $GW/v1/gov/proposals \
  -H 'Content-Type: application/json' \
  -d "{\"domain_id\":\"$DOMAIN\",\"title\":\"Fund community garden\",\"description\":\"Allocate 500 hours to community garden project\",\"payload\":{\"Text\":{\"body\":\"Allocate 500 hours\"}}}" \
  | jq -r '.proposal_id // .id')

# 4. Vote yes
curl -s -X POST $GW/v1/gov/proposals/$PROP/vote \
  -H 'Content-Type: application/json' \
  -d '{"vote":"For"}'

# 5. Close the proposal
curl -s -X POST $GW/v1/gov/proposals/$PROP/close

# 6. Retrieve the cryptographic proof
curl -s $GW/v1/gov/proposals/$PROP/proof | jq .
```

The proof contains: proposal hash, outcome, tally, signer DID, Ed25519 signature, and timestamp. Anyone can verify it independently.

## TUI Console

```bash
./target/release/icn-console --gateway http://localhost:8000 --coop-id $COOP
```

5 tabs: Dashboard, Members, Ledger (journal entries), Governance, Trust. Press `r` to refresh, `Tab` to switch, `q` to quit.

## Known Limitations

- **WASM execution** requires `--features wasm` at build time (wasmtime)
- **mDNS discovery** is disabled in Docker; devnet uses explicit bootstrap peers
- **JWT auth** is required for gateway; use `ICN_GATEWAY_JWT_SECRET` env var
- **No persistence across `make clean`** -- identities regenerated from scratch
- **Federation governance** requires 2+ nodes; single-node demo shows local governance only
- **Trust scores** start at 0.0 for new peers; build up through attestations
