.PHONY: up down restart logs clean status build demo

# Start the 3-node devnet
up:
	docker-compose up -d
	@echo "Devnet started. Waiting for nodes to be ready..."
	@sleep 5
	@$(MAKE) status

# Stop the devnet
down:
	docker-compose down

# Restart the devnet
restart: down up

# Follow logs from all nodes
logs:
	docker-compose logs -f

# Show logs from a specific node (make logs-a, logs-b, logs-c)
logs-a:
	docker-compose logs -f node-a

logs-b:
	docker-compose logs -f node-b

logs-c:
	docker-compose logs -f node-c

# Clean all data volumes
clean: down
	docker-compose down -v
	@echo "All devnet data volumes removed"

# Check status of all nodes
status:
	@echo "=== Node Status ==="
	@docker-compose ps
	@echo ""
	@echo "=== Node A (localhost:7000) ==="
	@curl -s http://localhost:8000/health || echo "Node A not ready"
	@echo ""
	@echo "=== Node B (localhost:7001) ==="
	@curl -s http://localhost:8001/health || echo "Node B not ready"
	@echo ""
	@echo "=== Node C (localhost:7002) ==="
	@curl -s http://localhost:8002/health || echo "Node C not ready"

# Rebuild images without cache
build:
	docker-compose build --no-cache

# Run smoke tests against the devnet
test:
	@echo "Running devnet smoke tests..."
	cd ../../icn && cargo test --test devnet_smoke -- --test-threads=1 --nocapture

# Run the pilot demo (all three flows) against a running devnet
demo:
	@echo "Running pilot demo flows..."
	bash ../../scripts/demo-runner.sh
