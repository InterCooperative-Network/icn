#!/usr/bin/env bash
#
# List all deployed ICN coop nodes
#

K3S_HOST="${K3S_HOST:-ubuntu@10.8.10.40}"

echo "ICN Coop Nodes:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Get all namespaces with icn-coop prefix
NAMESPACES=$(ssh "$K3S_HOST" "sudo kubectl get namespaces -o jsonpath='{.items[*].metadata.name}'" | tr ' ' '\n' | grep "^icn-coop-")

if [ -z "$NAMESPACES" ]; then
    echo "No coop nodes found"
    exit 0
fi

for NS in $NAMESPACES; do
    COOP_NAME=$(echo "$NS" | sed 's/icn-coop-//')
    echo "Coop: $COOP_NAME"
    echo "  Namespace: $NS"
    
    # Get pod status
    POD_STATUS=$(ssh "$K3S_HOST" "sudo kubectl -n $NS get pods -l app=icn,coop=$COOP_NAME -o jsonpath='{.items[0].status.phase}'" 2>/dev/null || echo "not-found")
    echo "  Status: $POD_STATUS"
    
    # Get identity if pod is running
    if [ "$POD_STATUS" = "Running" ]; then
        DID=$(ssh "$K3S_HOST" "sudo kubectl -n $NS exec deployment/icn-$COOP_NAME -- icnctl id show 2>&1" 2>/dev/null | grep -o "did:icn:[a-zA-Z0-9]*" | head -1 || echo "unknown")
        echo "  DID: $DID"
        
        # Get ports from service
        QUIC_PORT=$(ssh "$K3S_HOST" "sudo kubectl -n $NS get svc icn-$COOP_NAME -o jsonpath='{.spec.ports[?(@.name==\"quic\")].port}'" 2>/dev/null || echo "?")
        echo "  QUIC Port: $QUIC_PORT"
    fi
    
    echo ""
done

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

