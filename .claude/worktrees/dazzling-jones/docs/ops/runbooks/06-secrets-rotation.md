# Secrets Rotation Procedure

## Summary

Procedure for rotating cryptographic secrets and credentials used by ICN nodes.

**Use when**:
- Scheduled rotation (recommended: quarterly)
- Suspected key compromise
- Personnel change with key access
- Security audit requirement

**Do NOT use when**:
- Normal restart (no rotation needed)
- Key is confirmed compromised (use Security Incident runbook first)

## Secret Types

| Secret | Location | Rotation Frequency |
|--------|----------|-------------------|
| Node Identity (Ed25519) | `~/.icn/identity.age` | Annually or on compromise |
| Keystore Passphrase | Environment/secrets manager | Quarterly |
| TLS Certificate | `~/.icn/tls-cert.pem` | Annually |
| Bootstrap Tokens | Config file | On personnel change |
| API Keys (if used) | Config/environment | Quarterly |

## Prerequisites

- [ ] Current backup verified (`icnctl verify-backup`)
- [ ] New passphrase generated (if rotating)
- [ ] Maintenance window scheduled
- [ ] Peer operators notified (for identity rotation)

---

## Keystore Passphrase Rotation

### Step 1: Create Backup

```bash
# Backup with current passphrase
ICN_PASSPHRASE="current-passphrase" icnctl backup ~/icn-backup-pre-rotation.tar

# Verify backup
icnctl verify-backup ~/icn-backup-pre-rotation.tar
```

### Step 2: Stop Node

```bash
# Kubernetes
kubectl -n icn scale deployment/icn-daemon --replicas=0

# Systemd
systemctl stop icnd
```

### Step 3: Re-encrypt Keystore

```bash
# Export identity with current passphrase
ICN_PASSPHRASE="current-passphrase" icnctl id export > identity-backup.json

# Re-import with new passphrase
ICN_PASSPHRASE="new-secure-passphrase" icnctl id import identity-backup.json --force

# Securely delete plaintext export
shred -u identity-backup.json
```

### Step 4: Update Secret Storage

**Kubernetes Secret**:
```bash
# Update secret
kubectl -n icn create secret generic icn-passphrase \
  --from-literal=passphrase="new-secure-passphrase" \
  --dry-run=client -o yaml | kubectl apply -f -

# Restart deployment to pick up new secret
kubectl -n icn rollout restart deployment/icn-daemon
```

**Systemd Environment**:
```bash
# Update environment file
sudo vim /etc/icn/environment
# Change: ICN_PASSPHRASE=new-secure-passphrase

# Reload and restart
sudo systemctl daemon-reload
systemctl start icnd
```

### Step 5: Verify

```bash
# Check node starts successfully
icnctl status

# Verify identity accessible
icnctl id show
```

---

## Node Identity Rotation

**WARNING**: Identity rotation changes your DID. All trust relationships must be re-established.

### Step 1: Notify Peers

Before rotating, notify all peers who have trust edges to your node:
- They will need to revoke trust in old DID
- They will need to add trust to new DID

### Step 2: Create Backup

```bash
icnctl backup ~/icn-backup-pre-identity-rotation.tar
icnctl verify-backup ~/icn-backup-pre-identity-rotation.tar
```

### Step 3: Stop Node

```bash
systemctl stop icnd
# OR
kubectl -n icn scale deployment/icn-daemon --replicas=0
```

### Step 4: Rotate Identity

```bash
# Generate new identity with rotation proof
ICN_PASSPHRASE="your-passphrase" icnctl id rotate --reason "scheduled rotation"

# This will:
# - Generate new Ed25519 keypair
# - Sign rotation proof with old key
# - Update keystore with new identity
# - Output new DID

# Note: Old DID is preserved in rotation proof for verification
```

### Step 5: Update Configuration

If your DID is referenced in config files or bootstrap lists:

```bash
# Get new DID
icnctl id show

# Update any config references
vim ~/.icn/config.toml
```

### Step 6: Restart and Verify

```bash
systemctl start icnd

# Verify new identity
icnctl id show

# Check peers reconnecting
curl -s http://localhost:9100/metrics | grep icn_gossip_peers
```

### Step 7: Re-establish Trust

Peers need to:
```bash
# Revoke trust in old DID
icnctl trust revoke <old-did>

# Add trust to new DID
icnctl trust add <new-did> --weight 0.8
```

---

## TLS Certificate Rotation

### Step 1: Generate New Certificate

