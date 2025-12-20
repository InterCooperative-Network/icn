# ICN Docker Compose Deployment

Simple Docker Compose stack for local development and small-scale production deployments.

## Quick Start

```bash
cd deploy/compose
docker-compose up -d
```

## Services

| Service | Port | URL |
|---------|------|-----|
| ICN Node | 8000 | http://localhost:8000 |
| Dashboard | 8080 | http://localhost:8080 |
| API Docs | 3000 | http://localhost:3000 |
| Prometheus | 9091 | http://localhost:9091 |
| Grafana | 3001 | http://localhost:3001 |
| P2P Network | 3000/udp | - |

## Configuration

### Environment Variables

Create `.env` file:

```bash
JWT_SECRET=your-random-256-bit-key
GRAFANA_PASSWORD=your-secure-password
```

Generate JWT secret:

```bash
openssl rand -base64 32
```

### Volumes

- `icn-data`: Node data (ledger, keys, state)
- `prometheus-data`: Metrics (30 days retention)
- `grafana-data`: Dashboards and config

## Usage

### Start Stack

```bash
docker-compose up -d
```

### View Logs

```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f icnd
```

### Stop Stack

```bash
docker-compose down
```

### Restart Service

```bash
docker-compose restart icnd
```

### Update Services

```bash
docker-compose pull
docker-compose up -d
```

## Accessing Services

### API Gateway

```bash
curl http://localhost:8000/health
```

### Dashboard

Visit: http://localhost:8080

### API Documentation

Visit: http://localhost:3000

### Grafana

1. Visit: http://localhost:3001
2. Login: admin / (your GRAFANA_PASSWORD)
3. Explore dashboards

### Prometheus

Visit: http://localhost:9091

## Data Persistence

### Backup

```bash
# Backup all volumes
docker run --rm -v icn-data:/data -v $(pwd):/backup alpine tar czf /backup/icn-backup.tar.gz /data

# Backup specific service
docker-compose exec icnd tar czf - /data/icn > icn-backup.tar.gz
```

### Restore

```bash
# Restore from backup
docker run --rm -v icn-data:/data -v $(pwd):/backup alpine tar xzf /backup/icn-backup.tar.gz -C /
```

## Troubleshooting

### Service Won't Start

```bash
# Check logs
docker-compose logs icnd

# Check status
docker-compose ps

# Restart
docker-compose restart icnd
```

### Port Already in Use

Edit `docker-compose.yml` and change the port mapping:

```yaml
ports:
  - "8001:8000"  # Changed from 8000:8000
```

### Reset Everything

```bash
docker-compose down -v
docker-compose up -d
```

## Production Use

For production, consider:

1. Use secrets management
2. Enable TLS/SSL
3. Set up reverse proxy (nginx)
4. Configure backups
5. Enable monitoring alerts
6. Use Docker Swarm or Kubernetes

See `../kubernetes/` for production-grade deployment.

## License

MIT
