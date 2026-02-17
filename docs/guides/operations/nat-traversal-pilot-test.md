# NAT Traversal Pilot Test Guide

Manual testing guide for the C3 NAT Traversal feature.

## 5-Minute Smoke Test

Start a node and verify the full stack in two commands:

```bash
cd icn
cargo build --release -p icnd -p icnctl

export ICND=./target/release/icnd
export ICNCTL=./target/release/icnctl
export BASE=/tmp/icn-pilot-test

mkdir -p $BASE/node-a
ICN_KEYSTORE_PASSPHRASE=test $ICND --init --data-dir $BASE/node-a
sed -i 's/min_trust_threshold = 0.1/min_trust_threshold = 0.0/' $BASE/node-a/config.toml

ICN_KEYSTORE_PASSPHRASE=test $ICND --config $BASE/node-a/config.toml &
sleep 5

ICN_KEYSTORE_PASSPHRASE=test $ICNCTL --data-dir $BASE/node-a -e 127.0.0.1:5601 network status
ICN_KEYSTORE_PASSPHRASE=test $ICNCTL --data-dir $BASE/node-a -e 127.0.0.1:5601 ledger head
```

**Expected output:**

```
Network Actor Status:
  Running:               true
  Listening on:      127.0.0.1:5601
  NAT Traversal:
    Public endpoint:  <your-public-IP>:<port>
    TURN relay:       none
    Active relays:    0
    Last traversal:   Unknown
```

```
Ledger is empty
```

If both commands succeed, the following are all functioning:
- STUN public endpoint discovery
- NAT classification and candidate derivation
- Authenticated CLI-to-RPC path
- Keystore unlock via `ICN_KEYSTORE_PASSPHRASE`
- Gossip topic initialization (`network:candidates`)
- Clean null-result handling for empty ledger state

## Prerequisites

- ICN repo on `main` (commit `861dec3c` or later)
- Rust toolchain (stable)
- Two terminals (or `tmux`/`screen`) for two-node testing
- Internet access (for STUN servers)

## Full Two-Node Test

### 1. Build

```bash
cd icn
cargo build --release -p icnd -p icnctl
```

### 2. Initialize Two Nodes

```bash
export ICND=./target/release/icnd
export ICNCTL=./target/release/icnctl
export BASE=/tmp/icn-pilot-test

mkdir -p $BASE/node-a $BASE/node-b

ICN_KEYSTORE_PASSPHRASE=test $ICND --init --data-dir $BASE/node-a
ICN_KEYSTORE_PASSPHRASE=test $ICND --init --data-dir $BASE/node-b
```

Record the DIDs printed during init — you'll need them for peer connectivity.

### 3. Configure Non-Conflicting Ports

Both nodes default to the same ports. Edit Node B's config:

| Setting | Node A (default) | Node B (change to) |
|---------|------------------|--------------------|
| `[network] listen_addr` | `0.0.0.0:9000` | `0.0.0.0:9001` |
| `[network] rpc_port` | `5601` | `5602` |
| `[observability] metrics_port` | `9100` | `9101` |
| `[observability] health_port` | `8080` | `8081` |
| `[gateway] bind_addr` | `0.0.0.0:8000` | `0.0.0.0:8001` |

On **both** nodes, set `min_trust_threshold = 0.0` so fresh nodes (with no mutual trust) can connect:

```toml
[network]
min_trust_threshold = 0.0
```

### 4. Start Both Nodes

**Terminal 1 (Node A):**
```bash
ICN_KEYSTORE_PASSPHRASE=test $ICND --config $BASE/node-a/config.toml
```

**Terminal 2 (Node B):**
```bash
ICN_KEYSTORE_PASSPHRASE=test $ICND --config $BASE/node-b/config.toml
```

The gateway starts automatically (enabled by default in the generated config).

### 5. Verify Both Nodes

**Primary check — `network status`:**

```bash
# Node A
ICN_KEYSTORE_PASSPHRASE=test $ICNCTL --data-dir $BASE/node-a -e 127.0.0.1:5601 network status

# Node B
ICN_KEYSTORE_PASSPHRASE=test $ICNCTL --data-dir $BASE/node-b -e 127.0.0.1:5602 network status
```

Both should show:
- `Running: true`
- A STUN-derived public endpoint (e.g. `173.x.x.x:port`)
- NAT traversal section with relay status and traversal mode

**Gateway health:**

```bash
curl -s http://127.0.0.1:8000/v1/health
# {"status":"ok","version":"0.1.0"}

curl -s http://127.0.0.1:8001/v1/health
# {"status":"ok","version":"0.1.0"}
```

**Ledger (confirms RPC null-result handling):**

```bash
ICN_KEYSTORE_PASSPHRASE=test $ICNCTL --data-dir $BASE/node-a -e 127.0.0.1:5601 ledger head
# Ledger is empty
```

### 6. NAT Traversal Config

The generated config includes a `[network.nat_dial]` section:

```toml
[network.nat_dial]
parallel_dial = true              # Try direct + relay simultaneously
local_dial_timeout_ms = 2000      # LAN direct dial timeout
public_dial_timeout_ms = 10000    # Public direct dial timeout
relay_dial_timeout_ms = 30000     # TURN relay dial timeout
candidate_announce_interval_secs = 150  # How often to re-announce
```

### Optional: TURN relay testing

To test relay fallback, add TURN server config:

```toml
[network]
turn_server = "turn.example.com:3478"
turn_username = "user"
turn_password = "pass"
```

Without a TURN server, relay paths won't activate. The transport layer will use direct connections only. The relay path is covered by the `relay_fallback` integration test.

### 7. Cleanup

```bash
# Stop both nodes (Ctrl+C in each terminal)
rm -rf /tmp/icn-pilot-test
```

## Advanced Diagnostics

Use these steps only if `network status` shows unexpected values.

### STUN discovery (log inspection)

Look for this line in daemon output:
```
INFO icn_net::session: Discovered public endpoint: <IP>:<PORT> (local: 0.0.0.0:9000)
```

### NAT candidate announcement (log inspection)

```
INFO icn_core::supervisor::init_bootstrap: Connection candidate: local=0.0.0.0:9000, public=Some(<IP>:<PORT>), relay=None
```

### Prometheus metrics

```bash
curl -s http://127.0.0.1:9100/metrics | grep -E "stun|nat_|relay"
```

| Metric | Meaning |
|--------|---------|
| `icn_stun_discovery_total{result="success"}` | STUN binding succeeded |
| `icn_nat_dial_attempts_total{type="candidate_change"}` | NAT dial triggered |
| `icn_nat_relay_active` | Active TURN relay sessions (0 without TURN) |

### Peer connectivity

Two nodes on the same machine discover each other via mDNS but may not auto-establish QUIC sessions. To force a connection, add the other node as a bootstrap peer:

```toml
[network]
bootstrap_peers = ["127.0.0.1:9001"]  # In node-a's config, point to node-b
```

## What This Pilot Proves

- Full STUN public endpoint discovery
- NAT classification and address derivation
- Connection candidate gossip initialization
- Authenticated CLI-to-RPC path (unified passphrase handling)
- Clean empty-state responses (ledger, network)
- Configurable dial timeouts and parallel dial strategy
- TURN relay fallback (with integration test; requires real TURN server for live test)
