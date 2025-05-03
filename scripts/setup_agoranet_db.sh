#!/bin/bash
set -e

# Colors for better output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Setting up PostgreSQL for AgoraNet...${NC}"

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Docker is not installed. Please install Docker first.${NC}"
    exit 1
fi

# Check if PostgreSQL container is already running
if docker ps | grep -q "icn-postgres"; then
    echo -e "${YELLOW}PostgreSQL container is already running.${NC}"
else
    # Start PostgreSQL container
    echo -e "${YELLOW}Starting PostgreSQL container...${NC}"
    docker run --name icn-postgres \
        -e POSTGRES_PASSWORD=postgres \
        -e POSTGRES_USER=postgres \
        -p 5432:5432 \
        -d postgres
    
    # Wait for PostgreSQL to start
    echo -e "${YELLOW}Waiting for PostgreSQL to start...${NC}"
    sleep 5
fi

# Set environment variable
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/icn_agoranet
echo -e "${YELLOW}Setting DATABASE_URL: ${DATABASE_URL}${NC}"
echo "export DATABASE_URL=${DATABASE_URL}" >> ~/.bashrc

# Check if sqlx-cli is installed
if ! command -v sqlx &> /dev/null; then
    echo -e "${YELLOW}Installing sqlx-cli...${NC}"
    cargo install sqlx-cli
fi

# Create database and run migrations
echo -e "${YELLOW}Creating database...${NC}"
sqlx database create || echo "Database already exists"

echo -e "${YELLOW}Running migrations...${NC}"
cd agoranet
if [ -d "migrations" ]; then
    sqlx migrate run
else
    echo -e "${RED}No migrations directory found. Skipping migrations.${NC}"
fi

# Create prepare file for offline mode
echo -e "${YELLOW}Preparing SQLx offline mode...${NC}"
cargo sqlx prepare -- --lib || echo "Failed to prepare SQLx offline mode"

echo -e "${GREEN}Database setup complete!${NC}"
echo -e "${YELLOW}You can now build AgoraNet with:${NC}"
echo -e "SQLX_OFFLINE=true cargo build -p icn-agoranet" 