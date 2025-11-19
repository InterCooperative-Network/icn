# ICN Deployment

Tools and configuration for deploying ICN (Intercooperative Network) pilots.

## Deployment Options

- **Native/Bare-metal**: Direct installation with systemd (recommended for production)
- **Docker Compose**: Containerized stack with monitoring

---

## Native Installation

For production deployments on Linux servers.

### Quick Install

```bash
# Clone the repository
git clone https://github.com/InterCooperative-Network/icn.git
cd icn

# Run the installer (requires root)
sudo deploy/install.sh
```

### Manual Installation

1. **Build from source**:
   ```bash
   cd icn
   cargo build --release
   ```

2. **Copy binaries**:
   ```bash
   sudo cp target/release/icnd /usr/local/bin/
   sudo cp target/release/icnctl /usr/local/bin/
   ```

3. **Create user and directories**:
   ```bash
   sudo useradd --system --home-dir /var/lib/icn icn
   sudo mkdir -p /var/lib/icn /etc/icn
   sudo chown icn:icn /var/lib/icn
   ```

4. **Configure environment**:
   ```bash
   sudo cp deploy/icnd.env.example /etc/icn/icnd.env
   sudo chmod 600 /etc/icn/icnd.env
   # Edit and set JWT_SECRET
   sudo nano /etc/icn/icnd.env
   ```

5. **Install systemd service**:
   ```bash
   sudo cp deploy/icnd.service /etc/systemd/system/
   sudo systemctl daemon-reload
   ```

6. **Initialize identity**:
   ```bash
   sudo -u icn icnctl --data-dir /var/lib/icn id init
   ```

7. **Start service**:
   ```bash
   sudo systemctl enable icnd
   sudo systemctl start icnd
   ```

### Health Monitoring

```bash
# Check service status
icn-health-check

# JSON output for monitoring systems
icn-health-check --json
```

### Files Installed

| Path | Description |
|------|-------------|
| `/usr/local/bin/icnd` | ICN daemon |
| `/usr/local/bin/icnctl` | CLI tool |
| `/usr/local/bin/icn-health-check` | Health check script |
| `/etc/icn/icnd.env` | Environment configuration |
| `/var/lib/icn/` | Data directory |
| `/usr/share/icn/static/` | Web UI files |

---

## Docker Deployment

Docker Compose configuration for deploying a complete ICN pilot stack.

### Components

- **icnd**: ICN daemon with gateway API enabled
- **prometheus**: Metrics collection
- **grafana**: Monitoring dashboards
- **web-ui**: Nginx serving the pilot web interface

## Quick Start

### Automated Setup

The easiest way to get started is with the quickstart script:

```bash
cd deploy
./quickstart.sh "My Timebank"
```

This will:
- Configure environment with a random JWT secret
- Build and start all Docker containers
- Initialize your identity
- Display access URLs and next steps

### Manual Setup

If you prefer manual configuration:

#### 1. Configure

```bash
cd deploy

# Copy and edit environment file
cp .env.example .env

# IMPORTANT: Set a strong JWT secret
# Generate one with: openssl rand -hex 32
vim .env
```

### 2. Build and Start

```bash
# Build ICN daemon image
docker-compose build

# Start all services
docker-compose up -d

# Check status
docker-compose ps
```

#### 3. Initialize Identity

```bash
# Create identity for the daemon
docker-compose exec icnd icnctl id init

# Show the DID
docker-compose exec icnd icnctl id show
```

#### 4. Get Authentication Token

```bash
# Get a JWT token for the web UI
docker-compose exec icnd icnctl auth token --coop my-coop

# Copy the token and use it in the web UI
```

#### 5. Seed Demo Data (Optional)

```bash
# From the sdk/typescript directory
cd ../sdk/typescript
npm install
npm run seed -- --gateway http://localhost:8080 --token <your-token>
```

### 6. Access Services

