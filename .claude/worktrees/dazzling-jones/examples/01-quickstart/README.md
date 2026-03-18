# Quick Start: Two-Node ICN Network

This example demonstrates setting up a two-node ICN network on your local machine and testing basic operations.

**Time:** ~5 minutes
**Difficulty:** Beginner
**Prerequisites:** ICN binaries built (`cargo build --release`)

## What You'll Learn

- Starting ICN daemon nodes
- Initializing node identities
- Automatic peer discovery via mDNS
- Checking network status
- Managing trust relationships
- Using icnctl CLI

## Architecture

```
┌─────────────────┐         mDNS          ┌─────────────────┐
│   Alpha Node    │◄──────Discovery──────►│   Beta Node     │
├─────────────────┤                        ├─────────────────┤
│ QUIC:  4433     │◄──────QUIC/TLS───────►│ QUIC:  4434     │
│ RPC:   5050     │                        │ RPC:   5051     │
│ Metrics: 9100   │                        │ Metrics: 9101   │
│ Data: /tmp/icn- │                        │ Data: /tmp/icn- │
│       alpha/    │                        │       beta/     │
└─────────────────┘                        └─────────────────┘
```

## Manual Setup

### Step 1: Build ICN

```bash
# From repository root
cd icn
cargo build --release
cd ..
```

### Step 2: Start Alpha Node

Open a terminal and run:

```bash
# Start alpha node
./icn/target/release/icnd --config config/icn-alpha.toml
```

You'll be prompted to create a new identity and set a passphrase. Choose a simple passphrase for testing (e.g., "test123").

Expected output:
```
INFO icn_core::runtime: Starting ICN daemon
INFO icn_identity: Initializing new identity keystore
Enter passphrase:
Confirm passphrase:
INFO icn_identity: Identity created: did:icn:z6Mk...
INFO icn_net: QUIC listener started on 0.0.0.0:4433
INFO icn_net: mDNS discovery enabled
INFO icn_obs: Metrics server started on :9100
INFO icn_core::supervisor: All actors spawned successfully
```

### Step 3: Start Beta Node

Open a second terminal and run:

```bash
# Start beta node
./icn/target/release/icnd --config config/icn-beta.toml
```

Again, create a new identity with a passphrase.

### Step 4: Verify Peer Discovery

Open a third terminal for CLI commands:

```bash
# Check alpha's network status
./icn/target/release/icnctl --endpoint 127.0.0.1:5050 network status

# List alpha's discovered peers
./icn/target/release/icnctl --endpoint 127.0.0.1:5050 network peers

# Check beta's network status
./icn/target/release/icnctl --endpoint 127.0.0.1:5051 network status

# List beta's discovered peers
./icn/target/release/icnctl --endpoint 127.0.0.1:5051 network peers
```

**Expected:** Both nodes should show each other as discovered peers within 5-10 seconds via mDNS.

### Step 5: Check Identities

```bash
# Show alpha's DID
./icn/target/release/icnctl --endpoint 127.0.0.1:5050 id show

# Show beta's DID
./icn/target/release/icnctl --endpoint 127.0.0.1:5051 id show
```

Copy these DIDs for the next step.

### Step 6: Add Trust Relationships

```bash
# From alpha, add trust to beta
./icn/target/release/icnctl --endpoint 127.0.0.1:5050 trust add \
  did:icn:BETA_DID_HERE \
  --score 0.8 \
  --label partner

# From beta, add trust to alpha
./icn/target/release/icnctl --endpoint 127.0.0.1:5051 trust add \
  did:icn:ALPHA_DID_HERE \
  --score 0.8 \
  --label partner
```

### Step 7: Query Trust Graph

```bash
# List alpha's trust edges
./icn/target/release/icnctl --endpoint 127.0.0.1:5050 trust list

# Show computed trust score for beta
./icn/target/release/icnctl --endpoint 127.0.0.1:5050 trust show did:icn:BETA_DID_HERE

# List beta's trust edges
./icn/target/release/icnctl --endpoint 127.0.0.1:5051 trust list
```

### Step 8: Monitor with Prometheus

