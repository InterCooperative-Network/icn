#!/usr/bin/env bash
#
# Deploy a new ICN coop node (new identity)
#
# Usage:
#   ./deploy-coop.sh <coop-name> [coop-display-name] [passphrase]
#
# Examples:
#   ./deploy-coop.sh alpha "Alpha Cooperative"
#   ./deploy-coop.sh beta "Beta Timebank" "my-secure-passphrase"

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MULTI_NODE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
K8S_DIR="$(cd "$MULTI_NODE_DIR/.." && pwd)"
K3S_HOST="${K3S_HOST:-ubuntu@10.8.10.40}"

COOP_NAME="${1:-}"
COOP_DISPLAY_NAME="${2:-$COOP_NAME Cooperative}"
PASSPHRASE="${3:-}"

if [ -z "$COOP_NAME" ]; then
    echo "Error: Coop name required"
    echo "Usage: $0 <coop-name> [coop-display-name] [passphrase]"
    exit 1
fi

# Validate coop name (alphanumeric + dashes)
if ! [[ "$COOP_NAME" =~ ^[a-z0-9-]+$ ]]; then
    echo "Error: Coop name must be lowercase alphanumeric with dashes only"
    exit 1
fi

NAMESPACE="icn-coop-${COOP_NAME}"
TEMP_DIR=$(mktemp -d)

echo "╔════════════════════════════════════════════════════════════╗"
echo "║       Deploying ICN Coop: $COOP_NAME                      ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Coop Name: $COOP_NAME"
echo "Display Name: $COOP_DISPLAY_NAME"
echo "Namespace: $NAMESPACE"
echo ""

# Check if namespace already exists
if ssh "$K3S_HOST" "sudo kubectl get namespace $NAMESPACE" &>/dev/null; then
    echo "⚠️  Warning: Namespace $NAMESPACE already exists"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Generate passphrase if not provided
if [ -z "$PASSPHRASE" ]; then
    PASSPHRASE=$(openssl rand -base64 32 | tr -d "=+/" | cut -c1-32)
    echo "Generated passphrase (save this!): $PASSPHRASE"
    echo ""
fi

# Calculate ports based on coop name (simple hash)
COOP_INDEX=$(echo -n "$COOP_NAME" | md5sum | head -c 2 | od -An -tx1 | head -1 | awk '{print $1}')
BASE_PORT=$((7777 + (0x$COOP_INDEX % 100)))
QUIC_PORT=$BASE_PORT
RPC_PORT=$((5601 + (BASE_PORT - 7777)))
METRICS_PORT=$((9100 + (BASE_PORT - 7777)))
NODEPORT_QUIC=$((30777 + (BASE_PORT - 7777)))
NODEPORT_RPC=$((30601 + (BASE_PORT - 7777)))

echo "Port Allocation:"
echo "  QUIC:    $QUIC_PORT (NodePort: $NODEPORT_QUIC)"
echo "  RPC:     $RPC_PORT (NodePort: $NODEPORT_RPC)"
echo "  Metrics: $METRICS_PORT"
echo ""

# Step 1: Create namespace
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 1: Creating namespace..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
cat > "$TEMP_DIR/namespace.yaml" <<EOF
apiVersion: v1
kind: Namespace
metadata:
  name: $NAMESPACE
  labels:
    name: $NAMESPACE
    app: icn
    coop: $COOP_NAME
EOF

ssh "$K3S_HOST" "sudo kubectl apply -f -" < "$TEMP_DIR/namespace.yaml"
echo "✓ Namespace created"
echo ""

# Step 2: Create ConfigMap
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 2: Creating ConfigMap..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
cat > "$TEMP_DIR/configmap.yaml" <<EOF
apiVersion: v1
kind: ConfigMap
metadata:
  name: icn-${COOP_NAME}-config
  namespace: $NAMESPACE
  labels:
    app: icn
    coop: $COOP_NAME