```bash
# Stop node first
systemctl stop icnd

# Generate new self-signed cert (if using self-signed)
openssl req -x509 -newkey ed25519 \
  -keyout ~/.icn/tls-key-new.pem \
  -out ~/.icn/tls-cert-new.pem \
  -days 365 -nodes \
  -subj "/CN=$(icnctl id show | grep DID | awk '{print $2}')"

# Backup old certs
mv ~/.icn/tls-cert.pem ~/.icn/tls-cert.pem.old
mv ~/.icn/tls-key.pem ~/.icn/tls-key.pem.old

# Install new certs
mv ~/.icn/tls-cert-new.pem ~/.icn/tls-cert.pem
mv ~/.icn/tls-key-new.pem ~/.icn/tls-key.pem

# Set permissions
chmod 600 ~/.icn/tls-key.pem
chmod 644 ~/.icn/tls-cert.pem
```

### Step 2: Restart and Verify

```bash
systemctl start icnd

# Verify TLS working
openssl s_client -connect localhost:5601 -brief
```

---

## Bootstrap Token Rotation

If using static bootstrap tokens for initial peer authentication:

### Step 1: Generate New Token

```bash
# Generate cryptographically secure token
openssl rand -base64 32 > new-bootstrap-token.txt
```

### Step 2: Update Configuration

```toml
# In config.toml
[network]
bootstrap_token = "new-token-value"
```

### Step 3: Coordinate with Peers

All peers using the bootstrap token must update simultaneously:
1. Distribute new token securely
2. Schedule coordinated config update
3. Rolling restart all nodes

---

## Kubernetes Secrets Rotation

### Full Secret Rotation

```bash
# Generate new passphrase
NEW_PASS=$(openssl rand -base64 32)

# Create new secret version
kubectl -n icn create secret generic icn-secrets-v2 \
  --from-literal=passphrase="${NEW_PASS}" \
  --from-file=tls-cert=~/.icn/tls-cert.pem \
  --from-file=tls-key=~/.icn/tls-key.pem

# Update deployment to use new secret
kubectl -n icn patch deployment icn-daemon \
  -p '{"spec":{"template":{"spec":{"volumes":[{"name":"secrets","secret":{"secretName":"icn-secrets-v2"}}]}}}}'

# After verification, delete old secret
kubectl -n icn delete secret icn-secrets-v1
```

### Using External Secrets Operator

If using external-secrets or similar:

```bash
# Update source secret in vault/AWS SM/etc
aws secretsmanager update-secret --secret-id icn/prod/passphrase \
  --secret-string "${NEW_PASS}"

# Trigger sync
kubectl -n icn annotate externalsecret icn-secrets force-sync=$(date +%s)
```

---

## Rotation Schedule

### Recommended Cadence

| Secret Type | Frequency | Notes |
|-------------|-----------|-------|
| Keystore Passphrase | Quarterly | Or on personnel change |
| Node Identity | Annually | Requires trust re-establishment |
| TLS Certificates | Annually | Before expiration |
| Bootstrap Tokens | On change | When operators join/leave |
| API Keys | Quarterly | If using external integrations |

### Rotation Calendar

```bash
# Add to cron or calendar
# Q1 (Jan): Keystore passphrase, API keys
# Q2 (Apr): Keystore passphrase
# Q3 (Jul): Keystore passphrase, API keys, TLS certs
# Q4 (Oct): Keystore passphrase, Node identity (if scheduled)
```

---

## Verification Checklist

After any rotation:

- [ ] Node starts successfully
- [ ] Identity shows expected DID
- [ ] Metrics endpoint responding
- [ ] Gossip peers reconnecting
- [ ] No authentication errors in logs
- [ ] Backup created with new credentials

## Rollback

If rotation causes issues:

```bash
# Stop node
systemctl stop icnd

# Restore from pre-rotation backup
icnctl restore ~/icn-backup-pre-rotation.tar --force

# Restore old passphrase in environment
# Update /etc/icn/environment or K8s secret

# Restart
systemctl start icnd
```

---

## Security Best Practices

1. **Never store passphrases in plain text** - Use secrets manager or encrypted storage
2. **Use strong passphrases** - Minimum 32 bytes of entropy
3. **Rotate on compromise** - Don't wait for schedule if breach suspected
4. **Document rotations** - Keep audit log of when secrets were rotated
5. **Test rotation procedure** - Practice in staging before production
6. **Secure deletion** - Use `shred` for any temporary plaintext files

---

## Related

- [Security Incident](./04-security-incident.md) - If key compromise is confirmed
- [Data Recovery](./02-data-recovery.md) - If rotation corrupts data
- [Emergency Restart](./01-emergency-restart.md) - If rotation causes crash

