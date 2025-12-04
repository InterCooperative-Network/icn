.PHONY: help build sync deploy full-deploy status logs clean

# Configuration
IMAGE_NAME ?= icn
IMAGE_TAG ?= latest
K3S_HOST ?= ubuntu@10.8.10.40
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

