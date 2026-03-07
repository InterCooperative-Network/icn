# Demo Gateway Secrets

JWT secrets for each coop gateway are stored as K8s Secrets. They are NOT committed to git.

## Coop Topology

| Persona | Namespace | Deployment | gRPC NodePort | Gateway (port-forward) |
|---|---|---|---|---|
| BrightWorks Cooperative | icn-coop-alpha | icn-alpha | 10.8.30.40:30651 | kubectl port-forward svc/icn-alpha 18081:8080 -n icn-coop-alpha |
| River City Tool Library | icn-coop-beta | icn-beta | 10.8.30.40:30658 | kubectl port-forward svc/icn-beta 18082:8080 -n icn-coop-beta |
| Harbor Homes Cooperative | icn-coop-gamma | icn-gamma | 10.8.30.40:30649 | kubectl port-forward svc/icn-gamma 18083:8080 -n icn-coop-gamma |
| Finger Lakes CDN | icn-coop-delta | icn-delta | 10.8.30.40:30655 | kubectl port-forward svc/icn-delta 18084:8080 -n icn-coop-delta |

## Where Secrets Live

Each coop's K8s Secret (`icn-{coop}-secrets`) contains two keys:

| Key | Purpose |
|---|---|
| `passphrase` | Age-encrypted keystore passphrase (pre-existing) |
| `jwt-secret` | 32-byte hex secret for gateway JWT signing (added 2026-03-07) |

The `ICN_GATEWAY_JWT_SECRET` env var in each pod references `jwt-secret` from the coop's secret.

## How to Check Current Secrets

```bash
# Verify keys present (values are redacted):
kubectl get secret icn-alpha-secrets -n icn-coop-alpha -o jsonpath='{.data}' | python3 -c "import sys,json; print(list(json.load(sys.stdin).keys()))"
# Expected: ['passphrase', 'jwt-secret']
```

## How to Regenerate JWT Secrets

If a pod is redeployed and the secret is lost, generate new ones:

```bash
# Generate new secrets
JWT_ALPHA=$(openssl rand -hex 32)
JWT_BETA=$(openssl rand -hex 32)
JWT_GAMMA=$(openssl rand -hex 32)
JWT_DELTA=$(openssl rand -hex 32)

# Get existing passphrases
PP_ALPHA=$(kubectl get secret icn-alpha-secrets -n icn-coop-alpha -o jsonpath='{.data.passphrase}' | base64 -d)
PP_BETA=$(kubectl get secret icn-beta-secrets -n icn-coop-beta -o jsonpath='{.data.passphrase}' | base64 -d)
PP_GAMMA=$(kubectl get secret icn-gamma-secrets -n icn-coop-gamma -o jsonpath='{.data.passphrase}' | base64 -d)
PP_DELTA=$(kubectl get secret icn-delta-secrets -n icn-coop-delta -o jsonpath='{.data.passphrase}' | base64 -d)

# Apply (preserves passphrase, rotates jwt-secret)
kubectl create secret generic icn-alpha-secrets -n icn-coop-alpha \
  --from-literal=passphrase="$PP_ALPHA" --from-literal=jwt-secret="$JWT_ALPHA" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl create secret generic icn-beta-secrets -n icn-coop-beta \
  --from-literal=passphrase="$PP_BETA" --from-literal=jwt-secret="$JWT_BETA" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl create secret generic icn-gamma-secrets -n icn-coop-gamma \
  --from-literal=passphrase="$PP_GAMMA" --from-literal=jwt-secret="$JWT_GAMMA" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl create secret generic icn-delta-secrets -n icn-coop-delta \
  --from-literal=passphrase="$PP_DELTA" --from-literal=jwt-secret="$JWT_DELTA" \
  --dry-run=client -o yaml | kubectl apply -f -

# Restart pods to pick up new secrets
kubectl rollout restart deployment/icn-alpha -n icn-coop-alpha
kubectl rollout restart deployment/icn-beta -n icn-coop-beta
kubectl rollout restart deployment/icn-gamma -n icn-coop-gamma
kubectl rollout restart deployment/icn-delta -n icn-coop-delta
```

## How Gateway Was Enabled

Gateway was enabled via pod args and env var (NOT via icn.toml ConfigMap changes):

- Args added: `--gateway-enable --gateway-bind 0.0.0.0:8080`
- Env var added: `ICN_GATEWAY_JWT_SECRET` from `icn-{coop}-secrets[jwt-secret]`

The patch template is in `deploy/k8s/multi-node/gateway-patch.yaml`.

## Verifying Gateway Health

```bash
# Port-forward and check health
kubectl port-forward svc/icn-alpha 18081:8080 -n icn-coop-alpha &
curl http://localhost:18081/v1/health
# Expected: {"status":"ok","version":"0.1.0",...}

# Auth challenge (requires a real registered DID)
kubectl exec -n icn-coop-alpha deploy/icn-alpha -- icnctl id show 2>/dev/null
# Use that DID:
curl -X POST http://localhost:18081/v1/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"did":"did:icn:z<actual-did>"}'
# Expected: {"nonce":"<hex>","expires_in":300}
```

## Note on icn.toml

The `deploy-coop.sh` script generates a `[gateway]` section in `icn.toml` for new deployments.
The existing four coop pods were deployed before this was added, so their ConfigMaps do not have
`[gateway]`. The gateway is active via pod args only. When redeploying from scratch, use the
current `deploy-coop.sh` which generates a complete config.
