# ICN Runtime Federation Devnet

This directory contains everything needed to run a local development federation for ICN Runtime.

## Quick Start

To start a local 3-node federation:

```bash
# Build the Docker image
docker-compose build

# Start the federation
docker-compose up -d
```

This will start:
- 3 federation nodes (genesis + 2 validators)
- Prometheus for metrics collection
- Grafana for visualization (accessible at http://localhost:3000)

## Manual Setup

If you want to run the federation without Docker:

1. Create the required directories:
   ```bash
   mkdir -p data/genesis data/node1 data/node2
   ```

2. Run the initialization script:
   ```bash
   ./scripts/init_federation.sh
   ```

3. Check the federation status:
   ```bash
   icn-runtime federation status --federation <FEDERATION_ID>
   ```

## Federation Configurations

This devnet includes several federation configuration examples:

- `config/federation_icn.toml` - Simple development federation
- `config/cooperative_alpha.toml` - Cooperative federation model with equal governance
- `config/community_beta.toml` - Community federation with permissioned validators and public observers

## Working with the Federation

### Checking Status

```bash
# For Docker setup
docker exec icn-federation-genesis icn-runtime federation status --federation <FEDERATION_ID>

# For manual setup
icn-runtime federation status --federation <FEDERATION_ID>
```

### Verifying Federation Integrity

```bash
# For Docker setup
docker exec icn-federation-genesis icn-runtime federation verify --federation <FEDERATION_ID>

# For manual setup
icn-runtime federation verify --federation <FEDERATION_ID>
```

### Viewing Metrics

Prometheus is available at http://localhost:9200
Grafana is available at http://localhost:3000 (login: admin/admin)

## Debugging

Logs for each node can be viewed with:

```bash
# For Docker setup
docker logs icn-federation-genesis
docker logs icn-federation-node1
docker logs icn-federation-node2

# For manual setup
tail -f data/genesis/node.log
tail -f data/node1/node.log
tail -f data/node2/node.log
``` 