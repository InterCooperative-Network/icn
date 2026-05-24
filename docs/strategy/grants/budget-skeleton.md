---
Status: operational
Canonical: yes
Last Reviewed: 2026-05-19
---

# ICN Budget Skeleton

Structural template for grant applications. Adapt amounts and line items per funder. **Numbers are placeholders requiring Matt's input** — they're starting points to be tuned, not authoritative quotes.

This replaces the March 2026 stale version; the major change is the **Hardware Infrastructure** category, which was missing entirely from the previous version.

For the funding pipeline this budget feeds into, see [`funding-pipeline.md`](funding-pipeline.md).

---

## Budget Categories

### 1. Development Labor

| Item | Monthly Cost | Duration | Total |
|------|-------------|----------|-------|
| Lead developer (Matt Faherty) | [PLACEHOLDER: SSDI-compatible rate, ~$2,000-2,500] | 6 months | [PLACEHOLDER] |
| Contract developer (mobile UX) | [PLACEHOLDER: $4,000-6,000] | 3 months | [PLACEHOLDER] |
| **Subtotal** | | | **[PLACEHOLDER]** |

**Notes:**
- Lead developer rate must respect SSDI income limits ($1,210/mo TWP, $1,690/mo SGA in 2026). Structure either as: reimbursed expenses through fiscal sponsor (preferred), or sub-SGA monthly with surplus to coop entity.
- Contract mobile developer needed for React Native mobile UX (voting, receipt verification, member shell v0).
- ACCES-VR plan may cover up to $15K of startup costs separately (see [`applications/acces-vr-self-employment.md`](applications/acces-vr-self-employment.md)).

### 2. Hardware Infrastructure (new — was missing from previous skeleton)

ICN's grant pitch says "infrastructure cooperatives can own." The budget needs to reflect that. Three tiers, configured per the grant size:

#### Tier A — Minimum-viable hardening (~$500–800)

Closes the most immediate single-points-of-failure and unblocks paused work.

| Item | Cost | Why |
|------|------|-----|
| UPS for node-1 stack | $200–300 | No UPS today; brownouts = outage |
| Zigbee USB dongle + spare | $50–100 | Blocking HA work per current hardware plans |
| Secondary AD DC VM resources (existing hardware) | $0 | Free; just allocation |
| CI runner disk expansion (existing hardware) | $0 | Free; just allocation |
| Cloud backup target (3 months) | $50–100 | Atlas-down recovery path |
| **Subtotal** | **$300–500** | |

**Funder fit:** Could come out of ACCES-VR equipment allocation. Doesn't need foundation funding.

#### Tier B — Pilot-ready (~$2,000–3,500)

What's needed for a credible institutional pilot (NYCN deployment) without external dependencies.

| Item | Cost | Why |
|------|------|-----|
| RAM upgrades across i5 nodes (4× node 1-4: 8GB → 32GB each) | $400–600 | Current 16GB ceiling constrains k3s growth |
| Backup firewall (small mini-PC, OPNsense-capable) | $300–400 | Single-firewall SPOF removed |
| Managed L3 switch with VLAN support | $300–500 | Current VLAN segmentation is constrained |
| 2× 4TB SSD for TrueNAS expansion | $400–600 | atlas headroom |
| 10GbE NIC for additional node | $200–300 | Storage network expansion |
| 1-year off-site backup storage | $200–400 | DR coverage |
| **Subtotal** | **$1,800–2,800** | |

**Funder fit:** NLnet ($30–50K range) easily covers this as part of "hardware to deploy and demo the federation runtime." Capital Impact ($10–50K) similarly.

#### Tier C — Federation-ready (~$5,000–10,000)

What's required to host the federation services ICN's architecture promises (Matrix homeserver, Forgejo, video, package mirror) and maintain a second site for DR.

| Item | Cost | Why |
|------|------|-----|
| 2× modern compute nodes (Ryzen 7 / i7-13xxx, 64GB RAM, 2TB NVMe) | $3,000–5,000 | Capacity for Matrix homeserver, Forgejo, video, package mirror |
| Enterprise 10GbE switch | $400–800 | Federation requires real network |
| Second site (small co-located mini-PC or trusted ally hosting, 1 year) | $1,200–2,500 | DR; "ICN runs on hardware you control" needs more than one location |
| Production-grade UPS + power distribution | $500–800 | Site reliability |
| Bandwidth / public IP capacity uplift | $300–800 | Hosting public-facing federated services |
| **Subtotal** | **$5,400–9,900** | |

**Funder fit:** Sovereign Tech Fund (€50K–€1M range) makes sense — STF specifically funds the kind of foundational infrastructure ICN runs. Pair this with the kernel-hardening proposal in [`applications/sovereign-tech-fund.md`](applications/sovereign-tech-fund.md).

---

### 3. Cloud / Hosting (pilot phase)

