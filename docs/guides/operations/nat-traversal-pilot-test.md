# NAT Traversal Pilot Test Guide

Manual testing guide for the C3 NAT Traversal feature (PR #1183).

## Prerequisites

- ICN repo checked out on `feat/c3-nat-traversal` branch
- Rust toolchain installed (stable)
- Two terminals (or `tmux`/`screen`)
- Internet access (for STUN servers)

## 1. Build

```bash
cd icn
cargo build --release -p icnd -p icnctl
```

Binaries land in `target/release/` (or `$CARGO_TARGET_DIR/release/` if configured).

## 2. Initialize Two Nodes

```bash
# Terminal setup
export ICND=./target/release/icnd
export ICNCTL=./target/release/icnctl
export BASE=/tmp/icn-pilot-test

mkdir -p $BASE/node-a $BASE/node-b

# Init Node A
ICN_KEYSTORE_PASSPHRASE=test $ICND --init --data-dir $BASE/node-a

# Init Node B
ICN_KEYSTORE_PASSPHRASE=test $ICND --init --data-dir $BASE/node-b
```

**Record the DIDs** printed during init — you'll need them for Step 5.

## 3. Configure Non-Conflicting Ports

Both nodes default to the same ports. Edit Node B's config to avoid conflicts:

```bash
# Edit $BASE/node-b/config.toml - change these values:
```

| Setting | Node A (default) | Node B (change to) |
|---------|------------------|--------------------|
| `[network] listen_addr` | `0.0.0.0:9000` | `0.0.0.0:9001` |
| `[network] rpc_port` | `5601` | `5602` |
| `[observability] metrics_port` | `9100` | `9101` |
| `[observability] health_port` | `8080` | `8081` |
| `[gateway] bind_addr` | `0.0.0.0:8000` | `0.0.0.0:8001` |

**For LAN testing** (both nodes on same machine), also set on both nodes:

```toml
[network]
min_trust_threshold = 0.0
```

This disables trust-gated TLS so the two fresh nodes (with no mutual trust) can connect.

## 4. Start Both Nodes

**Terminal 1 (Node A):**
```bash
ICN_KEYSTORE_PASSPHRASE=test $ICND \
  --config $BASE/node-a/config.toml \
  --gateway-enable \
  --insecure-gateway-no-jwt
```

**Terminal 2 (Node B):**
```bash
ICN_KEYSTORE_PASSPHRASE=test $ICND \
  --config $BASE/node-b/config.toml \
  --gateway-enable \
  --insecure-gateway-no-jwt
```

## 5. Verify Startup

### Health check (gateway)

```bash
# Node A
curl -s http://127.0.0.1:8000/v1/health | python3 -m json.tool
# Expected: {"status":"ok","version":"0.1.0"}

# Node B
curl -s http://127.0.0.1:8001/v1/health | python3 -m json.tool
# Expected: {"status":"ok","version":"0.1.0"}
```

### Detailed health

```bash
curl -s http://127.0.0.1:8000/v1/health/detailed | python3 -m json.tool
```

Expected: `status: ok` with components (identity, ledger, coop_manager, etc.)

### STUN discovery (check logs)

Look for this line in each terminal's output:
```
INFO icn_net::session: ✅ Discovered public endpoint: <IP>:<PORT> (local: 0.0.0.0:9000)
```

If you see this, STUN is working and the node knows its public address.

### NAT candidate announcement (check logs)

Look for:
```
INFO icn_core::supervisor::init_bootstrap: Connection candidate: local=0.0.0.0:9000, public=Some(<IP>:<PORT>), relay=None
```

This confirms the node is announcing its NAT traversal candidate (local + public + relay addresses).

**Note:** You may see `WARN: Failed to publish connection candidate: Topic 'network:candidates' not found` — this is expected. The gossip topic for candidate exchange isn't auto-created yet. Candidates still work via mDNS and direct dial.

## 6. Check Prometheus Metrics

```bash
# Node A metrics
curl -s http://127.0.0.1:9100/metrics | grep -E "nat|stun|relay|traversal"

# Node B metrics
curl -s http://127.0.0.1:9101/metrics | grep -E "nat|stun|relay|traversal"
```

Expected metrics:
| Metric | Meaning |
|--------|---------|
| `icn_stun_discovery_total{result="success"}` | STUN binding request succeeded |
| `icn_nat_dial_attempts_total{type="candidate_change"}` | NAT dial triggered by address change |
| `icn_nat_relay_active` | Number of active TURN relay sessions (0 without TURN config) |

## 7. NAT Traversal Config Options

The `config.toml` has a `[network.nat_dial]` section:

```toml
[network.nat_dial]
parallel_dial = true              # Try direct + relay simultaneously
local_dial_timeout_ms = 2000      # LAN direct dial timeout
public_dial_timeout_ms = 10000    # Public direct dial timeout
relay_dial_timeout_ms = 30000     # TURN relay dial timeout
candidate_announce_interval_secs = 150  # How often to re-announce
```

### Optional: TURN relay config

To test relay fallback, add TURN server config to `config.toml`:

```toml
[network]
turn_server = "turn.example.com:3478"
turn_username = "user"
turn_password = "pass"
```

**Note:** Without a real TURN server, relay paths won't be available. The transport layer will attempt direct connections only.

## 8. Known Limitations (Pre-existing, Not NAT-Related)

### `icnctl network status` doesn't work

Two pre-existing bugs prevent `icnctl network status` from functioning:

1. **Nested tokio runtime** — `handle_network_command()` in `icnctl/src/main.rs:2910` has `#[tokio::main]` but is called from within the main async runtime. Produces: `Cannot start a runtime from within a runtime`.

2. **RPC server crash on unauthenticated request** — `ANONYMOUS_DID` construction in `icn-rpc/src/server.rs:87` uses all-zero seeds for `KeyPair::from_bytes()`, which panics because the public key doesn't match the derived key. This poisons the `LazyLock` and crashes all subsequent unauthenticated RPC requests.

**Workaround:** Use gateway health endpoints and Prometheus metrics (shown above) to verify node status. The NAT status fields are fully wired in the RPC handler and will work once these pre-existing bugs are fixed.

### Peer connectivity requires bootstrap peers

Two nodes on the same machine will discover each other via mDNS but won't automatically establish QUIC sessions without being configured as bootstrap peers or having a shared gossip topic. To force a connection, add the other node as a bootstrap peer:

```toml
[network]
bootstrap_peers = ["127.0.0.1:9001"]  # In node-a's config, point to node-b
```

## 9. Cleanup

```bash
# Stop both nodes (Ctrl+C in each terminal)
# Remove test data
rm -rf /tmp/icn-pilot-test
```

## What This PR Tests

| Feature | Status | How to Verify |
|---------|--------|---------------|
| STUN public endpoint discovery | Working | Check logs for `Discovered public endpoint` |
| NatStatus struct in NetworkActor | Working | Prometheus metrics (stun/relay/dial counters) |
| Connection candidate announcement | Working | Check logs for `Connection candidate: local=..., public=..., relay=...` |
| `icnctl network status` NAT section | Blocked | Pre-existing bugs in icnctl + RPC (not NAT-related) |
| TURN relay fallback | Code complete | Requires real TURN server; covered by integration test |
| Configurable dial timeout | Working | Set in `[network.nat_dial]` section |
| Parallel direct+relay dial | Working | `parallel_dial = true` in config (default) |
| Relay proxy (Quinn-over-TURN) | Code complete | Covered by `relay_fallback` integration test |

## Quick Smoke Test (Copy-Paste)

```bash
# Full smoke test - run from icn/ directory
export ICND=./target/release/icnd
export BASE=/tmp/icn-pilot-test

# Build
cargo build --release -p icnd -p icnctl

# Init
mkdir -p $BASE/node-a
ICN_KEYSTORE_PASSPHRASE=test $ICND --init --data-dir $BASE/node-a

# Patch config
sed -i 's/min_trust_threshold = 0.1/min_trust_threshold = 0.0/' $BASE/node-a/config.toml

# Start (background)
ICN_KEYSTORE_PASSPHRASE=test $ICND --config $BASE/node-a/config.toml \
  --gateway-enable --insecure-gateway-no-jwt &
ICND_PID=$!
sleep 5

# Verify
echo "=== Health ==="
curl -s http://127.0.0.1:8000/v1/health

echo -e "\n=== NAT Metrics ==="
curl -s http://127.0.0.1:9100/metrics | grep -E "stun|nat_"

echo -e "\n=== Logs (NAT lines) ==="
grep -E "public endpoint|Connection candidate|TURN|relay" $BASE/node-a/stdout.log 2>/dev/null || \
  echo "(logs go to terminal, check there)"

# Cleanup
kill $ICND_PID 2>/dev/null
rm -rf $BASE
```
