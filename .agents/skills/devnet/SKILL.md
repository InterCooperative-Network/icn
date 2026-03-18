---
name: devnet
description: Manage the ICN devnet (3-node Docker Compose cluster). Operations - up, down, logs, status, test, restart.
argument-hint: "<up|down|logs|status|test|restart>"
user-invocable: true
allowed-tools: "Bash, Read"
---

Manage the ICN devnet environment.

## Operations

Based on `$ARGUMENTS`:

### `up` (default if no argument)
```bash
cd /home/ubuntu/projects/icn/deploy/devnet && make up
```
Wait for nodes to be healthy, then run `make status`.

### `down`
```bash
cd /home/ubuntu/projects/icn/deploy/devnet && make down
```

### `logs`
```bash
cd /home/ubuntu/projects/icn/deploy/devnet && make logs
```
If following specific node: `docker compose logs -f node-a`

### `status`
```bash
cd /home/ubuntu/projects/icn/deploy/devnet && make status
```
Show container status and health.

### `test`
```bash
cd /home/ubuntu/projects/icn/icn && cargo test -p icn-core --test devnet_smoke -- --nocapture
```
Run devnet smoke tests (reachability, unique DIDs, gossip connectivity).

### `restart`
```bash
cd /home/ubuntu/projects/icn/deploy/devnet && make down && make up
```

## Node Reference

| Node | REST Port | P2P Port | Metrics Port |
|------|-----------|----------|-------------|
| node-a | 8080 | 9000 | 9090 |
| node-b | 8081 | 9001 | 9091 |
| node-c | 8082 | 9002 | 9092 |

## Troubleshooting

If nodes fail to start:
1. Check Docker is running: `docker ps`
2. Check for port conflicts: `ss -tlnp | grep -E '808[0-2]|900[0-2]|909[0-2]'`
3. Rebuild images: `cd deploy/devnet && docker compose build --no-cache`
4. Check logs: `docker compose logs --tail=50`
