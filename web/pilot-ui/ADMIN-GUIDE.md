# Administrator Guide

Complete guide for ICN timebank administrators managing the system, members, and cooperative operations.

## Your Role as Administrator

As admin, you're responsible for:

- **System Operations**: Keeping the ICN gateway and daemon running
- **Member Management**: Adding/removing members, managing roles
- **Security**: Protecting the system and member data
- **Governance Setup**: Creating domains and facilitating proposals
- **Technical Support**: Helping members with login and access issues
- **Configuration**: Managing cooperative settings and policies

## Quick Reference Commands

### Identity Management
```bash
# Show your DID
icnctl id show

# Create new identity
icnctl id init

# Rotate keypair
icnctl id rotate

# Export identity backup
icnctl id export backup.age

# Import identity
icnctl id import backup.age
```

### Cooperative Management
```bash
# Create cooperative
icnctl coops create --id "my-coop" --name "My Cooperative"

# Add member
icnctl coops member add --coop "my-coop" --did "did:icn:abc..." --role member

# Remove member
icnctl coops member remove --coop "my-coop" --did "did:icn:abc..."

# List cooperatives
icnctl coops list

# Show cooperative details
icnctl coops show --id "my-coop"
```

### Governance Management
```bash
# Create governance domain
icnctl gov domain create --domain-id "coop:my-coop" --name "My Coop" \
  --members "did:icn:alice,did:icn:bob"

# Create proposal
icnctl gov proposal create --domain-id "coop:my-coop" \
  --title "Approve new policy" --kind text

# Open proposal for voting
icnctl gov proposal open --proposal-id <id>

# Close proposal and calculate outcome
icnctl gov proposal close --proposal-id <id>

# List proposals
icnctl gov proposal list --domain-id "coop:my-coop"
```

### System Operations
```bash
# Start ICN daemon with gateway
icnd --gateway-enable --gateway-bind 127.0.0.1:8080 \
  --gateway-jwt-secret "your-strong-secret"

# Check system status
icnctl status

# Create backup
icnctl backup create --output backup-2024-01-15.tar.gz

# Restore from backup
icnctl backup restore --input backup-2024-01-15.tar.gz

# View metrics
curl http://localhost:9090/metrics
```

## Initial Setup

### 1. Install ICN

```bash
# Build from source (development)
cd icn
cargo build --release

# Binaries will be in icn/target/release/
./target/release/icnd --version
./target/release/icnctl --version
```

### 2. Create Your Administrator Identity

```bash
# Initialize new identity
icnctl id init

# Enter a strong passphrase when prompted
# IMPORTANT: Save this passphrase securely!

# Your identity is stored in ~/.icn/keystore.age

# Show your DID
icnctl id show
# Output: did:icn:abc123...

# Back up your identity immediately
icnctl id export backup-admin.age
# Store backup-admin.age in secure, encrypted location
```

### 3. Start the ICN Daemon

```bash
# Set JWT secret (NEVER share this!)
export ICN_GATEWAY_JWT_SECRET="$(openssl rand -base64 32)"

# Start daemon with gateway enabled
icnd --gateway-enable --gateway-bind 127.0.0.1:8080

# Daemon will prompt for your keystore passphrase
# Enter the passphrase you set during 'icnctl id init'

# Daemon is now running on:
# - Gateway API: http://localhost:8080
# - Metrics: http://localhost:9090/metrics
```

**Production Deployment**: See [deployment-guide.md](../../docs/deployment-guide.md) for systemd setup, reverse proxy configuration, and TLS.

### 4. Create Your Cooperative

```bash
# In another terminal:
icnctl coops create --id "garden-coop" --name "Community Garden Cooperative"

# Add yourself as owner
YOUR_DID=$(icnctl id show)
icnctl coops member add --coop "garden-coop" --did "$YOUR_DID" --role owner
```

### 5. Create Governance Domain

```bash
# Create domain for cooperative governance
icnctl gov domain create \
  --domain-id "coop:garden-coop" \
  --name "Garden Coop Governance" \
  --members "$YOUR_DID"

# Verify creation
icnctl gov domain show --domain-id "coop:garden-coop"
```

### 6. Serve the Pilot UI

```bash
# Navigate to pilot UI directory
cd web/pilot-ui

# Serve with Python
python -m http.server 3000

# OR with Node.js
npx serve -s . -l 3000

# UI accessible at: http://localhost:3000
```

### 7. Get Authentication Token

