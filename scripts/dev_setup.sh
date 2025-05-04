#!/usr/bin/env bash
# ICN Development Environment Setup Script

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Ensure DB is running
function ensure_db() {
  echo -e "${BLUE}Checking PostgreSQL...${NC}"
  if docker ps | grep -q "icn-postgres"; then
    echo -e "${GREEN}PostgreSQL is running${NC}"
  else
    echo -e "${YELLOW}PostgreSQL is not running, starting...${NC}"
    docker run --name icn-postgres -e POSTGRES_PASSWORD=postgres -e POSTGRES_USER=postgres -p 5432:5432 -d postgres:14
    echo "Waiting for PostgreSQL to start..."
    sleep 3
    echo "Creating icn database..."
    docker exec icn-postgres psql -U postgres -c "CREATE DATABASE icn;" || true
    echo -e "${GREEN}PostgreSQL ready${NC}"
  fi
}

# Set up environment variables
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/icn
export RUST_LOG=info,sqlx=warn,tower_http=debug
export ICN_BOOTSTRAP_CONFIG=config/bootstrap_nodes.toml

# Check for required dependencies
echo -e "${BLUE}Checking dependencies...${NC}"
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed${NC}"
    exit 1
fi

if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: Rust/Cargo is not installed${NC}"
    exit 1
fi

# Ensure database is running
ensure_db

# Run migrations if needed
echo -e "${BLUE}Running database migrations...${NC}"
cd agoranet && cargo run --bin sqlx migrate run && cd ..

# Start the dev environment components
echo -e "${GREEN}Starting ICN development environment...${NC}"

# Start the runtime in the background
echo -e "${BLUE}Starting ICN Runtime...${NC}"
cd runtime && cargo run --bin icn -- --dev &
RUNTIME_PID=$!
cd ..

# Give the runtime a moment to start
sleep 2

# Start AgoraNet API
echo -e "${BLUE}Starting AgoraNet API...${NC}"
cd agoranet && cargo run --bin agoranet &
AGORANET_PID=$!
cd ..

# Start frontend in development mode
echo -e "${BLUE}Starting Dashboard Frontend...${NC}"
cd frontend/dashboard && npm start &
FRONTEND_PID=$!
cd ../..

# Set up cleanup on exit
trap 'echo -e "${YELLOW}Shutting down...${NC}"; kill $RUNTIME_PID $AGORANET_PID $FRONTEND_PID; echo -e "${GREEN}Done!${NC}"; exit 0' SIGINT SIGTERM

echo -e "${GREEN}ICN development environment running!${NC}"
echo -e "${GREEN}Runtime:${NC} PID $RUNTIME_PID"
echo -e "${GREEN}AgoraNet:${NC} PID $AGORANET_PID"
echo -e "${GREEN}Frontend:${NC} PID $FRONTEND_PID"
echo -e "${YELLOW}Press Ctrl+C to stop all components${NC}"

# Wait for Ctrl+C
wait 