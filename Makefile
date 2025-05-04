.PHONY: help dev check-db db-start db-stop runtime agoranet wallet frontend tests clean

# Default target
help:
	@echo "ICN Development Commands:"
	@echo "  make dev            - Start all components for development"
	@echo "  make check-db       - Check if PostgreSQL is running"
	@echo "  make db-start       - Start PostgreSQL in Docker"
	@echo "  make db-stop        - Stop PostgreSQL Docker container"
	@echo "  make runtime        - Start the ICN runtime only"
	@echo "  make agoranet       - Start AgoraNet API only"
	@echo "  make wallet         - Build the wallet"
	@echo "  make frontend       - Start the dashboard frontend"
	@echo "  make tests          - Run all tests"
	@echo "  make clean          - Clean built artifacts"

# Start everything for development
dev: check-db
	@echo "Starting ICN development environment..."
	@./scripts/dev_setup.sh

# Check if PostgreSQL is running
check-db:
	@echo "Checking PostgreSQL..."
	@if docker ps | grep -q "icn-postgres"; then \
		echo "PostgreSQL is running"; \
	else \
		echo "PostgreSQL is not running, starting..."; \
		make db-start; \
	fi

# Start PostgreSQL
db-start:
	@echo "Starting PostgreSQL in Docker..."
	@docker run --name icn-postgres -e POSTGRES_PASSWORD=postgres -e POSTGRES_USER=postgres -p 5432:5432 -d postgres:14
	@echo "Waiting for PostgreSQL to start..."
	@sleep 3
	@echo "Creating icn database..."
	@docker exec -it icn-postgres psql -U postgres -c "CREATE DATABASE icn;" || true
	@echo "PostgreSQL ready"

# Stop PostgreSQL
db-stop:
	@echo "Stopping PostgreSQL..."
	@docker stop icn-postgres || true
	@docker rm icn-postgres || true

# Start ICN runtime
runtime:
	@echo "Starting ICN runtime..."
	@cd runtime && cargo run --bin icn -- --dev

# Start AgoraNet API
agoranet: check-db
	@echo "Starting AgoraNet API..."
	@cd agoranet && DATABASE_URL=postgres://postgres:postgres@localhost:5432/icn cargo run

# Build wallet
wallet:
	@echo "Building ICN wallet..."
	@cd wallet && cargo build

# Start frontend dashboard
frontend:
	@echo "Starting dashboard frontend..."
	@cd frontend/dashboard && npm start

# Run all tests
tests:
	@echo "Running tests..."
	@cargo test --workspace

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	@cargo clean
	@find . -name "node_modules" -type d -exec rm -rf {} +
	@find . -name "dist" -type d -exec rm -rf {} + 