# ICN Examples

Welcome to the ICN examples directory! These tutorials and demos will help you get started with ICN.

## Getting Started

### Prerequisites

Before running any examples, ensure you have:

1. **Built ICN binaries:**
   ```bash
   cd icn/
   cargo build --release
   ```

2. **Binaries are available at:**
   - `icn/target/release/icnd` - The ICN daemon
   - `icn/target/release/icnctl` - CLI management tool

## Examples

### [01-quickstart](01-quickstart/)
**Difficulty: Beginner** | **Time: 5 minutes**

Get a two-node ICN network running on your local machine and test peer discovery, trust management, and network operations.

**What you'll learn:**
- Starting ICN nodes
- Initializing identities
- Peer discovery via mDNS
- Network status queries
- Trust graph basics

**Run it:**
```bash
cd examples/01-quickstart
./run.sh
```

### 02-docker (Coming Soon)
**Difficulty: Beginner** | **Time: 10 minutes**

Deploy ICN using Docker and Docker Compose with Prometheus monitoring.

**What you'll learn:**
- Containerized deployment
- Multi-node networking
- Monitoring with Prometheus
- Data persistence with volumes

### [contracts](contracts/)
**Difficulty: Intermediate** | **Time: 15 minutes**

Example CCL (Cooperative Contract Language) contracts.

**What you'll learn:**
- CCL syntax and semantics
- TimeBank example contract
- Contract JSON format

### [wasm-compute](wasm-compute/)
**Difficulty: Intermediate** | **Time: 10 minutes**

Build and deploy WebAssembly modules for ICN distributed compute.

**What you'll learn:**
- Writing WASM modules in Rust
- ICN host functions (log, timestamp)
- Submitting WASM tasks via CLI, REST API, and TypeScript SDK
- Size optimization for WASM

**Build it:**
```bash
cd examples/wasm-compute
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

**Run it:**
```bash
icnctl compute submit-wasm \
    --wasm target/wasm32-unknown-unknown/release/icn_wasm_example.wasm \
    --fuel 10000
```

### 03-contracts-advanced (Coming Soon)
**Difficulty: Intermediate** | **Time: 15 minutes**

Write, deploy, and invoke CCL contracts via the distributed compute layer.

**What you'll learn:**
- Contract deployment
- Capability system
- Fuel metering
- Payment settlement

### 04-ledger (Coming Soon)
**Difficulty: Intermediate** | **Time: 15 minutes**

Explore the mutual credit ledger with double-entry accounting.

**What you'll learn:**
- Creating ledger entries
- Multi-currency support
- Credit limits
- Balance queries
- Conflict resolution & quarantine

### 05-trust-network (Coming Soon)
**Difficulty: Intermediate** | **Time: 20 minutes**

Build a trust network with multiple nodes and explore transitive trust computation.

**What you'll learn:**
- Adding trust edges
- Trust score computation
- Trust classes (Isolated, Known, Partner, Federated)
- Access control via trust gating
- Trust graph queries

### 06-production-deployment (Coming Soon)
**Difficulty: Advanced** | **Time: 30 minutes**

Deploy ICN in a production environment with systemd, monitoring, and backup.

**What you'll learn:**
- Systemd service configuration
- Prometheus + Grafana monitoring
- Automated backups
- Security hardening
- Troubleshooting

## Support

If you encounter issues with any example:

1. Check the example's README for troubleshooting steps
2. Verify your build is up-to-date: `cargo build --release`
3. Check ICN logs: `journalctl -u icnd -f` (if running as service)
4. See [docs/troubleshooting.md](../docs/troubleshooting.md) (coming soon)

## Contributing Examples

Have an idea for a useful example? Contributions are welcome!

**Good example characteristics:**
- Clear learning objective
- Self-contained (no external dependencies)
- Well-commented
- Includes cleanup steps
- Runs in < 30 minutes

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines (coming soon).
