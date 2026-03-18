# Next Steps: Priority Roadmap
**Based on**: ACTUAL_IMPLEMENTATION_STATUS_AUDIT.md  
**Date**: 2025-12-17

## Current Status: SDK Ready, Backend Incomplete

The TypeScript SDK now has complete amendment voting hooks, but the Rust backend gateway API endpoints don't exist yet. This creates a **partial stack** situation:

```
Mobile App (React Native)    ✅ Has VotingScreen.tsx
         ↓
SDK (@icn/react-native)      ✅ Has useAmendmentVoting/Results hooks
         ↓
Client HTTP Methods          ✅ Has voteOnAmendment(), getAmendmentResults()
         ↓
Gateway API (icn-gateway)    ❌ Missing /v1/constitutional/amendments/:id/vote
         ↓
Governance Actor             ✅ Has data structures, storage
         ↓
Storage (Sled)               ✅ Working
```

## Priority 1: Complete Amendment Voting Backend (URGENT)

### Tasks

#### 1.1: Add Gateway API Endpoints

**File**: `icn/crates/icn-gateway/src/api/governance.rs`

Add three new endpoints:

```rust
// POST /v1/constitutional/amendments/:id/vote
pub async fn vote_on_amendment(
    Path(amendment_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<VoteOnAmendmentRequest>,
) -> Result<Json<AmendmentVote>, ApiError> {
    // 1. Parse amendment_id (hex)
    // 2. Get voter DID from auth
    // 3. Validate choice: approve, reject, abstain
    // 4. Call governance_handle.vote_on_amendment()
    // 5. Return vote record
}

// GET /v1/constitutional/amendments/:id/my-vote
pub async fn get_my_amendment_vote(
    Path(amendment_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AmendmentVote>, ApiError> {
    // 1. Parse amendment_id
    // 2. Get voter DID from auth
    // 3. Call governance_handle.get_amendment_vote(amendment_id, did)
    // 4. Return vote record or 404
}

// GET /v1/constitutional/amendments/:id/results
pub async fn get_amendment_results(
    Path(amendment_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AmendmentResults>, ApiError> {
    // 1. Parse amendment_id
    // 2. Call governance_handle.get_amendment_results(amendment_id)
    // 3. Compute:
    //    - Vote counts (approve, reject, abstain)
    //    - Eligible voters
    //    - Quorum status
    //    - Approval status
    // 4. Return results
}
```

**Request/Response Types**:
```rust
#[derive(Deserialize)]
pub struct VoteOnAmendmentRequest {
    pub choice: VoteChoice,
    pub comment: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub enum VoteChoice {
    Approve,
    Reject,
    Abstain,
}

#[derive(Serialize)]
pub struct AmendmentVote {
    pub amendment_id: String,
    pub voter_did: String,
    pub vote: VoteChoice,
    pub comment: Option<String>,
    pub voted_at: u64,
}

#[derive(Serialize)]
pub struct AmendmentResults {
    pub amendment_id: String,
    pub status: AmendmentStatus,
    pub approve_count: u32,
    pub reject_count: u32,
    pub abstain_count: u32,
    pub total_votes: u32,
    pub eligible_voters: u32,
    pub has_quorum: bool,
    pub quorum_threshold: f64,
    pub approval_threshold: f64,
    pub is_approved: bool,
}
```

#### 1.2: Add Governance Actor Methods

**File**: `icn/crates/icn-governance/src/handle.rs`

Add methods to `GovernanceHandle`:

```rust
impl GovernanceHandle {
    /// Vote on an amendment
    pub async fn vote_on_amendment(
        &self,
        amendment_id: [u8; 32],
        voter_did: String,
        choice: VoteChoice,
        comment: Option<String>,
    ) -> Result<AmendmentVote> {
        // Send message to actor
        // Actor validates:
        // - Amendment exists and is in "voting" status
        // - Voter is eligible (member of jurisdiction)
        // - Voter hasn't already voted
        // Store vote in amendment_votes table
        // Update amendment vote counts
        // Broadcast VoteCast event
    }

    /// Get a specific vote
    pub async fn get_amendment_vote(
        &self,
        amendment_id: [u8; 32],
        voter_did: String,
    ) -> Result<Option<AmendmentVote>> {
        // Query amendment_votes table
        // Return vote or None
    }

    /// Get amendment voting results
    pub async fn get_amendment_results(
        &self,
        amendment_id: [u8; 32],
    ) -> Result<AmendmentResults> {
        // Load amendment
        // Count votes by choice
        // Get eligible voter count from jurisdiction membership
        // Calculate quorum: (total_votes / eligible_voters) >= quorum_threshold
        // Calculate approval: (approve_count / (approve + reject)) >= approval_threshold
        // Return results
    }
}
```

