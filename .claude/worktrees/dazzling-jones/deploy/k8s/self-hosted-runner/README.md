# Self-Hosted GitHub Actions Runner for ICN

This guide sets up a self-hosted GitHub Actions runner on your homelab for automated CI/CD.

## Prerequisites

- Linux server on the same network as K3s (or the K3s control plane itself)
- SSH access to the K3s cluster
- GitHub repository admin access

## Option A: Run on K3s Control Plane (Recommended)

Since k3s-control (10.8.10.40) already has kubectl access, it's the ideal location.

### 1. Create Runner User

```bash
ssh ubuntu@10.8.10.40

# Create dedicated runner user
sudo useradd -m -s /bin/bash github-runner
sudo usermod -aG sudo github-runner

# Allow passwordless sudo for kubectl
echo "github-runner ALL=(ALL) NOPASSWD: /usr/local/bin/kubectl, /usr/local/bin/k3s, /usr/local/bin/crictl" | sudo tee /etc/sudoers.d/github-runner
```

### 2. Install Dependencies

```bash
sudo su - github-runner

# Install dependencies
sudo apt-get update
sudo apt-get install -y curl jq docker.io

# Add to docker group (for building images)
sudo usermod -aG docker github-runner
```

### 3. Download and Configure Runner

Go to: https://github.com/InterCooperative-Network/icn/settings/actions/runners/new

Select **Linux** and **x64**, then run the commands shown:

```bash
# Create runner directory
mkdir actions-runner && cd actions-runner

# Download (use version from GitHub UI)
curl -o actions-runner-linux-x64-2.321.0.tar.gz -L https://github.com/actions/runner/releases/download/v2.321.0/actions-runner-linux-x64-2.321.0.tar.gz
tar xzf ./actions-runner-linux-x64-2.321.0.tar.gz

# Configure (use token from GitHub UI)
./config.sh --url https://github.com/InterCooperative-Network/icn --token YOUR_TOKEN_HERE

# Configure with labels
./config.sh --url https://github.com/InterCooperative-Network/icn \
  --token YOUR_TOKEN_HERE \
  --name "homelab-runner" \
  --labels "self-hosted,linux,x64,homelab,k3s" \
  --work "_work"
```

### 4. Install as Service

```bash
# Install and start service
sudo ./svc.sh install
sudo ./svc.sh start

# Check status
sudo ./svc.sh status
```

### 5. Verify Runner is Online

Go to: https://github.com/InterCooperative-Network/icn/settings/actions/runners

You should see "homelab-runner" with status "Idle".

## Option B: Run as Kubernetes Deployment

For a more cloud-native approach, run the runner as a K8s pod.

See: https://github.com/actions/actions-runner-controller

## Security Considerations

1. **Network Access**: The runner has access to your K3s cluster
2. **Secrets**: Don't store secrets in workflows; use GitHub Secrets
3. **Isolation**: Consider running untrusted workflows in containers
4. **Updates**: Keep the runner updated for security patches

## Troubleshooting

```bash
# Check runner logs
sudo journalctl -u actions.runner.InterCooperative-Network-icn.homelab-runner -f

# Restart runner
sudo ./svc.sh restart

# Check connectivity
curl -s https://api.github.com | head -5
```

## Uninstall

```bash
cd ~/actions-runner
sudo ./svc.sh stop
sudo ./svc.sh uninstall
./config.sh remove --token YOUR_TOKEN_HERE
```
