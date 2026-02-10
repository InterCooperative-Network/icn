# Development Journal: Strategic Pivot to Platform Layer

**Date**: 2025-01-15
**Phase**: Strategic Pivot - Platform Layer (Phase 14)
**Context**: Post-Phase 12 strategic reassessment

## The Insight

**User feedback**: "I want co-ops to be able to run their own app on the ICN infra."

This triggered a fundamental reframing:

**OLD MODEL**: "People run ICN" - cooperatives interact with `icnd` and `icnctl` directly

**NEW MODEL**: "Co-ops run apps that quietly sit on ICN" - cooperatives build their own apps (React + Node, whatever), use ICN under the hood

## The Food Co-op Example

A food co-op builds their "Shopper's Club" app:
- Phase 1: Use ICN for member identity + trust anchor
- Phase 2: Add "loyalty points" (mutual credit under the hood)
- Phase 3: Accept ICN credits for payment at checkout

**Key insight**: Members never see `icnd` or `icnctl`. They just log in to the Shopper's Club and use it.

## What This Requires

ICN needs to grow from **substrate daemon** → **cooperative backend platform**.

### Layer Cake

```
Apps (what co-ops build)
  ↕
Platform Layer (what we build now) ← THE GAP
  ↕
Substrate (what we have)
```

### Platform Layer Components

1. **icn-gateway** - REST + WebSocket API server
2. **Coop Namespaces** - Isolation (each co-op = separate space)
3. **App Auth** - Capability tokens (OAuth-ish for co-ops)
4. **TypeScript SDK** - `@icn/client` npm package
5. **Reference App** - "Food Co-op Shopper's Club" demo

## Strategic Alignment

This directly addresses Gap Analysis findings:

- **Gap #3** (Client Layer/SDK) - 🔴 Critical Path → Addressed by TS SDK
- **Gap #15** (UX of Cooperation) - 🔴 Critical Path → Addressed by reference app
- **Gap #12** (Onboarding Flows) - 🔴 Critical Path → Addressed by app auth flow
- **Gap #14** (Observability UX) - 🚧 Partial → Addressed by event streaming

This is the missing piece between substrate and deployment.

## Decision Point

**What I was doing**: Phase 13 (Governance Primitives)
- Adding CCL governance functions (proposal_create, proposal_vote, etc.)
- Building template contracts
- Useful, but substrate-level

**What I'm now doing**: Phase 14 (Platform Layer)
- Building the layer that makes ICN *usable* by cooperative apps
- This is what unlocks real adoption

**Governance work status**:
- ✅ Governance types added to `icn-ccl/src/types.rs` (Proposal, Vote, ProposalStatus, MemberRole)
- ✅ `icn-ccl/src/governance.rs` created (GovernanceManager with 6 tests passing)
- ⏸️ Paused: CCL interpreter extensions, template contracts, icnctl commands
- **Decision**: Keep governance foundation, pivot to platform layer

Governance is still important, but it's a substrate primitive. Platform layer is what makes governance *accessible* to apps.

## Platform Layer Design

Created comprehensive design doc: `docs/platform-layer-design.md`

### API Surface

**Auth:**
- `POST /auth/challenge` - Generate auth challenge
- `POST /auth/verify` - Verify signature, issue token
- `POST /auth/revoke` - Revoke token

**Co-ops:**
- `POST /coops` - Create co-op namespace
- `GET /coops/{coopId}` - Get co-op details
- `GET /coops/{coopId}/members` - List members
- `POST /coops/{coopId}/members` - Add member

**Ledger:**
- `GET /ledger/{coopId}/balance/{did}` - Get balance
- `POST /ledger/{coopId}/payment` - Send payment
- `GET /ledger/{coopId}/history/{did}` - Transaction history

**Events:**
- `WS /events/subscribe` - WebSocket event stream
- Event types: transaction, member_joined, member_left, role_changed, etc.