data:
  icn.toml: |
    data_dir = "/data"

    [network]
    listen_addr = "0.0.0.0:${QUIC_PORT}"
    rpc_port = ${RPC_PORT}
    mdns_enabled = true
    bootstrap_peers = []

    [observability]
    metrics_port = ${METRICS_PORT}
    health_port = 8080
    log_level = "info"

    [rate_limiting]
    enabled = true
    refill_interval_ms = 100

    [rate_limiting.isolated]
    max_messages_per_second = 10
    burst_capacity = 2

    [rate_limiting.known]
    max_messages_per_second = 50
    burst_capacity = 10

    [rate_limiting.partner]
    max_messages_per_second = 100
    burst_capacity = 20

    [rate_limiting.federated]
    max_messages_per_second = 200
    burst_capacity = 50

    [rate_limiting.fallback]
    max_messages_per_second = 100
    burst_capacity = 20

    [topology]
    region = "faherty-homelab"
    cluster_id = "k3s-${COOP_NAME}"
    role = "edge"

    [topology.neighbor_limits]
    max_local_cluster = 10
    max_regional = 10
    max_backbone = 5
    max_trusted = 20

    [topology.fanout]
    local_cluster = 8
    regional = 6
    global = 4
EOF

ssh "$K3S_HOST" "sudo kubectl apply -f -" < "$TEMP_DIR/configmap.yaml"
echo "✓ ConfigMap created"
echo ""

# Step 3: Create Secret
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 3: Creating Secret..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
cat > "$TEMP_DIR/secret.yaml" <<EOF
apiVersion: v1
kind: Secret
metadata:
  name: icn-${COOP_NAME}-secrets
  namespace: $NAMESPACE
  labels:
    app: icn
    coop: $COOP_NAME
type: Opaque
stringData:
  passphrase: "$PASSPHRASE"
EOF

ssh "$K3S_HOST" "sudo kubectl apply -f -" < "$TEMP_DIR/secret.yaml"
echo "✓ Secret created"
echo ""

# Step 4: Create PVC
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 4: Creating PersistentVolumeClaim..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
cat > "$TEMP_DIR/pvc.yaml" <<EOF
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: icn-${COOP_NAME}-data
  namespace: $NAMESPACE
  labels:
    app: icn
    coop: $COOP_NAME
spec:
  accessModes:
    - ReadWriteOnce
  storageClassName: atlas-nfs
  resources:
    requests:
      storage: 10Gi
EOF

ssh "$K3S_HOST" "sudo kubectl apply -f -" < "$TEMP_DIR/pvc.yaml"
echo "✓ PVC created"
echo ""

# Step 5: Create Deployment
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 5: Creating Deployment..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
cat > "$TEMP_DIR/deployment.yaml" <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: icn-${COOP_NAME}
  namespace: $NAMESPACE
  labels:
    app: icn
    coop: $COOP_NAME
    component: daemon
spec:
  replicas: 1
  strategy:
    type: Recreate
  selector:
    matchLabels:
      app: icn
      coop: $COOP_NAME
  template:
    metadata:
      labels:
        app: icn
        coop: $COOP_NAME
        component: daemon
    spec:
      containers:
      - name: icnd
        image: icn:latest
        imagePullPolicy: Never
        args:
          - --config
          - /etc/icn/icn.toml
        env:
        - name: ICN_PASSPHRASE
          valueFrom:
            secretKeyRef:
              name: icn-${COOP_NAME}-secrets
              key: passphrase
        - name: ICN_DATA_DIR
          value: "/data"
        - name: HOME
          value: "/data"
        ports:
        - name: quic
          containerPort: ${QUIC_PORT}
          protocol: UDP
        - name: rpc
          containerPort: ${RPC_PORT}
          protocol: TCP
        - name: metrics
          containerPort: ${METRICS_PORT}
          protocol: TCP
        - name: health
          containerPort: 8080
          protocol: TCP
        livenessProbe:
          httpGet:
            path: /metrics
            port: ${METRICS_PORT}
          initialDelaySeconds: 30
          periodSeconds: 30
          timeoutSeconds: 1
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /metrics
            port: ${METRICS_PORT}
          initialDelaySeconds: 10
          periodSeconds: 10
          timeoutSeconds: 1
          failureThreshold: 3
        resources:
          requests:
            cpu: 100m
            memory: 512Mi
          limits:
            cpu: "1"
            memory: 2Gi
        volumeMounts:
        - name: config
          mountPath: /etc/icn
          readOnly: true
        - name: data
          mountPath: /data
      securityContext:
        runAsUser: 0
        runAsGroup: 0
      volumes:
      - name: config
        configMap:
          name: icn-${COOP_NAME}-config
      - name: data
        persistentVolumeClaim:
          claimName: icn-${COOP_NAME}-data
EOF

