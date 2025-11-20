# Deployment Overview - ICN Pilot UI

Visual guide to deploying the ICN Pilot UI for different use cases.

---

## Deployment Paths

```
┌─────────────────────────────────────────────────────────────────┐
│                     CHOOSE YOUR DEPLOYMENT                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                ┌─────────────┼─────────────┐
                │             │             │
                ▼             ▼             ▼
         ┌─────────┐    ┌─────────┐    ┌──────────┐
         │ Testing │    │  Pilot  │    │Production│
         │  Local  │    │Community│    │Deployment│
         └─────────┘    └─────────┘    └──────────┘
              │              │               │
              │              │               │
         5 minutes      30 minutes      2-4 hours
```

---

## Path 1: Testing Locally (5 minutes)

**Use Case**: "I want to see what this looks like"

**Tools**: Simple scripts, no production infrastructure needed

```bash
# Method A: Absolute quickest (Docker)
cd deploy
./quickstart.sh "Demo Timebank"
# Visit http://localhost:3000

# Method B: Manual control
cd web/pilot-ui
./deploy-ui.sh 3000
# Follow prompts to choose Python/Node/Docker
```

**What You Get**:
- ✅ Working UI on localhost
- ✅ Sample data to explore features
- ✅ Authentication flow
- ✅ All features functional
- ⚠️ HTTP only (no TLS)
- ⚠️ Single node (no P2P)
- ⚠️ Not suitable for real members

**Next Steps**:
- [Getting Started Guide](GETTING-STARTED.md)
- Use `seed-demo-data.sh` to add sample transactions
- Explore features with keyboard shortcuts (Ctrl+1-5)

---

## Path 2: Pilot Community (30 minutes)

**Use Case**: "We have 10-50 members ready to test this"

**Tools**: Docker Compose, basic VPS, simple domain

### Requirements

- VPS with 1 CPU, 2GB RAM (DigitalOcean, Linode, etc.)
- Domain name pointing to server
- SSH access

### Setup Steps

**1. Provision Server** (5 minutes)

```bash
# Create Droplet/VPS (Ubuntu 22.04)
# Point domain: timebank.yourcoop.org → server IP
# SSH into server
ssh root@timebank.yourcoop.org
```

**2. Install Docker** (5 minutes)

```bash
# Install Docker and Docker Compose
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
newgrp docker
```

**3. Deploy ICN** (10 minutes)

```bash
# Clone repository
cd /opt
git clone https://github.com/anthropics/icn.git
cd icn/deploy

# Configure environment
cp .env.example .env
nano .env  # Set JWT_SECRET, GRAFANA_PASSWORD

# Start services
docker compose up -d

# Initialize identity
docker compose exec icnd icnctl id init
```

**4. Setup TLS** (5 minutes)

```bash
# Install certbot
apt install certbot python3-certbot-nginx

# Get certificate
certbot --nginx -d timebank.yourcoop.org
```

**5. Create Cooperative** (5 minutes)

```bash
# Get your DID
DID=$(docker compose exec icnd icnctl id show | grep did:icn)

# Create cooperative
docker compose exec icnd icnctl coops create \
    --id "your-coop" \
    --name "Your Cooperative Name"

# Get token for admin
TOKEN=$(docker compose exec icnd icnctl auth login \
    --gateway http://localhost:8080 \
    --coop "your-coop")

echo "Admin token: $TOKEN"
```

**What You Get**:
- ✅ HTTPS with Let's Encrypt
- ✅ Professional domain
- ✅ Multiple members supported
- ✅ Real-time features working
- ✅ Basic monitoring (Grafana)
- ⚠️ Single server (no redundancy)
- ⚠️ Manual backups

**Member Onboarding**:
1. Share [Quick Start Guide](QUICK-START.md) with members
2. Each member creates identity: `icnctl id init`
3. Admin adds members: `icnctl coops member add`
4. Members get tokens and connect to UI

**Ongoing Maintenance**:
- Weekly: Review logs, check backups
- Monthly: Review metrics, update security patches
- See [Admin Guide](ADMIN-GUIDE.md) for details

---

## Path 3: Production Deployment (2-4 hours)

**Use Case**: "We have 200+ members, need high availability"

**Tools**: Load balancer, monitoring, automated backups, redundancy

### Architecture

```
                    ┌─────────────┐
                    │ Load Balancer│
                    │   (nginx)   │
                    └──────┬──────┘
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
        ▼                  ▼                  ▼
  ┌──────────┐       ┌──────────┐       ┌──────────┐
  │  ICN     │       │  ICN     │       │  ICN     │
  │ Node 1   │◄─────►│ Node 2   │◄─────►│ Node 3   │
  └──────────┘       └──────────┘       └──────────┘
        │                  │                  │
        └──────────────────┼──────────────────┘
                           │
                    ┌──────┴──────┐
                    │ Prometheus  │
                    │  + Grafana  │
                    └─────────────┘
```

### Production Checklist

See [DEPLOYMENT-CHECKLIST.md](DEPLOYMENT-CHECKLIST.md) for complete step-by-step.