```bash
# Generate token for yourself
icnctl auth login --gateway http://localhost:8080 --coop garden-coop

# Copy the token output and use it to sign in to the web UI
```

🎉 **Setup Complete!** You can now access the web UI and start adding members.

## Member Management

### Adding New Members

**Prerequisites**:
- Member must have created their identity (`icnctl id init`)
- You need their DID

**Steps**:

1. **Get member's DID**:
   ```bash
   # Member runs this on their machine:
   icnctl id show
   # They share the output with you (e.g., did:icn:xyz789...)
   ```

2. **Add member to cooperative**:
   ```bash
   icnctl coops member add \
     --coop "garden-coop" \
     --did "did:icn:xyz789..." \
     --role member
   ```

3. **Add member to governance domain**:
   ```bash
   # Get existing members
   EXISTING=$(icnctl gov domain show --domain-id "coop:garden-coop" | grep members)

   # Add new member (append to list)
   icnctl gov domain update \
     --domain-id "coop:garden-coop" \
     --members "$EXISTING,did:icn:xyz789..."
   ```

4. **Provide member with credentials**:
   - Gateway URL: `http://localhost:8080` (or your domain)
   - Cooperative ID: `garden-coop`
   - Their DID: `did:icn:xyz789...`
   - Instructions to get token: `icnctl auth login --gateway <url> --coop <id>`

5. **Send welcome email** (template below)

### Member Roles

**Three roles available**:

1. **owner**
   - Full administrative access
   - Can add/remove members
   - Can change cooperative settings
   - Can delete cooperative

2. **admin**
   - Can add/remove regular members
   - Can update settings
   - Cannot remove owners or other admins

3. **member**
   - Can log transactions
   - Can view history and members
   - Can vote on proposals
   - Cannot manage other members

**Change member role**:
```bash
# Promote member to admin
icnctl coops member update \
  --coop "garden-coop" \
  --did "did:icn:xyz789..." \
  --role admin
```

### Removing Members

**When to remove**:
- Member requests removal
- Member inactive for extended period (per policy)
- Member violates community guidelines (governance decision)

**Steps**:

1. **Check member's balance**:
   ```bash
   # Access web UI, go to Members tab
   # Note the member's balance
   ```

2. **Resolve outstanding balance** (if needed):
   - **Positive balance**: Member has credit owed by community
     - Option A: Transfer to community fund
     - Option B: Pay out (if policy allows)
     - Option C: Write off (governance decision)
   - **Negative balance**: Member owes community
     - Option A: Collect (if policy allows)
     - Option B: Write off (most common for departures)

3. **Remove from governance domain**:
   ```bash
   icnctl gov domain update \
     --domain-id "coop:garden-coop" \
     --members "[list without removed member]"
   ```

4. **Remove from cooperative**:
   ```bash
   icnctl coops member remove \
     --coop "garden-coop" \
     --did "did:icn:xyz789..."
   ```

5. **Document removal**:
   - Log the date and reason
   - Keep records per data retention policy

### Member Onboarding Checklist

Use this checklist for each new member:

```
□ Member has ICN software installed
□ Member has created identity (icnctl id init)
□ Member has backed up keystore
□ Member has shared DID with you
□ Added to cooperative (icnctl coops member add)
□ Added to governance domain
□ Sent welcome email with:
  □ Gateway URL
  □ Cooperative ID
  □ Login instructions
  □ Link to Quick Start guide
  □ Treasurer's contact info
□ Member successfully logged in to web UI
□ Member completed skills inventory (if applicable)
□ Member introduced to community
```

### Welcome Email Template

```
Subject: Welcome to [Cooperative Name] Timebank!

Hi [Member Name],

Welcome to our timebank community! You've been added as a member of [Cooperative Name].

Here's how to get started:

1. Access the Timebank
   URL: [Your Gateway URL, e.g., https://timebank.example.com]

2. Sign In
   - Gateway URL: [URL]
   - Cooperative ID: [coop-id]
   - Your DID: [their-did]
   - Get Token: Run this command in your terminal:
     icnctl auth login --gateway [URL] --coop [coop-id]

3. Learn the Basics
   Read the Quick Start Guide: [link to QUICK-START.md]

4. Questions?
   - Technical: Contact [Admin Name] at [admin@example.com]
   - Financial: Contact [Treasurer Name] at [treasurer@example.com]
   - General: Reply to this email

Looking forward to exchanging with you!

[Your Name]
[Cooperative Name] Administrator
```

