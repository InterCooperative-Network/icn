---
paths:
  - "deploy/**"
  - "Dockerfile*"
  - "docker-compose*"
---

# Deployment Rules

## Security

- **Never commit secrets** - use placeholders (`__PLACEHOLDER__`) or environment variables
- CI checks deployment manifests for leaked secrets
- Keystore passphrases are runtime-only (never in config files)

## Devnet

- 3-node devnet in `deploy/devnet/docker-compose.yml`
- Nodes: node-a (8080/9000/9090), node-b (8081/9001/9091), node-c (8082/9002/9092)
- Lifecycle via `deploy/devnet/Makefile` (up/down/logs/status/test)

## K3s Production

- Deploy via `deploy/k8s/Makefile`
- Always create backup before deploying: `make backup`
- Verify health after deploy: `make verify-health`
- Rollback available: `make rollback`

## Changes to Deploy Manifests

1. Check for secrets: `grep -rn 'password\|secret\|token\|key' deploy/ --include='*.yml' --include='*.yaml'`
2. Verify placeholder pattern is used for sensitive values
3. Update deployment docs if behavior changes
4. Test in devnet before K3s
