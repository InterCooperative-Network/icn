# Quick Start - Deploy ICN to K3s

Get ICN deployed on your K3s cluster in 5 minutes!

## Prerequisites Check

```bash
# 1. SSH access to K3s cluster
ssh ubuntu@10.8.10.40 "echo 'SSH works!'"

# 2. Docker installed
docker --version

# 3. In ICN repo
cd /home/matt/projects/icn
```

## First Time Setup (5 minutes)

### 1. Create Secrets (2 min)

```bash
cd deploy/k8s

# Copy example secret
cp secret.yaml.example secret.yaml

# Edit with your passphrase (use nano/vim)
nano secret.yaml
# Change CHANGE_ME to your actual ICN passphrase

# Deploy secrets
ssh ubuntu@10.8.10.40 "sudo kubectl apply -f -" < secret.yaml
```

### 2. Full Deployment (3 min)

```bash
cd deploy/k8s

# One command does everything!
make full-deploy-dev
```

This will:
- Build Docker image from your source code
- Sync image to all K3s nodes  
- Deploy ICN to the cluster

### 3. Verify Deployment

```bash
# Check status
make status

# Watch logs
make logs
```

## Daily Development Workflow

After making code changes:

```bash
cd /home/matt/projects/icn/deploy/k8s

# Re-deploy with git hash tag
make full-deploy-dev
```

That's it! Your changes are live.

## Common Commands

```bash
cd deploy/k8s

make status        # Check pod status
make logs          # Tail logs
make logs-recent   # Recent logs  
make restart       # Restart deployment
make help          # Show all commands
```

## Next Steps

- Read [README.md](README.md) for detailed documentation
- Read [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md) for advanced usage
- Check monitoring: `make status`

## Troubleshooting

**Pod won't start?**
```bash
make logs
# Check for passphrase errors, missing secrets, etc.
```

**Image not found?**
```bash
# Re-sync image
make sync
```

**Need to see more details?**
```bash
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn describe pod <pod-name>"
```

For more help, see [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md#troubleshooting).

