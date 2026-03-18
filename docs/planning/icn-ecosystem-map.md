# ICN Ecosystem Map — How Everything Connects
**Last updated:** 2026-03-12 | **Author:** Matt Faherty

---

## The Core Insight

ICN is not an app. It is not a blockchain. It is not a platform.

ICN is the operating system for democratic institutions — the infrastructure layer that lets any group of people run collective governance on their own terms, on their own hardware, with cryptographic proof of every decision. The way Linux is to computing, ICN is to cooperation.

Everything else in this document is either: something that runs ON ICN, something that ATTRACTS people to ICN, or something that SUSTAINS the work.

---

## Layer 1: The Entity Hierarchy

ICN models the real structure of cooperative movements:

```
FEDERATION  ← network of communities/coops (e.g. NY Cooperative Network)
  └── COMMUNITY  ← broad regional or topical collective
        └── COOPERATIVE  ← the basic democratic institution
              └── PERSON  ← individual, self-sovereign identity
```

Each level is a node. Each node is sovereign. A federation does not control its member coops — it provides coordination infrastructure. A community does not own its members' identities — it recognizes them.

**The NY Cooperative Network (NYCN) as a live example:**

```
NYCN Federation Node (intercooperative.network)
  ├── NY Coop Summit Planning Committee Coop (2026)
  ├── Rochester Cooperative Collective (RCC) — future
  ├── Abundance Food Co-op — future
  ├── Cooperation Buffalo — future
  └── TechCollective — potential partner/operator
```

The October 2026 summit is not just a demonstration of ICN. It is the first public event of a real federation running on ICN infrastructure.

---

## Layer 2: Local Identity Wallets

The most important architectural fact about ICN, and the one that differentiates it from every centralized platform:

**People hold their own identity. ICN never does.**

When Matt joins the summit planning committee, the committee doesn't create his identity. Matt creates his identity on his device. His DID (`did:icn:matt-faherty`) lives in a local wallet. His private keys never leave his control. The cooperative recognizes his identity; it does not own it.

**Wallet interfaces:**

| Interface | User | Implementation |
|---|---|---|
| CLI wallet | Operators, developers | `icnctl identity create/export/import` |
| Browser wallet | Members using pilot-ui | SDIS enrollment/proofs/recovery (already built in pilot-ui) |
| Mobile wallet | Members on phones | React Native SDK (sdk/react-native/) |
| Recovery | Anyone | SDIS recovery flow: backup phrases + recovery contacts |

**pilot-ui's SDIS screens are the wallet UI.** The four SDIS views (enrollment, identity, proofs, recovery) are not timebank-specific — they are the generic self-sovereign identity onboarding flow. Every cooperative member goes through this regardless of what apps their coop runs. This means pilot-ui is two things: a wallet/identity layer AND a mutual credit/governance app layer on top.

**What this means for the demo:**
"Your identity belongs to you. Not to us. Not to the cooperative. When you leave a cooperative, your identity goes with you. When you join a new one, you bring your history."

---

## Layer 3: Node Types and Dev Nodes

ICN nodes are not all equal. Different nodes serve different purposes:

**Current node types (implemented):**
- `icnd` — the standard cooperative governance daemon
- K3s pods (alpha/beta/delta/gamma) — four cooperative nodes in the pilot cluster

**Dev nodes** — the current K3s cluster IS a dev node cluster. Four fictional cooperative nodes (Harbor Homes, Lakewood Growers, etc.) federating together to test governance flows. These nodes are:
1. The demo substrate — `present.sh` runs four flows against these nodes
2. The test environment — all ICN integration tests run here
3. The experimentation platform — new node behaviors get prototyped here

**Future node types to build (using dev nodes as substrate):**

| Node Type | Stage | Purpose |
|---|---|---|
| Standard cooperative | A (now) | Basic democratic institution |
| Community hub | B (Phase 2) | Aggregates multiple coops, provides services |
| Federation hub | B (Phase 2) | Connects communities, routes federation governance |
| Compute node | C (Phase 3+) | Runs WASM workloads, earns mana |
| Archive node | C+ | Long-term storage, audit preservation |
| Research node | ongoing | Experimental governance models, sandboxed |

Dev nodes make new node type development tractable: spin up a dev federation, experiment with the node behavior, integrate into the test suite, graduate to production. The K3s cluster structure (3 worker nodes + control plane) maps directly to this — each worker can run a different node type.

---

## Layer 4: The Protocol Stack

```
┌─────────────────────────────────────────────────────┐
│                    APPS LAYER                        │
│  governance · ledger · trust · echo                  │
│  (define MEANING: what is a vote, a budget, a member)│
├─────────────────────────────────────────────────────┤
│               MEANING FIREWALL                       │
│  kernel never interprets meaning — only mechanisms  │
├─────────────────────────────────────────────────────┤
│                   KERNEL LAYER                       │
│  identity · trust · audit · federation · compute     │
│  (enforces MECHANISMS regardless of meaning)         │
├─────────────────────────────────────────────────────┤
│               TRANSPORT LAYER                        │
│  P2P gossip · NAT traversal · TLS                    │
└─────────────────────────────────────────────────────┘
```

