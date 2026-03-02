# Network Policy Design — ICN K3s Cluster

**Status**: Design only — not yet implemented
**Reason for deferral**: Keep flexible for active development
**Revisit when**: Moving toward a pilot / hardening sprint
**Last reviewed**: 2026-03-02

---

## Current State

The `icn` namespace has four policies (deployed at cluster creation):

| Policy | Effect |
|--------|--------|
| `default-deny-ingress` | Block all inbound by default |
| `allow-dns-egress` | Permit UDP 53 to kube-dns |
| `allow-icn-mesh` | Allow mesh traffic between ICN daemon pods |
| `allow-pilot-ui` | Allow external access to Pilot UI port 3000 |

All other namespaces (`icn-coop-*`, `monitoring`, `registry`) have **no policies** — full open mesh.

---

## Port Map (from live cluster inspection)

| Namespace | QUIC (UDP) | RPC (TCP) | Metrics (TCP) | Health (TCP) |
|-----------|-----------|-----------|---------------|-------------|
| `icn` (main daemon) | 7777 | 5601 | 9100 | 8080 |
| `icn-coop-alpha` | 7827 | 5651 | 9150 | 8080 |
| `icn-coop-beta` | 7834 | 5658 | 9157 | 8080 |
| `icn-coop-gamma` | 7825 | 5649 | 9148 | 8080 |
| `icn-coop-delta` | 7831 | 5655 | 9154 | 8080 |

Each coop also has a NodePort service exposing QUIC and RPC externally for P2P federation.

**mDNS note**: All coop configs have `mdns_enabled = true`. mDNS uses multicast UDP to `224.0.0.251:5353`. Kubernetes NetworkPolicies operate on unicast only — multicast bypasses them entirely. Do not rely on NetworkPolicy to control mDNS; it will still broadcast within the node's subnet regardless.

---

## Design: `icn-coop-*` Namespaces

Apply the same policy template to all four coop namespaces. Use a label selector approach so the same manifests work across all coops.

### What traffic to allow

**Ingress:**
- From other `icn-coop-*` namespaces → QUIC (UDP) + RPC (TCP) ports
  *(P2P ICN mesh: coops form trust relationships with each other)*
- From `icn` namespace → QUIC + RPC
  *(Main daemon participates in the same mesh)*
- From `monitoring` namespace → metrics port only (TCP 9150/9157/9148/9154)
  *(Prometheus scraping)*
- From `kube-system` namespace → any
  *(kubelet probes, health checks)*

**Egress:**
- To `kube-system` (kube-dns) → UDP 53
- To other `icn-coop-*` namespaces → QUIC + RPC ports
- To `icn` namespace → QUIC + RPC
- To `0.0.0.0/0` → QUIC ports (UDP)
  *(External P2P federation — coops need to reach peers outside the cluster)*

**Do NOT restrict:**
- Egress to external internet on QUIC ports — ICN is a P2P system, coops federate with nodes outside the cluster. Blocking external egress breaks real-world federation.

### Sketch

```yaml
# Template — apply per-coop with appropriate port numbers
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: default-deny-ingress
  namespace: icn-coop-alpha   # repeat per namespace
spec:
  podSelector: {}
  policyTypes: [Ingress]
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: allow-icn-mesh-ingress
  namespace: icn-coop-alpha
spec:
  podSelector:
    matchLabels:
      app: icn
  policyTypes: [Ingress]
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          icn-mesh: "true"   # label all coop + icn namespaces with this
    ports:
    - port: 7827    # QUIC — varies per coop
      protocol: UDP
    - port: 5651    # RPC — varies per coop
      protocol: TCP
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: allow-metrics-scrape
  namespace: icn-coop-alpha
spec:
  podSelector:
    matchLabels:
      app: icn
  policyTypes: [Ingress]
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          kubernetes.io/metadata.name: monitoring
    ports:
    - port: 9150    # metrics — varies per coop
      protocol: TCP
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: allow-dns-egress
  namespace: icn-coop-alpha
spec:
  podSelector: {}
  policyTypes: [Egress]
  egress:
  - to:
    - namespaceSelector:
        matchLabels:
          kubernetes.io/metadata.name: kube-system
    ports:
    - port: 53
      protocol: UDP
    - port: 53
      protocol: TCP
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: allow-icn-mesh-egress
  namespace: icn-coop-alpha
spec:
  podSelector:
    matchLabels:
      app: icn
  policyTypes: [Egress]
  egress:
  - to:
    - namespaceSelector:
        matchLabels:
          icn-mesh: "true"
  - to:
    - ipBlock:
        cidr: 0.0.0.0/0
        except:
        - 10.0.0.0/8      # block intra-cluster non-mesh traffic via external path
        - 192.168.0.0/16
    ports:
    - port: 7825    # QUIC range covering all coops + main daemon
      protocol: UDP
    - port: 7827
      protocol: UDP
    - port: 7831
      protocol: UDP
    - port: 7834
      protocol: UDP
    - port: 7777
      protocol: UDP
```