### TypeScript SDK Example

```typescript
import { ICNClient } from '@icn/client';

const icn = new ICNClient({ gatewayUrl: 'http://localhost:8000' });

// Auth
const user = await icn.auth.login({ did, signChallenge });
const token = await icn.auth.getToken({ coopId, appId, capabilities });

// Ledger
const balance = await icn.ledger.getBalance({ coopId, did, currency });
const tx = await icn.ledger.sendPayment({ coopId, from, to, amount, currency });

// Events
const stream = icn.events.subscribe({ coopId, eventTypes });
stream.on('transaction', (event) => { /* update UI */ });
```

### Coop Namespace Model

```rust
pub struct CoopNamespace {
    pub id: CoopId,
    pub name: String,
    pub members: Vec<Did>,
    pub roles: HashMap<Did, Vec<String>>,
    pub currencies: Vec<String>,
    pub contracts: Vec<ContentHash>,
    pub apps: Vec<AppRegistration>,
}
```

Each co-op is isolated:
- Own member list
- Own role bindings
- Own ledger domains
- Own contracts
- Apps authorized per-co-op

### App Auth Model

Capability tokens (JWT-ish):
```json
{
  "sub": "did:icn:abc123",
  "coop": "food-coop-pdx",
  "app": "shoppers-club",
  "capabilities": ["read:balance", "write:payment"],
  "exp": 1234567890
}
```

Apps get scoped tokens to act on behalf of users. Not OAuth (no third-party IdP), but similar developer experience.

## Implementation Plan

**5 Milestones, ~8 weeks total**:

### Milestone 1: Gateway + Namespaces (2 weeks)
- Create `icn-gateway` crate (Actix-web)
- Auth endpoints (challenge, verify, revoke)
- Coop namespace storage (extend icn-store)
- Coop CRUD endpoints
- Capability token middleware

### Milestone 2: Ledger API (1 week)
- Ledger endpoints (balance, payment, history)
- Ledger scoping per co-op namespace
- Test with curl/Postman

### Milestone 3: Events (1 week)
- WebSocket event streaming
- Hook ledger operations to emit events
- Hook membership operations to emit events
- Real-time update tests

### Milestone 4: TypeScript SDK (2 weeks)
- `sdk/typescript` package
- `ICNClient` class
- Auth, coops, ledger, events modules
- Comprehensive tests
- Publish to npm as `@icn/client`

### Milestone 5: Reference App (2 weeks)
- "Tiny Food Co-op Shopper's Club" (Next.js)
- Features: login, balance, payment, history, live events
- Deploy as demo

## Technical Decisions

### Why Actix-web for Gateway?
- Stay in Rust ecosystem
- Async/await support
- WebSocket support built-in
- Can talk to `icnd` via internal RPC (already using gRPC in `icn-rpc`)