Open your browser and visit:

- **Alpha metrics:** http://localhost:9100/metrics
- **Beta metrics:** http://localhost:9101/metrics

Look for metrics like:
- `icn_network_connections_active`
- `icn_network_peers_discovered`
- `icn_gossip_announces_sent_total`

### Step 9: Cleanup

Stop both nodes with `Ctrl+C` in their terminals, then:

```bash
# Remove test data
rm -rf /tmp/icn-alpha /tmp/icn-beta
```

## Automated Setup

Use the provided script to run all steps automatically:

```bash
cd examples/01-quickstart
./run.sh
```

The script will:
1. Check that ICN is built
2. Start both nodes in the background
3. Wait for peer discovery
4. Run status checks
5. Demonstrate trust management
6. Show metrics endpoints
7. Stop nodes and cleanup

## Troubleshooting

### Nodes don't discover each other

**Problem:** `network peers` shows empty list after 10+ seconds

**Solutions:**
1. Verify mDNS is working:
   - Linux: `avahi-browse -a | grep icn`
   - macOS: `dns-sd -B _icn._udp`

2. Check firewall isn't blocking UDP 4433 and 4434

3. Ensure both nodes are on the same network segment (mDNS doesn't cross subnets)

4. Try manual dialing as fallback:
   ```bash
   ./icnctl --endpoint 127.0.0.1:5050 network dial did:icn:BETA_DID 127.0.0.1:4434
   ```

### "Address already in use" error

**Problem:** Node fails to start with port conflict

**Solutions:**
1. Check if another instance is running:
   ```bash
   lsof -i :4433
   lsof -i :4434
   ```

2. Kill existing processes:
   ```bash
   pkill icnd
   ```

3. Change ports in config files

### "Failed to unlock keystore"

**Problem:** Wrong passphrase or corrupted keystore

**Solutions:**
1. Delete and recreate identity:
   ```bash
   rm -rf /tmp/icn-alpha/keystore.age
   # Restart node to create new identity
   ```

2. Ensure you're entering the same passphrase you set during init

### Can't connect with icnctl

**Problem:** `icnctl` commands fail with connection error

**Solutions:**
1. Verify node is running:
   ```bash
   ps aux | grep icnd
   ```

2. Check RPC port is listening:
   ```bash
   lsof -i :5050
   lsof -i :5051
   ```

3. Verify you're using correct endpoint:
   - Alpha: `--endpoint 127.0.0.1:5050`
   - Beta: `--endpoint 127.0.0.1:5051`

## What's Next?

Now that you have a working network, try:

1. **Explore gossip:** See [docs/topic-subscriptions-api.md](../../docs/topic-subscriptions-api.md)
2. **CCL contracts:** See [examples/contracts/](../contracts/) for example contracts
3. **WASM compute:** See [examples/wasm-compute/](../wasm-compute/) for WASM modules
4. **TypeScript SDK:** See [sdk/typescript/examples/](../../sdk/typescript/examples/) for SDK examples
5. **Docker deployment:** See [docker/](../../docker/)
6. **Production setup:** See [docs/deployment-guide.md](../../docs/deployment-guide.md)

## Key Concepts Demonstrated

### Identity (DID)
- Each node has a unique Decentralized Identifier
- DIDs are derived from Ed25519 public keys
- Format: `did:icn:z6Mk...` (base58btc-encoded)

### mDNS Discovery
- Nodes automatically find peers on the local network
- No central server required
- Works across common LAN topologies

### Trust Graph
- Explicit trust relationships between peers
- Trust scores range from 0.0 (untrusted) to 1.0 (full trust)
- Trust classes: Isolated, Known, Partner, Federated
- Used for access control and rate limiting

### QUIC/TLS Transport
- Secure, encrypted peer-to-peer communication
- Built-in connection multiplexing
- TLS certificates derived from node DIDs

### Observability
- Prometheus metrics for monitoring
- Structured logging with configurable levels
- Health check endpoints

## Questions?

- [Project README](../../README.md)
- [Documentation](../../docs/)
- [Architecture](../../docs/ARCHITECTURE.md)