**Implementation note**: Because each coop has a different port number, the QUIC ingress rule needs to be per-coop (no shared port). Consider standardising all coops to the same port in a future refactor — that would make a single policy template work cleanly.

---

## Design: `monitoring` Namespace

Prometheus needs to scrape all namespaces — its egress must remain fully open. The main concern is controlling inbound access to Grafana and Prometheus UIs.

**Ingress:**
- NodePort access is handled at the node/iptables level, not by NetworkPolicy — no policy needed for NodePort ingress
- Intra-namespace: Prometheus → Alertmanager, Grafana → Prometheus — allow freely
- From `kube-system` → any (kubelet probes)

**Egress:**
- To all namespaces → any port (Prometheus must scrape everything)
- To external internet → TCP 443 (for remote write, alert webhooks if added)

**Assessment**: Restricting monitoring egress is high friction for low security gain in a homelab. Recommend leaving `monitoring` egress **unrestricted** indefinitely. Only add ingress default-deny if/when Grafana gets real user auth.

---

## Design: `registry` Namespace

The in-cluster registry (`10.8.30.40:30500`) is accessed two ways:
1. **ci-runner** (external, 10.8.30.46) → NodePort 30500 (Docker push)
2. **K3s containerd** on each node → NodePort 30500 (image pull via `registries.yaml`)

Both paths go through NodePort/iptables at the node level, not through pod-to-pod networking. The registry pod IP (`10.43.x.x`) is only directly accessed by in-cluster actors (e.g., `kubectl` commands against the registry API).

**Ingress:**
- From `icn` namespace → TCP 5000 (registry API, for any in-cluster push tooling)
- From `kube-system` → any (kubelet)
- NodePort access: not controlled by NetworkPolicy

**Egress:**
- DNS only (`kube-system` UDP 53)
- Registry is stateless for egress — it only serves content from its PVC

**Assessment**: Default-deny ingress on the registry namespace is safe and has no development impact, since dev workflow (ci-runner → NodePort → containerd) bypasses pod networking entirely.

---

## Implementation Order (when ready)

1. **Label namespaces** — add `icn-mesh: "true"` to `icn`, `icn-coop-alpha/beta/gamma/delta`
2. **Registry** — lowest risk, no dev impact. Add default-deny + allow-dns-egress
3. **icn-coop-*** — medium risk. Requires careful port mapping. Test P2P formation after applying
4. **monitoring** — low priority. Egress must stay open; only add ingress deny if Grafana gets auth

### Before implementing, verify

```bash
# Confirm coops can see each other via mDNS (multicast — won't be affected by NP)
kubectl exec -n icn-coop-alpha deploy/icn-alpha -- icnctl peer list

# Confirm Prometheus is scraping all coop metrics endpoints
curl -s http://10.8.30.40:30090/api/v1/targets | jq '.data.activeTargets | length'

# After applying any policy, immediately recheck mesh formation
kubectl exec -n icn-coop-alpha deploy/icn-alpha -- icnctl peer list
```

---

## Decision: Why Not Now

During active development:
- Developers need `kubectl exec` / `kubectl port-forward` into any pod freely
- Feature branches may add new ports or communication patterns before they're documented
- Broken NetworkPolicy is subtle — a misconfigured egress rule silently drops gossip messages with no error visible in app logs
- The cluster is on a private VLAN (10.8.30.0/24) behind OPNsense — the perimeter is already controlled

**Re-evaluate when**: First external pilot participant connects, OR when coop namespaces start holding real identity/ledger data.
