# Monitoring overlays for the ICN homelab K3s cluster

The monitoring stack itself is deployed via the upstream
`prometheus-community/kube-prometheus-stack` Helm chart. This directory holds
the ICN-specific overlays that live alongside the chart release.

## Files

| File | Purpose |
|------|---------|
| `servicemonitor.yaml` | ServiceMonitor + PrometheusRule for the ICN daemon. |
| `values-kube-prometheus-stack.yaml` | Helm values overlay: durable Atlas-backed storage for Prometheus + Alertmanager (replaces default `emptyDir`). See issue #1596. |

## Apply the Helm values overlay

Helm is installed on `k3s-control` (`10.8.30.40`) at `/usr/local/bin/helm`. It must be
invoked with `KUBECONFIG=/etc/rancher/k3s/k3s.yaml` in non-interactive SSH sessions
(the default kubeconfig falls back to `localhost:8080` which fails over SSH).

**Copy values file to control node, then upgrade:**

```bash
# 1. Copy values file to control node
scp deploy/k8s/monitoring/values-kube-prometheus-stack.yaml \
    ubuntu@10.8.30.40:/tmp/values-kube-prometheus-stack.yaml

# 2. Dry-run first
ssh ubuntu@10.8.30.40 \
  'KUBECONFIG=/etc/rancher/k3s/k3s.yaml sudo -E helm upgrade prometheus \
    prometheus-community/kube-prometheus-stack \
    --namespace monitoring \
    --reuse-values \
    -f /tmp/values-kube-prometheus-stack.yaml \
    --dry-run'

# 3. Apply
ssh ubuntu@10.8.30.40 \
  'KUBECONFIG=/etc/rancher/k3s/k3s.yaml sudo -E helm upgrade prometheus \
    prometheus-community/kube-prometheus-stack \
    --namespace monitoring \
    --reuse-values \
    -f /tmp/values-kube-prometheus-stack.yaml \
    --wait --timeout 10m'
```

The release name is `prometheus` (not `kube-prometheus-stack`). Use `--reuse-values` to
avoid resetting other chart defaults that were set at install time.

**Alternative (if routed via icn-dev):**

```bash
scp deploy/k8s/monitoring/values-kube-prometheus-stack.yaml icn-dev-cursor:/tmp/
ssh icn-dev-cursor "scp /tmp/values-kube-prometheus-stack.yaml ubuntu@10.8.30.40:/tmp/"
ssh icn-dev-cursor "ssh ubuntu@10.8.30.40 \
  'KUBECONFIG=/etc/rancher/k3s/k3s.yaml sudo -E helm upgrade prometheus \
    prometheus-community/kube-prometheus-stack \
    --namespace monitoring --reuse-values \
    -f /tmp/values-kube-prometheus-stack.yaml --wait --timeout 10m'"
```

## Verify after rollout

```bash
kubectl -n monitoring get pvc
kubectl -n monitoring get sts prometheus-kube-prometheus-stack-prometheus \
  -o jsonpath='{.spec.volumeClaimTemplates[*].spec.storageClassName}{"\n"}'
kubectl -n monitoring describe pod \
  prometheus-kube-prometheus-stack-prometheus-0 | grep -A2 'Volumes:'
```

Expect: a `Bound` PVC on `atlas-nfs`, storage class `atlas-nfs` on the
volumeClaimTemplate, and the `/prometheus` mount backed by a PVC (not
`emptyDir`).
