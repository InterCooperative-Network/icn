# Security Incident Response

## Summary

Procedure for responding to suspected security incidents affecting ICN nodes.

**Use when**:
- Unauthorized access suspected
- Unusual network activity detected
- Key compromise suspected
- Byzantine behavior from known peer
- Malicious transactions detected

## Severity Levels

| Level | Description | Response Time |
|-------|-------------|---------------|
| **Critical** | Active exploitation, key compromise | Immediate |
| **High** | Vulnerability being probed | < 1 hour |
| **Medium** | Suspicious activity, no confirmed exploit | < 4 hours |
| **Low** | Policy violation, minor anomaly | < 24 hours |

## Immediate Actions (All Severities)

### 1. Document the Incident

```bash
# Create incident log
INCIDENT_ID="INC-$(date +%Y%m%d-%H%M%S)"
mkdir -p ~/incidents/$INCIDENT_ID
cd ~/incidents/$INCIDENT_ID

# Start logging
echo "Incident started: $(date)" > incident.log
echo "Reporter: $(whoami)" >> incident.log
echo "Initial observation: <describe what you saw>" >> incident.log
```

### 2. Preserve Evidence

```bash
# Capture current state
icnctl status > status.txt 2>&1
curl -s http://localhost:9100/metrics > metrics.txt

# Capture logs
journalctl -u icnd -n 10000 > icnd-logs.txt
# OR for K8s:
kubectl -n icn logs deployment/icn-daemon --tail=10000 > icnd-logs.txt

# Capture network state
ss -tuln > network-state.txt
netstat -an > netstat.txt
```

## Critical: Key Compromise

If private key may be compromised:

### 1. Isolate Node Immediately

```bash
# K8s: Scale to zero
kubectl -n icn scale deployment/icn-daemon --replicas=0

# Systemd: Stop and disable
systemctl stop icnd
systemctl disable icnd

# Block network (if needed)
iptables -A OUTPUT -p udp --dport 7777 -j DROP
```

### 2. Rotate Keys

```bash
# Generate new identity (AFTER securing old one)
icnctl id rotate --reason compromised

# This creates new DID and signs rotation proof
```

### 3. Notify Peers

Contact known peers to:
- Revoke trust in old DID
- Add trust to new DID
- Report suspicious activity from old DID

### 4. Forensic Analysis

```bash
# Check for unauthorized keystore access
ls -la ~/.icn/identity.age
stat ~/.icn/identity.age

# Check shell history
cat ~/.bash_history | grep -i icn

# Check auth logs
grep -i "icn\|identity" /var/log/auth.log
```

## High: Byzantine Peer Detected

If a peer is behaving maliciously:

### 1. Identify the Peer

```bash
# Check misbehavior metrics
curl -s http://localhost:9100/metrics | grep misbehavior

# List peers with issues
icnctl peers list --verbose
```

### 2. Block the Peer

```bash
# Add to blocklist (if supported)
icnctl peers block <DID>

# Or via config:
# [network]
# blocked_peers = ["did:icn:..."]
```

### 3. Report Violation

```bash
# The misbehavior is automatically:
# - Logged locally
# - Reported to connected peers
# - Used to reduce trust score

# Check violation records
curl -s http://localhost:9100/metrics | grep icn_misbehavior
```

## Medium: Suspicious Network Activity

### 1. Analyze Traffic

```bash
# Check connection counts
curl -s http://localhost:9100/metrics | grep icn_net_connections

# Check rate limiting triggers
curl -s http://localhost:9100/metrics | grep rate_limit

# Check message rates
curl -s http://localhost:9100/metrics | grep icn_gossip_messages
```

### 2. Review Trust Relationships

```bash
# List trust edges
icnctl trust list

# Check for unexpected trust
icnctl trust list | grep -v "expected-did-pattern"
```

### 3. Increase Logging

```bash
# Temporarily increase log level
# In config.toml:
# [observability]
# log_level = "debug"

# Restart to apply
systemctl restart icnd
```

## Low: Policy Violation

### 1. Document Violation

```bash
# Add to incident log
echo "Policy violation observed:" >> incident.log
echo "Type: <governance/resource/trust>" >> incident.log
echo "Details: <specific violation>" >> incident.log
```

### 2. Review Governance Logs

```bash
# Check proposal activity
icnctl gov proposals list

# Check voting activity
icnctl gov votes list
```

## Post-Incident Actions

### 1. Root Cause Analysis

- [ ] What vulnerability was exploited?
- [ ] How was it detected?
- [ ] What was the impact?
- [ ] How can it be prevented?

### 2. Remediation

- [ ] Patch vulnerability if code issue
- [ ] Update configurations if misconfiguration
- [ ] Rotate credentials if exposed
- [ ] Update monitoring if detection was slow

### 3. Documentation

```bash
# Finalize incident report
cat > incident-report.md << EOF
# Incident Report: $INCIDENT_ID

## Summary
[Brief description]

## Timeline
- [Time]: [Event]
- [Time]: [Event]

## Impact
[What was affected]

## Root Cause
[Why it happened]

## Remediation
[What was done to fix]

## Prevention
[How to prevent recurrence]
EOF
```

### 4. Communication

- [ ] Notify affected parties
- [ ] Update security documentation
- [ ] Share learnings (without sensitive details)

## Emergency Contacts

| Role | Contact |
|------|---------|
| Security Lead | [TBD] |
| On-Call | [TBD] |
| Escalation | [TBD] |

## Related

- [Emergency Restart](./01-emergency-restart.md) - If need to restart after incident
- [Data Recovery](./02-data-recovery.md) - If data compromised
- [Troubleshooting](./05-troubleshooting.md) - Common security-related issues
