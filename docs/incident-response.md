# ICN Incident Response Playbook

This document provides operational procedures for responding to common incidents in ICN deployments. While some responses are currently manual or crude in v0.1, having documented procedures is critical for operational readiness.

**Audience**: ICN node operators, cooperative system administrators, incident responders

**Status**: Living document - procedures will evolve as ICN matures

---

## Table of Contents

1. [General Incident Response Framework](#general-incident-response-framework)
2. [Incident: Node Compromise](#incident-node-compromise)
3. [Incident: Ledger Corruption Detected](#incident-ledger-corruption-detected)
4. [Incident: Key Suspected Stolen](#incident-key-suspected-stolen)
5. [Incident: Network Partition](#incident-network-partition)
6. [Incident: Gossip Storm](#incident-gossip-storm)
7. [Incident: Quarantine Growth](#incident-quarantine-growth)
8. [Monitoring and Detection](#monitoring-and-detection)

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
   curl http://localhost:9090/metrics | grep icn_ledger_quarantine_size
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
   curl http://localhost:9090/metrics | grep icn_network_connections_active
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
   curl http://localhost:9090/metrics | grep icn_gossip
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

- **2025-01-14**: Initial version (Track B1)
- Future: Will be updated as ICN evolves and real incidents occur