ssh "$K3S_HOST" "sudo kubectl apply -f -" < "$TEMP_DIR/deployment.yaml"
echo "✓ Deployment created"
echo ""

# Step 6: Create Services
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 6: Creating Services..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
cat > "$TEMP_DIR/services.yaml" <<EOF
apiVersion: v1
kind: Service
metadata:
  name: icn-${COOP_NAME}
  namespace: $NAMESPACE
  labels:
    app: icn
    coop: $COOP_NAME
spec:
  type: ClusterIP
  selector:
    app: icn
    coop: $COOP_NAME
  ports:
  - name: quic
    port: ${QUIC_PORT}
    protocol: UDP
    targetPort: quic
  - name: rpc
    port: ${RPC_PORT}
    protocol: TCP
    targetPort: rpc
  - name: metrics
    port: ${METRICS_PORT}
    protocol: TCP
    targetPort: metrics
  - name: health
    port: 8080
    protocol: TCP
    targetPort: health
---
apiVersion: v1
kind: Service
metadata:
  name: icn-${COOP_NAME}-nodeport
  namespace: $NAMESPACE
  labels:
    app: icn
    coop: $COOP_NAME
spec:
  type: NodePort
  selector:
    app: icn
    coop: $COOP_NAME
  ports:
  - name: quic
    port: ${QUIC_PORT}
    protocol: UDP
    targetPort: quic
    nodePort: ${NODEPORT_QUIC}
  - name: rpc
    port: ${RPC_PORT}
    protocol: TCP
    targetPort: rpc
    nodePort: ${NODEPORT_RPC}
EOF

ssh "$K3S_HOST" "sudo kubectl apply -f -" < "$TEMP_DIR/services.yaml"
echo "✓ Services created"
echo ""

# Step 7: Wait for pod to be ready
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 7: Waiting for pod to start..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
for i in {1..60}; do
    if ssh "$K3S_HOST" "sudo kubectl -n $NAMESPACE get pods -l coop=$COOP_NAME -o jsonpath='{.items[0].status.phase}'" 2>/dev/null | grep -q Running; then
        echo "✓ Pod is running"
        break
    fi
    if [ $i -eq 60 ]; then
        echo "⚠️  Pod not ready after 60 seconds, check manually"
        break
    fi
    sleep 1
done
echo ""

# Step 8: Initialize identity
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Step 8: Initializing identity..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Wait a bit more for pod to be fully ready
sleep 5

# Check if identity already exists
IDENTITY_CHECK=$(ssh "$K3S_HOST" "sudo kubectl -n $NAMESPACE exec deployment/icn-${COOP_NAME} -- icnctl id show 2>&1" || echo "not-found")

if echo "$IDENTITY_CHECK" | grep -q "did:icn:"; then
    DID=$(echo "$IDENTITY_CHECK" | grep -o "did:icn:[a-zA-Z0-9]*" | head -1)
    echo "✓ Identity already exists: $DID"
else
    echo "Initializing new identity..."
    ssh "$K3S_HOST" "sudo kubectl -n $NAMESPACE exec deployment/icn-${COOP_NAME} -- bash -c 'echo \"$PASSPHRASE\" | icnctl id init'" || {
        echo "⚠️  Identity initialization may have failed, check logs"
    }
    
    # Get the DID
    sleep 2
    DID=$(ssh "$K3S_HOST" "sudo kubectl -n $NAMESPACE exec deployment/icn-${COOP_NAME} -- icnctl id show 2>&1" | grep -o "did:icn:[a-zA-Z0-9]*" | head -1 || echo "unknown")
    echo "✓ Identity initialized: $DID"
fi
echo ""

# Cleanup
rm -rf "$TEMP_DIR"

# Success!
echo "╔════════════════════════════════════════════════════════════╗"
echo "║       Coop Deployed Successfully!                         ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Coop: $COOP_NAME ($COOP_DISPLAY_NAME)"
echo "Namespace: $NAMESPACE"
echo "DID: $DID"
echo ""
echo "Access:"
echo "  Check status: kubectl -n $NAMESPACE get pods"
echo "  View logs:    kubectl -n $NAMESPACE logs -f deployment/icn-${COOP_NAME}"
echo "  Show identity: kubectl -n $NAMESPACE exec deployment/icn-${COOP_NAME} -- icnctl id show"
echo ""
echo "⚠️  Save this passphrase: $PASSPHRASE"