**The Charter** sits at the boundary between apps and kernel. It is a TOML configuration file that tells the apps layer what "governance" means for this specific cooperative:

```toml
# charters/worker-coop.toml  (NEEDS TO BE CREATED — Phase 2 Track A)
[membership]
admission = "application_plus_vote"
quorum = "two_thirds"
voting_method = "consent"  # consent, consensus, simple_majority

[treasury]
allocation_method = "governance_vote"
patronage_distribution = "ny_corp_law_93"  # PatronageTracker
reserve_requirement = 0.10

[governance]
proposal_types = ["budget", "membership", "policy", "dissolution"]
decision_method = "consensus"
fallback = "simple_majority"
receipt_required = true  # ExecutionReceiptGate
```

Charter templates are the "app store" of ICN. Different cooperative types use different charters. The charter is what makes the meaning firewall real — the same kernel enforces governance for a housing coop and a worker coop differently, because the charters are different.

**Charter library to build (Phase 2 deliverable):**
- `charters/worker-coop.toml` — equal voting, consent process, patronage distribution
- `charters/event-planning.toml` — temporary structure, budget authorization, consensus
- `charters/consumer-coop.toml` — member-owner, board governance, patronage refunds
- `charters/community-land.toml` — housing-focused, long-term stability, limited equity
- `charters/federation.toml` — meta-governance for coordinating coops

---

## Layer 5: The Four Front-End Surfaces

Four distinct surfaces, four distinct audiences, one shared ICN backend.

### Surface 1: pilot-ui (Member App + Wallet)
**What it is:** Progressive Web App for cooperative members. Time banking, mutual credit, governance participation, identity management.
**Audience:** Individual members of cooperatives using ICN.
**State:** Feature-complete vanilla JS PWA, fully documented, Docker-ready. Has SDIS (wallet), steward dashboard, timebank flows, offline support, i18n (EN + ES).
**Gap:** Not wired to real `icnd`. The `anchor-manager.js` component is the stub for ICN connection. **Phase 2 Track D** wires this up.
**Role in demo:** "This is what a member sees." The SDIS enrollment is the identity wallet onboarding.

### Surface 2: NYCN Platform (Organizer Coordination + ICN Demo)
**What it is:** `ny-coop-net` — FastAPI + React/TypeScript coordination platform for the summit planning committee. Tracks decisions, budget, tasks, meetings, registrations, committees.
**Audience:** Summit planning committee organizers (15 people), eventually any cooperative org running multi-event networks.
**State:** 85% prod-ready. Frontend pages: Dashboard, Committees, Decisions, Budget, Meetings, Registrations, Sponsors, Tasks. Backend models cover all organizing workflows. **Missing:** ICN receipt views, `icn_service.py`, ICN schema additions (see nycn-platform-spec.md).
**Role in demo:** "This is what the planning committee used. Every anchored decision has an ICN receipt."

### Surface 3: Demo UI (Summit Workshop Instrument)
**What it is:** A purpose-built, single-page React + TypeScript SDK app for the summit workshop. NOT a real product. A presentation instrument.
**Audience:** 150 cooperative organizers in a room who have never heard of ICN.
**State:** Does not exist yet. Needs to be built after Phase 2 Track D.
**Spec:** See below.

### Surface 4: Node Dashboard (Operator Admin)
**What it is:** `web/dashboard/` — static HTML admin panel for people running `icnd` nodes.
**Audience:** Technical operators managing ICN infrastructure.
**State:** Placeholder only (app.js, index.html, style.css). No real functionality.
**Priority:** Post v1.0 — operators can use `icnctl` CLI until this is built. Not on the critical path.

### Surface 5: ICN Website (Developer + Org Discovery)
**What it is:** `icn-website` — Astro marketing site at intercooperative.network.
**Audience:** Two distinct visitor types: Rust developers (via HN/GitHub) and cooperative organizers (via summit/community referral).
**State:** Live but stale stats, 1 blog post, no community links. Phase 3 adds nightly sync CI, 4 blog posts, Matrix room, good-first-issues, mailing list.
**Role:** Top of both funnels. Developers land here from HN. Organizers land here from summit QR codes.

---

## Layer 6: The Demo UI Spec

**Purpose:** Enable a 30-minute workshop where cooperative organizers experience ICN governance without touching a terminal.

**Architecture:** Single React/TypeScript file using the ICN TypeScript SDK. Connects to the K3s dev cluster (or a local `icnd` instance).

**Four scenes:**

