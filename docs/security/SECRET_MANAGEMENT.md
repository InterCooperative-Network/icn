# Secret Management Best Practices

This guide covers secure handling of secrets in ICN deployments, including passphrases, JWT secrets, and API keys.

## Overview

ICN uses several types of secrets:

| Secret | Purpose | Storage |
|--------|---------|---------|
| Keystore Passphrase | Encrypts Ed25519 private keys | Memory only (on unlock) |
| JWT Secret | Signs Gateway API tokens | Config/Environment |
| FCM Credentials | Push notifications | Config file |
| TLS Private Key | Secure connections | Generated per-node |

## Environment Variable Security

### Avoid Shell History

Prevent secrets from being logged in shell history:

```bash
# Add to ~/.bashrc or ~/.zshrc
export HISTIGNORE="*ICN_*:*PASSPHRASE*:*SECRET*:*PASSWORD*"

# Or use a space prefix (most shells ignore commands starting with space)
 export ICN_JWT_SECRET="your-secret"
```

### Interactive Passphrase Entry

Use `read -s` for secure input:

```bash
#!/bin/bash
echo -n "Enter keystore passphrase: "
read -s ICN_PASSPHRASE
echo
export ICN_PASSPHRASE
icnd --unlock
unset ICN_PASSPHRASE
```

### .env Files

For development, use `.env` files with restricted permissions:

```bash
# Create .env file
cat > .env << 'EOF'
ICN_JWT_SECRET=your-development-secret-here
ICN_GATEWAY_PORT=8080
EOF

# Restrict permissions (owner read/write only)
chmod 600 .env

# Load in your shell
set -a; source .env; set +a
```

**Never commit `.env` files to version control.**

## Secret Managers

### HashiCorp Vault

Recommended for production deployments:

```bash
# Store secret
vault kv put secret/icn jwt_secret="$(openssl rand -base64 32)"

# Retrieve at runtime
export ICN_JWT_SECRET=$(vault kv get -field=jwt_secret secret/icn)
icnd --gateway-enabled
```

### Kubernetes Secrets

For K8s deployments:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: icn-secrets
  namespace: icn
type: Opaque
stringData:
  jwt-secret: "your-production-secret-here"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: icnd
spec:
  template:
    spec:
      containers:
      - name: icnd
        env:
        - name: ICN_JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: icn-secrets
              key: jwt-secret
```

### AWS Secrets Manager

```bash
# Store secret
aws secretsmanager create-secret \
  --name icn/jwt-secret \
  --secret-string "$(openssl rand -base64 32)"

# Retrieve at runtime
export ICN_JWT_SECRET=$(aws secretsmanager get-secret-value \
  --secret-id icn/jwt-secret \
  --query SecretString --output text)
```

### GCP Secret Manager

```bash
# Store secret
echo -n "$(openssl rand -base64 32)" | \
  gcloud secrets create icn-jwt-secret --data-file=-

# Retrieve at runtime
export ICN_JWT_SECRET=$(gcloud secrets versions access latest \
  --secret=icn-jwt-secret)
```

## Keystore Security

### Strong Passphrases

Requirements for keystore passphrases:

- Minimum 16 characters
- Mix of uppercase, lowercase, numbers, symbols
- Avoid dictionary words
- Consider using a passphrase generator:

```bash
# Generate a strong passphrase
openssl rand -base64 24
# Example output: 7Kx9mP2qL5nR8vT3wY6zB1cF

# Or use words (easier to remember)
shuf -n4 /usr/share/dict/words | tr '\n' '-' | sed 's/-$//'
# Example output: correct-horse-battery-staple
```

### Backup Procedures

1. **Encrypt backups**: Never store unencrypted keystore backups
2. **Multiple locations**: Store backups in geographically separate locations
3. **Test recovery**: Regularly test that backups can be restored

```bash
# Export keystore (encrypted)
icnctl id export --output ~/backup/keystore-$(date +%Y%m%d).age

# Verify backup
icnctl id verify ~/backup/keystore-*.age
```

### Key Rotation

Rotate keys periodically or after suspected compromise:

```bash
# Rotate to new keypair
icnctl id rotate --reason scheduled

# The old DID remains valid during transition period
# Network will propagate the rotation event
```

## JWT Secret Requirements

### Generation

Generate cryptographically secure JWT secrets:

```bash
# Recommended: 256-bit (32 bytes) secret
openssl rand -base64 32

# Or using /dev/urandom
head -c 32 /dev/urandom | base64
```

### Rotation

1. Generate new secret
2. Update secret manager
3. Restart Gateway with new secret
4. Existing tokens remain valid until expiry

```bash
# Generate new secret
NEW_SECRET=$(openssl rand -base64 32)

# Update in Vault
vault kv put secret/icn jwt_secret="$NEW_SECRET"

# Rolling restart of icnd pods (K8s)
kubectl rollout restart deployment/icnd -n icn
```

## Configuration File Security

### File Permissions

```bash
# Restrict config file permissions
chmod 600 ~/.icn/config.toml
chmod 700 ~/.icn/

# Verify permissions
ls -la ~/.icn/
# Should show: -rw------- for files, drwx------ for directories
```

### Sensitive Fields

Never log configuration containing secrets:

```rust
// BAD: Logs the secret
tracing::info!("Config: {:?}", config);

// GOOD: Redact sensitive fields
tracing::info!("Config loaded, gateway_enabled={}", config.gateway.enabled);
```

## Deployment Checklist

Before deploying to production:

- [ ] JWT secret is at least 32 bytes, randomly generated
- [ ] Secrets are stored in a secret manager (not in code/config files)
- [ ] Environment variables are not logged
- [ ] Shell history ignores secret-related commands
- [ ] Config files have restricted permissions (600)
- [ ] Keystore passphrase meets strength requirements
- [ ] Backup procedures are documented and tested
- [ ] Key rotation procedures are documented
- [ ] `--insecure-gateway-no-jwt` flag is NOT used

## Incident Response

If a secret is compromised:

1. **Immediate**: Rotate the compromised secret
2. **Assess**: Determine scope of potential exposure
3. **Revoke**: Invalidate any tokens signed with old secret
4. **Audit**: Review logs for unauthorized access
5. **Document**: Record incident and response actions

```bash
# Emergency JWT secret rotation
vault kv put secret/icn jwt_secret="$(openssl rand -base64 32)"
kubectl rollout restart deployment/icnd -n icn

# For keystore compromise, rotate DID
icnctl id rotate --reason compromised
```

## See Also

- [Production Hardening Guide](../production-hardening.md)
- [Kubernetes Deployment](../KUBERNETES_DEPLOYMENT.md)
- [Incident Response](../incident-response.md)