### Why JWT-ish Tokens?
- Familiar to app developers
- Stateless verification (gateway doesn't need to call icnd on every request)
- Can include capabilities in claims
- Easy to revoke via revocation list

### Why TypeScript SDK First?
- Web apps are most common starting point
- JavaScript/TypeScript is ubiquitous
- Can generate from OpenAPI spec later
- Python SDK can follow same pattern

### Why Namespaces vs. Multi-Tenancy?
- Strong isolation between co-ops
- Each co-op can set own policies
- Prevents cross-co-op data leaks
- Maps to real-world cooperative boundaries

## Open Questions

1. **Token Lifetime**: 1 hour? 24 hours? Refresh flow?
2. **Multi-Coop Users**: Can one DID be in multiple co-ops? (Yes, but token structure?)
3. **Rate Limiting**: Per-app, per-user, or per-co-op?
4. **Governance in v1**: Should gateway expose governance endpoints now or phase 2?
5. **Event Privacy**: Can apps subscribe to events for all DIDs or just token holder?

## Success Criteria

**For app developers**:
- Can build working app in < 1 day
- Doesn't need to understand ICN substrate internals
- SDK docs answer 90% of questions
- Reference app is copy-paste starting point

**For co-op members**:
- Sign in with wallet (eventually phone biometric)
- See balance, send payment, view history
- Updates are instant (WebSocket)
- Never see "daemon" or "CLI"

**For ICN**:
- Platform layer is stable API boundary
- Substrate can evolve without breaking apps
- Multiple co-ops per ICN node (via namespaces)
- Apps don't feel "blockchain-y"

## Relation to Roadmap

**Original Roadmap**:
- ✅ Phases 1-12 complete (substrate)
- ⏳ Phase 13: Governance Primitives (speculative)
- ⏳ Track C1: Community Selection
- ⏳ Track C2: Pilot MVP

**Revised Roadmap**:
- ✅ Phases 1-12 complete (substrate)
- 🆕 **Phase 14: Platform Layer** (THIS) - enables everything else
- Phase 13: Governance (can be built on platform layer)
- Track C1: Community Selection (can now offer "build your app on ICN")
- Track C2: Pilot MVP (reference app is starting point)

**Key Realization**: Platform layer was the missing piece. Without it:
- Track C2 would require custom app for each pilot
- No reusable components
- Every co-op would reinvent auth, ledger API, event streaming

With platform layer:
- Track C2 becomes "customize reference app for pilot community"
- Other co-ops can self-serve (build their own apps)
- ICN becomes a *platform*, not just infrastructure

## Governance Work Disposition

**Completed**:
- Governance types (Proposal, Vote, ProposalStatus, MemberRole, VoteCounts)
- GovernanceManager (proposal creation, voting, role management)
- 6 unit tests passing

**Paused**:
- CCL interpreter extensions (governance primitives as built-ins)
- Template contracts (consensus, sociocracy, delegation, emergency)
- icnctl governance subcommands

**When to Resume**:
- After Milestone 2 (Gateway + Ledger API complete)
- Can expose governance via gateway endpoints:
  - `POST /governance/{coopId}/proposals`
  - `POST /governance/{coopId}/proposals/{id}/vote`
  - `GET /governance/{coopId}/proposals/{id}`
- Add to TypeScript SDK:
  - `icn.governance.createProposal({ ... })`
  - `icn.governance.vote({ proposalId, vote })`
- Build governance UI into reference app

Governance is important, but it's a *feature of the platform*, not a prerequisite.

## Next Actions

1. ✅ Document platform layer design (complete)
2. ✅ Create dev journal entry (this doc)
3. ⏭️ Update ROADMAP.md with Phase 14
4. ⏭️ Start Milestone 1: Gateway + Namespaces
   - Create `icn-gateway` crate
   - Implement auth endpoints
   - Extend `icn-store` with namespace storage

**User Decision Point**: Which component to build first?
- Option A: Complete Gateway Milestone 1 (auth + namespaces)
- Option B: Start with TypeScript SDK (can mock gateway for now)
- Option C: Design reference app first (clarifies requirements)

Recommendation: **Option A** - gateway is foundation for SDK and app.

## Conclusion

This pivot represents a fundamental shift in how ICN positions itself:

**Before**: "We built a substrate daemon for cooperatives"
**After**: "We built a backend platform so cooperatives can build their own apps"

The substrate (Phases 1-12) was necessary but not sufficient. The platform layer is what makes ICN *usable* and *adoptable*.

**The food co-op example crystallized this**: Co-ops don't want to run `icnd`. They want to build their Shopper's Club, Inventory Manager, or Community Portal, and have ICN handle the hard parts (identity, trust, mutual credit) invisibly.

Platform layer = the missing link between infrastructure and impact.

---

**Session Status**: Platform layer design complete, ready to implement
**Next Session**: Start Milestone 1 (Gateway + Namespaces)
**Est. Timeline**: 8 weeks to complete platform layer (all 5 milestones)