## Governance Administration

### Creating Proposals

**Proposal Types**:
- **Text**: General decisions, policy changes
- **Budget**: Financial allocations
- **Membership**: Adding/removing members (formal vote)
- **ConfigChange**: System configuration updates

**Steps**:

1. **Create proposal**:
   ```bash
   icnctl gov proposal create \
     --domain-id "coop:garden-coop" \
     --title "Implement weekend work exchanges" \
     --description "Allow weekend time exchanges to encourage participation" \
     --kind text
   ```

2. **Note the proposal ID** from output

3. **Open proposal for voting**:
   ```bash
   icnctl gov proposal open --proposal-id [id]
   ```

4. **Announce to members**:
   - Send email notification
   - Post in community channels
   - Mention in next meeting

5. **Monitor voting**:
   ```bash
   # Check vote status
   icnctl gov vote show --proposal-id [id]
   ```

6. **Close proposal** (after voting period):
   ```bash
   icnctl gov proposal close --proposal-id [id]
   ```

7. **Implement decision** (if accepted)

### Managing Voting Periods

**Recommended Timeline**:
- **Discussion Period**: 3-7 days (before opening vote)
- **Voting Period**: 7-14 days
- **Implementation**: 7-30 days after approval

**Best Practices**:
- Announce proposals in advance
- Provide clear descriptions and rationale
- Allow time for questions and debate
- Set consistent voting periods (e.g., always 2 weeks)
- Close proposals promptly after period ends

### Handling Governance Conflicts

**Disputed Outcomes**:
1. Verify vote counts
2. Check member eligibility
3. Review voting rules
4. Consider re-vote if irregularities found

**Low Participation**:
- Require quorum (50%+ members voting)
- Extend voting period if needed
- Increase communication efforts
- Consider proposal fatigue (too many votes)

**Controversial Decisions**:
- Allow longer discussion period
- Host community meeting
- Consider compromise amendments
- Document minority opinions

## System Administration

### Daily Operations

**Monitor These**:

1. **Gateway Health**:
   ```bash
   curl http://localhost:8080/v1/health
   # Should return: {"status":"ok"}
   ```

2. **Daemon Status**:
   ```bash
   icnctl status
   # Check that all components are running
   ```

3. **Logs** (if using systemd):
   ```bash
   journalctl -u icnd -f
   # Watch for errors or warnings
   ```

4. **Metrics**:
   ```bash
   curl http://localhost:9090/metrics | grep icn_
   # Check transaction counts, connection stats
   ```

### Backups

**Backup Schedule**:
- **Daily**: Automated backup of ledger and state
- **Weekly**: Full backup including keystore
- **Before upgrades**: Always backup before changes

**Create Backup**:
```bash
# Full backup
icnctl backup create --output backup-$(date +%Y-%m-%d).tar.gz

# Includes:
# - Keystore (identity.age)
# - Store (Sled database)
# - State snapshot (gossip, network)
# - Configuration (icn.toml)

# Verify backup
tar -tzf backup-$(date +%Y-%m-%d).tar.gz

# Store securely (encrypted, off-site)
```

**Restore Backup**:
```bash
# Stop daemon first
pkill icnd

# Restore
icnctl backup restore --input backup-2024-01-15.tar.gz

# Restart daemon
icnd --gateway-enable --gateway-bind 127.0.0.1:8080
```

**Backup Storage**:
- **Location**: Separate drive or server
- **Encryption**: Always encrypt backups
- **Retention**: Keep 30 days daily, 12 weeks weekly, 7 years annual
- **Testing**: Test restore quarterly

### Updates and Upgrades

**Before Updating**:
1. ✅ Create full backup
2. ✅ Test update on staging environment (if available)
3. ✅ Review changelog for breaking changes
4. ✅ Schedule maintenance window (notify members)
5. ✅ Have rollback plan ready

**Update Process**:
```bash
# 1. Create backup
icnctl backup create --output backup-pre-upgrade.tar.gz

# 2. Stop daemon
pkill icnd

# 3. Update software
cd icn
git pull origin main
cargo build --release

# 4. Check for migrations
# Read CHANGELOG.md and ARCHITECTURE.md

# 5. Start daemon
./target/release/icnd --gateway-enable --gateway-bind 127.0.0.1:8080

# 6. Verify health
curl http://localhost:8080/v1/health

# 7. Test core functions (login, transaction, vote)

# 8. Notify members that system is back online
```

