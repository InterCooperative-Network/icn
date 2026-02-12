# ICN Development Environment

## icn-dev VM

Dedicated development VM for ICN work, hosted on Hyperion (10.8.10.15).

| Setting | Value |
|---------|-------|
| **VMID** | 600 |
| **Name** | icn-dev |
| **IP** | 10.8.10.45 |
| **RAM** | 12GB |
| **CPU** | 6 vCPUs |
| **Disk** | 100GB |
| **OS** | Ubuntu 24.04.3 LTS |

### Access

```bash
# SSH
ssh ubuntu@10.8.10.45

# Web VSCode (code-server)
http://10.8.10.45:8443
# Password: 264a2290faa81c3b6d19fa4b
```

### Installed Tools

| Tool | Version | Notes |
|------|---------|-------|
| Rust | 1.92.0 | via rustup |
| Cargo | 1.92.0 | |
| kubectl | 1.35.0 | K3s kubeconfig configured |
| code-server | 4.107.1 | Web-based VSCode |
| gh | latest | GitHub CLI |
| Build tools | - | gcc, make, pkg-config, libssl-dev |

### Project Locations

```bash
# ICN source code
~/projects/icn

# K3s kubeconfig
~/.kube/config

# Cargo/Rust binaries
~/.cargo/bin
```

### Build & Test

```bash
cd ~/projects/icn/icn

# Build
cargo build --release

# Test
cargo test

# Run daemon locally
./target/debug/icnd
```

### K3s Access

The dev VM has kubectl configured to access the K3s cluster:

```bash
# Check cluster
kubectl get nodes

# ICN pods
kubectl -n icn get pods

# ICN daemon logs
kubectl -n icn logs deploy/icn-daemon --tail=50

# Show ICN identity
kubectl -n icn exec deploy/icn-daemon -- /usr/local/bin/icnctl id show
```

## Claude Code Setup

To set up Claude Code on the dev VM:

```bash
# SSH to dev VM
ssh ubuntu@10.8.10.45

# Install Claude Code
curl -fsSL https://claude.ai/code/install.sh | sh

# Configure GitHub CLI (for PRs)
gh auth login

# Navigate to project
cd ~/projects/icn

# Start Claude Code
claude
```

The ICN repo contains `CLAUDE.md` with all project-specific instructions.

## Network Diagram

```
Workstation (matt)
    |
    +-- SSH --> icn-dev (10.8.10.45) --> ~/projects/icn
    |              |
    |              +-- kubectl --> K3s Cluster
    |                                  |
    +-- SSH --> k3s-control (10.8.10.40)
                    |
                    +-- k3s-worker-1 (10.8.10.41)
                    +-- k3s-worker-2 (10.8.10.42)
                           |
                           +-- ICN pods (NFS from Atlas 10.8.10.25)
```

## Related Documentation

- [HOMELAB_DEPLOYMENT.md](./HOMELAB_DEPLOYMENT.md) - K3s cluster details
- [GETTING_STARTED.md](./GETTING_STARTED.md) - ICN quickstart guide
- [ARCHITECTURE.md](./ARCHITECTURE.md) - System architecture
