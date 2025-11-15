# Minimal Viable Coop (MVC) Track

**Goal**: Ship one end-to-end cooperative use case that works in production for 6 months

**Target**: 10-person worker cooperative using ICN for governance + mutual credit

**Timeline**: 13 weeks (3 months) from start to pilot deployment

**Success Metric**: Real cooperative (not ICN developers) operates for 6 months without catastrophic failure

---

## Why MVC?

ICN is a **promising prototype** with solid technical foundations, but significant gaps remain between "daemon runs" and "cooperatives can use this." The MVC track closes those gaps by focusing ruthlessly on **one real use case** rather than polishing individual components in isolation.

**Current status** (per gap-analysis.md):
- ✅ Core substrate works (identity, trust, gossip, ledger, contracts, gateway)
- 🚧 Production gaps exist (recovery, NAT, conflicts, UX, ops)
- ❌ No complete end-to-end flow from "install" to "operate for months"

**MVC approach**:
1. Pick ONE specific cooperative use case
2. Close ONLY the gaps blocking that use case
3. Deploy with a real pilot cooperative  
4. Learn from real-world operation
5. Use learnings to prioritize next work

---

## MVC Scope: Worker Cooperative Use Case

**Pilot Profile**: 10-person worker-owned tech cooperative

**Core Workflows**:
1. **Onboarding**: New member joins, creates identity, gets vouched in
2. **Governance**: Propose and vote on decisions (e.g., "hire contractor X")
3. **Economic**: Track time contributions, exchange mutual credit hours
4. **Operations**: Backup, recover from failures, upgrade without downtime

**Out of Scope** (defer until after pilot):
- Advanced governance (delegation, liquid democracy, appeals)
- Complex economic patterns (multi-currency, demurrage, cross-ledger)
- Federation across multiple cooperatives
- Mobile apps, fancy UIs
- Scale beyond 50 members

---

## Required Work (13 weeks)

### Week 1-2: Identity Recovery (Tier 1 Gap #1)

**Problem**: Users WILL lose devices/keys. No recovery = catastrophic data loss.

**Solution**: Social recovery pattern (M-of-N trusted friends)

**Work**:
1. Extend DID Document with recovery trustees
2. Implement `icnctl identity recover` flow:
   - User requests recovery (proves identity via trustees)
   - M trustees sign recovery attestation
   - New key authorized, old key revoked
3. UI for configuring recovery (who are your trustees? how many needed?)
4. Test: Lose device, recover via social recovery, regain full access

**Deliverables**:
- Recovery trustee system in `icn-identity`
- CLI recovery flow in `icnctl`
- Documentation: "How to set up recovery for your coop"
- Tests: Full recovery flow (lose device → trustees help → back online)

**Success**: Member can lose laptop, recover identity same day with help from 3 colleagues

---

### Week 3-4: NAT Traversal (Tier 1 Gap #2)

**Problem**: Real cooperatives ARE behind NAT/firewalls. Can't require public IPs.

**Solution**: STUN/TURN support for NAT hole-punching + relay

**Work**:
1. Integrate STUN client into `icn-net`:
   - Discover public IP/port
   - Attempt direct connection
   - Fall back to TURN relay if needed
2. Add relay node capability:
   - Node can volunteer as relay
   - Relay permissions via trust graph
3. Configure bootstrap nodes as relays
4. Test: Two nodes behind NAT connect via relay

**Deliverables**:
- STUN/TURN integration in `icn-net`
- Relay node configuration
- Public bootstrap relay nodes (icn.coop infrastructure)
- Documentation: "Connecting behind firewalls"

**Success**: Members with home routers can connect without port forwarding

---

### Week 5-6: Ledger Conflict Resolution (Tier 1 Gap #3)

**Problem**: Network partitions WILL happen. Need deterministic conflict resolution.

**Solution**: Last-write-wins with trust-weighted voting for conflicts

**Work**:
1. Define conflict detection:
   - Same account, different balances across nodes
   - Merkle root mismatch for same entry ID
2. Implement resolution policy:
   - Trust-weighted majority wins
   - Quarantine minority view
   - Notify affected parties
3. Add conflict monitoring:
   - Prometheus metric: `ledger_conflicts_detected_total`
   - Alert when conflicts occur
4. Test: Partition network, create conflicting transactions, verify resolution

**Deliverables**:
- Conflict detection in `icn-ledger`
- Trust-weighted resolution policy
- Conflict monitoring and alerting
- Documentation: "Understanding ledger conflicts"

**Success**: Network partition resolves automatically without human intervention

---

### Week 7: Trust Lifecycle (Tier 2 Gap, needed for MVC)

**Problem**: Trust graph is static. Real trust evolves with interactions.

**Solution**: Simple trust decay + transaction-based trust growth

