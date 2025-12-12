.PHONY: help build sync deploy full-deploy status logs clean build-ui deploy-ui

# Configuration
IMAGE_NAME ?= icn
UI_IMAGE_NAME ?= icn-pilot-ui
IMAGE_TAG ?= latest
K3S_HOST ?= ubuntu@10.8.10.40
K3S_WORKER1 ?= ubuntu@10.8.10.41
K3S_WORKER2 ?= ubuntu@10.8.10.42
NAMESPACE ?= icn

help: ## Show this help message
	@echo "ICN Kubernetes Deployment"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-20s %s\n", $$1, $$2}'

build: ## Build Docker image
	@echo "Building ICN Docker image..."
	./scripts/build-image.sh $(IMAGE_TAG)

sync: ## Sync Docker image to K3s cluster
	@echo "Syncing image to K3s cluster..."
	./scripts/sync-image.sh $(IMAGE_TAG) $(K3S_HOST)

deploy: ## Deploy ICN to K3s cluster
	@echo "Deploying ICN to K3s cluster..."
	./scripts/deploy.sh $(K3S_HOST) $(IMAGE_TAG)

full-deploy: ## Full deployment pipeline: build, sync, and deploy
	@echo "Running full deployment pipeline..."
	./scripts/full-deploy.sh $(IMAGE_TAG) $(K3S_HOST)

status: ## Check deployment status
	@echo "Checking ICN deployment status..."
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) get pods,svc,pvc"

logs: ## Tail ICN daemon logs
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) logs -f deployment/icn-daemon"

logs-recent: ## Show recent ICN daemon logs
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) logs --tail=100 deployment/icn-daemon"

restart: ## Restart ICN deployment
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) rollout restart deployment/icn-daemon"

clean: ## Remove ICN deployment (keeps PVC)
	@echo "Removing ICN deployment..."
	ssh $(K3S_HOST) "sudo kubectl delete deployment,svc,configmap -n $(NAMESPACE) -l app=icn || true"

clean-all: clean ## Remove ICN deployment and PVC (WARNING: deletes data)
	@echo "WARNING: This will delete all ICN data!"
	@read -p "Are you sure? [y/N] " -n 1 -r; \
	echo; \
	if [[ $$REPLY =~ ^[Yy]$$ ]]; then \
		ssh $(K3S_HOST) "sudo kubectl delete namespace $(NAMESPACE) || true"; \
	fi

# Convenience targets with git hash
build-dev: ## Build with git hash tag
	$(MAKE) build IMAGE_TAG=$(shell git rev-parse --short HEAD)

full-deploy-dev: ## Full deploy with git hash tag
	$(MAKE) full-deploy IMAGE_TAG=$(shell git rev-parse --short HEAD)

# ============================================================================
# Pilot UI Targets
# ============================================================================

build-ui-image: ## Build Pilot UI Docker image
	@echo "Building Pilot UI Docker image..."
	cd ../../web/pilot-ui && docker build -t $(UI_IMAGE_NAME):$(IMAGE_TAG) .

sync-ui-image: ## Sync Pilot UI image to all K3s nodes
	@echo "Syncing Pilot UI image to K3s nodes..."
	@echo "Saving image..."
	docker save $(UI_IMAGE_NAME):$(IMAGE_TAG) | gzip > /tmp/icn-pilot-ui.tar.gz
	@echo "Copying to k3s-control..."
	scp /tmp/icn-pilot-ui.tar.gz $(K3S_HOST):/tmp/
	ssh $(K3S_HOST) "sudo ctr -n k8s.io images import /tmp/icn-pilot-ui.tar.gz && sudo rm /tmp/icn-pilot-ui.tar.gz"
	@echo "Copying to k3s-worker-1..."
	scp /tmp/icn-pilot-ui.tar.gz $(K3S_WORKER1):/tmp/
	ssh $(K3S_WORKER1) "sudo ctr -n k8s.io images import /tmp/icn-pilot-ui.tar.gz && sudo rm /tmp/icn-pilot-ui.tar.gz"
	@echo "Copying to k3s-worker-2..."
	scp /tmp/icn-pilot-ui.tar.gz $(K3S_WORKER2):/tmp/
	ssh $(K3S_WORKER2) "sudo ctr -n k8s.io images import /tmp/icn-pilot-ui.tar.gz && sudo rm /tmp/icn-pilot-ui.tar.gz"
	rm /tmp/icn-pilot-ui.tar.gz
	@echo "✓ Pilot UI image synced to all nodes"

deploy-ui: ## Deploy Pilot UI to K3s
	@echo "Deploying Pilot UI to K3s..."
	ssh $(K3S_HOST) "sudo kubectl apply -f -" < pilot-ui-deployment.yaml
	@echo "✓ Pilot UI deployed"

ui-status: ## Check Pilot UI status
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) get pods,svc -l component=pilot-ui"

ui-logs: ## Tail Pilot UI logs
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) logs -f deployment/pilot-ui"

restart-ui: ## Restart Pilot UI
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) rollout restart deployment/pilot-ui"

delete-ui: ## Delete Pilot UI
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) delete deployment,svc -l component=pilot-ui || true"

ui-shell: ## Shell into Pilot UI pod
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) exec -it deployment/pilot-ui -- /bin/sh"

ui-port-forward: ## Forward Pilot UI to localhost:3000
	ssh $(K3S_HOST) "sudo kubectl -n $(NAMESPACE) port-forward deployment/pilot-ui 3000:3000"

# ============================================================================
# Combined Targets
# ============================================================================

build-all-images: build build-ui-image ## Build both ICN daemon and Pilot UI images

sync-all-images: sync sync-ui-image ## Sync both images to K3s nodes

deploy-all: deploy deploy-ui ## Deploy both ICN daemon and Pilot UI

full-deploy-with-ui: build-all-images sync-all-images deploy-all ## Full deployment: build + sync + deploy both
	@echo ""
	@echo "✓ Full deployment complete!"
	@echo ""
	@echo "Access points:"
	@echo "  Gateway: http://10.8.10.40:30080"
	@echo "  Pilot UI: http://10.8.10.40:30030"
	@echo "  Metrics: http://10.8.10.40:30091/metrics"
	@echo ""
	@echo "Check status: make status"
	@echo "View logs: make logs (daemon) or make ui-logs (UI)"

status-all: status ui-status ## Check status of all deployments

