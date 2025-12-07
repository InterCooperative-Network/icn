#!/usr/bin/env bash
#
# Setup GitHub Actions self-hosted runner on K3s control plane
#
# Usage:
#   ./setup-runner.sh <github-token>
#
# Get the token from:
#   https://github.com/InterCooperative-Network/icn/settings/actions/runners/new
#

set -e

GITHUB_TOKEN="${1:-}"
RUNNER_VERSION="2.321.0"
RUNNER_NAME="homelab-runner"
RUNNER_LABELS="self-hosted,linux,x64,homelab,k3s"
REPO_URL="https://github.com/InterCooperative-Network/icn"
K3S_HOST="${K3S_HOST:-ubuntu@10.8.10.40}"

if [ -z "$GITHUB_TOKEN" ]; then
    echo "Usage: $0 <github-token>"
    echo ""
    echo "Get the token from:"
    echo "  https://github.com/InterCooperative-Network/icn/settings/actions/runners/new"
    exit 1
fi

echo "╔════════════════════════════════════════════════════════════╗"
echo "║       GitHub Actions Runner Setup                          ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Target: ${K3S_HOST}"
echo "Runner: ${RUNNER_NAME}"
echo ""

# Check SSH connectivity
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 1: Checking connectivity..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if ! ssh -o ConnectTimeout=5 -o BatchMode=yes "$K3S_HOST" "echo 'SSH OK'" &>/dev/null; then
    echo "❌ Cannot connect to ${K3S_HOST}"
    exit 1
fi
echo "✓ SSH connection OK"

# Install dependencies
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 2: Installing dependencies..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
ssh "$K3S_HOST" "sudo apt-get update && sudo apt-get install -y curl jq docker.io"
echo "✓ Dependencies installed"

# Setup runner
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 3: Setting up runner..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

ssh "$K3S_HOST" bash << RUNNER_SETUP
set -e

# Create runner directory
mkdir -p ~/actions-runner
cd ~/actions-runner

# Download runner if not present
if [ ! -f ./config.sh ]; then
    echo "Downloading runner v${RUNNER_VERSION}..."
    curl -sL -o actions-runner.tar.gz "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz"
    tar xzf actions-runner.tar.gz
    rm actions-runner.tar.gz
fi

# Configure runner
echo "Configuring runner..."
./config.sh --url "${REPO_URL}" \
    --token "${GITHUB_TOKEN}" \
    --name "${RUNNER_NAME}" \
    --labels "${RUNNER_LABELS}" \
    --work "_work" \
    --replace \
    --unattended

echo "✓ Runner configured"
RUNNER_SETUP

echo "✓ Runner setup complete"

# Install as service
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 4: Installing as service..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

ssh "$K3S_HOST" bash << SERVICE_SETUP
cd ~/actions-runner

# Stop existing service if running
sudo ./svc.sh stop 2>/dev/null || true
sudo ./svc.sh uninstall 2>/dev/null || true

# Install and start
sudo ./svc.sh install
sudo ./svc.sh start

# Check status
sudo ./svc.sh status
SERVICE_SETUP

echo "✓ Service installed and running"

# Verify
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 5: Verification..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

sleep 3
ssh "$K3S_HOST" "cd ~/actions-runner && sudo ./svc.sh status"

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║       Runner Setup Complete!                               ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "The runner should now appear at:"
echo "  https://github.com/InterCooperative-Network/icn/settings/actions/runners"
echo ""
echo "Next steps:"
echo "  1. Verify runner is 'Idle' in GitHub UI"
echo "  2. Copy docker-build-deploy.yml to .github/workflows/"
echo "  3. Push changes to trigger a build"
echo ""
echo "To check logs:"
echo "  ssh ${K3S_HOST} 'journalctl -u actions.runner.* -f'"
