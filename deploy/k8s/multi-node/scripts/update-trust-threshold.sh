#!/usr/bin/env bash
#
# Update all ICN coop ConfigMaps to set min_trust_threshold = 0.0
# This fixes connection rejection in cooperative deployments
#

set -e

K3S_HOST="${K3S_HOST:-ubuntu@10.8.10.40}"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Updating min_trust_threshold for all ICN coops            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Coop configs: name -> quic_port, rpc_port, metrics_port
# These match the values from deploy-coop.sh based on the md5 hash
declare -A COOPS=(
    ["alpha"]="7827:5651:9150"
    ["beta"]="7834:5658:9157"
    ["gamma"]="7825:5649:9148"
    ["delta"]="7831:5655:9154"
)

for COOP_NAME in "${!COOPS[@]}"; do
    IFS=':' read -r QUIC_PORT RPC_PORT METRICS_PORT <<< "${COOPS[$COOP_NAME]}"
    NAMESPACE="icn-coop-${COOP_NAME}"
    CONFIGMAP="icn-${COOP_NAME}-config"

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Updating $NAMESPACE / $CONFIGMAP"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Generate ConfigMap with min_trust_threshold = 0.0
    cat <<EOF | ssh "$K3S_HOST" "sudo kubectl apply -f -"
apiVersion: v1
kind: ConfigMap
metadata:
  name: $CONFIGMAP
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
    # Set min_trust_threshold to 0.0 for cooperative deployment
    # This allows nodes to connect without pre-established trust relationships
    min_trust_threshold = 0.0

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

    echo "✓ ConfigMap updated"

    # Restart the deployment
    echo "Restarting deployment icn-${COOP_NAME}..."
    ssh "$K3S_HOST" "sudo kubectl -n $NAMESPACE rollout restart deployment icn-${COOP_NAME}"
    echo ""
done

echo ""
echo "Waiting for all rollouts to complete..."
for COOP_NAME in "${!COOPS[@]}"; do
    NAMESPACE="icn-coop-${COOP_NAME}"
    echo "Waiting for $NAMESPACE/icn-${COOP_NAME}..."
    ssh "$K3S_HOST" "sudo kubectl -n $NAMESPACE rollout status deployment icn-${COOP_NAME} --timeout=120s"
done

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║  ✅ All coops updated with min_trust_threshold = 0.0       ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Check connectivity with:"
echo "  ssh $K3S_HOST 'sudo kubectl -n icn-coop-alpha logs deployment/icn-alpha | grep -E \"(trust|connect)\"'"