```
Scene 1: The Cooperative  (5 min)
  ─ "Harbor Homes Housing Cooperative"
  ─ Charter: community-land.toml
  ─ Show: DID created, treasury linked, charter terms visible
  ─ "This cooperative runs on its own node. Nobody else holds the keys."

Scene 2: The Member  (8 min)
  ─ New member "Ana" applies
  ─ She creates her local identity (wallet QR code or keygen)
  ─ Committee votes: consent process, quorum check
  ─ DecisionReceipt produced → show the receipt
  ─ "Ana's membership is now cryptographically provable."

Scene 3: The Decision  (10 min)
  ─ Proposal: $2,400 for roof repair
  ─ AllocationProposal → vote → accepted → AllocationReceipt
  ─ Contractor submits invoice → ExecutionReceipt closes the loop
  ─ "The money was authorized by governance. The governance authorized the money. The audit trail is unbroken."

Scene 4: The Audit  (7 min)
  ─ icnctl audit verify in a terminal pane (or embedded call)
  ─ Show the full chain: Decision → Allocation → Execution
  ─ "Verify this yourself. You don't have to trust us."
  ─ Show federation: Harbor Homes federating with Lakewood Growers
  ─ "This is what cooperative sovereignty looks like in software."
```

**Build spec (for Claude Code, post Phase 2 Track D):**
- Single `.tsx` file, React + Vite
- Uses `@icn/typescript-sdk` for all ICN calls
- No server-side state — reads from live `icnd` at a hardcoded (configurable) endpoint
- Designed to run from a laptop: `npm run dev` → localhost:5173
- Presenter mode: large fonts, high contrast, no debug noise
- Four scenes navigable via keyboard arrows (like a slideshow but live)
- Each scene shows: what command ran, what ICN returned, what it means in plain English

---

## Layer 7: The Two Funnels

### Developer Funnel
```
HN post (Show HN: "We built a P2P cooperative governance daemon")
  → icn-website: stats, architecture overview, AGENTS.md
  → GitHub: 1330+ PRs, 440K LoC Rust, clean architecture
  → Blog Post 1: "What ICN Is (And What It Isn't)"
  → Matrix room: first question answered within 24h
  → Good first issues: 10-12 labeled, well-scoped
  → First PR: contributor
  → Second PR: regular contributor
  → Governance participation: co-developer
```

**Good first issue candidates to label (Claude Code task):**
- Terminology: any remaining `payment`/`balance` occurrences after Phase 1 (easy grep)
- Test coverage: specific crates with <70% coverage
- Documentation: missing doc comments on public APIs
- TypeScript SDK: type completeness, example improvements
- `icnctl`: missing help text, better error messages
- Charter templates: write the TOML configs (content work, not Rust)

### Cooperative Operator Funnel
```
Summit workshop (October 2026)
  → "This is how we ran this summit"
  → QR code → icn-website
  → Blog Post 3: live demo walkthrough
  → Mailing list signup (Buttondown)
  → Monthly "ICN Update" email
  → Blog Post 4: regulatory safety
  → Operator tries pilot deployment
  → "How do I set this up?" → Matrix room
  → Operator deploys → submits case study
  → Case study becomes next year's intro slide
```

---

## Layer 8: The Unified Narrative

Different words for different rooms, same underlying truth:

**For Rust developers (HN):**
> "ICN is a P2P governance daemon. It enforces cooperative governance rules at the protocol level — identity, membership, proposals, votes, receipts — without any central server. The architecture separates kernel (mechanism) from apps (meaning). 34 crates, 1327+ merged PRs, running on K3s. Not a blockchain. Show HN."

**For cooperative organizers (summit):**
> "ICN is the infrastructure your cooperative has been missing. Every platform you use — Slack, Google Docs, Signal — is owned by someone else. ICN is cooperative governance that runs on your hardware, with your rules, and produces cryptographic proof of every decision your group makes."

**For grant reviewers (STF):**
> "ICN is open-source digital public infrastructure for democratic governance. It provides identity, membership, collective decision-making, and resource allocation without hosted balances, without central servers, and without operator control of user funds. It is the governance substrate that cooperative organizations, community groups, and democratic institutions need to operate sovereignty in the digital age."

**For regulators (if it ever comes to that):**
> "ICN is a governance coordination tool. It does not hold balances. It does not route payments. It tracks obligations between members of voluntary associations and produces auditable records of the governance decisions that authorize those obligations. The Seven Invariants ensure this by architectural constraint."

All four descriptions are accurate. The frame changes, the substance doesn't.

---

## Layer 9: The Flywheel