**Rollback** (if update fails):
```bash
# Stop new version
pkill icnd

# Restore backup
icnctl backup restore --input backup-pre-upgrade.tar.gz

# Start old version
# (Keep old binaries until new version is proven stable)
```

### Security Hardening

**Essential Security Practices**:

1. **Strong JWT Secret**:
   ```bash
   # Generate strong secret (32+ characters)
   openssl rand -base64 32

   # Set in environment or config
   export ICN_GATEWAY_JWT_SECRET="[generated-secret]"

   # NEVER commit secrets to git
   # NEVER share secrets in plain text
   ```

2. **Firewall Rules**:
   ```bash
   # Allow only necessary ports
   ufw allow 8080/tcp   # Gateway (or use reverse proxy)
   ufw allow 22/tcp     # SSH (admin only)
   ufw deny 9090/tcp    # Metrics (localhost only)
   ufw enable
   ```

3. **Reverse Proxy** (production):
   ```nginx
   # nginx configuration
   server {
       listen 443 ssl;
       server_name timebank.example.com;

       ssl_certificate /path/to/cert.pem;
       ssl_certificate_key /path/to/key.pem;

       location / {
           root /var/www/icn-pilot-ui;
           try_files $uri /index.html;
       }

       location /v1 {
           proxy_pass http://127.0.0.1:8080;
           proxy_set_header Host $host;
           proxy_set_header X-Real-IP $remote_addr;
       }

       location /ws {
           proxy_pass http://127.0.0.1:8080;
           proxy_http_version 1.1;
           proxy_set_header Upgrade $websocket;
           proxy_set_header Connection "upgrade";
       }
   }
   ```

4. **Access Controls**:
   - Use SSH keys (not passwords)
   - Limit sudo access
   - Enable fail2ban
   - Monitor login attempts

5. **Data Protection**:
   - Encrypt backups
   - Secure keystore passphrase
   - Use full-disk encryption
   - Implement data retention policy

6. **Monitoring**:
   - Set up log aggregation
   - Configure alerts (disk space, failed logins, errors)
   - Review logs weekly
   - Monitor unusual transaction patterns

### Troubleshooting

**Gateway Won't Start**:
```bash
# Check if port is in use
lsof -i :8080

# Check JWT secret is set
echo $ICN_GATEWAY_JWT_SECRET

# Check logs
journalctl -u icnd -n 50

# Common fixes:
# - Kill process on port: kill $(lsof -t -i:8080)
# - Set JWT secret: export ICN_GATEWAY_JWT_SECRET="secret"
# - Check file permissions: ls -la ~/.icn/
```

**Members Can't Connect**:
```bash
# Verify gateway is accessible
curl http://localhost:8080/v1/health

# Check firewall
ufw status

# Check member's token
# Token expires after 24 hours - have them get new one

# Verify member exists in cooperative
icnctl coops show --id [coop-id]
```

**Transactions Not Appearing**:
```bash
# Check ledger status
icnctl status

# Verify member DIDs are correct
# Check WebSocket connection (footer in UI)

# Restart daemon if needed
pkill icnd && icnd --gateway-enable --gateway-bind 127.0.0.1:8080
```

**Proposals Not Showing**:
```bash
# List proposals
icnctl gov proposal list --domain-id "coop:garden-coop"

# Check governance domain exists
icnctl gov domain show --domain-id "coop:garden-coop"

# Verify member is in domain member list
```

**High Memory/CPU Usage**:
```bash
# Check metrics
curl http://localhost:9090/metrics | grep process_

# Check disk usage
df -h ~/.icn/

# Check number of connections
netstat -an | grep 8080 | wc -l

# If high:
# - Review transaction volume (may need rate limiting)
# - Check for memory leaks (report bug)
# - Consider vertical scaling (more RAM/CPU)
```

## Monitoring and Observability

### Prometheus Metrics

**Key Metrics to Monitor**:

```bash
# Transaction volume
icn_ledger_entries_total

# Network health
icn_network_connections_active
icn_network_connections_total

# Gossip sync
icn_gossip_announces_sent_total
icn_gossip_requests_sent_total

# Gateway API
icn_gateway_requests_total
icn_gateway_auth_success_total
```

**Set Up Monitoring Dashboard**:

1. **Install Prometheus**:
   ```bash
   # Download and run Prometheus
   wget https://github.com/prometheus/prometheus/releases/download/v2.45.0/prometheus-2.45.0.linux-amd64.tar.gz
   tar xvf prometheus-*.tar.gz
   cd prometheus-*
   ```

