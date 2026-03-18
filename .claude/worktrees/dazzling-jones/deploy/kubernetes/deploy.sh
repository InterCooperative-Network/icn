#!/bin/bash
# Deploy ICN to Kubernetes

set -e

echo "🚀 ICN Kubernetes Deployment"
echo "============================="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v kubectl &> /dev/null; then
    echo -e "${RED}❌ kubectl not found${NC}"
    echo "Install: https://kubernetes.io/docs/tasks/tools/"
    exit 1
fi

if ! kubectl cluster-info &> /dev/null; then
    echo -e "${RED}❌ Cannot connect to Kubernetes cluster${NC}"
    echo "Configure kubectl to connect to your cluster"
    exit 1
fi

echo -e "${GREEN}✓${NC} kubectl configured"
echo ""

# Deployment steps
echo "Deploying ICN components..."
echo ""

# 1. Create namespace
echo "1️⃣  Creating namespace..."
kubectl apply -f namespace.yaml
echo -e "${GREEN}✓${NC} Namespace created"
echo ""

# 2. Create ConfigMap and Secrets
echo "2️⃣  Creating configuration..."
kubectl apply -f configmap.yaml
kubectl apply -f secret.yaml
echo -e "${YELLOW}⚠${NC}  Remember to update secrets in production!"
echo -e "${GREEN}✓${NC} Configuration created"
echo ""

# 3. Create PersistentVolumeClaims
echo "3️⃣  Creating storage..."
kubectl apply -f pvc.yaml
echo -e "${GREEN}✓${NC} Storage provisioned"
echo ""

# 4. Deploy ICN node
echo "4️⃣  Deploying ICN node..."
kubectl apply -f deployment.yaml
echo -e "${GREEN}✓${NC} ICN node deployed"
echo ""

# 5. Create services
echo "5️⃣  Creating services..."
kubectl apply -f service.yaml
echo -e "${GREEN}✓${NC} Services created"
echo ""

# 6. Deploy monitoring (optional)
if [ "$1" == "--monitoring" ]; then
    echo "6️⃣  Deploying monitoring stack..."
    kubectl apply -f prometheus.yaml
    kubectl apply -f grafana.yaml
    echo -e "${GREEN}✓${NC} Monitoring deployed"
    echo ""
fi

# 7. Create ingress (optional)
if [ "$1" == "--ingress" ] || [ "$2" == "--ingress" ]; then
    echo "7️⃣  Creating ingress..."
    kubectl apply -f ingress.yaml
    echo -e "${YELLOW}⚠${NC}  Update ingress hostnames in ingress.yaml"
    echo -e "${GREEN}✓${NC} Ingress created"
    echo ""
fi

# Wait for deployment
echo "Waiting for pods to be ready..."
kubectl wait --for=condition=ready pod -l app=icn-node -n icn --timeout=300s

echo ""
echo -e "${GREEN}✅ Deployment complete!${NC}"
echo ""

# Show status
echo "Current status:"
kubectl get pods -n icn
echo ""

# Show services
echo "Services:"
kubectl get svc -n icn
echo ""

# Show access information
echo "📋 Access Information:"
echo "---------------------"
echo ""

# Get gateway URL
GATEWAY_IP=$(kubectl get svc icn-gateway -n icn -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null || echo "pending")
if [ "$GATEWAY_IP" != "pending" ] && [ -n "$GATEWAY_IP" ]; then
    echo "Gateway API: http://$GATEWAY_IP:8000"
else
    echo "Gateway API: Use port-forward or ingress"
    echo "  Port-forward: kubectl port-forward -n icn svc/icn-gateway 8000:8000"
fi

# Get P2P port
P2P_IP=$(kubectl get svc icn-p2p -n icn -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null || echo "pending")
if [ "$P2P_IP" != "pending" ] && [ -n "$P2P_IP" ]; then
    echo "P2P Network: $P2P_IP:3000 (UDP)"
else
    echo "P2P Network: Use NodePort or LoadBalancer"
fi

if [ "$1" == "--monitoring" ]; then
    echo ""
    echo "Monitoring:"
    echo "  Prometheus: kubectl port-forward -n icn svc/prometheus 9090:9090"
    echo "  Grafana: kubectl port-forward -n icn svc/grafana 3000:3000"
fi

echo ""
echo "📚 Next steps:"
echo "-------------"
echo "1. Check logs: kubectl logs -n icn -l app=icn-node -f"
echo "2. Test API: curl http://\$GATEWAY_IP:8000/health"
echo "3. Configure ingress hostnames in ingress.yaml"
echo "4. Update secrets with production values"
echo "5. Set up TLS certificates (cert-manager)"
echo ""
echo "For more info, see: deploy/kubernetes/README.md"
