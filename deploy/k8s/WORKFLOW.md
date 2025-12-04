# Deployment Workflow

## Overview

**Everything runs on your local machine (WSL/Ubuntu)** and automatically syncs/deploys to the K3s cluster on Hyperion.

```
┌─────────────────────────────────────────────────────────────┐
│  Your Local Machine (WSL/Ubuntu)                            │
│                                                              │
│  1. You make code changes in /home/matt/projects/icn       │
│                                                              │
│  2. You run: make full-deploy-dev                          │
│     ↓                                                        │
│  3. Scripts build Docker image locally                      │
│     ↓                                                        │
│  4. Scripts sync image to cluster via SSH                   │
│     ↓                                                        │
│  5. Scripts apply Kubernetes manifests via SSH/kubectl      │
│                                                              │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        │ SSH Connection
                        ↓
┌─────────────────────────────────────────────────────────────┐
│  K3s Cluster on Hyperion (10.8.10.40)                       │
│                                                              │
│  • Image imported to containerd                              │
│  • Kubernetes manifests applied                              │
│  • ICN pod running with your changes                         │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Step-by-Step Flow

### 1. **Local Development**
You work on your local machine:
```bash
cd /home/matt/projects/icn
# Edit code, make changes, etc.
vim icn/src/...
```

### 2. **Build Locally**
The build script runs **on your machine**:
```bash
cd deploy/k8s
make build-dev

# This runs: ./scripts/build-image.sh <git-hash>
# Which executes: docker build ... (on your machine)
# Result: Docker image "icn:latest" in your local Docker
```

### 3. **Sync to Cluster**
The sync script **on your machine** exports the image and pushes it to the cluster:
```bash
make sync

# This runs: ./scripts/sync-image.sh <tag>
# Which:
#   1. Exports image from your Docker: docker save icn:latest -o /tmp/icn.tar
#   2. Copies to cluster via SSH: scp /tmp/icn.tar ubuntu@10.8.10.40:/tmp/
#   3. Imports on cluster nodes via SSH: ssh ubuntu@10.8.10.40 "sudo ctr images import ..."
```

### 4. **Deploy to Cluster**
The deploy script **on your machine** applies manifests to the cluster:
```bash
make deploy

# This runs: ./scripts/deploy.sh
# Which:
#   - Applies manifests via SSH: ssh ubuntu@10.8.10.40 "sudo kubectl apply ..."
#   - Updates deployment on the cluster
```

### 5. **Everything Happens Remotely**
The cluster receives:
- ✅ Docker image in containerd
- ✅ Updated Kubernetes resources
- ✅ Running pod with your new code

## Example: Complete Deployment

```bash
# On your local machine
cd /home/matt/projects/icn

# Make some code changes
vim icn/crates/icn-core/src/lib.rs
git add .
git commit -m "Fix bug in gossip protocol"

# Deploy (all from your local machine!)
cd deploy/k8s
make full-deploy-dev

# Output you'll see:
# Building ICN Docker image...        ← Happens locally
# Syncing image to K3s cluster...     ← Pushes to cluster via SSH
# Deploying to K3s cluster...         ← Applies via SSH/kubectl
# ✓ Deployment complete!
```

## What Runs Where?

| Action | Where It Runs | How |
|--------|---------------|-----|
| Code editing | **Local machine** | Your editor |
| Docker build | **Local machine** | `docker build` |
| Image export | **Local machine** | `docker save` |
| Image copy | **Local → Cluster** | `scp` via SSH |
| Image import | **Cluster nodes** | Via SSH: `sudo ctr import` |
| kubectl apply | **Cluster** | Via SSH: `sudo kubectl apply` |
| Pod runs | **Cluster** | Kubernetes |

## No Need To SSH Manually

You **never need to SSH into the cluster** manually. Everything is automated:

```bash
# ❌ You DON'T need to do this:
ssh ubuntu@10.8.10.40
cd /some/path
docker build ...
kubectl apply ...

# ✅ You just do this:
cd deploy/k8s
make full-deploy-dev
```

The scripts handle all the SSH connections automatically.

## What About Secrets?

Secrets are the **one exception** - you need to create the file locally first:

```bash
# On your local machine
cd deploy/k8s
cp secret.yaml.example secret.yaml
nano secret.yaml  # Add your passphrase

# Then push it once:
ssh ubuntu@10.8.10.40 "sudo kubectl apply -f -" < secret.yaml

# After that, it stays on the cluster
```

## Development Workflow

**Your typical workflow:**

1. **Edit code** on local machine
2. **Test locally** (optional):
   ```bash
   cd /home/matt/projects/icn/icn
   cargo test
   cargo build --release
   ```
3. **Deploy to cluster**:
   ```bash
   cd /home/matt/projects/icn/deploy/k8s
   make full-deploy-dev
   ```
4. **Check status** (from local machine):
   ```bash
   make status  # Runs: ssh ubuntu@10.8.10.40 "sudo kubectl ..."
   ```
5. **View logs** (from local machine):
   ```bash
   make logs  # Runs: ssh ubuntu@10.8.10.40 "sudo kubectl logs ..."
   ```

## Remote Operations

Even checking status and logs happens **from your local machine**:

```bash
# All of these run on your local machine,
# but execute commands on the cluster via SSH:

make status          # ssh ... kubectl get pods
make logs           # ssh ... kubectl logs
make restart        # ssh ... kubectl rollout restart
```

## Summary

✅ **Everything runs on your local machine**  
✅ **Automatically pushes/syncs to cluster via SSH**  
✅ **No manual SSH needed**  
✅ **One command deploys everything**  

Just edit code locally and run `make full-deploy-dev` - that's it!