| Service | URL | Description |
|---------|-----|-------------|
| Web UI | http://localhost:3000 | Timebank interface |
| Gateway API | http://localhost:8080 | REST/WebSocket API |
| Grafana | http://localhost:3001 | Monitoring (admin/admin) |
| Prometheus | http://localhost:9091 | Metrics |

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `JWT_SECRET` | Secret for JWT token signing | (required) |
| `GRAFANA_PASSWORD` | Grafana admin password | admin |

### ICN Configuration

Edit `config/icn.toml` to customize:
- Network settings (ports, mDNS)
- Rate limiting
- Gateway options

### Adding Bootstrap Peers

For distributed deployments, add bootstrap peers in `config/icn.toml`:

```toml
[network]
bootstrap_peers = [
    "icn://did:icn:abc@192.168.1.10:7777",
    "icn://did:icn:def@192.168.1.11:7777"
]
```

## Operations

### View Logs

```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f icnd
```

### Restart Services

```bash
# Restart all
docker-compose restart

# Restart specific service
docker-compose restart icnd
```

### Backup Data

```bash
# Backup ICN data
docker-compose exec icnd icnctl backup create -o /root/.icn/backup.tar.enc

# Copy backup out of container
docker cp icn-daemon:/root/.icn/backup.tar.enc ./backup-$(date +%Y%m%d).tar.enc
```

### Restore Data

```bash
# Copy backup into container
docker cp backup.tar.enc icn-daemon:/root/.icn/backup.tar.enc

# Restore
docker-compose exec icnd icnctl backup restore -i /root/.icn/backup.tar.enc
```

### Stop Services

```bash
# Stop but keep data
docker-compose down

# Stop and remove data volumes
docker-compose down -v
```

## Production Deployment

For production use:

### 1. Security

- Use strong random `JWT_SECRET` (32+ characters)
- Change Grafana admin password
- Consider TLS termination with reverse proxy
- Restrict port exposure (remove localhost binding)

### 2. Persistence

Data is stored in Docker volumes:
- `icn-data`: ICN keystore, ledger, config
- `prometheus-data`: Metrics history
- `grafana-data`: Dashboard customizations

Back up these volumes regularly.

### 3. Resource Limits

Add resource limits to docker-compose.yml:

```yaml
services:
  icnd:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 2G
```

### 4. External Access

For external access, add a reverse proxy with TLS:

```yaml
services:
  caddy:
    image: caddy:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy-data:/data
```

Example Caddyfile:
```
timebank.example.com {
    reverse_proxy web-ui:80
}

api.timebank.example.com {
    reverse_proxy icnd:8080
}
```

## Troubleshooting

### Container won't start

Check logs:
```bash
docker-compose logs icnd
```

Common issues:
- Missing JWT_SECRET in .env
- Port already in use
- Insufficient permissions

### Can't connect to API

1. Check icnd is healthy: `docker-compose ps`
2. Check gateway is enabled in config
3. Test health endpoint: `curl http://localhost:8080/health`

### Grafana shows no data

1. Check Prometheus is scraping: http://localhost:9091/targets
2. Verify datasource in Grafana: Configuration > Data Sources
3. Check icnd metrics: `curl http://localhost:9090/metrics`

### Web UI can't connect

1. Verify API is accessible: `curl http://localhost:8080/health`
2. Check browser console for CORS errors
3. Ensure nginx is proxying correctly

## Files

```
deploy/
├── install.sh              # Native installation script
├── quickstart.sh           # Docker quickstart script
├── health-check.sh         # Health monitoring script
├── icnd.service            # systemd service file
├── icnd.env.example        # Environment template
├── docker-compose.yml      # Docker compose file
├── Dockerfile.icnd         # ICN daemon image
├── .env.example            # Docker environment template
├── README.md               # This file
└── config/
    ├── icn.toml            # ICN configuration
    ├── prometheus.yml      # Prometheus config
    ├── nginx.conf          # Nginx config
    └── grafana/
        └── provisioning/   # Grafana auto-setup
```

## License

MIT OR Apache-2.0
