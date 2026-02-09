.PHONY: up down restart logs clean status build demo test

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

# Clean all data volumes (fresh identities on next up)
clean: down
	docker-compose down -v
	@echo "All devnet data volumes removed"

# Check status of all nodes
status:
	@echo "=== Node Status ==="
	@docker-compose ps
	@echo ""
	@echo "=== Node A (localhost:8000) ==="
	@curl -sf http://localhost:8000/v1/health 2>/dev/null | python3 -m json.tool 2>/dev/null || echo "  Not ready"
	@echo ""
	@echo "=== Node B (localhost:8001) ==="
	@curl -sf http://localhost:8001/v1/health 2>/dev/null | python3 -m json.tool 2>/dev/null || echo "  Not ready"
	@echo ""
	@echo "=== Node C (localhost:8002) ==="
	@curl -sf http://localhost:8002/v1/health 2>/dev/null | python3 -m json.tool 2>/dev/null || echo "  Not ready"

# Rebuild images without cache
build:
	docker-compose build --no-cache

# Run smoke tests against the devnet
test:
	@echo "Running devnet smoke tests..."
	cd ../../icn && cargo test --test devnet_smoke -- --test-threads=1 --nocapture

# Run the comprehensive demo against node-a
demo:
	@echo "Running demo against devnet node-a..."
	ICN_GATEWAY=http://localhost:8000 bash ../../scripts/demo-single-node.sh
