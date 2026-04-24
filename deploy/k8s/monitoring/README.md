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

```bash
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
helm repo update

helm upgrade --install kube-prometheus-stack \
  prometheus-community/kube-prometheus-stack \
  --namespace monitoring --create-namespace \
  -f deploy/k8s/monitoring/values-kube-prometheus-stack.yaml
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
