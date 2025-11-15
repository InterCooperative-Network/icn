# Hosted Pilot Approach

**Date**: 2025-01-15
**Phase**: 15 (follows Platform Layer implementation)
**Purpose**: Get real signal from actual co-ops without forcing them to self-host

## The Problem

We have a production-ready substrate (Phases 1-12) and will soon have a platform layer (Phase 14: gateway + SDK + reference app).

But we have **zero pilots**. We don't know:
- What co-ops actually need
- Where the UX breaks down
- What governance patterns emerge
- Whether mutual credit is intuitive
- If self-hosting is a blocker or non-issue

**Building more infrastructure without user signal is speculation.**

## The Solution: Host Apps for Early Pilots

### Approach

**Deploy the reference app (Shopper's Club) multi-tenant:**
- One Next.js instance
- One `icn-gateway` instance
- One `icnd` node (with multi-coop namespaces)
- Co-ops access via subdomain or path: `shoppers.icn.coop/food-coop-pdx`

**Onboard 1-2 pilot co-ops:**
- "Here's your app, here's your login"
- No devops required from co-op
- Focus 100% on using ICN, not operating ICN

**Watch what hurts:**
- Weekly feedback sessions (30-60 min)
- Document learnings in `docs/pilots/learnings/`
- Iterate rapidly based on feedback

### What This Is NOT

- ❌ SaaS platform (it's temporary hosting for learning)
- ❌ Production service (it's a pilot, expect downtime)
- ❌ Vendor lock-in (co-ops can export data + self-host anytime)

### What This IS

- ✅ Fastest path to real user feedback
- ✅ Lowest barrier for non-technical co-ops
- ✅ Validates platform before investing in runtime/templates/etc.

## Technical Setup

### Infrastructure

**Hosting** (example - Digital Ocean, Hetzner, or similar):
```
Server 1 (icnd + icn-gateway):
  - icnd with multi-coop namespaces
  - icn-gateway (REST + WebSocket API)
  - Cost: ~$40-60/month

Server 2 (Reference App):
  - Next.js Shopper's Club (multi-tenant)
  - Talks to icn-gateway on Server 1
  - Cost: ~$20-40/month

Total: ~$60-100/month for 1-5 pilot co-ops
```

**Multi-Tenancy via Namespaces:**
```typescript
// Each co-op gets isolated namespace
coop:food-coop-pdx:members
coop:food-coop-pdx:ledger
coop:food-coop-pdx:proposals

coop:timebank-austin:members
coop:timebank-austin:ledger
// etc.
```

**App Routing:**
```
Option A (subdomain):
  food-coop-pdx.shoppers.icn.coop → coopId = "food-coop-pdx"
  timebank-austin.shoppers.icn.coop → coopId = "timebank-austin"

Option B (path):
  shoppers.icn.coop/food-coop-pdx → coopId = "food-coop-pdx"
  shoppers.icn.coop/timebank-austin → coopId = "timebank-austin"
```

### Data Sovereignty

**Co-ops retain full control:**
- Export ledger anytime (CSV, JSON)
- Export member data (GDPR compliance)
- Migrate to self-hosted icnd + gateway + app anytime

**No lock-in:**
- Reference app is open source (forkable)
- API is documented (any app can use it)
- Data is portable (standard formats)

## Learning Goals

### Week 1-2: Onboarding
**Questions:**
- How hard is it to get co-op members signed up?
- Do they understand DID-based login?
- What's confusing about the initial setup?

### Week 3-4: Basic Usage
**Questions:**
- Can members check their balance easily?
- Is sending payments intuitive?
- Do they understand mutual credit (negative balances OK)?

### Week 5-8: Real Workflows
**Questions:**
- Do governance patterns emerge organically?
- What custom features do they request?
- Where does the app fail their needs?

### Week 9-12: Pain Points
**Questions:**
- Is self-hosting a request? ("We want to run this ourselves")
- Is customization a request? ("We need to modify X")
- Is the substrate insufficient? ("We need Y feature in the protocol")

## Success Criteria (3-month pilot)

**Usage Metrics:**
- 10+ active users logging transactions weekly
- 50+ total transactions
- 3+ governance decisions attempted (proposals, votes)

**Qualitative Feedback:**
- Co-op prefers ICN over previous system (spreadsheet, other tool)
- Members find app intuitive (< 5 min to send first payment)
- Clear signal on what to build next

**Network Effects:**
- 2-3 other co-ops express interest based on pilot results
- Pilot co-op refers another community

## What We Learn Determines Phase 16+

**If pilots reveal:**

**"We need custom backend logic but can't run servers"**
→ Build app runtime (Phase 16)

**"Governance patterns are clear and repeatable"**
→ Build governance templates (Phase 16)

**"Self-hosting is hard, we want to stay hosted"**
→ Build multi-tenant SaaS offering (Phase 17)

**"Self-hosting is important, we want full control"**
→ Build easy self-hosting tools (Phase 16)

**"We want to connect with other co-ops"**
→ Build federation (Phase 17+)

**"Mobile is critical, web-on-phone isn't enough"**
→ Build native mobile apps (Phase 17+)

## Migration Path (Pilot → Production)

### If Co-op Wants to Self-Host

**Step 1: Export data**
```bash
icnctl backup --coop food-coop-pdx --output ./backup.tar.gz
```

**Step 2: Deploy their own infrastructure**
```bash
# On co-op's server
icnctl restore ./backup.tar.gz
icnd --data-dir /opt/icn/food-coop-pdx
icn-gateway --bind 0.0.0.0:8000
```

**Step 3: Deploy reference app (or custom app)**
```bash
# Clone reference app
git clone https://github.com/icn/shoppers-club
cd shoppers-club
npm install

# Configure for self-hosted gateway
echo "NEXT_PUBLIC_ICN_GATEWAY=https://gateway.foodcoop.org" > .env.local

# Deploy to Vercel/Railway/wherever
npm run build
npm run start
```

**Step 4: Redirect users**
```
Old URL: food-coop-pdx.shoppers.icn.coop
New URL: shoppers.foodcoop.org
```

### If Co-op Wants to Stay Hosted

**Option 1: ICN Foundation continues hosting**
- Co-op pays nominal hosting fee ($10-20/month)
- ICN Foundation maintains infrastructure
- Co-op retains data export rights

**Option 2: Cooperative hosting collective**
- Multiple co-ops pool resources
- Tech-savvy co-op runs shared infrastructure
- Cooperative governance of the platform

## Risk Mitigation

**What if pilot fails?**
- Document learnings thoroughly
- Iterate on platform layer
- Find different pilot community
- Total sunk cost: ~$500 (3 months hosting)

**What if we can't scale hosting?**
- Platform layer still ships (gateway + SDK)
- Self-hosting becomes primary path
- Pilots still provided valuable feedback

**What if co-op loses trust in hosted model?**
- Export data immediately
- Help them migrate to self-hosted
- No lock-in, clean exit

## Timeline

**Month 1** (Week 1-4):
- Deploy hosted infrastructure
- Onboard first pilot co-op
- Focus: Onboarding + basic usage

**Month 2** (Week 5-8):
- Onboard second pilot co-op
- Focus: Real workflows + edge cases
- Weekly feedback sessions

**Month 3** (Week 9-12):
- Focus: Pain point identification
- Document learnings
- Decide Phase 16+ priorities

**Month 4+**:
- Continue pilot if valuable
- OR migrate co-ops to self-hosted
- OR build Phase 16 based on learnings

## Deliverables

1. **Hosted deployment** (live at `shoppers.icn.coop`)
2. **Pilot onboarding guide** (how to sign up, use app)
3. **Weekly learnings** in `docs/pilots/learnings/YYYY-MM-DD.md`
4. **Phase 16 decision doc** (what to build next, based on evidence)

---

**Last Updated**: 2025-01-15
**Next Review**: After Phase 14 (gateway + SDK) completion
