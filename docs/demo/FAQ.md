# ICN FAQ & Talking Points

**For**: Demo Q&A, investor meetings, cooperative conversations
**Last Updated**: 2026-02-11

---

## The Basics

### What is ICN?

**Short**: Infrastructure for the cooperative economy - identity, governance, and value exchange without corporate dependencies.

**Medium**: ICN is a peer-to-peer coordination layer that lets cooperatives manage member identities, make democratic decisions, and exchange value - all on infrastructure they collectively own and control.

**Technical**: A Rust daemon implementing decentralized identity (DIDs), trust-weighted gossip, mutual credit accounting, and democratic governance primitives.

### What problem does it solve?

Today, cooperatives depend on corporate tools:
- Slack for communication
- QuickBooks for accounting
- Google Docs for governance
- Stripe for payments

Each dependency means:
- Data leaving cooperative control
- Terms of service they can't negotiate
- Prices that can increase without consent
- Services that can be discontinued

ICN replaces these with cooperative-owned infrastructure.

### Who is it for?

- **Worker cooperatives** managing hours and revenue sharing
- **Tool libraries** tracking lending and contributions
- **Housing cooperatives** handling governance and fees
- **Food cooperatives** managing member credits
- **Any organization** wanting democratic, transparent operations

---

## Technical Questions

### How is this different from blockchain?

| | ICN | Blockchain |
|--|-----|------------|
| **Consensus** | Local-first | Global |
| **Sovereignty** | Each coop controls their data | One chain rules all |
| **Fees** | None | Gas/mining fees |
| **Trust model** | Social (web of trust) | Trustless (adversarial) |
| **Energy** | Minimal | Proof of work is expensive |
| **Speed** | Instant local, eventual sync | Block time delays |

**The key insight**: Cooperatives don't need trustless consensus. They already trust each other - they're cooperatives. ICN amplifies that trust instead of fighting it.

### What happens if a node goes down?

**Short-term**: The cooperative continues operating. Data syncs when the node comes back.

**Long-term**: Backup to another node or restore from snapshot.

**Federation**: If multiple coops are federated, peer nodes can help with recovery.

ICN is designed for resilience, not perfect uptime. Cooperatives are patient institutions.

### How do nodes sync?

**Gossip protocol**: Nodes push announcements and pull missing data.

**Vector clocks**: Track causal ordering to prevent duplicate processing.

**Anti-entropy**: Periodic Bloom filter exchange to find gaps.

**Trust-weighted**: Prioritize sync with higher-trust peers.

### Is the data encrypted?

**In transit**: Yes, QUIC/TLS with certificate pinning.

**At rest**: Keystore is encrypted with passphrase. Ledger data is signed but readable to cooperative members.

**End-to-end**: Optional for private messages.

### What about post-quantum cryptography?

**Current**: Ed25519 signatures (not quantum-resistant).

**Roadmap**: Hybrid Ed25519 + ML-DSA (Dilithium) available as opt-in feature.

**Philosophy**: Defense in depth - both algorithms must be broken.

---

## Governance Questions

### How do cooperatives make decisions?

**Proposals**: Any member can submit (text, budget, membership, config).

**Voting**: Members vote directly or delegate to trusted representatives.

**Execution**: Approved proposals automatically take effect.

**Transparency**: All proposals and votes are recorded on the ledger.

### Can voting be anonymous?

**Default**: Votes are transparent (you can see who voted how).

**Optional**: Anonymous voting can be configured for sensitive decisions.

**Tradeoff**: Transparency enables accountability; anonymity protects against social pressure.

### What if members disagree?

**Governance**: The democratic process resolves disagreement.

**Disputes**: Mediation mechanisms with trust-weighted arbiters.

**Exit**: Members can leave cooperatives (and data portability is supported).

### Who controls the software?

**Open source**: Anyone can inspect, modify, or fork the code.

**Governance**: ICN development follows cooperative principles.

**No lock-in**: Cooperatives can run their own nodes, choose their federation partners, and migrate data.

---

## Economic Questions

### What is mutual credit?

**Simple**: Members exchange value without money.

**Example**: Alice does 2 hours of work for Bob. Alice gets +2 credit, Bob gets -2. Later, Bob does work for Carol. The credits circulate.

**Key property**: No external currency needed. The cooperative is its own monetary system.

### How do credit limits work?

**Configurable**: Each cooperative sets policies.

**Example**: New members might start with ±10 hour limit. Trusted members might have ±100.

**Governance**: Credit limits can be adjusted by proposal and vote.

### Can cooperatives exchange value?

**Federation**: Yes, cooperatives can establish bilateral or multilateral exchange agreements.

**Settlement**: Cross-cooperative transactions settle according to federation agreements.

**Sovereignty**: Each cooperative decides who to federate with and on what terms.

### Is this a cryptocurrency?

**No**. There's no coin, no mining, no speculation.

**Mutual credit** is a bookkeeping system, not a speculative asset.

**The value** comes from real goods and services exchanged between members.

---

## Adoption Questions

### Is this ready for production?

**Pilot phase**: Real cooperatives are testing it now.

**Code maturity**: 2,000+ tests passing, core infrastructure solid.

**Focus now**: Usability, documentation, and expanding pilots.

### How do we get started?

**Try it**: Run the demo (`./demo/scripts/run-tool-library-demo.sh`).

**Pilot**: Contact us about running a pilot with your cooperative.

**Contribute**: Check the code, open issues, submit PRs.

### What's the business model?

**ICN is infrastructure**, not a company.

**No fees**: Running a node is free.

**Support**: Organizations may offer paid support, hosting, or customization.

**Sustainability**: Like other cooperative infrastructure, supported by the cooperatives using it.

### What about integration with existing tools?

**API**: REST + WebSocket gateway for integration.

**SDK**: TypeScript SDK for building applications.

**Import**: Data migration tools for common formats.

**Hybrid**: Cooperatives can use ICN alongside existing tools during transition.

---

## Skeptical Questions

### Isn't this just reinventing databases?

**No**. Databases require a trusted operator. ICN operates across trust boundaries without a central authority.

**The hard problem** isn't storage - it's coordination without hierarchy.

### Why would cooperatives switch from working tools?

**They might not** - and that's okay. ICN is for cooperatives who:
- Want to own their infrastructure
- Are concerned about corporate dependencies
- Value transparency and democratic control
- Are building for the long term

### What if the core developers disappear?

**Open source**: Code is public, forkable, maintainable.

**Documentation**: Architecture and design decisions are documented.

**Community**: Multiple people understand the codebase.

**Simplicity**: Intentionally minimal dependencies.

### How is this different from previous attempts?

**Focus**: We're building infrastructure, not an app or a platform.

**Patience**: Designed for decades, not hockey-stick growth.

**Pragmatism**: Working with real cooperatives, not theoretical ideals.

---

## Quick Soundbites

**For elevator pitch**:
> "Own your cooperative's infrastructure the way you own your business."

**For tech audience**:
> "Peer-to-peer coordination with social trust instead of blockchain consensus."

**For cooperative audience**:
> "Democratic governance, mutual credit, and member identity - all under your control."

**For funders**:
> "Patient infrastructure that compounds as the cooperative economy grows."

---

## Resources

- **Architecture**: `docs/ARCHITECTURE.md`
- **Demo Script**: `docs/demo/DEMO_SCRIPT.md`
- **Getting Started**: `docs/GETTING_STARTED.md`
- **Vision**: `VISION.md`
