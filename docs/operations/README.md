# Operations Documentation

This directory contains documentation for deploying, operating, and monitoring ICN nodes and clusters.

## Directory Structure

### `deployment/`
Deployment guides and operational procedures:
- [HOMELAB_DEPLOYMENT.md](deployment/HOMELAB_DEPLOYMENT.md) - Current K3s deployment guide (active since 2025-12-03)
- Monitoring and verification guides
- Distributed tracing configuration
- Incident response procedures

## Quick Links

- **Current Deployment**: [HOMELAB_DEPLOYMENT.md](deployment/HOMELAB_DEPLOYMENT.md)
- **Monitoring**: See deployment guide for Prometheus/Grafana setup
- **Runbooks**: [../ops/runbooks/](../ops/runbooks/) - Operational runbooks

## Related Documentation

- [../observability/](../observability/) - Observability and metrics documentation
- [../deployment/](../deployment/) - Legacy deployment files (may contain historical guides)
- [STATE.md](../STATE.md) - Current system state

## Deployment Status

**Current Status**: ICN daemon running on K3s cluster (deployed 2025-12-03, automated 2025-12-04).

See [HOMELAB_DEPLOYMENT.md](deployment/HOMELAB_DEPLOYMENT.md) for:
- Cluster details and node identity
- Quick access commands
- CI/CD pipeline information
- Monitoring dashboards
- Pilot testing status

## Contributing

When adding operational documentation:
1. Place deployment guides in `deployment/`
2. Ensure guides are tested and validated
3. Update this README when adding new guides
4. Archive superseded guides to `../../archive/`