**Work**:
1. Implement time-based decay:
   - Trust decays 5% per month of inactivity
   - Configurable per cooperative
2. Implement transaction-based growth:
   - Successful mutual credit transaction → +0.05 trust
   - Capped at 0.8 (never full auto-trust)
3. Add trust audit log:
   - "Why is my trust 0.6?" → shows events
4. Test: Inactive member's trust decays, active trading increases trust

**Deliverables**:
- Trust decay policy in `icn-trust`
- Transaction hooks for trust updates
- Trust audit log
- Documentation: "How trust works in your coop"

**Success**: Trust reflects actual cooperative relationships, not just initial setup

---

### Week 8: Governance Templates (Tier 2 Gap, needed for MVC)

**Problem**: Every coop needs votes/quorum. No one should write contracts from scratch.

**Solution**: 3 battle-tested governance contract templates

**Work**:
1. **Simple majority vote** contract:
   - Propose, vote yes/no, >50% wins
   - Time-bounded (7 days default)
2. **Quorum vote** contract:
   - Requires N% participation
   - Then simple majority of participants
3. **Budget approval** contract:
   - Propose spending, requires 2/3 majority
   - Auto-executes ledger transfer if passes
4. Add contract deployment UI to gateway
5. Test: Deploy template, run vote, verify execution

**Deliverables**:
- 3 contract templates in `icn-ccl/templates/`
- Template deployment API in gateway
- Template instantiation CLI: `icnctl governance deploy simple-vote --duration 7d`
- Documentation: "Governance patterns" with examples

**Success**: Coop can hold binding vote in <5 minutes without writing code

---

### Week 9-10: Onboarding UX (Tier 1 Gap #4)

**Problem**: "Install ICN" is 20 CLI commands and scary errors. Humans need simplicity.

**Solution**: Guided onboarding wizard for new cooperatives

**Work**:
1. **Coop creation wizard**:
   - `icnctl coop init "My Worker Coop"`
   - Walks through: name, governance model, currency, founding members
   - Generates initial DID, trust graph, governance contracts
2. **Member invitation flow**:
   - `icnctl coop invite alice@example.com`
   - Sends invitation link
   - Recipient clicks link, downloads ICN, auto-configures
3. **Visual trust builder**:
   - Simple web UI: "Who do you trust?"
   - Drag trust slider, suggests reasonable defaults
4. **Credit explainer**:
   - Interactive tutorial: "What is mutual credit?"
   - Simulation: Make fake transactions, see balances change
5. Test: Non-technical user completes onboarding in <30 minutes

**Deliverables**:
- `icnctl coop init` wizard
- Member invitation system (email + link)
- Basic web UI for trust + credit explanation
- Onboarding documentation with screenshots

**Success**: Non-developer can join coop and understand system in 30 minutes

---

### Week 11: Operations Playbooks (Tier 1 Gap #5)

**Problem**: Things WILL go wrong. Need runbooks for common incidents.

**Solution**: Comprehensive operational documentation + tooling

**Work**:
1. **Deployment guide**:
   - "How to run ICN for a 10-person coop"
   - Recommended: 3 always-on nodes + per-member mobile apps
   - Docker Compose setup for quick start
2. **Backup strategy**:
   - Automated daily backups via `icnctl backup`
   - Encrypted backup to S3/local storage
   - Tested restore procedure
3. **Upgrade procedure**:
   - Rolling upgrade (keep 2/3 nodes running)
   - Data migration for breaking changes
   - Rollback procedure if upgrade fails
4. **Incident runbooks**:
   - "Node crashed" → How to restart, check for data loss
   - "Network partition" → How to verify ledger consistency
   - "Lost member key" → Social recovery procedure
   - "Suspicious transaction" → Dispute resolution flow

**Deliverables**:
- `docs/operations/deployment-guide.md`
- `docs/operations/backup-restore.md`
- `docs/operations/upgrade-procedure.md`
- `docs/operations/incident-runbooks.md`
- Docker Compose file for quick deployment

**Success**: Coop admin can handle common incidents without contacting ICN devs

---

### Week 12: Integration Testing & Polish

**Problem**: Individual components work, but end-to-end flow might be broken.

**Solution**: Full integration test suite + UX polish

**Work**:
1. **End-to-end test scenarios**:
   - New coop: 10 members onboard, vote, transact
   - Device loss: Member loses laptop, recovers via social recovery
   - Network partition: Nodes split, later reconcile
   - Node failure: 1 node crashes, others continue operating
   - Upgrade: Deploy new version without downtime
2. **UX polish**:
   - Fix confusing error messages
   - Add progress indicators to long operations
   - Improve CLI help text
   - Visual polish on web UI
3. **Performance testing**:
   - 100 transactions in 1 hour
   - 50 concurrent votes
   - 1000+ gossip messages/minute