| Item | Monthly Cost | Duration | Total |
|------|-------------|----------|-------|
| Cloud hosting (interim pilot nodes for partners without on-prem) | $50–100 | 6 months | $300–600 |
| Domain + DNS (icn.zone, intercooperative.network) | $30/yr | 1 year | $30 |
| SSL certificates | $0 (Let's Encrypt) | — | $0 |
| **Subtotal** | | | **$330–630** |

**Notes:**
- Current development runs on homelab; cloud is for pilot cooperatives who don't yet have on-prem.
- 2–3 pilot nodes at $20–50/month each.

### 4. Pilot Program

| Item | Cost | Notes |
|------|------|-------|
| Cooperative onboarding (travel, meetings) | [PLACEHOLDER: $500–1,500] | Upstate NY (NYCN), within driving distance |
| Workshop materials | $200–500 | Printed guides, demo devices |
| Pilot support (3 months) | [Included in dev labor] | — |
| **Subtotal** | | **[PLACEHOLDER]** |

### 5. Community & Outreach

| Item | Cost | Notes |
|------|------|-------|
| NY Cooperative Summit participation | $0 | Matt is core organizer |
| Conference travel (FOSDEM, NCBA CLUSA, Eastern Conference, 38C3, OuiShare Fest) | [PLACEHOLDER: $1,000–3,000] | 2–4 events |
| Documentation + guides | $0 | Included in dev labor |
| **Subtotal** | | **[PLACEHOLDER]** |

### 6. Legal & Administrative

| Item | Cost | Notes |
|------|------|-------|
| Fiscal sponsor fees | [PLACEHOLDER: 5–10% of total] | Current sponsor: Alchemical. Compare against Open Source Collective. |
| Legal review (cooperative formation, ICN entity structure) | [PLACEHOLDER: $500–2,000] | NY State coop law specifics |
| SSDI-compatible accounting | [PLACEHOLDER: $1,000–2,500] | Disability-aware tax + income planning |
| Security audit (Trail of Bits / NCC / Cure53 class) | [PLACEHOLDER: $50,000–80,000] | Only if Sovereign Tech Fund grant lands; otherwise deferred |
| **Subtotal** | | **[PLACEHOLDER]** |

---

## Budget Tiers by Funder

### Micro-grant ($500–3,000)
*e.g., ACCES-VR equipment allocation, CDF small grant if eligible*

Focus: Hardware Tier A + initial coop legal formation
- Hardware Tier A (minimum-viable): $500
- Initial legal review: $500–1,500
- Workshop materials for first organizer presentation: $500
- Travel for NYCN onboarding meetings: $500

### Small grant ($5,000–15,000)
*e.g., NLnet NGI Zero (low end), Capital Impact Co-op Innovation Award*

Focus: Hardware Tier B + pilot deployment + mobile UX start
- Hardware Tier B (pilot-ready): $2,000–3,500
- Mobile dev contract (1 month): $4,000–6,000
- Cloud hosting: $300–600
- Pilot onboarding: $1,000–1,500
- Fiscal sponsor fees + legal: $1,000–2,000

### Medium grant ($30,000–$60,000 / €25,000–€50,000)
*e.g., NLnet NGI Zero Commons full ask, Mozilla Democracy x AI*

Focus: Full slice — dev labor + hardware Tier B + pilot deployment
- Lead developer (6 months, SSDI-compatible): $12,000–18,000
- Mobile dev contract (3 months): $12,000–18,000
- Hardware Tier B: $3,000–4,000
- Cloud hosting: $600
- Pilot program (1 cooperative): $2,000–3,000
- Community outreach: $1,500–3,000
- Legal/admin: $2,500–4,000
- Fiscal sponsor fees: $2,500–5,000

### Large grant (€100,000–€200,000 / Sovereign Tech Fund)
*e.g., Sovereign Tech Fund — kernel + identity hardening slice*

Focus: Kernel hardening + security audit + Hardware Tier C
- Lead developer (9 months, SSDI-compatible): $25,000–35,000
- External security audit (Trail of Bits class): $60,000–80,000
- Hardware Tier C (federation-ready): $5,000–10,000
- Test hardware + adversarial-load infrastructure: $10,000–15,000
- Fiscal sponsor fees: $10,000–15,000
- Publication + dissemination: $3,000–5,000

---

## Missing Inputs (needed before any application submits)

- [ ] **Exact SSDI-compliant compensation rate.** Must stay under TWP/SGA thresholds. Need accountant input.
- [ ] **ACCES-VR plan status.** Is the IPE signed? What's been claimed so far? See [`applications/acces-vr-self-employment.md`](applications/acces-vr-self-employment.md).
- [ ] **Named pilot partner specifics.** NYCN status — formal commitment vs intended.
- [ ] **Fiscal sponsor decision.** Alchemical (current) vs Open Source Collective vs other. Compare fee structure + capabilities.
- [ ] **Contract mobile developer rate.** Market: $50–100/hr. Need actual quote.
- [ ] **Hardware quotes.** Above numbers are estimates. Get real quotes from preferred vendors before any submission.
- [ ] **NY State coop law specifics.** Article 5-A conversion path + cost from BCL Corp.

---

## How to use this budget

For each grant application:
1. Pick the right tier based on funder's typical award size
2. Fill in the placeholders with concrete numbers from the application's specific deliverables
3. Add or remove line items as appropriate to the funder's eligible categories
4. Cross-check against the "Missing Inputs" list — don't submit anything where critical inputs are still PLACEHOLDER

The pipeline at [`funding-pipeline.md`](funding-pipeline.md) names which funders need which budget tier.
