# Capstone: Maintainer Demonstration

> **Status**: Final gate to **Maintainer** tier  
> **Required**: Complete all of Phase 4  
> **Time estimate**: 1-2 weeks

## Goal

Demonstrate end-to-end mastery by deploying a two-node ICN network, exercising all major subsystems, and proving observability/persistence.

## Prerequisites

- Completed Phases 1-4 and all gates
- Built ICN binaries (`cargo build --release`)
- Access to Kubernetes cluster OR Docker Compose environment

## Part 1: Deployment (30 points)

**Task**: Deploy ICN in production configuration.

**Requirements**:
1. Two nodes (alpha, beta) with persistent storage
2. Nodes discover each other via mDNS or bootstrap peers
3. Gateway API enabled on at least one node
4. Metrics endpoint exposed on both nodes
5. Logs structured and queryable

**Verification**:
- `icnctl network peers` shows both nodes connected
- Gateway health endpoint returns 200 OK
- Prometheus `/metrics` endpoint returns ICN metrics

**Artifact**: Screenshot of running pods/containers + metrics output.

---

## Part 2: Ledger Flow (20 points)

**Task**: Execute a ledger transaction via Gateway API.

**Steps**:
1. Get JWT token via `icnctl auth` flow (or Gateway API)
2. Create a cooperative via `POST /v1/coop`
3. Add members to the cooperative
4. Post a ledger payment via `POST /v1/ledger/entries`
5. Verify balance via `GET /v1/ledger/balance/{did}`

**Verification**:
- Entry accepted (not quarantined)
- Balance reflects the payment
- Entry visible on both nodes (gossip replication)

**Artifact**: curl commands + JSON responses showing the flow.

---

## Part 3: Governance Flow (20 points)

**Task**: Submit and activate a governance proposal.

**Steps**:
1. Create proposal to change protocol parameter (e.g., `max_proposal_size`)
2. Cast votes via `POST /v1/gov/proposals/{id}/vote`
3. Wait for voting period to end
4. Verify proposal activated
5. Query new parameter value via `GET /v1/gov/parameters/{name}`
6. Verify oracle uses new value (log tracing or test constraint change)

**Verification**:
- Proposal transitions: Draft → Proposed → Active
- Parameter value changed
- Oracle constraints reflect new value

**Artifact**: Proposal lifecycle log + parameter query showing new value.

---

## Part 4: Metrics Verification (15 points)

**Task**: Prove system activity visible in metrics.

**Requirements**:
- Query Prometheus `/metrics` endpoint
- Identify these metrics with non-zero values:
  - `gossip_entries_total`
  - `ledger_entries_total`
  - `network_connections_active`
  - `governance_proposals_total`

**Verification**: Metrics output showing activity from Parts 2-3.

**Artifact**: Metrics output with highlighted relevant counters.

---

## Part 5: Persistence Verification (15 points)

**Task**: Simulate node restart and verify state recovery.

**Steps**:
1. Export gossip state: `icnctl gossip export > state.json`
2. Stop node alpha
3. Start node alpha (should restore from persistent storage)
4. Verify ledger balance unchanged
5. Verify gossip entries unchanged
6. Verify node reconnects to peers

**Verification**:
- No data loss after restart
- Gossip state matches pre-restart
- Ledger balance matches pre-restart

**Artifact**: Before/after state comparison showing persistence.

---

## Grading Rubric

| Component | Points | Criteria |
|-----------|--------|----------|
| Deployment | 30 | Nodes running, discoverable, metrics exposed |
| Ledger Flow | 20 | Transaction accepted, replicated, balanced |
| Governance Flow | 20 | Proposal activated, parameter changed, oracle updated |
| Metrics | 15 | Activity visible in Prometheus metrics |
| Persistence | 15 | State survives restart, no data loss |
| **Total** | **100** | **Pass: 80+ points** |

---

## After Passing

**Congratulations!** You are now a **Maintainer**.

You can:
- ✅ Review kernel/app boundary PRs for invariant safety
- ✅ Design and implement architectural changes
- ✅ Mentor contributors through onboarding
- ✅ Own subsystems and shepherd their evolution
- ✅ Participate in architecture decisions

**Next step**: Take ownership of a subsystem or mentor new contributors!

---

## Submission

Submit all five artifacts to your mentor or in the #maintainers channel:
1. Deployment proof (screenshots, config files)
2. Ledger flow (curl + JSON responses)
3. Governance flow (proposal lifecycle log)
4. Metrics verification (Prometheus output)
5. Persistence verification (before/after state)

Once reviewed and approved, you'll receive the **Maintainer** badge and be invited to the maintainers channel.

---

## Need Help?

- **Deployment issues?** → `reference/module-09-ops-deploy.md`
- **API confusion?** → `docs/api/openapi.generated.yaml`
- **Governance unclear?** → `reference/module-14-governance-ccl-deep-dive.md`

This is the final test — take your time and demonstrate mastery!
