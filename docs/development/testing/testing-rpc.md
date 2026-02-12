# Testing RPC Communication

This guide explains how to test the JSON-RPC communication between `icnd` (daemon) and `icnctl` (CLI).

## Prerequisites

1. Build both binaries:
   ```bash
   cargo build --bin icnd
   cargo build --bin icnctl
   ```

2. Create an identity (required for daemon to spawn network actors and RPC server):
   ```bash
   ./target/debug/icnctl id init
   ```

   Follow the prompts to set a passphrase. This will create `~/.icn/identity.age`.

## Testing Procedure

### 1. Start the Daemon

In terminal 1:
```bash
./target/debug/icnd
```

You should see output like:
```
INFO icnd: ICNd starting
INFO icnd: Data directory: "/home/user/.local/share/icn"
INFO icn_core::supervisor: Supervisor starting
INFO icn_core::supervisor: Network actor spawned on 0.0.0.0:7777
INFO icn_core::supervisor: RPC server spawned on 127.0.0.1:5050
```

The daemon will prompt for your passphrase to unlock the keystore.

### 2. Test Network Commands

In terminal 2, run these commands:

#### Check Network Status
```bash
./target/debug/icnctl network status
```

Expected output:
```
Network Actor Status:

  Running:               true
  Listener address:      0.0.0.0:7777
```

#### View Network Statistics
```bash
./target/debug/icnctl network stats
```

Expected output:
```
Network Statistics:

  Peers discovered:      0
  Active connections:    0
  Total connections:     0
```

#### List Discovered Peers
```bash
./target/debug/icnctl network peers
```

Expected output (when no peers are available):
```
No peers discovered yet.

Tip: Ensure other ICN nodes are running on the network.
```

#### Dial a Peer (Example)
```bash
./target/debug/icnctl network dial did:icn:z6MkpTHR8VNs... --addr 192.168.1.100:7777
```

Note: This requires a valid peer DID and address.

## Testing Without an Identity

If you run `icnd` without creating an identity first, you'll see:
```
WARN icnd: No identity keystore found
WARN icnd: Run 'icnctl id init' to create an identity
WARN icnd: Daemon will run without Identity and Network actors
```

In this case, the RPC server won't be spawned and `icnctl network` commands will fail with:
```
Error: Failed to get status from daemon. Is icnd running?
```

## RPC Protocol Details

- **Protocol**: JSON-RPC 2.0 over HTTP/1.1
- **Server Address**: 127.0.0.1:5050
- **Transport**: HTTP POST requests with JSON payload

### Available RPC Methods

1. `network.peers` - List discovered peers
2. `network.dial` - Connect to a peer
3. `network.stats` - Get network statistics
4. `network.status` - Get network actor status

### Example Raw RPC Request

You can test the RPC server directly with curl:

```bash
curl -X POST http://127.0.0.1:5050 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "network.status",
    "params": {},
    "id": 1
  }'
```

Expected response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "running": true,
    "listen_addr": "0.0.0.0:7777"
  },
  "id": 1
}
```

## Troubleshooting

### "Failed to get status from daemon. Is icnd running?"

1. Check if `icnd` is running: `ps aux | grep icnd`
2. Verify the RPC server is listening: `lsof -i :5050`
3. Ensure you created an identity: `ls ~/.icn/identity.age`

### "Connection refused"

The daemon's RPC server only spawns when an identity exists. Run `icnctl id init` first.

### "No peers discovered yet"

mDNS discovery requires:
1. Other ICN nodes running on the same local network
2. Multicast DNS support in your network
3. Firewall rules allowing mDNS traffic (UDP port 5353)

## Next Steps

To test peer discovery and connections, you'll need to:
1. Run multiple `icnd` instances on the same network (different machines or VMs)
2. Each instance needs its own data directory: `icnd --data-dir ~/.icn-node2`
3. Peers should automatically discover each other via mDNS
4. Use `icnctl network peers` to verify discovery
5. Use `icnctl network dial <did>` to establish connections
