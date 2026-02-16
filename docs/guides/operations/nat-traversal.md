# NAT Traversal Operations Guide

This guide covers configuring and operating ICN nodes behind NAT (Network Address Translation) devices. It applies to pilot deployments where nodes communicate across different networks.

## How NAT Traversal Works in ICN

ICN uses a **direct-then-relay** strategy for peer connections:

1. **Direct dial** — the node tries to connect directly to the peer's advertised address via QUIC/TLS
2. **TURN relay fallback** — if direct dial fails and a relay path is available, the node creates a per-peer relay proxy that transparently wraps QUIC traffic in TURN framing

The relay path is entirely data-plane: Quinn (the QUIC implementation) sends raw UDP to a local proxy socket, the proxy wraps packets as TURN SEND-INDICATION messages, and the TURN server relays them to the peer. Quinn is unaware that TURN is involved.

### Connection flow

```text
Node A (Quinn) → [local proxy 127.0.0.1:X] → [TURN server] → Node B (Quinn)
```

### What triggers relay fallback

Relay fallback activates when ALL of the following are true:
- Direct dial to the peer failed (timeout, connection refused, unreachable)
- The peer advertised a `peer_relay_addr` (their TURN relay allocation)
- This node has a TURN allocation (`relay_addr` is set)
- This node has a TURN server configured (`turn_server_addr` is set)

If any condition is missing, the dial fails with an operator-readable hint (e.g., "no peer relay candidate provided", "no TURN allocation", "no TURN server configured").

## Monitoring NAT Status

### `icnctl network status`

The NAT Traversal section shows:

```
NAT Traversal:
  Public endpoint:  203.0.113.5:4433
  Relay address:    198.51.100.1:49152
  Active relays:    1
  Last traversal:   Relayed
  Last direct err:  Timeout dialing peer (direct): deadline has elapsed
  Last relay err:   none
```

| Field | Meaning |
|-------|---------|
| **Public endpoint** | STUN-discovered public IP:port. `none` if STUN failed or is disabled. |
| **Relay address** | TURN-allocated relay address on the TURN server. `none` if no TURN allocation. |
| **Active relays** | Number of per-peer relay proxies currently running. One proxy per relayed peer. |
| **Last traversal** | How the most recent dial was established: `Direct`, `Relayed`, or `Unknown` (no dial yet). |
| **Last direct err** | Error from the most recent direct dial attempt. `none` if the last direct dial succeeded. |
| **Last relay err** | Error from the most recent relay attempt. `none` if relay succeeded or was not attempted. |

### Interpreting the output

- **Public endpoint = none, Relay address = none**: Node has no NAT traversal configured. It can only reach peers on the local network (mDNS discovery).
- **Public endpoint = X, Relay address = none**: STUN discovered the public address, but no TURN server is configured. Direct dial to peers behind symmetric NAT will fail.
- **Last traversal = Relayed, Active relays >= 1**: At least one peer connection is going through TURN. Check if direct connectivity can be restored (firewall rules, port forwarding).
- **Last direct err = "Timeout..."**: Direct connection attempts are timing out. This is expected when peers are behind restrictive NATs. The 30-second timeout is hardcoded.

## Configuring TURN for Pilot Deployments

### Setting up coturn