#### 1.3: Add Storage Schema

**File**: `icn/crates/icn-governance/src/store.rs`

Add amendment votes table:

```rust
// Key: amendment_id || voter_did
// Value: AmendmentVote
pub fn store_amendment_vote(&self, vote: &AmendmentVote) -> Result<()> {
    let key = format!("amendment_votes:{}:{}", 
        hex::encode(vote.amendment_id),
        vote.voter_did
    );
    self.db.insert(key.as_bytes(), bincode::serialize(vote)?)?;
    Ok(())
}

pub fn get_amendment_vote(
    &self,
    amendment_id: [u8; 32],
    voter_did: &str,
) -> Result<Option<AmendmentVote>> {
    let key = format!("amendment_votes:{}:{}", 
        hex::encode(amendment_id),
        voter_did
    );
    match self.db.get(key.as_bytes())? {
        Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
        None => Ok(None),
    }
}

pub fn list_amendment_votes(
    &self,
    amendment_id: [u8; 32],
) -> Result<Vec<AmendmentVote>> {
    let prefix = format!("amendment_votes:{}:", hex::encode(amendment_id));
    // Scan with prefix, deserialize all votes
}
```

#### 1.4: Register Routes

**File**: `icn/crates/icn-gateway/src/server.rs`

Add to router:

```rust
.route("/v1/constitutional/amendments/:id/vote", post(governance::vote_on_amendment))
.route("/v1/constitutional/amendments/:id/my-vote", get(governance::get_my_amendment_vote))
.route("/v1/constitutional/amendments/:id/results", get(governance::get_amendment_results))
```

#### 1.5: Test End-to-End

```bash
# 1. Start daemon
./target/debug/icnd

# 2. Create amendment
curl -X POST http://localhost:8080/v1/constitutional/amendments \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "amendment_type": "policy",
    "scope_type": "jurisdiction",
    "scope_id": "coop:test",
    "title": "Test Amendment",
    "description": "Testing voting",
    "changes": []
  }'

# 3. Open voting
curl -X POST http://localhost:8080/v1/constitutional/amendments/$AMENDMENT_ID/voting \
  -H "Authorization: Bearer $TOKEN"

# 4. Vote (as different users)
curl -X POST http://localhost:8080/v1/constitutional/amendments/$AMENDMENT_ID/vote \
  -H "Authorization: Bearer $ALICE_TOKEN" \
  -d '{"choice": "approve", "comment": "Looks good"}'

curl -X POST http://localhost:8080/v1/constitutional/amendments/$AMENDMENT_ID/vote \
  -H "Authorization: Bearer $BOB_TOKEN" \
  -d '{"choice": "reject"}'

# 5. Check my vote
curl http://localhost:8080/v1/constitutional/amendments/$AMENDMENT_ID/my-vote \
  -H "Authorization: Bearer $ALICE_TOKEN"

# 6. Get results
curl http://localhost:8080/v1/constitutional/amendments/$AMENDMENT_ID/results
```

#### 1.6: Test Mobile App

```typescript
import { useAmendmentVoting, useAmendmentResults } from '@icn/react-native';

function VotingScreen({ amendmentId }) {
  const { vote, loading, myVote } = useAmendmentVoting(client, amendmentId);
  const { results } = useAmendmentResults(client, amendmentId);

  return (
    <View>
      {!myVote && (
        <>
          <Button onPress={() => vote('approve')} title="Approve" />
          <Button onPress={() => vote('reject')} title="Reject" />
        </>
      )}
      {results && (
        <Text>
          Approve: {results.approve_count} | Reject: {results.reject_count}
        </Text>
      )}
    </View>
  );
}
```