**Security Hardening**:
- [ ] TLS 1.3 only
- [ ] Strong JWT secrets (32+ chars)
- [ ] Firewall configured (UFW/iptables)
- [ ] Fail2ban for brute-force protection
- [ ] Rate limiting (nginx)
- [ ] Regular security audits

**Monitoring**:
- [ ] Prometheus metrics collection
- [ ] Grafana dashboards
- [ ] Health check endpoints
- [ ] Log aggregation
- [ ] Alert rules configured
- [ ] Uptime monitoring (external)

**Backups**:
- [ ] Automated daily backups
- [ ] Offsite backup storage
- [ ] Backup restoration tested
- [ ] 30-day retention policy

**Performance**:
- [ ] CDN for static assets
- [ ] nginx caching configured
- [ ] Database optimization
- [ ] Load testing completed
- [ ] Capacity planning done

**Documentation**:
- [ ] Runbooks for common incidents
- [ ] Escalation procedures
- [ ] Contact information
- [ ] Recovery procedures

**Resources**:
- [Production Deployment Guide](PRODUCTION-DEPLOY.md)
- [Deployment Checklist](DEPLOYMENT-CHECKLIST.md)
- [Admin Guide](ADMIN-GUIDE.md)
- [Operations Guide](../../docs/operations-guide.md)

---

## Comparison Matrix

| Feature | Testing Local | Pilot Community | Production |
|---------|--------------|-----------------|------------|
| **Setup Time** | 5 minutes | 30 minutes | 2-4 hours |
| **Members** | 1 (you) | 10-50 | 200+ |
| **HTTPS/TLS** | ❌ | ✅ | ✅ |
| **Domain** | localhost | Single domain | Custom domain |
| **High Availability** | ❌ | ❌ | ✅ |
| **Automated Backups** | ❌ | Manual | ✅ Automated |
| **Monitoring** | Basic | Grafana | Full stack |
| **Cost/Month** | $0 | $5-10 (VPS) | $50-200 (HA) |
| **Maintenance** | None | Weekly | Daily |
| **Best For** | Demos, testing | Small coops | Large coops |

---

## Migration Paths

### From Testing → Pilot

1. Export identity: `icnctl id export backup.age`
2. Set up VPS following "Pilot Community" path
3. Import identity: `icnctl id import backup.age`
4. Recreate cooperative with same ID
5. Point members to new domain

**Data Migration**: Transactions can be exported to CSV and re-imported if needed.

### From Pilot → Production

1. Plan downtime window (announce to members)
2. Backup all data: `icnctl backup create`
3. Set up production infrastructure
4. Restore backup: `icnctl backup restore`
5. Update DNS to new servers
6. Test thoroughly before announcing

**Zero-Downtime Migration**: Possible with multi-node setup (contact support)

---

## Deployment Decision Tree

```
Do you have real cooperative members ready to use this?
│
├─ No → Use "Testing Local" path
│       Try features, show to stakeholders
│       Resources: GETTING-STARTED.md
│
└─ Yes → How many members?
         │
         ├─ 1-50 members → Use "Pilot Community" path
         │                 Single VPS, basic monitoring
         │                 Resources: deploy/quickstart.sh
         │
         └─ 50+ members → Use "Production" path
                          High availability, full monitoring
                          Resources: PRODUCTION-DEPLOY.md
```

---

## Quick Reference

### Testing Local

```bash
cd deploy && ./quickstart.sh
```
**Docs**: [GETTING-STARTED.md](GETTING-STARTED.md)

### Pilot Community

```bash
# On VPS
cd /opt && git clone https://github.com/anthropics/icn.git
cd icn/deploy && docker compose up -d
```
**Docs**: [deploy/README.md](../../deploy/README.md)

### Production

```bash
# Follow complete checklist
cat web/pilot-ui/DEPLOYMENT-CHECKLIST.md
```
**Docs**: [PRODUCTION-DEPLOY.md](PRODUCTION-DEPLOY.md)

---

## Getting Help

**For setup questions**:
1. Check [FAQ](FAQ.md)
2. Read [Getting Started](GETTING-STARTED.md)
3. Review [Admin Guide](ADMIN-GUIDE.md)

**For deployment issues**:
1. Check [Troubleshooting](PRODUCTION-DEPLOY.md#troubleshooting)
2. Review [Deployment Checklist](DEPLOYMENT-CHECKLIST.md)
3. Open issue: https://github.com/anthropics/icn/issues

**For community support**:
- Discussions: https://github.com/anthropics/icn/discussions
- IRC: #icn on libera.chat

---

## Next Steps

Based on your deployment path:

**If Testing Locally**:
→ [Getting Started Guide](GETTING-STARTED.md)

**If Deploying Pilot**:
→ [Deployment Checklist](DEPLOYMENT-CHECKLIST.md)

**If Going Production**:
→ [Production Deployment](PRODUCTION-DEPLOY.md)

**If Onboarding Members**:
→ Share [Quick Start Guide](QUICK-START.md) with members
→ Share [Treasurer's Guide](TREASURER-GUIDE.md) with financial managers
→ Share [Admin Guide](ADMIN-GUIDE.md) with administrators

---

**Good luck with your deployment!** 🚀🌱