```
      PROTOCOL QUALITY
     (Phase 0→2, v1.0.0)
            │
            ▼
   REAL DEPLOYMENT (NYCN)  ──────────────────────────┐
            │                                          │
            ▼                                          │
   COMPELLING STORY                                   │
   (4 blog posts, demo)                               │
            │                                          │
      ┌─────┴─────┐                                   │
      ▼           ▼                                   │
DEVELOPER    OPERATOR                                 │
DISCOVERY    DISCOVERY                                │
(HN, r/rust) (summit, orgs)                          │
      │           │                                   │
      ▼           ▼                                   │
MATRIX ROOM  MAILING LIST                            │
CONTRIBUTORS PILOT OPERATORS                         │
      │           │                                   │
      └─────┬─────┘                                   │
            ▼                                         │
      STF GRANT (€50K+)                               │
            │                                         │
            ▼                                         │
   MORE DEVELOPMENT ──────────────────────────────────┘
   (compute substrate, federation at scale)
```

Each loop tightens the next. The real deployment (NYCN) is what makes the story credible. The credible story is what attracts developers and operators. The developers improve the protocol. The operators generate case studies. The case studies win the grant. The grant funds the next phase.

The flywheel starts with **getting NYCN running and the planning committee using it before the March 19 call.**

---

## Layer 10: The Parallel Tracks

ICN is the primary track. Two others run parallel:

### Writing Track (Villain + BionicWritingLab)
- **Villain in the Verse** (74K/80K words, 93% done) — finish, fix CI, query agents
- **BionicWritingLab** (3 books) — publish books 1+2 on existing brand, build audience
- **Connection to ICN:** Zero direct. These are income diversification and intellectual reputation. A published novelist/political essayist with a cooperative tech infrastructure project is a more interesting grant applicant than a pure techie. The writing voice shapes how ICN gets communicated.

### Funding/Business Track
- **ACCES-VR** ($11K equipment/training) — waiting on mail (medical release critical)
- **Outta Excuses + Verizon** (March 31 deadline) — work block March 25
- **STF** (April) — primary, requires v1.0.0 + compliance sprint + 2 blog posts
- **PASS** (SSA work incentive) — not yet applied, stacks with ACCES-VR
- **CFNE + Working World** (future) — non-extractive loans when ICN needs capital
- **TechCollective** (employment) — 85+ days cold, follow up as peer (share ICN, not just job-seeking)

---

## Layer 11: Open Decisions and Gaps

**Decisions needed now:**

| Decision | Options | Recommendation |
|---|---|---|
| pilot-ui framework | Keep vanilla JS or rewrite in React + TS SDK | Keep vanilla JS for now. Rewrite is scope creep until real icnd connection is wired. |
| Demo UI timing | Build before summit call (May) or after | After Phase 2 Track D. Terminal demo + blog post 3 is sufficient for May proposal. Demo UI for October. |
| NYCN deployment | K3s (same cluster as ICN) or separate Proxmox LXC | Separate LXC. K3s is for ICN protocol dev, not app hosting. |
| Charter template format | TOML configs or programmatic DSL | TOML for now. DSL is Phase 3+ after CCL PolicyOracle is done. |
| Matrix vs Discord | Matrix (federated, sovereign) or Discord (easy) | Matrix. The audience are cooperative organizers. Running on Discord is philosophically wrong. |

**Gaps that need to be built (in priority order):**

1. `charters/` directory with 3 initial templates — Phase 2 Track A deliverable
2. ICN service layer + receipt views in ny-coop-net — after ICN schema additions
3. `icnctl identity` wallet CLI commands — Phase 2 pre-work
4. Operator getting-started guide for `icnd` — parallel with blog post 2
5. Demo UI — after Phase 2 Track D
6. Node dashboard (real) — post v1.0
7. Charter DSL / CCL PolicyOracle integration — Phase 3+

---

## The Summary in One Sentence Per Layer

- **Protocol:** ICN is a Rust daemon enforcing cooperative governance via cryptographic receipts, running P2P without any central server.
- **Entities:** Individuals hold local wallets → cooperatives are sovereign → communities coordinate coops → federations connect communities.
- **Nodes:** Dev nodes are the experimentation substrate; different node types serve different roles in the hierarchy.
- **Apps:** The meaning firewall separates kernel (mechanism) from apps (meaning); charters configure the meaning.
- **Surfaces:** pilot-ui (members), NYCN (organizers), Demo UI (workshop), node dashboard (operators), website (discovery).
- **Funnels:** Developers find ICN via Hacker News. Organizers find it via the summit. Both land on the same website, diverge from there.
- **Narrative:** Four true descriptions, one underlying architecture — pick the frame that fits the room.
- **Flywheel:** NYCN → story → discovery → community → grant → development → better NYCN.
- **Parallel tracks:** Writing builds reputation and income; funding builds runway; ICN builds the thing.

---

*Document generated 2026-03-12. Companion documents: icn-forward-plan.md (ICN development), nycn-platform-spec.md (NYCN platform). This document is the connective tissue between them.*
