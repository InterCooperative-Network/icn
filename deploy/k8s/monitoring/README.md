# Monitoring overlays for the ICN homelab K3s cluster

The monitoring stack itself is deployed via the upstream
`prometheus-community/kube-prometheus-stack` Helm chart. This directory holds
the ICN-specific overlays that live alongside the chart release.

| Fact | Value |
|------|-------|
| Release name | `prometheus` (**not** `kube-prometheus-stack`) |
| Namespace | `monitoring` |
| Chart version | **pinned to `82.4.3`** (appVersion `v0.89.0`) |
| StatefulSet | `prometheus-prometheus-kube-prometheus-prometheus` |
| Prometheus CR | `prometheus-kube-prometheus-prometheus` |

## Files

| File | Purpose |
|------|---------|
| `servicemonitor.yaml` | ServiceMonitor + PrometheusRule for the ICN daemon. |
| `values-kube-prometheus-stack.yaml` | Helm values overlay: Prometheus TSDB on a static **local** PV on k3s-worker-1; Alertmanager on `atlas-nfs`. See issue #1596. |

## Two things that have caused outages -- read before upgrading

**1. Always pin `--version 82.4.3`.** Without it, `helm upgrade` silently
resolves to whatever the chart repository currently serves, so an unrelated
routine upgrade can jump chart versions and rewrite the CRs.

**2. Always use `sudo -E helm`, never plain `sudo helm`.** Helm reads its
repository list from `$HOME/.config/helm/repositories.yaml`. Plain `sudo`
resets `HOME` to `/root`, where that file may not exist, and the upgrade then
fails with `repo prometheus-community not found`. On 2026-08-05 exactly this
happened, and because the surrounding script did not gate on the failure it
continued into destructive steps anyway.

Helm is installed on `k3s-control` (`10.8.30.40`) at `/usr/local/bin/helm` and
must be invoked with `KUBECONFIG=/etc/rancher/k3s/k3s.yaml` in non-interactive
SSH sessions (the default kubeconfig falls back to `localhost:8080`, which
fails over SSH).

## Apply the Helm values overlay

```bash
# 0. Chart repository must resolve BEFORE anything else. Verify, do not assume.
ssh ubuntu@10.8.30.40 \
  'KUBECONFIG=/etc/rancher/k3s/k3s.yaml sudo -E helm repo add prometheus-community \
     https://prometheus-community.github.io/helm-charts; \
   KUBECONFIG=/etc/rancher/k3s/k3s.yaml sudo -E helm repo update; \
   KUBECONFIG=/etc/rancher/k3s/k3s.yaml sudo -E helm show chart \
     prometheus-community/kube-prometheus-stack --version 82.4.3 | grep ^version:'

# 1. Copy values file to the control node
scp deploy/k8s/monitoring/values-kube-prometheus-stack.yaml \
    ubuntu@10.8.30.40:/tmp/values-kube-prometheus-stack.yaml

# 2. Dry-run first -- MANDATORY, and its exit status is a gate
ssh ubuntu@10.8.30.40 \
  'KUBECONFIG=/etc/rancher/k3s/k3s.yaml sudo -E helm upgrade prometheus \
    prometheus-community/kube-prometheus-stack \
    --version 82.4.3 \
    --namespace monitoring \
    --reuse-values \
    -f /tmp/values-kube-prometheus-stack.yaml \
    --dry-run'

# 3. Apply
ssh ubuntu@10.8.30.40 \
  'KUBECONFIG=/etc/rancher/k3s/k3s.yaml sudo -E helm upgrade prometheus \
    prometheus-community/kube-prometheus-stack \
    --version 82.4.3 \
    --namespace monitoring \
    --reuse-values \
    -f /tmp/values-kube-prometheus-stack.yaml'
```

`--reuse-values` preserves chart defaults set at install time (for example the
Grafana admin password), merging this overlay on top.

**Never delete the StatefulSet or a PVC because an upgrade "seems stuck".**
Re-read the live Prometheus CR first and confirm it actually holds the intended
storage spec. If Helm exited non-zero, nothing downstream may proceed.

## Verify after rollout

```bash
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

# Release still on the pinned chart
sudo -E helm list -n monitoring

# Desired state landed in the CR
sudo -E kubectl -n monitoring get prometheus prometheus-kube-prometheus-prometheus \
  -o jsonpath='{.spec.storage.volumeClaimTemplate.spec.storageClassName}{"\n"}'

# PVC bound to the prepared static local PV, not a dynamic NFS volume
sudo -E kubectl -n monitoring get pvc
sudo -E kubectl get pv prometheus-tsdb-worker1

# The mount inside the pod is ext4 -- NOT nfs4
sudo -E kubectl -n monitoring exec \
  prometheus-prometheus-kube-prometheus-prometheus-0 -c prometheus \
  -- df -PT /prometheus
```

Expect: chart `kube-prometheus-stack-82.4.3`, storage class `prometheus-local`,
the PVC `Bound` to `prometheus-tsdb-worker1`, the pod on `k3s-worker-1`, and
`/prometheus` reported as `ext4`. Seeing `nfs4` means the migration did not
take effect.