[coturn](https://github.com/coturn/coturn) is the recommended TURN server for pilot deployments.

**Minimal coturn configuration** (`/etc/turnserver.conf`):

```ini
# Listen on all interfaces
listening-port=3478

# The public IP of this server (replace with your actual IP)
external-ip=YOUR_PUBLIC_IP

# Relay address range
min-port=49152
max-port=65535

# Simple static credentials (acceptable for pilot only)
user=icn:icnpilot
realm=icn.coop

# Disable TLS for initial testing (add TLS for production)
no-tls
no-dtls

# Logging
log-file=/var/log/turnserver.log
verbose
```

**Start coturn**:
```bash
# Install
sudo apt install coturn

# Enable service
sudo systemctl enable coturn
echo 'TURNSERVER_ENABLED=1' | sudo tee /etc/default/coturn

# Start
sudo systemctl start coturn

# Verify it's listening
ss -ulnp | grep 3478
```

### Configuring ICN nodes to use TURN

TURN configuration is passed to the network actor at startup via `TurnConfig`:

```rust
use icn_net::TurnConfig;

let turn_config = TurnConfig::new("YOUR_TURN_SERVER:3478".parse()?)
    .with_username(Some("icn".to_string()))
    .with_password(Some("icnpilot".to_string()))
    .with_timeout(std::time::Duration::from_secs(10));
```

This is passed to `NetworkActor::spawn()` as the `turn_config` parameter. The session manager will attempt TURN allocation on startup.

### STUN configuration

STUN servers are passed as `stun_servers: Option<Vec<SocketAddr>>` to `NetworkActor::spawn()`. Default public STUN servers can be used for discovery:

```rust
let stun_servers = vec![
    "stun.l.google.com:19302".parse()?,
    "stun1.l.google.com:19302".parse()?,
];
```

## Pilot Validation Procedure

### Prerequisites
- Two hosts on different networks (different NATs)
- One coturn server reachable by both hosts
- ICN built from the `feat/c3-nat-traversal` branch or later

### Steps

1. **Start coturn** on a publicly reachable server (see above)

2. **Start Node A** with TURN config pointing to coturn:
   ```bash
   icnd --listen 0.0.0.0:4433 \
        --stun stun.l.google.com:19302 \
        --turn YOUR_COTURN_IP:3478
   ```

3. **Start Node B** on a different network with the same TURN config:
   ```bash
   icnd --listen 0.0.0.0:4433 \
        --stun stun.l.google.com:19302 \
        --turn YOUR_COTURN_IP:3478
   ```

4. **Check NAT status** on both nodes:
   ```bash
   icnctl network status
   ```
   Both should show a `Relay address` (TURN allocation succeeded).

5. **Trigger a dial** from Node A to Node B. If direct connectivity fails, the relay fallback should activate. Check:
   - `Last traversal: Relayed` on Node A
   - `Active relays: 1` on Node A
   - Messages are exchanged between the nodes

### What to look for in logs

- `TURN relay fallback enabled` — TURN config was accepted
- `TURN allocation successful` — TURN allocation from coturn worked
- `Direct dial failed, checking relay fallback` — direct path failed, relay logic activated
- `TURN relay connection established` — relay QUIC handshake completed
- `TURN relay proxy started` — per-peer proxy is running

### Common failures

| Symptom | Cause | Fix |
|---------|-------|-----|
| `TURN allocate request timed out` | coturn unreachable or firewall blocking UDP 3478 | Check firewall rules, verify coturn is running |
| `no TURN allocation` | TURN allocation failed at startup | Check coturn logs, verify credentials |
| `no peer relay candidate provided` | Peer didn't advertise a relay address | Ensure both nodes have TURN configured |
| `relay QUIC handshake failed` | TURN relay working but QUIC handshake failed through it | Check coturn relay port range, ensure UDP relay ports are open |

## VPN Fallback

If TURN is unavailable or unreliable, a VPN provides guaranteed connectivity:

### WireGuard

```bash
# On each node, create a WireGuard interface
# Peers see each other on the WireGuard subnet (e.g., 10.10.0.0/24)
# ICN nodes listen on the WireGuard IP instead of the public interface

icnd --listen 10.10.0.1:4433  # Node A on WireGuard IP
icnd --listen 10.10.0.2:4433  # Node B on WireGuard IP
```

With a VPN, nodes appear to be on the same network. No STUN or TURN is needed — direct dial works because the VPN handles NAT traversal at the network layer.

### Tailscale

```bash
# After installing Tailscale on both nodes:
icnd --listen $(tailscale ip -4):4433
```

Tailscale assigns stable IPs and handles NAT traversal transparently.

### When to use VPN vs TURN

| Scenario | Recommendation |
|----------|---------------|
| Pilot with 2-5 nodes | VPN (simpler, guaranteed connectivity) |
| Production with many cooperatives | TURN (no central VPN infrastructure needed) |
| Mixed environment | Both — TURN as primary, VPN as fallback for stubborn NATs |
| Symmetric NAT on both sides | VPN (TURN may not help if both sides are symmetric) |

## Current Limitations

- **No ICE**: ICN does not implement full ICE (Interactive Connectivity Establishment). The strategy is direct → TURN relay, with no STUN hole-punching attempts.
- **IPv4 only**: TURN relay proxy currently supports IPv4 peer addresses only.
- **No CLI flags yet**: TURN/STUN config is passed programmatically. CLI flags (`--turn`, `--stun`) are planned but not yet implemented.
- **Single TURN server**: Each node can be configured with one TURN server. Multi-server failover is not implemented.
- **30-second direct dial timeout**: The direct dial timeout is hardcoded. In environments where direct connectivity is known to be impossible, this adds latency to the first connection.
