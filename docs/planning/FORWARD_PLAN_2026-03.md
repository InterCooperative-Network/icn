# ICN Forward Plan — March 2026

**Document status:** Living planning document
**Last updated:** 2026-03-12
**Author:** Matt Faherty
**Canonical location:** `.claude_launchpad/icn-forward-plan.md` (launchpad) + `docs/planning/FORWARD_PLAN_2026-03.md` (ICN repo)

---

## Table of Contents

1. [Current State Audit](#1-current-state-audit)
2. [Strategic Frame](#2-strategic-frame)
3. [Phase 0 — Clean Slate](#3-phase-0--clean-slate-today-2-hours)
4. [Phase 1 — Compliance Sprint](#4-phase-1--compliance-sprint-days-14)
5. [Phase 2 — Vertical Slice](#5-phase-2--vertical-slice-weeks-24)
6. [Phase 3 — Website Overhaul](#6-phase-3--website-overhaul-parallel-with-phase-2)
7. [Phase 4 — Grant Applications & Summit](#7-phase-4--grant-applications--summit-aprilmay)
8. [Developer Attraction Funnel](#8-developer-attraction-funnel)
9. [Timeline Summary](#9-timeline-summary)
10. [Open Issues Reference](#10-open-issues-reference)

---

## 1. Current State Audit

**Audit date:** 2026-03-12
**Source:** Live icn-dev inspection via SSH + GitHub API

### Repository Structure

| Component | Location | State |
|---|---|---|
| Rust workspace | `icn/` | 34 crates, 3 binaries (icnd, icnctl, icn-console) |
| Apps | `icn/apps/` (canonical) + `apps/` (legacy) | governance, ledger, trust, echo |
| TypeScript SDK | `sdk/typescript/` | Active, dependabot bump pending (#1329) |
| React Native SDK | `sdk/react-native/` | Active |
| Pilot UI | `web/pilot-ui/` | Running in K3s |
| Demo scripts | `demo/scripts/` | 4 flows complete (governance/patronage/federation/reporting) |
| OPS MCP server | `ops/mcp/` | v2 schema, 8 tool sets, zombie prevention — committed 2026-03-12 |
| Website | `icn-website/` (separate repo) | Astro, deployed to intercooperative.network |

### Season 14 — What Landed (March 5–6, 2026)

Five PRs merged in a two-day sprint:

| PR | Task | Description |
|---|---|---|
| #1323 | s14-t10 | Fix: assign + register treasury during coop activation |
| #1324 | s14-t5 | Test: multi-node commons contribution integration test (#949) |
| #1325 | s14-t11 | PatronageTracker: internal capital accounts per NY Corp Law §93 |
| #1326 | s14-t15 | DelegationManager: persist + gossip sync |
| #1327 | s14-t16 | ExecutionReceiptGate: Invariant 7 enforcement (governance→execution linkage) |

Last main-branch commit: March 6. Nothing landed in 6 days — development parked on demo branch.

### Current Branch

`feat/demo-federation-system` — **27 commits ahead of main**, all demo infrastructure:

- 4 presenter-mode demo flows (governance, patronage, federation, reporting)
- `present.sh` tmux two-pane launcher
- Beat markers, narrated output mode
- `reseed-federation-demo.sh`, `reset-demo.sh`
- Seed data for four federation personas
- `ops/mcp` v2 commit (should be on main already)

**Status:** Done. Needs to merge.

### Open Epics

| Issue | Priority | Title | Status |
|---|---|---|---|
| #1147 | P0 | Vertical Slice: Identity→Governance→Compute→Receipts→Audit | Open, 5 tracks |
| #1302 | P0 | Regulatory-safe verifiable state architecture (Phases 22–24) | ~60% done |
| #1295 | P1 | IPv6 dual-stack transport (Phases 21–22) | Open, 6 sub-issues |

### #1302 Compliance Epic — Actual Status

| Issue | Description | Status |
|---|---|---|
| #1303 | Rename payment→settlement, currency→unit, balance→position | ❌ Open |
| #1304 | UX language guide + Seven Invariants PR checklist | ❌ Open |
| #1305 | JournalEntry.provenance required (ProvenanceRef) | ❌ Open |
| #1306 | Obligation type with lifecycle (Issued→Accepted→Settled/Defaulted) | ❌ Open |
| #1307 | PatronageTracker for NY Corp Law §93 | ✅ Done (t11, PR #1325) |
| #1308 | Extract commons credit formula to CCL PolicyOracle | ❌ Open |
| #1309 | DelegationManager persist + gossip sync | ✅ Done (t15, PR #1326) |
| #1310 | ExecutionReceiptGate | ✅ Done (t16, PR #1327) |
| #1311 | AllocationProposal for participatory budgeting | ❌ Open |
| #1312 | CI compliance linter (custodial/payment-rail patterns) | ❌ Open |

Three checkboxes in #1302 are done but not marked. Fix immediately.

### #1147 Vertical Slice — Track Summary

| Track | Items | Notes |
|---|---|---|
| A: Kernel/App Separation | A1–A4 | Remaining cleanup after kernel/app init (Jan 26) |
| B: Treasury & Economics | B1–B3 | Critical path — treasury-coop integration, asset types, receipt chain |
| C: Operations & Pilot Prep | C1–C3 | Backup validation, health check, NAT traversal |
| D: Integration | D1–D2 | D1=vertical slice test, D2=demo scripts use real calls |
| E: Compute Substrate | E1–E7 | Workload manifest, WASM compute governance, mana loop |

**Success criteria:**
```
icnctl coop create
  → treasury DID
  → member applies → governance vote → approved
  → Decision Receipt → Allocation Receipt → Execution Receipt
  → icnctl audit verify ✓
  → CI test passes
```

### Infrastructure

| Component | Status |
|---|---|
| K3s cluster | 3 nodes Ready, v1.34.4+k3s1, up 12 days at 10.8.30.40–42 |
| ICN pods | alpha/beta/delta/gamma all 1/1 Running (5d16h), daemon + pilot-ui (12d) |
| Monitoring | Prometheus + Grafana + Alertmanager running |
| etcd snapshots | Completing on schedule |
| icn-dev disk | 48% used (232G/484G) — healthy; icn-wt worktrees at 55G, periodic clean needed |
| Proxmox nodes | 11–14 UP; node-15 (hyperion) DOWN — RMA submitted Feb 20 |
| Ollama | 7 models loaded including qwen2.5-coder:32b |
| Polyglot gateway | Active at :7800, routes to Ollama (claude available once billing credits added) |

### Website Audit

| Item | State |
|---|---|
| Domain | intercooperative.network (CNAME confirmed) |
| Framework | Astro + gh-pages deploy |
| Stats | **STALE** — last synced 2026-02-18, showing 142 PRs (actual: 1327+) |
| Blog | 1 post ("Welcome to the ICN Blog", Feb 18) |
| Community | 6 GitHub links only — no Matrix, no Discord, no mailing list |
| Roadmap | Renders PHASE_HISTORY.md — historical only, no live epic tracking |
| Sync CI | `sync-from-icn.sh` exists but no automated cron deploy |

---

## 2. Strategic Frame

ICN has three time-sensitive events:

### March 19 — NY Coop Summit Planning Call
- Joe Marraffino hosting via Google Meet, 2pm EDT
- Matt's goal: volunteer for tech/infrastructure track + propose ICN workshop for May call for presenters
- **Required state:** Clear demo narrative ready, "Institution-in-a-Box" framing locked
- RSVP still `needsAction` — must confirm to Joe before March 19

### May — Call for Presenters (NY Coop Summit, Oct 2026)
- ICN workshop proposal goes in: "Live Demo: Spin Up a Cooperative in 5 Minutes"
- **Required state:** Demo flows running against real system calls (Track D done), vertical slice test passing
- The cooperative movement audience is organizers, not developers — pitch is Stage A of VISION.md

### Sovereign Tech Fund (Rolling Deadline → Target: April)
- €50K+ grant for open source digital infrastructure
- ~6 months to contract: April application → October funding (pre-summit)
- **Required state:** Regulatory-clean language (#1302 done), working demo, vertical slice proof
- ICN framing: "Digital public infrastructure for cooperative governance" — never "ledger" or "payment"

### Funding Gate A (Background)
- Fiscal sponsorship (Hack Club Bank) + W-2 payroll at $1,200/month
- Requires ACCES-VR medical release (waiting on mail) + business plan v2 (done)
- Working vertical slice is the "product" that justifies the business plan

---

## 3. Phase 0 — Clean Slate (Today, ~2 hours)

These four actions unblock everything downstream.

### 0.1 Merge `feat/demo-federation-system` → main

```bash
# On icn-dev
cd ~/projects/icn
git checkout main
git merge feat/demo-federation-system --no-ff -m "feat(demo): merge federation demo system — four flows, presenter mode, ops/mcp v2"
git push origin main
```

**Why first:** 27 commits of shipped work sitting off-branch is noise. After merge, PR count crosses 1330+, which updates the stats display meaningfully. Also resets the working context to main.

### 0.2 Fix PR #1329 — Add missing labels, merge dependabot bump

```bash
# Add labels to the InterCooperative-Network/icn repo
gh label create "sdk" --color "0075ca" --description "SDK-related changes" -R InterCooperative-Network/icn
gh label create "typescript" --color "3178c6" --description "TypeScript code" -R InterCooperative-Network/icn

# Approve and merge
gh pr review 1329 --approve -R InterCooperative-Network/icn
gh pr merge 1329 --squash -R InterCooperative-Network/icn
```

### 0.3 Mark completed #1302 checklist items

Edit issue body for #1302 to check:
- [x] #1307 PatronageTracker (done via PR #1325)
- [x] #1309 DelegationManager persist (done via PR #1326)
- [x] #1310 ExecutionReceiptGate (done via PR #1327)

```bash
# Use gh to edit the issue body or just do it in the GitHub web UI
gh issue edit 1302 --body "$(gh issue view 1302 --json body -q .body | \
  sed 's/- \[ \] #1307/- [x] #1307/' | \
  sed 's/- \[ \] #1309/- [x] #1309/' | \
  sed 's/- \[ \] #1310/- [x] #1310/')" -R InterCooperative-Network/icn
```

### 0.4 Trigger website sync + deploy

```bash
# On icn-dev — rebuild and deploy with current stats
cd ~/projects/icn-website
npm run sync && npm run deploy
```

Stats will show: 1330+ PRs, current commit, accurate branch count. Takes ~10 minutes.

---

## 4. Phase 1 — Compliance Sprint (Days 1–4)

Closes epic #1302 entirely. Required before Sovereign Tech Fund application.

**Why this before Vertical Slice:** The grant and cooperative movement positioning both depend on ICN using regulatory-safe language. The codebase is architecturally correct already — it just uses words (`payment`, `currency`, `balance`) that create unnecessary exposure. Phase 1 fixes the vocabulary without changing any behavior.

### Day 1 — Terminology Rename (#1303 + #1304)

**#1303:** Rename across gateway API, OpenAPI spec, field names, docs:
- `payment` → `settlement`
- `currency` → `unit`
- `balance` → `position`

This is grep-and-replace plus test updates. Nothing behavioral changes.

**#1304:** Write the Seven Invariants as a PR checklist in `CONTRIBUTING.md`:

```
1. User-signed transitions only
2. No hosted balances
3. No operator routing of value
4. Derived views are not authoritative
5. No embedded convertibility
6. Matching/market features are opt-in, scoped, governance-authorized
7. Execution receipts close the governance loop
```

Also write the UX language guide: what words ICN uses and why (`settlement` not `payment`, `obligation` not `debt`, `position` not `balance`).

### Day 2 — Structural Attestation (#1305 + #1306)

**#1305:** Make `JournalEntry.provenance` required. Every ledger entry must carry a `ProvenanceRef` linking it to the governance decision that authorized it. No provenance = invalid entry = compile error.

**#1306:** Add `Obligation` type with explicit lifecycle:
```rust
enum ObligationState {
    Issued,
    Accepted,
    Settled,
    Defaulted,
    Disputed,
}
```

Uses `AssetType::Claim` (already defined). This is the structural proof that ICN tracks obligations, not money movement. An obligation is a claim on future value, not a balance.

### Day 3 — CCL Formula + AllocationProposal (#1308 + #1311)

**#1308:** Extract the commons credit formula from kernel Rust code into a CCL PolicyOracle parameter. This is pure meaning firewall hygiene — the kernel should not contain the formula for "how much credit does a contribution earn." That's cooperative policy, not protocol mechanics.

**#1311:** Add `AllocationProposal` — a governance proposal type that, when accepted, produces a linked `AllocationReceipt`. This closes the loop: you can't allocate resources without a governance decision, and every allocation proves its authorization. The `ExecutionReceiptGate` (already done, t16) then requires an `ExecutionReceipt` to close the allocation.

### Day 4 — CI Linter (#1312) + Close Epic

**#1312:** GitHub Actions check that fails CI if the strings `CreatePaymentRequest`, `payment` (as an HTTP endpoint), or `balance` (as a gateway-exposed field) appear in the codebase. Makes the terminology discipline self-enforcing — no future PR can accidentally revert it.

After this: close epic #1302. Update `CONTRIBUTING.md` to reference the Seven Invariants checklist in the PR template.

---

## 5. Phase 2 — Vertical Slice (Weeks 2–4)

Closes epic #1147. The proof that ICN works end-to-end.

**Success criteria (non-negotiable):**
```bash
icnctl coop create --name "Harbor Homes" --charter ./charters/default.toml
# → DID created, treasury DID linked

icnctl member apply --coop did:icn:harbor-homes --identity did:icn:alice
# → application recorded

icnctl vote --proposal governance/001 --choice yes
# → vote counted, quorum check, Decision Receipt produced

icnctl audit verify --receipt did:icn:receipt/decision/001
# → AllocationReceipt linked, ExecutionReceipt linked, chain verified ✓

cargo test --test vertical_slice_integration -- --nocapture
# → PASS in CI
```

### Track Execution Order

**Track B first (Treasury & Economics, ~5 days):**

This is the critical path. S14 gave us the receipt gate and patronage tracker. Track B wires them into the complete economic loop:
- B1: Treasury-coop integration — `icnctl coop create` produces a treasury DID, treasury is owned by the coop not by any individual
- B2: Asset type foundation — type hierarchy that `Obligation` and `AllocationProposal` sit inside
- B3: Allocation receipt chain — accepted proposals produce receipts, receipts close allocations, full audit trail is provable

**Track A in parallel (Remaining Kernel/App Separation, ~4 days):**

- A1: State generalization — remaining generic state types that still have domain-specific leakage
- A2: Governance extraction — remaining governance types still directly imported by kernel
- A3: Membership consolidation — unify the dual `apps/` and `icn/apps/` membership code paths (see ADR-0010)
- A4: Kernel cleanup — remove last residual direct imports; `cargo deny` check passes clean

**Track C (Ops & Pilot Prep, ~3 days):**

- C1: Backup validation — automated test that backup → restore → verify produces identical state
- C2: Pre-deployment health check — `icnctl health --full` that validates config, identity, storage, network before first run
- C3: NAT traversal — enables the four K3s coop pods to federate with each other for real. This transforms the demo from "simulated" to "live"

**Track D (Integration, ~2 days):**

- D1: Vertical slice integration test — single `cargo test --test vertical_slice_integration` that exercises the complete chain
- D2: Update demo scripts — the four flows in `demo/scripts/` now call real `icnctl` commands instead of simulated responses. When D2 is done, `present.sh` is a live demo, not theater.

**Track E (Compute Substrate, weeks 3–4+):**

The Stage C vision from VISION.md — communities contributing compute, earning mana, spending on shared services. Runs after D1 closes.
- E1: Workload manifest schema — defines what a compute task looks like
- E2: Execution receipt completion — compute tasks produce receipts that close governance allocations
- E3: Deterministic vs. advisory classification — which compute results must be deterministic
- E4: Storage specification — how compute tasks access and commit storage
- E5: Cell operator boundary — what it means to be a "cell" (compute node) in the network
- E6: Individual node ownership mode — a node can contribute compute without joining a coop
- E7: Commons compute governance — how the mana formula works, CCL integration

After Phase 2: **tag v1.0.0**. This is the number that matters. The vertical slice integration test passing is the definition of 1.0.

---

## 6. Phase 3 — Website Overhaul (Parallel with Phase 2)

### 6.1 Stats Sync Automation

Add a nightly GitHub Actions cron to `icn-website`:

```yaml
# .github/workflows/nightly-sync.yml
on:
  schedule:
    - cron: '0 6 * * *'  # 6am UTC daily
  workflow_dispatch:
jobs:
  sync-and-deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          repository: InterCooperative-Network/icn-website
      - uses: actions/checkout@v4
        with:
          repository: InterCooperative-Network/icn
          path: icn-repo
          token: ${{ secrets.ICN_REPO_TOKEN }}
      - run: ICN_REPO=./icn-repo npm run sync
      - run: npm run build
      - uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./dist
```

After this: stats always reflect the real repo. The homepage shows 1330+ PRs, 440K+ lines of Rust, 4420+ tests. These numbers are credible. Automate showing them.

### 6.2 Blog — Four Posts (Priority Order)

**Post 1: "What ICN Is (And What It Isn't)"**
- Target: Hacker News, r/rust, r/selfhosted
- Content: Constraint engine model, meaning firewall, why not blockchain, why not a federation server
- Tone: Matt's voice — confrontational, technically precise, anti-bullshit
- Hook: "We built a daemon that runs cooperative governance. Not a DAO. Not a blockchain. A daemon."
- Key point: ICN is P2P infrastructure. Not a product for end users. Infrastructure for groups.

**Post 2: "Season 14: Closing the Economic Loop"**
- Target: Cooperative tech community, open source contributors
- Content: What the five S14 PRs accomplished — PatronageTracker (NY Corp Law §93), ExecutionReceiptGate, DelegationManager
- Include: Rust code snippets, what these concepts mean for a real cooperative
- Hook: "A cooperative's treasury shouldn't trust any one person's computer. Here's how we wired that."

**Post 3: "Institution-in-a-Box: A Live Demo"**
- Target: Cooperative organizers, summit attendees, Sovereign Tech Fund reviewers
- Content: Walk through the four demo flows as a blog post with terminal output screenshots
- This is the post shared at the March 19 summit call and in the May workshop proposal
- Hook: "Spin up a cooperative. Give it rules. Give it money flows. Give it governance."

**Post 4: "Regulatory Safety by Architecture"**
- Target: Sovereign Tech Fund application pre-read, legal-minded cooperative developers
- Content: The Seven Invariants, why "no hosted balances" is a design choice not a limitation, how the obligation model works, why ICN is not a payment rail
- Hook: "The word 'payment' doesn't appear in our codebase anymore. Here's why that matters."

### 6.3 Community Page Overhaul

**Add Matrix room:**

Matrix is aligned with cooperative values (federated, sovereign, no corporate dependency). Create `#icn-dev:matrix.org` or self-host on the homelab (Element + Synapse). The room needs to exist and be responded to. One thoughtful reply per day is enough to feel alive.

**Add "Good First Issues" section:**

Label a dozen well-scoped issues `good-first-issue` in GitHub. Surface them on the community page with:
- Issue title
- One-sentence description of what's needed
- Estimated complexity (small/medium)
- Link to the issue

Good candidates from the current issue list: documentation gaps, test coverage improvements, terminology rename work from #1303, TypeScript SDK improvements.

**Add cooperative operator mailing list:**

The cooperative movement audience doesn't live on GitHub or Matrix. Organizers use email. Set up a monthly "ICN Update" on Buttondown (free tier, no tracking):
- 200–300 words
- One concrete demo or milestone
- One call to action (test this, share this, come to the workshop)
- Link to the latest blog post

List should be linked from the community page and the blog sidebar.

### 6.4 Roadmap Page — Make It Live

Current roadmap renders `PHASE_HISTORY.md` (historical). Add a "Current Focus" section above it that renders from a small `roadmap-current.yaml` file tracked in the website repo:

```yaml
current_phase: "Vertical Slice"
current_epic: "https://github.com/InterCooperative-Network/icn/issues/1147"
stage: "A"
stage_name: "Institution-in-a-Box"
milestones:
  - title: "Season 14 complete"
    date: "2026-03-06"
    done: true
  - title: "Compliance sprint (#1302)"
    date: "2026-03-20"
    done: false
  - title: "Vertical Slice integration test"
    date: "2026-04-10"
    done: false
  - title: "v1.0.0 tag"
    date: "2026-04-15"
    done: false
```

This gives visitors an honest sense of where the project is without requiring them to dig through 1300+ GitHub issues.

---

## 7. Phase 4 — Grant Applications & Summit (April–May)

### Sovereign Tech Fund Application (Target: April)

**URL:** apply.sovereigntechfund.de
**Grant size:** €50K+ (no stated maximum), rolling deadline
**Timeline:** ~6 months to contract → April application → October funding

**Application narrative (draft framing):**

> ICN is open-source digital infrastructure for cooperative governance. It provides: verifiable identity and membership management, democratic proposal and voting mechanisms with cryptographic receipts, mutual credit and resource allocation without hosted balances, and P2P federation between groups — all without blockchain, without central servers, and without operator-held funds.
>
> ICN is not a product. It is the infrastructure layer that cooperative organizations, community groups, and democratic institutions need to run their own digital governance without depending on corporate platforms.
>
> Deployment: ICN runs on a K3s cluster serving four pilot cooperative nodes. The daemon (`icnd`) maintains local state, gossips with peers, enforces governance constraints, and produces auditable receipts for every decision. The CLI (`icnctl`) provides operator and member interfaces. The TypeScript SDK integrates with web applications.

**Required before applying:**
- Phase 1 complete (regulatory-clean language)
- At least Track B of Phase 2 in progress (treasury integration)
- Post 4 blog post published (regulatory safety)

### NY Coop Summit Workshop Proposal (May Call for Presenters)

**Workshop title:** "Institution-in-a-Box: Live Demo of P2P Cooperative Governance"
**Format:** 20-minute live walkthrough + 10 min Q&A
**Target audience:** Cooperative organizers, not developers

**Demo script outline:**
1. `icnctl coop create --name "Harbor Homes"` → watch the DID and treasury appear
2. Member onboarding: `icnctl member apply` → governance vote
3. Proposal: allocate $500 for roof repair → vote → Decision Receipt
4. Execution: receipt links to work order, closes the governance loop
5. Audit: `icnctl audit verify` → full provenance chain visible

**Required before submitting:**
- Demo flows running against real `icnctl` commands (Track D done)
- NAT traversal working (Track C3) so it can run live from a laptop

### March 19 — Summit Planning Call Prep

**Prep checklist:**
- [ ] RSVP to Joe Marraffino's Google Meet invite (currently `needsAction`)
- [ ] Prepare 2-minute "ICN tech track" pitch — what it is, what's ready, what the workshop would look like
- [ ] Bring the "Institution-in-a-Box" framing from VISION.md
- [ ] Propose: ICN as the tech infrastructure track for the October summit
- [ ] Reference: the cooperative movement already does governance on Google Docs and Signal. ICN is the infrastructure they've been missing.

---

## 8. Developer Attraction Funnel

Three channels: search, referral, community.

### Search (Hacker News + r/rust)

One strong post reaches more developers than six months of GitHub activity. The "What ICN Is (And What It Isn't)" post is engineered for this:
- Rust community has strong opinions about P2P systems
- Cooperative/tech-left community has strong opinions about platform alternatives
- Anti-blockchain positioning is explicitly stated — this is controversy that generates discussion
- Submit to HN as "Show HN: ICN — a P2P daemon for cooperative governance (not a blockchain)"

### Referral (Cooperative Tech Network)

**TechCollective (Yochai Gal + Jeff Brite):** Worker-owned MSP with 20 years of cooperative tech experience. Joe Marraffino made warm intros in December 2025. Ball is in Matt's court — Yochai and Jeff both replied, no follow-up in 85+ days. Follow up separately from any job inquiry: "Here's where ICN is, here's the demo, does this fit anything TechCollective or its members might want to evaluate?" A credibility endorsement from TechCollective to the cooperative tech ecosystem is worth more than any amount of self-promotion.

**RCC (Rochester Cooperative Collective):** Local cooperative network where Matt is already known. ICN should be on the agenda at a future RCC meeting as a 10-minute demo. Real questions from real cooperative organizers are the best product feedback available.

**CFNE + ICA + summit network:** These are the distribution channels for the cooperative movement. The blog posts, workshop proposal, and mailing list are the instruments. The summit itself (October 2026) is the highest-leverage single event.

### Community (Matrix + Good First Issues)

**Matrix room:** Needs to exist. Low traffic is fine. The goal is that someone who finds the repo can ask a question and get a response within 24 hours. That alone differentiates ICN from 90% of interesting-but-abandoned GitHub repos.

**Good first issues:** Label them correctly. Write clear descriptions. Respond to PRs within 48 hours. One developer who closes a good-first-issue and gets a thoughtful review has a better chance of becoming a contributor than 100 people who starred the repo.

**The cooperative movement as deployment pipeline:** The most powerful developer recruitment is a real user. If Harbor Homes Cooperative runs ICN for their governance, that story — told in a blog post — attracts developers who want to build things that are used.

---

## 9. Timeline Summary

| Date | Milestone |
|---|---|
| 2026-03-12 (today) | Phase 0: merge demo branch, fix labels, mark #1302 done items, sync website |
| 2026-03-13–16 | Phase 1 compliance sprint (#1303, #1304, #1305, #1306, #1308, #1311, #1312) |
| 2026-03-17 | Close epic #1302. Begin Phase 2 Track B + Track A |
| 2026-03-19 | **Summit planning call** (2pm EDT). RSVP required. Pitch ICN tech track. |
| 2026-03-20–31 | Phase 2 Tracks B + A + C. Blog post 1 published. Matrix room created. |
| 2026-03-31 | **Grant deadlines**: Outta Excuses ($500–3K) + Verizon Digital Ready ($10K) |
| 2026-04-01–10 | Phase 2 Track D (integration test, real demo flows). Blog posts 2 + 3 published. |
| 2026-04-10 | Vertical slice integration test passing in CI |
| 2026-04-15 | **Tag v1.0.0**. Blog post 4 published. Sovereign Tech Fund application submitted. |
| 2026-05-01 | **Call for presenters opens**. ICN workshop proposal submitted. |
| 2026-04–06 | Phase 2 Track E (Compute Substrate). Website nightly sync live. |
| 2026-06 | Shoulder replacement follow-up (~9 months post-op → 12 months) |
| 2026-10-10/17 | **NY Coop Summit** — ICN workshop, live demo |
| 2026-10 | Sovereign Tech Fund contract expected (~6 months from April application) |

---

## 10. Open Issues Reference

### Must-do before STF application
- [ ] #1303 Rename payment→settlement, currency→unit, balance→position
- [ ] #1304 UX language guide + Seven Invariants PR checklist
- [ ] #1305 JournalEntry.provenance required
- [ ] #1306 Obligation type with lifecycle
- [ ] #1308 CCL formula extraction
- [ ] #1311 AllocationProposal
- [ ] #1312 Compliance CI linter

### Vertical slice critical path
- [ ] #1139 Treasury-coop integration (B1)
- [ ] #1140 Asset type foundation (B2)
- [ ] #1141 Allocation receipt chain (B3)
- [ ] #1145 Vertical slice integration test (D1)
- [ ] #1146 Demo script update to real calls (D2)

### Compute substrate (post v1.0)
- [ ] #1128 Workload manifest schema (E1)
- [ ] #1129 Execution receipt completion (E2)
- [ ] #1130 Deterministic vs advisory classification (E3)
- [ ] #1131 Storage specification (E4)
- [ ] #1132 Cell operator boundary (E5)
- [ ] #1133 Individual node ownership mode (E6)
- [ ] #1134 Commons compute governance (E7)

### Housekeeping
- [ ] #1329 Merge dependabot TS SDK bump (needs `sdk` + `typescript` labels)
- [ ] ADR-0010 App topology cleanup (dual apps/ roots)
- [ ] Periodic `cargo clean` in icn-wt worktrees (currently 55G)

---

*Document generated 2026-03-12. Update this file when phase completions occur. Canonical sources: GitHub issues for task state, WORKLOG.md for session activity, TASKS.md for personal deadlines.*
