# Entity Topology Patterns

Common organizational structures for institutions on ICN. Each pattern shows the entity tree, governance domains, and treasury layout. Choose the pattern closest to your institution and customize.

---

## Pattern 1: Flat Cooperative

A single cooperative with no internal committees. All members participate equally in governance. Common for small worker co-ops (< 20 members).

```
my-coop                       Cooperative(CooperativeProfile)
├── governance_domain_id:     "my-coop-gov"
├── treasury_account:         "my-coop-treasury"
└── members:
    ├── Alice (Founder)       → Vote, Propose, TreasuryAccess, Sign
    ├── Bob (Worker)          → Vote, Propose
    └── Carol (Worker)        → Vote, Propose
```

**Governance**: One domain. All decisions go to the full membership.
**Treasury**: One account. Spending thresholds defined in charter.
**Communication**: One gossip topic (`my-coop:general`).

**When to use**: Worker co-ops with fewer than ~20 people where everyone participates in every decision.

---

## Pattern 2: Cooperative with Committees

A cooperative with working groups that have delegated authority. The board/steering body governs, committees execute within their scope.

```
my-coop                       Cooperative(CooperativeProfile)
├── governance_domain_id:     "my-coop-gov"
├── treasury_account:         "my-coop-treasury"
│
├── my-coop-board             Community(CommunityProfile)
│   ├── parent_id:            "my-coop"
│   └── governance_domain_id: "my-coop-board-gov"
│
├── my-coop-finance           Community(CommunityProfile)
│   ├── parent_id:            "my-coop"
│   └── governance_domain_id: "my-coop-finance-gov"
│
└── my-coop-programs          Community(CommunityProfile)
    ├── parent_id:            "my-coop"
    └── governance_domain_id: "my-coop-programs-gov"
```

**Governance**: Board domain for organizational decisions. Committee domains for delegated scope.
**Treasury**: One account. Committees spend within board-approved allocations.
**Communication**: Per-committee gossip topics + org-wide channel.

**When to use**: Mid-size cooperatives (20–100 members) with distinct functional areas.

---

## Pattern 3: Federation of Cooperatives

A network of independent cooperatives coordinating through a shared governance layer. Each member retains sovereignty. The federation governs shared concerns.

**This is the NYCN pattern.** With real data:
- 236 organizations in the network
- 29 sponsors with 2026 pledges
- Committees: Content, Logistics, Marketing, Finance, Core Organizing
- Events: 2024 (Syracuse), 2025 (Albany), 2026 (Albany, planning)

```
nycn                          Federation(FederationProfile)
├── governance_domain_id:     "nycn-federation-gov"
├── treasury_account:         "nycn-federation-treasury"
├── member_entities:          [greenstar, ica-group, ncba, cdi, cfne, ...]
│
├── nycn-organizers           Cooperative(CooperativeProfile)
│   ├── parent_id:            "nycn"
│   ├── governance_domain_id: "nycn-organizers-gov"
│   ├── treasury_account:     "nycn-ops-treasury"
│   │
│   ├── nycn-steering         Community — steering committee
│   ├── nycn-content          Community — content committee (12 sessions for 2026)
│   ├── nycn-logistics        Community — logistics committee
│   ├── nycn-marketing        Community — marketing committee
│   ├── nycn-finance          Community — finance committee ($29K+ in sponsor pledges)
│   │
│   └── nycn-summit-2026      Community — time-bounded event entity
│       └── status:           Forming → Active → Complete
│
├── greenstar                 Cooperative — GreenStar Food Co-op (silver sponsor, $500)
├── ica-group                 Cooperative — ICA Group (platinum sponsor, $2,000)
├── national-coop-bank        Cooperative — National Cooperative Bank (platinum, $2,000)
├── cdi                       Cooperative — Cooperative Development Institute (gold, $1,000)
├── cfne                      Cooperative — Cooperative Fund of the Northeast (gold, $1,000)
└── ... (236 total organizations)
```

**Governance**: Federation domain for network-wide decisions (membership, charter). Steering domain for operational decisions (budget, venue). Committee domains for delegated scope.
**Treasury**: Federation treasury (dues, shared resources) + ops treasury (event budget). Committees have scoped allocations.
**Communication**: Layered gossip topics — federation announcements, organizer coordination, per-committee channels, public community.

**When to use**: Regional networks, sector alliances, any coordination of independent cooperatives.

---

## Pattern 4: Community Organization

An open-membership community with working groups and elected leadership. No treasury (or minimal), governance-focused.

```
my-community                  Community(CommunityProfile)
├── governance_domain_id:     "my-community-gov"
│
├── my-community-mutual-aid   Community(CommunityProfile)
│   ├── parent_id:            "my-community"
│   └── governance_domain_id: "my-community-aid-gov"
│
└── my-community-events       Community(CommunityProfile)
    ├── parent_id:            "my-community"
    └── governance_domain_id: "my-community-events-gov"
```

**Governance**: Low-threshold, open participation. Working groups form and dissolve by proposal.
**Treasury**: None by default. If money enters the picture, upgrade the root entity to `Cooperative`.
**Communication**: Open public channel + scoped working group channels.

**When to use**: Mutual aid networks, neighborhood councils, civic groups, movement organizations.

---

## Pattern 5: Nested Federation (Federation of Federations)

A national or sectoral body coordinating regional federations, each of which coordinates local cooperatives.

```
national-coop-assoc           Federation(FederationProfile)
├── governance_domain_id:     "national-gov"
├── member_entities:          [nycn, midwest-fed, pacific-fed, ...]
│
├── nycn                      Federation(FederationProfile)
│   ├── parent_id:            "national-coop-assoc"
│   ├── member_entities:      [greenstar, breadhive, ...]
│   └── ...
│
├── midwest-fed               Federation(FederationProfile)
│   ├── parent_id:            "national-coop-assoc"
│   ├── member_entities:      [...]
│   └── ...
│
└── pacific-fed               Federation(FederationProfile)
    ├── parent_id:            "national-coop-assoc"
    ├── member_entities:      [...]
    └── ...
```

**Governance**: Three levels — national (cross-regional), regional (cross-local), local (per-coop). Each level has its own governance domain and charter.
**Treasury**: Clearing relationships between federations. National treasury for shared programs.
**Communication**: Hierarchical gossip topology — national announcements propagate down, local discussion stays local.

**When to use**: National cooperative associations, global federation structures, multi-tier networks.

---

## Choosing Your Pattern

| Question | If yes... |
|----------|-----------|
| Do you have fewer than 20 members and make all decisions together? | Pattern 1: Flat Cooperative |
| Do you have working groups or committees with delegated authority? | Pattern 2: Cooperative with Committees |
| Are you a network of independent organizations? | Pattern 3: Federation |
| Are you a community group focused on coordination, not economics? | Pattern 4: Community Organization |
| Do you coordinate other federations? | Pattern 5: Nested Federation |

Most institutions start as Pattern 1 or 4 and evolve toward Pattern 2 or 3 as they grow.

---

## See Also

- [Institution Setup Guide](../guides/institution-setup.md) — Step-by-step bootstrap
- [CCL Charter Templates](ccl-charter-templates.md) — Governance parameters per pattern
- [Governance Playbook](governance-playbook.md) — Common decision patterns