2. **Configure scrape**:
   ```yaml
   # prometheus.yml
   scrape_configs:
     - job_name: 'icn'
       static_configs:
         - targets: ['localhost:9090']
   ```

3. **Run Prometheus**:
   ```bash
   ./prometheus --config.file=prometheus.yml
   # Access UI: http://localhost:9091
   ```

4. **Create alerts**:
   ```yaml
   # alerts.yml
   groups:
     - name: icn
       rules:
         - alert: HighTransactionFailureRate
           expr: rate(icn_gateway_requests_total{status="error"}[5m]) > 0.1
           annotations:
             summary: "High transaction failure rate"

         - alert: NoRecentTransactions
           expr: increase(icn_ledger_entries_total[1h]) == 0
           annotations:
             summary: "No transactions in past hour"
   ```

### Log Management

**Log Locations**:
- **Systemd**: `journalctl -u icnd`
- **File**: Check `~/.icn/logs/` if configured
- **Stdout**: If running daemon directly

**Log Levels**:
```bash
# Set log level
RUST_LOG=info icnd ...   # Normal (recommended)
RUST_LOG=debug icnd ...  # Verbose (troubleshooting)
RUST_LOG=warn icnd ...   # Minimal (production)
```

**What to Monitor**:
- Authentication failures (potential attacks)
- Rate limit triggers (abuse detection)
- Ledger errors (data integrity issues)
- Network disconnections (connectivity problems)
- Panics or errors (bugs)

## Data Management

### Data Retention Policy

**Recommended Policy**:

| Data Type | Retention | Reason |
|-----------|-----------|--------|
| Active transactions | Indefinite | Core ledger data |
| Closed proposals | 7 years | Governance history |
| Audit logs | 3 years | Compliance |
| Backups | 30-90 days | Recovery window |
| Removed member data | 90 days | Grace period |
| Error logs | 90 days | Troubleshooting |

**GDPR/Privacy Compliance**:
- Minimal data collection (only DIDs, amounts, memos)
- Member can request data export
- Member can request data deletion (may require balance resolution)
- Document all data processing in privacy policy

### Data Export

**Export All Cooperative Data**:
```bash
# Export transactions
# (Use web UI: History > All Time > Export CSV)

# Export members
icnctl coops show --id [coop-id] --format json > members.json

# Export proposals
icnctl gov proposal list --domain-id [domain] --format json > proposals.json

# Package everything
tar -czf coop-data-export-$(date +%Y-%m-%d).tar.gz *.json *.csv
```

**Member Data Request**:
```bash
# Export all transactions involving specific member
# Use CSV export, then filter in spreadsheet by DID

# Or use API:
curl -H "Authorization: Bearer [token]" \
  "http://localhost:8080/v1/ledger/[coop]/history?did=[member-did]"
```

## Performance Tuning

**For Small Cooperatives** (< 50 members):
- Default settings are fine
- Standard VPS (1 CPU, 1GB RAM) sufficient

**For Medium Cooperatives** (50-500 members):
- 2 CPU, 4GB RAM recommended
- Consider read replicas for high read load
- Monitor disk I/O for Sled database

**For Large Cooperatives** (500+ members):
- Vertical scaling: 4+ CPU, 8+ GB RAM
- SSD storage for database
- Distributed setup (future feature)
- Load balancing for gateway

**Optimization Tips**:
- Regular Sled compaction (automatic)
- Prune old gossip entries (configurable limits)
- Archive old closed proposals (export and remove)
- Monitor connection pool sizes

## Communication with Members

### Regular Communications

**Weekly** (if needed):
- System maintenance windows
- New member welcomes
- Feature updates

**Monthly**:
- Treasurer's report
- Community highlights
- Upcoming proposals

**Quarterly**:
- System health report
- Governance decisions summary
- Strategic updates

**Annual**:
- Year in review
- Member survey
- Planning for next year

### Incident Communication Template

```
Subject: [SYSTEM NOTICE] Timebank Maintenance - [Date]

Hi everyone,

We're performing [routine maintenance / emergency fix / upgrade] on [date] from [time] to [time].

What to expect:
- The timebank will be [offline / read-only / slower than usual]
- Estimated downtime: [X minutes/hours]
- All data is backed up and safe

Why we're doing this:
[Brief explanation]

What you need to do:
[Nothing / Logout before maintenance / Clear cache after / etc.]

Questions? Reply to this email.

Thanks for your patience!

[Your Name]
[Cooperative Name] Admin Team
```