### Estimated Effort
- **Gateway endpoints**: 3-4 hours
- **Actor methods**: 2-3 hours
- **Storage schema**: 1-2 hours
- **Testing**: 2-3 hours
- **Total**: ~1 day

### Success Criteria
- ✅ Can vote on amendments via REST API
- ✅ Can fetch my vote
- ✅ Can get live vote tallies
- ✅ Mobile app VotingScreen.tsx works end-to-end
- ✅ WebSocket broadcasts VoteCast events
- ✅ Quorum/approval logic correct

---

## Priority 2: Server-Side Economic Safety (3-5 days)

### Why Critical
Currently, budgets and credit limits are **client-side only**. A malicious client can bypass them. Must enforce on server.

### Tasks

#### 2.1: Credit Limits in Ledger
- Add `credit_limit` field to account metadata
- Enforce in `Ledger::record_transaction()`
- Return error if transaction would exceed limit

#### 2.2: Budget Validation in Gateway
- Before creating payment, check budgets via new method
- Return error if budget exceeded
- Update budget spent amount after payment

#### 2.3: Velocity Limits
- Track payment frequency per account
- Reject if too many payments in time window
- Configurable: "max 10 payments per hour"

#### 2.4: Dispute Resolution API
- Add endpoints for filing disputes
- Queue disputed transactions
- Admin endpoints for resolution

---

## Priority 3: Cooperative Lifecycle (1-2 weeks)

### Why Important
Cooperatives are the core entity but have no formation/dissolution logic.

### Tasks

#### 3.1: Create `icn-cooperative` Crate
- CooperativeActor
- Formation workflow (founding members, charter signature)
- Dissolution workflow
- Member admission/removal

#### 3.2: Governance Integration
- Hook member admission to governance votes
- Charter amendments
- Dissolution proposals

#### 3.3: Gateway APIs
- POST /v1/coops (formation)
- POST /v1/coops/:id/members (admission)
- DELETE /v1/coops/:id/members/:did (removal)
- POST /v1/coops/:id/dissolve

---

## Priority 4: Federation (2-3 weeks)

### Why Important
Enable inter-cooperative collaboration and resource sharing.

### Tasks

#### 4.1: Create `icn-federation` Crate
- FederationActor
- Inter-coop protocols
- Resource sharing contracts

#### 4.2: Federated Identity
- Cross-coop trust resolution
- Federated DID resolution

#### 4.3: Gateway APIs
- POST /v1/federations
- POST /v1/federations/:id/join
- GET /v1/federations/:id/resources

---

## Priority 5: Integrate PQ Crypto (1 week)

### Why Important
Quantum threat timeline: 10-15 years. Start migration now.

### Tasks

#### 5.1: Make Hybrid Default
- Update `icn-identity` to generate ML-DSA + Ed25519 by default
- Update `KeyPair` struct

#### 5.2: Hybrid Signing
- Update `SignedEnvelope` to include both signatures
- Verify both on receipt

#### 5.3: Hybrid Encryption
- Update `EncryptedEnvelope` to use ML-KEM + X25519
- Mix shared secrets

#### 5.4: Key Rotation
- Add `icnctl identity upgrade-pq` command
- Publish rotation transactions

---

## Summary Timeline

| Priority | Feature | Effort | Status |
|----------|---------|--------|--------|
| **P1** | Amendment voting backend | 1 day | ✅ **Complete** (2025-12-25) |
| **P2** | Economic safety | 3-5 days | ✅ **Complete** (2025-12-25) |
| **P3** | Cooperative lifecycle | 1-2 weeks | Not started |
| **P4** | Federation | 2-3 weeks | Not started |
| **P5** | PQ crypto integration | 1 week | PQ crypto exists, not integrated |

## Immediate Action
**Next sprint**: Priority 3 (Cooperative lifecycle) - formation, dissolution, member admission/removal.

**P1 Note**: Backend endpoints already existed in `icn-gateway/src/api/constitutional/voting.rs`. Only SDK bug fix was needed (commit 64fc08c9).

**P2 Note**: CreditPolicyManager and VelocityLimiter were already implemented but not wired up. PR #288 initializes them in supervisor and gateway respectively.
