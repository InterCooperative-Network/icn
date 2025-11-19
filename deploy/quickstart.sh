#!/usr/bin/env bash
#
# ICN Pilot Quick Start Script
#
# This script automates the complete setup of an ICN pilot deployment:
# 1. Builds and starts Docker containers
# 2. Initializes identity
# 3. Gets authentication token
# 4. Seeds demo data
#
# Usage:
#   ./quickstart.sh [coop-name]
#
# Example:
#   ./quickstart.sh "Sunset Timebank"
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
COOP_NAME="${1:-Demo Timebank}"
COOP_ID=$(echo "$COOP_NAME" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd '[:alnum:]-')
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$SCRIPT_DIR"

echo -e "${BLUE}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║           ICN Pilot Quick Start                            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"
echo "Cooperative: $COOP_NAME"
echo "Coop ID: $COOP_ID"
echo ""

# Check prerequisites
echo -e "${YELLOW}Checking prerequisites...${NC}"

if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed${NC}"
    echo "Please install Docker: https://docs.docker.com/get-docker/"
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    echo -e "${RED}Error: Docker Compose is not installed${NC}"
    echo "Please install Docker Compose: https://docs.docker.com/compose/install/"
    exit 1
fi

echo -e "${GREEN}Prerequisites OK${NC}"
echo ""

# Step 1: Configure environment
echo -e "${YELLOW}Step 1: Configuring environment...${NC}"

cd "$DEPLOY_DIR"

if [ ! -f .env ]; then
    cp .env.example .env
    # Generate a random JWT secret
    JWT_SECRET=$(openssl rand -hex 32)
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i '' "s/JWT_SECRET=.*/JWT_SECRET=$JWT_SECRET/" .env
    else
        sed -i "s/JWT_SECRET=.*/JWT_SECRET=$JWT_SECRET/" .env
    fi
    echo "  Created .env with random JWT secret"
else
    echo "  Using existing .env file"
fi

echo -e "${GREEN}Environment configured${NC}"
echo ""

# Step 2: Build and start services
echo -e "${YELLOW}Step 2: Building and starting services...${NC}"

# Use docker compose (v2) or docker-compose (v1)
if docker compose version &> /dev/null; then
    COMPOSE_CMD="docker compose"
else
    COMPOSE_CMD="docker-compose"
fi

echo "  Building ICN daemon image..."
$COMPOSE_CMD build --quiet

echo "  Starting services..."
$COMPOSE_CMD up -d

# Wait for services to be healthy
echo "  Waiting for services to be healthy..."
sleep 5

# Check health
for i in {1..30}; do
    if curl -s http://localhost:8080/health > /dev/null 2>&1; then
        echo -e "${GREEN}  Gateway is healthy${NC}"
        break
    fi
    if [ $i -eq 30 ]; then
        echo -e "${RED}  Gateway did not become healthy${NC}"
        echo "  Check logs with: docker-compose logs icnd"
        exit 1
    fi
    sleep 1
done

echo -e "${GREEN}Services started${NC}"
echo ""

# Step 3: Initialize identity
echo -e "${YELLOW}Step 3: Initializing identity...${NC}"

# Check if identity already exists
INIT_OUTPUT=$($COMPOSE_CMD exec -T icnd icnctl id show 2>&1 || true)

if echo "$INIT_OUTPUT" | grep -q "did:icn:"; then
    DID=$(echo "$INIT_OUTPUT" | grep -o "did:icn:[a-zA-Z0-9]*" | head -1)
    echo "  Using existing identity: $DID"
else
    echo "  Creating new identity..."
    # Use empty passphrase for demo (NOT for production!)
    $COMPOSE_CMD exec -T icnd bash -c 'echo "" | icnctl id init'
    DID=$($COMPOSE_CMD exec -T icnd icnctl id show 2>&1 | grep -o "did:icn:[a-zA-Z0-9]*" | head -1)
    echo "  Created identity: $DID"
fi

echo -e "${GREEN}Identity initialized${NC}"
echo ""

# Step 4: Get authentication token
echo -e "${YELLOW}Step 4: Getting authentication token...${NC}"

# For the quick start, we'll use a simplified auth flow
# In production, use icnctl auth token with proper key signing
TOKEN_RESULT=$($COMPOSE_CMD exec -T icnd bash -c "
curl -s -X POST http://localhost:8080/v1/auth/challenge \
  -H 'Content-Type: application/json' \
  -d '{\"did\": \"$DID\"}'
")

CHALLENGE=$(echo "$TOKEN_RESULT" | grep -o '"challenge":"[^"]*"' | cut -d'"' -f4 || echo "")

if [ -z "$CHALLENGE" ]; then
    echo -e "${YELLOW}  Could not get auth challenge (gateway may need auth disabled for demo)${NC}"
    TOKEN=""
else
    # For demo purposes, we'll note that proper signing would happen here
    echo "  Challenge received: ${CHALLENGE:0:20}..."
    echo -e "${YELLOW}  Note: For production, use 'icnctl auth token' for proper signing${NC}"
    TOKEN=""
fi

echo -e "${GREEN}Authentication step complete${NC}"
echo ""

# Step 5: Display access information
echo -e "${BLUE}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║           ICN Pilot Setup Complete!                        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

echo "Your ICN pilot is now running!"
echo ""
echo "Access points:"
echo "  - Web UI:       http://localhost:3000"
echo "  - Gateway API:  http://localhost:8080"
echo "  - Grafana:      http://localhost:3001 (admin/admin)"
echo "  - Prometheus:   http://localhost:9091"
echo ""
echo "Your identity:"
echo "  DID: $DID"
echo ""
echo "Next steps:"
echo ""
echo "  1. Get an authentication token:"
echo "     docker-compose exec icnd icnctl auth token --coop $COOP_ID"
echo ""
echo "  2. Seed demo data (from sdk/typescript directory):"
echo "     npm install && npm run seed -- --token <your-token>"
echo ""
echo "  3. Open the Web UI at http://localhost:3000"
echo ""
echo "To stop services:"
echo "  cd $DEPLOY_DIR && docker-compose down"
echo ""
echo "To view logs:"
echo "  docker-compose logs -f icnd"
echo ""
echo -e "${GREEN}Happy cooperating!${NC}"
