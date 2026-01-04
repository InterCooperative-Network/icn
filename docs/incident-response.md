# ICN Incident Response Playbook

This document provides operational procedures for responding to common incidents in ICN deployments. While some responses are currently manual or crude in v0.1, having documented procedures is critical for operational readiness.

**Audience**: ICN node operators, cooperative system administrators, incident responders

**Status**: Living document - procedures will evolve as ICN matures

---

## Table of Contents

1. [General Incident Response Framework](#general-incident-response-framework)
2. [K3s Deployment Quick Reference](#k3s-deployment-quick-reference)
3. [Incident: Node Compromise](#incident-node-compromise)
4. [Incident: Ledger Corruption Detected](#incident-ledger-corruption-detected)
5. [Incident: Key Suspected Stolen](#incident-key-suspected-stolen)
6. [Incident: Network Partition](#incident-network-partition)
7. [Incident: Pod Failure (K3s)](#incident-pod-failure-k3s)
8. [Incident: Gossip Storm](#incident-gossip-storm)
9. [Incident: Quarantine Growth](#incident-quarantine-growth)
10. [Incident: Storage Issues (K3s)](#incident-storage-issues-k3s)
11. [Monitoring and Detection](#monitoring-and-detection)
12. [Communication Templates](#communication-templates)

---

## General Incident Response Framework

### Severity Levels

**P0 - Critical**: Identity compromise, data loss, complete service outage
- Response time: Immediate
- Escalation: All hands

**P1 - High**: Partial service degradation, security concern
- Response time: Within 1 hour
- Escalation: On-call operator

**P2 - Medium**: Non-critical issues, performance degradation
- Response time: Within 4 hours
- Escalation: Normal channels

**P3 - Low**: Minor issues, cosmetic problems
- Response time: Next business day
- Escalation: Ticket queue

### Response Steps

1. **Detect**: Monitoring alerts, user reports, health checks
2. **Assess**: Determine severity and scope
3. **Contain**: Prevent further damage
4. **Recover**: Restore normal operations
5. **Document**: Record what happened and how it was resolved
6. **Review**: Post-mortem and process improvement

---

## K3s Deployment Quick Reference

This section provides K3s-specific commands for the live homelab deployment.

### Cluster Access

```bash
# SSH to control plane
ssh ubuntu@10.8.10.40

# All kubectl commands require sudo on control plane
sudo kubectl -n icn <command>
```

### Quick Diagnosis Commands

```bash
# Check pod status
sudo kubectl -n icn get pods -o wide

# View pod logs (live)
sudo kubectl -n icn logs -f deployment/icn-daemon

# View pod logs (last 100 lines)
sudo kubectl -n icn logs deployment/icn-daemon --tail=100

# Describe pod for events and status
sudo kubectl -n icn describe pod -l app=icn-daemon

# Check resource usage
sudo kubectl -n icn top pods

# View all ICN resources
sudo kubectl -n icn get all

# Check PVC status
sudo kubectl -n icn get pvc
```

### Health Checks

```bash
# ICN health endpoint
curl http://10.8.10.40:30080/health

# Prometheus metrics
curl http://10.8.10.40:30100/metrics | head -50

# Grafana dashboard
# Open: http://10.8.10.40:30300

# Check node identity
sudo kubectl -n icn exec deploy/icn-daemon -- /usr/local/bin/icnctl id show
```

### Restart Procedures

```bash
# Restart ICN daemon (graceful)
sudo kubectl -n icn rollout restart deployment/icn-daemon

# Wait for rollout to complete
sudo kubectl -n icn rollout status deployment/icn-daemon

# Force delete stuck pod
sudo kubectl -n icn delete pod -l app=icn-daemon --force --grace-period=0

# Full redeploy from local
cd /home/matt/projects/icn/deploy/k8s && make full-deploy-dev
```

### Backup & Recovery

```bash
# Backup current state (from within pod)
sudo kubectl -n icn exec deploy/icn-daemon -- /usr/local/bin/icnctl backup /data/backup.tar

# Copy backup from pod to local
sudo kubectl -n icn cp icn-daemon-xxx:/data/backup.tar /tmp/icn-backup.tar

# View backup location (NFS)
ls -la /mnt/atlas/k8s/icn-data/
```

### Network Diagnostics

```bash
# Check service endpoints
sudo kubectl -n icn get endpoints

# Test internal connectivity
sudo kubectl -n icn exec deploy/icn-daemon -- curl localhost:9100/metrics

# View network policies
sudo kubectl -n icn get networkpolicies

# Check node port exposure
sudo kubectl -n icn get svc
```

### Alertmanager

```bash
# View active alerts
curl -s http://10.8.10.40:30093/api/v1/alerts | jq '.data[] | {labels: .labels, state: .status.state}'

# Silence an alert (for maintenance)
# Use Alertmanager UI: http://10.8.10.40:30093
```

---

## Incident: Node Compromise

**Severity**: P0 - Critical

### Symptoms

- Unauthorized access to node detected
- Suspicious processes running
- Unexpected network traffic
- Alerts from intrusion detection systems
- DID signing messages you didn't authorize

### Immediate Actions (First 15 Minutes)

1. **Isolate the node immediately**:
   ```bash
   # Stop ICNd
   systemctl stop icnd
   # OR
   pkill icnd

   # Block network access (if available)
   sudo iptables -A INPUT -j DROP
   sudo iptables -A OUTPUT -j DROP
   ```

2. **Preserve evidence**:
   ```bash
   # Capture running processes
   ps auxf > /tmp/incident-processes.txt

   # Capture network connections
   netstat -tulpn > /tmp/incident-netstat.txt

   # Copy ICN logs
   cp -r ~/.icn/logs /tmp/incident-logs-$(date +%Y%m%d-%H%M%S)

   # Capture system logs
   journalctl -u icnd --since "1 hour ago" > /tmp/incident-journalctl.txt
   ```

3. **Notify cooperative members**:
   - Alert other node operators immediately
   - Warn them NOT to trust messages from your DID
   - Coordinate on out-of-band communication (Signal, phone, etc.)

### Recovery Actions (Next 2 Hours)

4. **Revoke compromised device**:

   **On a trusted device** (not the compromised one):
   ```bash
   # Restore your identity from backup to a secure device
   icnctl --data-dir /secure/path restore /backup/icn-backup.tar

   # List devices to identify the compromised one
   icnctl device list

   # Revoke the compromised device
   icnctl device revoke device-compromised-id --reason compromised
   ```

5. **Rotate all keys**:
   ```bash
   # On the secure device, rotate the main key
   icnctl id rotate --reason "Compromise detected - rotating all keys"
   ```

6. **Audit recent activity**:
   ```bash
   # Check ledger for unauthorized transactions
   icnctl ledger history --limit 100

   # Review trust edges - did attacker add malicious trust?
   icnctl trust list

   # Check deployed contracts
   # (Future: icnctl contract list)
   ```

### Investigation

7. **Determine attack vector**:
   - Check system logs for unauthorized SSH access
   - Review application logs for exploitation attempts
   - Examine network logs for command & control traffic
   - Check for malware or rootkits
   - Review recent software updates or configuration changes

8. **Assess damage**:
   - What data was accessed?
   - Were transactions authorized on your behalf?
   - Was trust graph manipulated?
   - Were contracts deployed or modified?

### Long-Term Actions

9. **Harden the replacement node**:
   - Reinstall OS from scratch (don't trust compromised system)
   - Apply all security patches
   - Enable fail2ban or equivalent
   - Configure firewall rules (only allow necessary ports)
   - Enable audit logging
   - Consider using a hardware security module (HSM) for keys

10. **Post-mortem**:
    - Document timeline of compromise
    - Root cause analysis
    - Update security procedures
    - Share lessons learned with cooperative

### Prevention

- **Principle of Least Privilege**: Run ICNd as non-root user
- **Network Segmentation**: Firewall rules limiting access
- **Regular Backups**: Daily encrypted backups to off-site location
- **Monitoring**: Set up alerts for unusual activity
- **Multi-Device**: Use separate devices for different risk profiles
- **HSM**: Consider hardware security modules for high-value identities

---

## Incident: Ledger Corruption Detected

**Severity**: P1 - High (can escalate to P0 if widespread)

### Symptoms

- Quarantine size growing rapidly (`icn_ledger_quarantine_size` metric)
- Merge conflict alerts (`icn_ledger_merge_conflicts_total`)
- Balance inconsistencies reported by users
- Monitoring dashboard shows ledger errors
- Failed double-entry validation

### Assessment

1. **Check quarantine status**:
   ```bash
   # View quarantine size from dashboard
   curl http://localhost:8080/health | jq '.ledger_quarantine_size'

   # Or check Prometheus metrics
   curl http://localhost:9100/metrics | grep icn_ledger_quarantine_size
   ```

2. **List quarantined entries** (future command):
   ```bash
   icnctl ledger quarantine list
   ```

3. **Determine scope**:
   - Is this affecting one account or many?
   - Is this a local issue or network-wide?
   - What's the time window of affected transactions?

### Recovery Procedures

#### Scenario 1: Small Number of Conflicting Entries

**If < 10 entries quarantined:**

1. **Inspect each entry**:
   ```bash
   icnctl ledger quarantine get <entry-hash>
   ```

2. **Manual resolution**:
   - If entry is valid but conflicted: Release from quarantine
   - If entry is malicious or erroneous: Drop permanently

   ```bash
   # Release valid entry (retry processing)
   icnctl ledger quarantine release <entry-hash>

   # Drop invalid entry
   icnctl ledger quarantine drop <entry-hash>
   ```

3. **Verify resolution**:
   ```bash
   icnctl ledger balance <account-id>
   ```

#### Scenario 2: Large-Scale Corruption

**If > 100 entries quarantined or balances severely wrong:**

**⚠️ This is a critical incident - coordinate with cooperative before proceeding**

1. **Stop the daemon**:
   ```bash
   systemctl stop icnd
   ```

2. **Backup current state** (even if corrupted):
   ```bash
   icnctl backup /backup/corrupted-state-$(date +%Y%m%d-%H%M%S).tar
   ```

3. **Restore from last known good backup**:
   ```bash
   # Identify last good backup (check timestamp and quarantine size)
   ls -lh /backup/

   # Restore
   icnctl restore /backup/icn-backup-20250114.tar --force
   ```

4. **Purge quarantine**:
   ```bash
   icnctl ledger quarantine purge
   ```

5. **Restart daemon**:
   ```bash
   systemctl start icnd
   ```

6. **Monitor gossip sync**:
   - Watch dashboard for entries being re-synced
   - Monitor quarantine to see if conflicts reappear
   - If they do, there's a systemic issue (see Investigation below)

#### Scenario 3: Unrecoverable Corruption

**If restore doesn't work or no good backup exists:**

**🚨 Nuclear option - coordinate with entire cooperative**

1. **Reconstruct from cooperative consensus**:
   - Poll all nodes for their ledger state
   - Identify the most common version (Byzantine consensus)
   - Majority state becomes canonical

2. **Manual ledger reconstruction** (requires all members):
   - Export ledger data from trusted nodes
   - Manually reconcile discrepancies
   - Re-import agreed-upon state

   *This is a last resort and requires governance decision*

### Investigation

**Why did corruption occur?**

Common causes:
1. **Concurrent updates** - Two nodes created conflicting transactions simultaneously
2. **Clock skew** - Node clocks out of sync causing timestamp issues
3. **Malicious entry** - Attacker injected invalid transaction
4. **Software bug** - Double-entry validation logic failure
5. **Disk corruption** - Hardware failure corrupting database

**Check for:**
```bash
# Clock skew
timedatectl status

# Disk errors
dmesg | grep -i error
smartctl -a /dev/sda

# Recent software updates
journalctl -u icnd --since "1 week ago" | grep upgrade
```

### Prevention

- **Regular backups**: Automated daily backups
- **Monitoring**: Alert on quarantine size > 10
- **Clock sync**: NTP properly configured
- **Disk health**: SMART monitoring enabled
- **Testing**: Validate ledger integrity weekly
- **Redundancy**: Multiple nodes per cooperative

---

## Incident: Key Suspected Stolen

**Severity**: P0 - Critical

### Symptoms

- Unauthorized transactions appearing in ledger
- Messages signed by your DID that you didn't send
- Your device missing or stolen
- Suspicious login attempts
- Passphrase may have been compromised

### Immediate Actions (First 30 Minutes)

**If you have access to an authorized device:**

1. **Revoke the compromised device IMMEDIATELY**:
   ```bash
   # From a secure device
   icnctl device list
   icnctl device revoke device-stolen-id --reason lost
   ```

2. **Rotate your main key**:
   ```bash
   icnctl id rotate --reason "Key compromise suspected"
   ```

3. **Change your passphrase**:
   ```bash
   # Export with old passphrase
   icnctl id export /tmp/identity-temp.age

   # Import with new passphrase (will prompt for new one)
   icnctl id import /tmp/identity-temp.age

   # Securely delete temp file
   shred -u /tmp/identity-temp.age
   ```

4. **Notify cooperative**:
   - Alert all members immediately
   - Provide new DID after rotation
   - Request they update trust edges

**If you DON'T have access to an authorized device:**

**This is the worst-case scenario - you need social recovery**

1. **Contact cooperative members urgently**:
   - Use out-of-band communication (phone, in-person)
   - Verify your identity through established procedures
   - Request they revoke trust edges to your compromised DID

2. **Create new identity**:
   ```bash
   # On a secure device
   icnctl --data-dir ~/.icn-new id init
   ```

3. **Social recovery** (Phase 11.6 - not yet implemented):
   ```bash
   # Future: Request guardians to approve identity recovery
   icnctl id recover --guardians did:icn:guardian1,did:icn:guardian2
   ```

4. **Rebuild trust**:
   - Request cooperative members add trust edges to new DID
   - Re-establish economic relationships
   - Accept that old ledger history is tied to compromised DID

   **⚠️ This is painful - emphasizes importance of multi-device setup**

### Key Rotation Ceremony (Planned Migration)

**For non-emergency key rotation** (e.g., annual security practice):

1. **Schedule rotation window** with cooperative:
   - Announce rotation 1 week in advance
   - Pick low-activity time window
   - Ensure all members are available for coordination

2. **Pre-rotation checks**:
   ```bash
   # Verify all devices are accessible
   icnctl device list

   # Create backup
   icnctl backup /backup/pre-rotation-$(date +%Y%m%d).tar

   # Verify backup
   icnctl restore /tmp/test-restore /backup/pre-rotation-*.tar
   ```

3. **Execute rotation**:
   ```bash
   icnctl id rotate --reason "Annual key rotation"
   ```

4. **Verify rotation**:
   ```bash
   # Check new DID
   icnctl id show

   # Verify old key is marked as rotated
   icnctl device list
   ```

5. **Update external systems**:
   - Notify cooperative members of new DID
   - Update any external databases or directories
   - Test signing and encryption with new keys

6. **Post-rotation monitoring** (24-48 hours):
   - Watch for any messages still signed with old key
   - Monitor gossip for identity updates
   - Verify all devices received the rotation event

### Prevention

- **Multi-device setup**: Never rely on single device
- **Secure passphrase storage**: Password manager, not written down
- **Device encryption**: Full-disk encryption on all devices
- **Physical security**: Lock devices when unattended
- **Social recovery setup**: Configure guardians (when available)
- **Regular rotation**: Annual planned key rotations
- **Backup verification**: Test restore monthly

---

## Incident: Network Partition

**Severity**: P1 - High

### Symptoms

- Peer count drops to zero
- Gossip sync stalls
- Monitoring shows no network activity
- Can't reach other nodes

### Diagnosis

1. **Check network connectivity**:
   ```bash
   # Test internet connection
   ping 8.8.8.8

   # Test DNS
   nslookup google.com

   # Check if ICNd is running
   systemctl status icnd
   ```

2. **Check ICN peer status**:
   ```bash
   # View peer count from dashboard
   curl http://localhost:8080/health | jq '.active_connections'

   # Check network metrics
   curl http://localhost:9100/metrics | grep icn_network_connections_active
   ```

3. **Check mDNS discovery**:
   ```bash
   # Verify mDNS is working
   avahi-browse -a
   ```

### Recovery

1. **Restart ICNd**:
   ```bash
   systemctl restart icnd
   ```

2. **Check firewall rules**:
   ```bash
   # Verify QUIC port is open (default: 5600)
   sudo iptables -L -n | grep 5600

   # Verify mDNS port is open (5353)
   sudo iptables -L -n | grep 5353
   ```

3. **Manual peer dial** (future feature):
   ```bash
   # If mDNS fails, manually dial known peers
   icnctl network dial <peer-multiaddr> <peer-did>
   ```

4. **Check for split-brain**:
   - If network partitions, different nodes may have divergent state
   - When partition heals, gossip anti-entropy will sync
   - Monitor quarantine for conflicts

### Prevention

- **Multiple network paths**: Don't rely on single network link
- **Monitoring**: Alert on peer count < 2
- **Fallback discovery**: Manual peer list in config
- **Regular testing**: Chaos engineering - test partition recovery

---

## Incident: Pod Failure (K3s)

**Severity**: P1 - High

### Symptoms

- Pod not in `Running` state
- Health endpoint returns 503 or times out
- Grafana shows gaps in metrics
- Alertmanager fires `ICNPodNotReady` alert

### Diagnosis

1. **Check pod status**:
   ```bash
   ssh ubuntu@10.8.10.40
   sudo kubectl -n icn get pods -o wide
   ```

2. **Check pod events**:
   ```bash
   sudo kubectl -n icn describe pod -l app=icn-daemon
   ```

   Common issues in events:
   - `ImagePullBackOff` - Image not available on node
   - `CrashLoopBackOff` - Application crashing repeatedly
   - `OOMKilled` - Out of memory
   - `Pending` - No resources or node selector mismatch

3. **Check logs**:
   ```bash
   # Current pod logs
   sudo kubectl -n icn logs deployment/icn-daemon --tail=200

   # Previous container (if crashed)
   sudo kubectl -n icn logs deployment/icn-daemon --previous
   ```

4. **Check node resources**:
   ```bash
   sudo kubectl top nodes
   sudo kubectl describe nodes | grep -A 5 "Allocated resources"
   ```

### Recovery

#### Scenario 1: CrashLoopBackOff

1. **Check logs for crash reason**:
   ```bash
   sudo kubectl -n icn logs deployment/icn-daemon --previous
   ```

2. **Common fixes**:
   - **Config error**: Check ConfigMap for typos
   - **Permission error**: Verify PVC is mounted correctly
   - **Port conflict**: Check if ports are available

3. **Restart after fix**:
   ```bash
   sudo kubectl -n icn rollout restart deployment/icn-daemon
   ```

#### Scenario 2: OOMKilled

1. **Check memory usage before OOM**:
   ```bash
   sudo kubectl -n icn describe pod -l app=icn-daemon | grep -A 3 "Last State"
   ```

2. **Increase memory limit** (edit deployment):
   ```bash
   sudo kubectl -n icn edit deployment icn-daemon
   # Change: resources.limits.memory: 2Gi -> 4Gi
   ```

   Or redeploy with updated manifests:
   ```bash
   cd /home/matt/projects/icn/deploy/k8s && make full-deploy-dev
   ```

#### Scenario 3: ImagePullBackOff

1. **Check image availability**:
   ```bash
   sudo crictl images | grep icn
   ```

2. **Sync image to nodes**:
   ```bash
   cd /home/matt/projects/icn/deploy/k8s
   make sync-images
   ```

#### Scenario 4: Stuck Pending

1. **Check for resource constraints**:
   ```bash
   sudo kubectl -n icn describe pod -l app=icn-daemon
   ```

2. **Check PVC binding**:
   ```bash
   sudo kubectl -n icn get pvc
   sudo kubectl -n icn describe pvc icn-data
   ```

3. **Check NFS server** (Atlas):
   ```bash
   ssh atlas "systemctl status nfs-kernel-server"
   showmount -e 10.8.10.25
   ```

### Prevention

- **Resource limits**: Set appropriate CPU/memory limits
- **Health probes**: Liveness and readiness probes configured
- **PodDisruptionBudget**: Prevent accidental disruption
- **Monitoring**: Alert on pod restarts > 3 in 10 minutes

---

## Incident: Gossip Storm

**Severity**: P2 - Medium

### Symptoms

- Extremely high network bandwidth usage
- CPU pegged at 100%
- Gossip metrics showing thousands of messages/sec
- Dashboard shows message count exploding

### Diagnosis

1. **Check gossip metrics**:
   ```bash
   curl http://localhost:9100/metrics | grep icn_gossip
   ```

2. **Identify problematic topic**:
   - Look for topic with disproportionate activity
   - Check for single peer sending excessive messages

### Mitigation

1. **Rate limiting is automatic**:
   - ICN has trust-based rate limiting built in
   - Untrusted peers limited to 10 msg/sec
   - Trusted peers limited to 200 msg/sec

2. **If rate limiting insufficient**:
   ```bash
   # Restart daemon (clears in-memory state)
   systemctl restart icnd
   ```

3. **Block malicious peer** (future feature):
   ```bash
   # Remove trust edge to spammer
   icnctl trust remove did:icn:spammer

   # Block peer entirely
   icnctl network block did:icn:spammer
   ```

### Prevention

- **Trust gating**: Only subscribe trusted peers to sensitive topics
- **Entry limits**: Configure max entries per topic
- **Monitoring**: Alert on unusual message rates

---

## Incident: Quarantine Growth

**Severity**: P2 - Medium (can escalate)

### Symptoms

- `icn_ledger_quarantine_size` metric growing
- Dashboard shows degraded health
- Merge conflicts incrementing

### Investigation

1. **List quarantined entries**:
   ```bash
   icnctl ledger quarantine list
   ```

2. **Identify patterns**:
   - Same account appearing repeatedly?
   - Specific time period?
   - Common error type?

### Resolution

1. **Manual review** (if < 50 entries):
   ```bash
   # Inspect each entry
   icnctl ledger quarantine get <hash>

   # Release or drop based on validity
   icnctl ledger quarantine release <hash>
   # OR
   icnctl ledger quarantine drop <hash>
   ```

2. **Automated cleanup** (if > 50 entries):
   ```bash
   # Purge expired entries (older than 7 days)
   icnctl ledger quarantine purge
   ```

3. **Root cause fix**:
   - If clock skew: Sync NTP
   - If malicious: Remove trust edge
   - If bug: Report to ICN developers

---

## Incident: Storage Issues (K3s)

**Severity**: P1 - High

### Symptoms

- Write failures in logs (`sled error`, `I/O error`)
- PVC shows as `Pending` or `Lost`
- NFS mount errors
- Disk space alerts
- Data not persisting across pod restarts

### Diagnosis

1. **Check PVC status**:
   ```bash
   ssh ubuntu@10.8.10.40
   sudo kubectl -n icn get pvc
   sudo kubectl -n icn describe pvc icn-data
   ```

2. **Check disk space on NFS server**:
   ```bash
   ssh atlas "df -h /mnt/storage"
   ```

3. **Check NFS service**:
   ```bash
   ssh atlas "systemctl status nfs-kernel-server"
   ssh atlas "exportfs -v"
   ```

4. **Check mount from pod**:
   ```bash
   sudo kubectl -n icn exec deploy/icn-daemon -- df -h /data
   sudo kubectl -n icn exec deploy/icn-daemon -- ls -la /data
   ```

5. **Check for Sled database issues**:
   ```bash
   sudo kubectl -n icn logs deployment/icn-daemon | grep -i "sled\|error\|corrupt"
   ```

### Recovery

#### Scenario 1: NFS Server Unreachable

1. **Check network connectivity**:
   ```bash
   ping 10.8.10.25
   ```

2. **Restart NFS service**:
   ```bash
   ssh atlas "sudo systemctl restart nfs-kernel-server"
   ```

3. **Verify exports**:
   ```bash
   showmount -e 10.8.10.25
   ```

4. **Restart ICN pod** (to remount):
   ```bash
   sudo kubectl -n icn rollout restart deployment/icn-daemon
   ```

#### Scenario 2: Disk Full

1. **Check space usage**:
   ```bash
   ssh atlas "du -sh /mnt/storage/k8s/icn-data/*"
   ```

2. **Clean old backups**:
   ```bash
   ssh atlas "find /mnt/storage/k8s/icn-data/backups -mtime +30 -delete"
   ```

3. **Compact Sled database** (if supported):
   ```bash
   sudo kubectl -n icn exec deploy/icn-daemon -- icnctl db compact
   ```

#### Scenario 3: Sled Corruption

1. **Stop the daemon**:
   ```bash
   sudo kubectl -n icn scale deployment icn-daemon --replicas=0
   ```

2. **Backup current state**:
   ```bash
   ssh atlas "cp -r /mnt/storage/k8s/icn-data /mnt/storage/k8s/icn-data-corrupted-$(date +%Y%m%d)"
   ```

3. **Restore from backup**:
   ```bash
   ssh atlas "ls -la /mnt/storage/k8s/icn-data/backups/"
   # Copy latest good backup to data directory
   ```

4. **Restart daemon**:
   ```bash
   sudo kubectl -n icn scale deployment icn-daemon --replicas=1
   ```

### Prevention

- **Monitoring**: Alert on disk usage > 80%
- **Automated backups**: Daily snapshots with rotation
- **NFS redundancy**: Consider replicated storage
- **Health checks**: Include storage health in liveness probe

---

## Monitoring and Detection

### Key Metrics to Monitor

**Critical Alerts** (page on-call):
- `icn_ledger_quarantine_size > 100` - Ledger issues
- `icn_network_connections_active == 0` - Network partition
- Health endpoint returns 503 - Node unhealthy

**Warning Alerts** (notify in Slack):
- `icn_gossip_subscriptions_rejected_total` incrementing - Trust issues
- `icn_network_messages_rate_limited_total` spiking - Possible attack
- `icn_ledger_merge_conflicts_total` incrementing - Sync problems

**Info Alerts** (log for trends):
- Peer count fluctuations
- Gossip topic growth
- Transaction volume changes

### Dashboard Checks

Visit `http://localhost:8080/` daily and verify:
- ✅ Status: Healthy (green banner)
- ✅ Active connections > 0
- ✅ Quarantine size < 10
- ✅ No unusual spikes in metrics

### Health Check Integration

Configure external monitoring:
```bash
# Kubernetes liveness probe
http://icn-node:8080/health

# Systemd watchdog
WatchdogSec=60s

# Nagios/Zabbix
curl -f http://localhost:8080/health || exit 1
```

---

## Communication Templates

### Status Update Template

Use this template for ongoing incident updates:

```
ICN Incident Update - [INCIDENT_ID]
Status: [Investigating | Identified | Monitoring | Resolved]
Severity: [P0 Critical | P1 High | P2 Medium | P3 Low]
Time: [YYYY-MM-DD HH:MM UTC]

Summary:
[Brief description of current status]

Impact:
- Affected services: [List affected components]
- User impact: [Description of user-facing effects]

Current Actions:
- [What is being done right now]

Next Update:
Expected in [X] minutes/hours

---
ICN Operations Team
```

### Initial Incident Notification

Use when first declaring an incident:

```
Subject: [P0/P1/P2/P3] ICN Incident: [Brief Description]

Team,

We are investigating an incident affecting [component/service].

Detected: [Time UTC]
Severity: [P0-P3]
Initial Symptoms: [What was observed]

Current Status:
- [What we know so far]

Immediate Actions:
- [Responder name] is investigating
- [Any containment steps taken]

Communication Channel:
[Slack channel / Video call link]

Next Update: [Time]

---
[Responder Name]
```

### Resolution Notification

Use when incident is resolved:

```
Subject: [RESOLVED] ICN Incident: [Brief Description]

Team,

The incident affecting [component/service] has been resolved.

Timeline:
- Detected: [Time UTC]
- Identified: [Time UTC]
- Resolved: [Time UTC]
- Total Duration: [X hours/minutes]

Root Cause:
[Brief explanation of what caused the incident]

Resolution:
[What was done to fix it]

User Impact:
[Summary of impact during incident]

Follow-up Actions:
- [ ] Post-mortem scheduled for [Date]
- [ ] [Any immediate improvements planned]

---
[Responder Name]
```

### Stakeholder Briefing (Non-Technical)

Use for executive or external stakeholder updates:

```
Subject: ICN Service Update - [Date]

Summary:
On [Date], the ICN network experienced [brief non-technical description].
The issue was resolved at [Time] after [Duration].

Impact:
- [What users/cooperatives experienced]
- [Any data or transaction concerns]

Resolution:
Our team [brief explanation of fix without technical jargon].

Prevention:
We are implementing [improvements] to prevent recurrence.

Questions:
Please contact [contact person] for additional information.

---
ICN Operations
```

---

## Emergency Contacts

**ICN Development Team**:
- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Email: [TBD]

**Cooperative Contacts**:
- Primary: [Your cooperative's emergency contact]
- Secondary: [Backup contact]
- Out-of-band: [Signal group, phone tree]

---

## Post-Incident Review Template

After resolving an incident, document:

1. **Incident Summary**:
   - Date/time of detection
   - Severity level
   - Duration of incident

2. **Timeline**:
   - When was it first detected?
   - What actions were taken and when?
   - When was it resolved?

3. **Root Cause**:
   - What caused the incident?
   - Why wasn't it prevented?
   - Why wasn't it detected sooner?

4. **Impact**:
   - How many nodes affected?
   - Data loss or corruption?
   - Economic impact?

5. **Action Items**:
   - What monitoring should be added?
   - What procedures should be updated?
   - What code changes are needed?

6. **Lessons Learned**:
   - What went well?
   - What could be improved?
   - How can we prevent this in the future?

---

## Version History

- **2026-01-04**: Added K3s-specific procedures, communication templates (#324)
- **2025-01-14**: Initial version (Track B1)
