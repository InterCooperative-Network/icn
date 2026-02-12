# ICN Gateway - Quick Start Guide

## Fastest Paths

### Option A: One-click demo from repo root (recommended)

```bash
# From the repo root (parent of icn/):
./demo/scripts/run-tool-library-demo.sh
```

This starts:
- Gateway on `http://localhost:8080`
- Pilot UI on `http://localhost:3000`

### Option B: Gateway-only manual run from `icn/`

```bash
cd icn
cargo build --release -p icnd -p icnctl
export ICN_PASSPHRASE="your-passphrase-here"
export ICN_GATEWAY_JWT_SECRET="$(openssl rand -hex 32)"
./target/release/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
```

Then open health endpoint: `http://localhost:9090/v1/health`

## Prerequisites

```bash
# If you do not have an identity yet
./target/release/icnctl id init
./target/release/icnctl id show
```

## Required Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `ICN_PASSPHRASE` | Keystore passphrase | `"my-secure-pass"` |
| `ICN_GATEWAY_JWT_SECRET` | JWT signing secret (min 32 bytes) | `$(openssl rand -hex 32)` |
| `ICN_STATIC_DIR` | Optional static files path override | `"/custom/path"` |

## Manual Start Modes

### Interactive

```bash
export ICN_GATEWAY_JWT_SECRET="$(openssl rand -hex 32)"
./target/release/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
```

### Non-interactive

```bash
export ICN_PASSPHRASE="your-passphrase"
export ICN_GATEWAY_JWT_SECRET="$(openssl rand -hex 32)"
./target/release/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
```

## Using the Web UI

For full demo UX, use `./demo/scripts/run-tool-library-demo.sh`.

If you run gateway manually on `9090`, set UI gateway URL to `http://localhost:9090` at login.

## Common Commands

```bash
cargo build --release
cargo build --release --bin icnd --bin icnctl
cargo test
./target/release/icnctl id show
curl http://localhost:9090/v1/health
```

## Troubleshooting

### Gateway fails to start

- Verify JWT secret length is at least 32 bytes.
- Check identity/passphrase setup (`icnctl id show`, `ICN_PASSPHRASE`).
- Check logs from the terminal running `icnd`.

### Port conflict

```bash
lsof -i :9090
lsof -i :8080
```

Use a different bind port if needed.

## Next Steps

- Run demo quick checks: `./demo/scripts/quick-test.sh`
- Read demo entry docs: `../docs/demo/README.md`
- Review architecture: `../docs/ARCHITECTURE.md`