**Deliverables**:
- Integration test suite (10+ scenarios)
- Performance benchmarks
- UX improvements across CLI + web
- Test report: "All scenarios pass"

**Success**: End-to-end flow works smoothly without manual intervention

---

### Week 13: Pilot Deployment

**Problem**: Real cooperative needs to validate this actually works.

**Solution**: Deploy with pilot cooperative, support their first 30 days

**Work**:
1. **Pilot selection**:
   - 10-person worker cooperative (ideally tech-adjacent for early feedback)
   - Committed to 6-month trial
   - Willing to provide weekly feedback
2. **Deployment**:
   - Install ICN on pilot's infrastructure
   - Walk through onboarding for all members
   - Configure governance contracts
   - Initialize mutual credit ledger
3. **Support**:
   - Weekly check-in calls
   - Monitor metrics and logs
   - Rapid bugfix turnaround
   - Document all issues and resolutions
4. **Measurement**:
   - Track: Uptime, transactions, votes, conflicts, recoveries
   - Collect: User feedback, pain points, feature requests

**Deliverables**:
- Pilot cooperative operational on ICN
- Weekly status reports
- Issue tracker: bugs, requests, learnings
- Success metrics dashboard

**Success**: Pilot operates for 1 month without catastrophic failure

---

## Success Criteria (End of 13 Weeks)

**Technical**:
- ✅ Social recovery works (tested with real device loss)
- ✅ NAT traversal works (members connect from home)
- ✅ Ledger conflicts resolve automatically
- ✅ Governance templates deploy and execute
- ✅ End-to-end tests pass (10+ scenarios)

**Operational**:
- ✅ Pilot coop deployed and running
- ✅ 100+ transactions recorded
- ✅ 10+ governance decisions made
- ✅ Zero data loss events
- ✅ All incidents resolved within 24 hours

**User Experience**:
- ✅ New member onboards in <30 minutes
- ✅ Non-technical members can use basic features
- ✅ Trust and credit concepts are understood
- ✅ No member locked out due to lost key

**Community**:
- ✅ Pilot coop commits to 6-month trial
- ✅ Weekly feedback collected
- ✅ At least 3 feature requests from real use
- ✅ Documented learnings inform next priorities

---

## What Happens After MVC?

**If pilot succeeds** (operates for 6 months):
1. **Phase 13**: Implement top 3 feature requests from pilot
2. **Phase 14**: Add 2-3 more pilot cooperatives (diversity of use cases)
3. **Phase 15**: Generalize learnings into stable platform

**If pilot struggles**:
1. Root cause analysis: What failed? Why?
2. Fix critical failures (data loss, unusability, etc.)
3. Re-attempt with adjusted scope

**If pilot fails**:
1. Honest retrospective: What did we learn?
2. Re-evaluate: Is ICN solving a real problem?
3. Pivot or persist based on evidence

---

## Resource Requirements

**Engineering**:
- 1 full-time developer for 13 weeks
- OR 2 developers for 6-7 weeks
- Mix of: Rust backend, basic web frontend, ops/devops

**Pilot Cooperative**:
- 10 members willing to be early adopters
- 1 technical admin for daily operations
- Commitment to weekly feedback

**Infrastructure**:
- 3 always-on nodes (cloud VMs: $30/month)
- 1 bootstrap relay node (cloud VM: $10/month)
- S3 bucket for backups ($5/month)
- **Total: ~$45/month for 6-month pilot = $270**

**Time Commitment**:
- Week 1-12: Development + testing
- Week 13: Pilot deployment + onboarding
- Months 2-6: Ongoing support (10 hours/week)

---

## Risks & Mitigations

**Risk**: Pilot coop drops out before 6 months
- **Mitigation**: Select committed coop, provide excellent support, have backup pilot lined up

**Risk**: Critical bug discovered in production
- **Mitigation**: Rapid bugfix process, backup/restore ready, clear rollback procedure

**Risk**: NAT traversal doesn't work in pilot's network
- **Mitigation**: Public bootstrap relay available, VPN fallback, manual port forwarding as last resort

**Risk**: Users find UX too confusing
- **Mitigation**: Onboarding wizard, documentation, weekly support calls, iterate based on feedback

**Risk**: 13 weeks isn't enough time
- **Mitigation**: Ruthlessly cut scope, defer nice-to-haves, focus on core workflows

---

## Conclusion

The MVC track is **the critical path** to proving ICN works for real cooperatives. It closes the gap between "interesting prototype" and "deployable infrastructure" by focusing on **one complete use case** rather than incomplete features across many.

**Bottom line**: 13 weeks, $270, one committed cooperative. If we can't make that work, we learn why. If we can, we have a foundation to build on.

**Next step**: Find the pilot cooperative.

---

**Document created**: 2025-01-15  
**Target start**: When pilot cooperative identified  
**Target completion**: 13 weeks after start