## Emergency Procedures

### System Compromise

**If you suspect unauthorized access**:

1. **Immediately**:
   ```bash
   # Stop the gateway
   pkill icnd

   # Change JWT secret
   export ICN_GATEWAY_JWT_SECRET="$(openssl rand -base64 32)"

   # Rotate admin keystore
   icnctl id rotate
   ```

2. **Investigate**:
   - Review audit logs
   - Check for unauthorized transactions
   - Identify attack vector

3. **Notify**:
   - Members (if their data affected)
   - Authorities (if illegal activity)
   - ICN developers (if software vulnerability)

4. **Remediate**:
   - Patch vulnerability
   - Reverse fraudulent transactions (if possible)
   - Restore from clean backup (if needed)

5. **Document**:
   - Incident report
   - Lessons learned
   - Updated security procedures

### Data Loss

**If backup restore is needed**:

1. Stop daemon
2. Restore from most recent clean backup
3. Document what was lost (transactions between backup and loss)
4. Communicate to members
5. Manually reconcile if needed

**Prevention**:
- Multiple backup copies
- Off-site backup storage
- Regular restore testing

### Key Member Departure

**If admin/treasurer leaves suddenly**:

1. **Admin**: Promote another member to admin role
2. **Keystore**: If admin keystore is lost, transfer ownership via governance
3. **Knowledge**: Maintain documentation for continuity

**Prevention**:
- Multiple admins (shared responsibility)
- Documented procedures
- Succession planning

## Scaling Your Cooperative

**Growth Trajectory**:

| Members | Transactions/Month | Hardware | Features Needed |
|---------|-------------------|----------|-----------------|
| 10-50 | < 100 | 1 CPU, 1GB RAM | Basic |
| 50-200 | 100-1000 | 2 CPU, 4GB RAM | Sub-groups, advanced search |
| 200-1000 | 1000-10000 | 4 CPU, 8GB RAM | Federation, advanced governance |
| 1000+ | 10000+ | Distributed | Multi-node, sharding |

**When to Scale Up**:
- Response times > 2 seconds
- CPU consistently > 80%
- Disk I/O saturated
- Members reporting slowness

**When to Split**:
- Single cooperative > 500 members
- Geographic distribution (latency issues)
- Desire for sub-group autonomy

## Resources and Support

**Documentation**:
- [ARCHITECTURE.md](../../docs/ARCHITECTURE.md) - System design
- [deployment-guide.md](../../docs/deployment-guide.md) - Production setup
- [CHANGELOG.md](../../CHANGELOG.md) - Version history
- [README.md](../../README.md) - Project overview

**Community**:
- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Discussions: https://github.com/InterCooperative-Network/icn/discussions

**Training**:
- [Quick Start Guide](QUICK-START.md) - For new users
- [Treasurer's Guide](TREASURER-GUIDE.md) - For financial management
- [FAQ](FAQ.md) - Common questions

## Appendix: Configuration Reference

### icn.toml Example

```toml
# Data directory (default: ~/.icn)
data_dir = "/var/lib/icn"

# Keystore passphrase (not recommended, use prompt instead)
# passphrase = "your-passphrase"

[network]
# Listen address for P2P
listen_addr = "0.0.0.0:7000"

# mDNS discovery enabled
mdns_enabled = true

[gateway]
# Enable REST API gateway
enabled = true

# Bind address for HTTP server
bind_addr = "127.0.0.1:8080"

# JWT secret (can also use environment variable)
# jwt_secret = "your-secret"

# Token expiry (hours)
token_expiry_hours = 24

# Challenge TTL (minutes)
challenge_ttl_minutes = 5

[metrics]
# Prometheus metrics server
bind_addr = "127.0.0.1:9090"

[logging]
# Log level: error, warn, info, debug, trace
level = "info"

# Log to file
# file = "/var/log/icn/icnd.log"
```

### Environment Variables

```bash
# Data directory
ICN_DATA_DIR="/var/lib/icn"

# Keystore passphrase (use with caution)
ICN_PASSPHRASE="your-passphrase"

# Gateway JWT secret
ICN_GATEWAY_JWT_SECRET="your-jwt-secret"

# Log level
RUST_LOG="info"
```

---

**You're now ready to administer your ICN cooperative!** Keep this guide handy and don't hesitate to reach out for support. 🚀
