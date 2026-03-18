# ICN Client and Wallet Architecture

**Version**: 0.1.0
**Status**: Draft
**Last Updated**: 2025-01-25

This document describes how end users interact with ICN through wallets and client applications.

---

## Overview

The ICN architecture distinguishes between three node types:

1. **Full Nodes** - Run by organizations, complete state, participate in consensus
2. **Personal Nodes** - Run by individuals, subset of state, offline-capable
3. **Light Clients** - Personal devices, minimal/no state, connect to full nodes

The **wallet** is the primary user interface for individuals. It manages identity, credentials, memberships, and economic interactions.

---

## Node Taxonomy

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           NODE TAXONOMY                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  FULL NODES (run by organizations)                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ • Complete state for their org's namespaces                          │   │
│  │ • Participate in consensus groups                                    │   │
│  │ • Serve client requests                                              │   │
│  │ • Federate with other orgs                                           │   │
│  │ • Run apps (governance, ledger, trust, etc.)                         │   │
│  │ • Example: FoodCoop's server cluster                                 │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  PERSONAL NODES (run by individuals)                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ • Complete state for personal namespace only                         │   │
│  │ • Can join orgs as a member (state synced from org nodes)            │   │
│  │ • Offline-capable (queue operations, sync later)                     │   │
│  │ • Optional: serve as light infrastructure for small groups           │   │
│  │ • Example: Developer's home server, activist's Raspberry Pi          │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  LIGHT CLIENTS (personal devices)                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ • No local state (or minimal cache)                                  │   │
│  │ • Connect to full nodes for all operations                           │   │
│  │ • Hold keys locally (sign without network)                           │   │
│  │ • Queue signed envelopes when offline                                │   │
│  │ • Example: Phone wallet app, browser extension                       │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## The Wallet

The **wallet** is the primary user interface for individuals interacting with ICN. It's NOT just for money - it manages identity, credentials, memberships, and economic interactions.

### Wallet Contents

```
┌─────────────────────────────────────────────────────────────────┐
│                         WALLET CONTENTS                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  IDENTITY                                                        │
│  ├── Primary DID (did:icn:z6MkAlice...)                         │
│  ├── Key material (private keys, NEVER exported)                │
│  ├── Anchor reference (survives key rotation)                   │
│  └── Recovery contacts (for social recovery)                    │
│                                                                  │
│  CREDENTIALS                                                     │
│  ├── Membership credentials (signed by orgs)                    │
│  │   └── "Alice is member of FoodCoop since 2024-01-15"        │
│  ├── Capability tokens (delegated permissions)                  │
│  │   └── "Alice can transfer up to 100 credits"                │
│  ├── Verification credentials (from steward ceremonies)         │
│  │   └── "Alice completed identity verification"               │
│  └── Reputation attestations (portable trust)                   │
│      └── "Alice has 0.75 trust at TechCoop"                    │
│                                                                  │
│  CACHED STATE (from connected nodes)                            │
│  ├── Account balances (across orgs)                             │
│  ├── Pending transactions                                       │
│  ├── Active proposals (governance participation)                │
│  └── Recent activity feed                                       │
│                                                                  │
│  OFFLINE QUEUE                                                   │
│  └── Signed envelopes waiting to sync                           │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Wallet Interface

```rust
/// Core wallet interface
pub trait Wallet {
    // === Identity ===

    /// Get the wallet's primary DID
    fn get_did(&self) -> Did;

    /// Sign payload with wallet keys (keys never leave wallet)
    fn sign(&self, payload: &[u8]) -> Signature;

    /// Rotate keys (e.g., after compromise suspicion)
    fn rotate_keys(&self) -> Result<()>;

    // === Credentials ===

    /// List all membership credentials
    fn list_memberships(&self) -> Vec<MembershipCredential>;

    /// List all capability tokens
    fn list_capabilities(&self) -> Vec<Capability>;

    /// Create a presentation for a credential request
    fn present_credential(&self, request: &CredentialRequest) -> Presentation;

    // === Economic (via connected nodes) ===

    /// Get balances across all orgs
    fn get_balances(&self) -> Vec<(OrgId, Balance)>;

    /// Create a transfer (returns signed envelope)
    fn create_transfer(&self, to: &Did, amount: Amount, org: &OrgId) -> SignedEnvelope;

    // === Governance (via connected nodes) ===

    /// List active proposals in an org
    fn list_active_proposals(&self, org: &OrgId) -> Vec<Proposal>;

    /// Cast a vote on a proposal
    fn cast_vote(&self, proposal: &ProposalId, vote: Vote) -> SignedEnvelope;

    // === Offline Support ===

    /// Queue an envelope for later sync
    fn queue_envelope(&self, envelope: SignedEnvelope);

    /// Sync queued envelopes with connected node
    fn sync(&self) -> Result<SyncReport>;
}
```

### Wallet Implementations

| Platform | Description | Use Case |
|----------|-------------|----------|
| Mobile App | iOS/Android | Most common, always available |
| Browser Extension | Web integration | Web-based interactions |
| Desktop App | Full-featured | Power users, developers |
| Hardware Wallet | YubiKey, Ledger | High-security key storage |
| CLI Wallet | Command line | Developers, automation |

---

## Client-Node Connection

Light clients connect to full nodes. The connection model:

```
┌─────────────────────────────────────────────────────────────────┐
│                    CLIENT CONNECTION MODEL                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  WALLET                                                          │
│    │                                                             │
│    ├──► PRIMARY NODE (org membership)                           │
│    │    • Alice is FoodCoop member                              │
│    │    • Wallet connects to FoodCoop node by default           │
│    │    • Full access to FoodCoop apps and state                │
│    │                                                             │
│    ├──► SECONDARY NODES (other memberships)                     │
│    │    • Alice is also TechCoop member                         │
│    │    • Wallet can connect to TechCoop when needed            │
│    │    • Credentials prove membership                          │
│    │                                                             │
│    └──► PUBLIC GATEWAYS (for unaffiliated access)               │
│         • Some orgs run public gateways                         │
│         • Limited access for non-members                        │
│         • Path to membership discovery                          │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Connection Configuration

```yaml
# ~/.icn/wallet-config.yaml
identity:
  did: did:icn:z6MkAlice...
  keystore: local  # or: hardware, tpm

connections:
  primary:
    org: did:icn:food-coop
    endpoints:
      - https://node1.foodcoop.example:8080
      - https://node2.foodcoop.example:8080

  secondary:
    - org: did:icn:tech-coop
      endpoints:
        - https://api.techcoop.example:8080

  public_gateways:  # For discovery and onboarding
    - https://gateway.icn.coop:8080
    - https://public.cooperative-cloud.example:8080

offline:
  queue_path: ~/.icn/outbox/
  max_queue_size: 1000
  sync_interval: 5m
```

---

## Wallet-Node Protocol

### SignedEnvelope

The `SignedEnvelope` is the universal unit of wallet→node communication:

```rust
/// Signed envelope - the universal unit of wallet→node communication
pub struct SignedEnvelope {
    /// The operation being requested
    pub operation: Operation,
    /// Wallet's DID (author of this envelope)
    pub author: Did,
    /// Logical timestamp when created
    pub timestamp: LogicalTimestamp,
    /// Nonce for replay protection
    pub nonce: u64,
    /// Wallet's signature over the above fields
    pub signature: Signature,
}

/// Operations that can be performed
pub enum Operation {
    Transfer(TransferRequest),
    Vote(VoteRequest),
    Proposal(ProposalRequest),
    MembershipRequest(MembershipRequest),
    Custom { app: AppId, payload: Vec<u8> },
}
```

### Protocol Interface

```rust
/// Wallet connects to node via WebSocket or HTTP
pub trait WalletNodeProtocol {
    // === Authentication ===

    /// Authenticate wallet to node via challenge-response
    fn authenticate(&self, did: &Did, challenge_response: &Signature) -> Session;

    // === State Queries (node serves from its state) ===

    /// Get account balances
    fn get_balances(&self, session: &Session) -> Vec<Balance>;

    /// Get active proposals
    fn get_proposals(&self, session: &Session, filter: ProposalFilter) -> Vec<Proposal>;

    /// Get recent activity
    fn get_activity(&self, session: &Session, since: Timestamp) -> Vec<ActivityItem>;

    // === Submit Operations (wallet signs, node processes) ===

    /// Submit a signed envelope for processing
    fn submit_envelope(&self, session: &Session, envelope: SignedEnvelope) -> Result<Receipt>;

    // === Sync (for offline support) ===

    /// Get pending operations from queue
    fn get_pending(&self, session: &Session) -> Vec<PendingOperation>;

    /// Sync offline queue with node
    fn sync_outbox(&self, session: &Session, envelopes: Vec<SignedEnvelope>) -> SyncResult;

    // === Subscriptions (real-time updates) ===

    /// Subscribe to topics for real-time updates
    fn subscribe(&self, session: &Session, topics: Vec<Topic>) -> Subscription;
}
```

### Offline Operation Flow

```
1. WALLET CREATES OPERATION
   └── User initiates transfer while offline
   └── Wallet creates SignedEnvelope
   └── Envelope stored in local outbox

2. WALLET RECONNECTS
   └── Network becomes available
   └── Wallet calls sync_outbox()
   └── Sends all queued envelopes

3. NODE PROCESSES
   └── Verifies signatures (valid)
   └── Checks timestamps (not too old, configurable)
   └── Checks nonces (no replay)
   └── Processes operations in order

4. NODE RESPONDS
   └── Success: operation applied
   └── Conflict: "balance insufficient" (state changed while offline)
   └── Expired: "timestamp too old" (configurable policy)

5. WALLET UPDATES
   └── Marks successful envelopes as synced
   └── Surfaces conflicts to user for resolution
```

---

## Unaffiliated Individuals

**Question:** Can someone use ICN without being a member of any coop?

**Answer:** Yes, with limitations.

### Capabilities

| Can Do | Cannot Do (without membership) |
|--------|--------------------------------|
| Create a DID (self-sovereign identity) | Initiate transfers (no account) |
| Hold credentials (portable) | Vote in governance (not a member) |
| Connect to public gateways | Access org-internal resources |
| Browse public information | Build trust score (no participation) |
| Request membership in orgs | |
| Complete verification ceremonies | |
| Receive transfers (if sender's org allows) | |

### Trust Level Progression

- **Default**: Isolated (0.0) - minimal trust
- **With verification**: Known (0.1-0.2) - verified human
- **With membership**: Progressive based on participation

### Onboarding Flow

```
1. DOWNLOAD WALLET
   └── Create DID locally (no network needed)

2. CONNECT TO PUBLIC GATEWAY
   └── Browse coop directory
   └── Find coops accepting members

3. OPTIONAL: VERIFY IDENTITY
   └── Complete steward ceremony (proves unique human)
   └── Raises trust from 0.0 to ~0.2

4. REQUEST MEMBERSHIP
   └── Submit application to chosen coop
   └── Include verification credentials

5. ORG APPROVES (per their governance)
   └── Membership credential issued
   └── Account created in org's ledger
   └── Can now participate fully

6. BUILD TRUST
   └── Participate in governance
   └── Complete transactions
   └── Trust score rises over time
```

---

## Personal Nodes

### Why Run a Personal Node?

- **Sovereignty**: Don't depend on org infrastructure
- **Privacy**: Keep your own data locally
- **Offline resilience**: Work without network
- **Development**: Test apps locally
- **Small groups**: Serve a household or informal collective

### Personal Node Configuration

```yaml
# Personal node configuration
node:
  type: personal
  owner: did:icn:z6MkAlice...

namespaces:
  # Personal namespace (always available)
  /alice/personal/:
    replication: local_only

  # Cached org state (read replica)
  /food-coop/:
    replication: sync_from_org
    sync_endpoint: https://node1.foodcoop.example:8080

apps:
  # Personal apps
  - name: notes
    namespace: /alice/personal/notes/
  - name: contacts
    namespace: /alice/personal/contacts/

  # Org apps (UI only, state from org)
  - name: food-coop-wallet
    namespace: /food-coop/  # Read from synced state

federation:
  # Personal nodes typically don't federate
  enabled: false
  # But CAN for small collectives:
  # enabled: true
  # peers: [did:icn:bob-node, did:icn:carol-node]
```

### Evolution Path

Personal nodes can evolve to support larger groups:

```
┌─────────────────────────────────────────────────────────────────┐
│           PERSONAL NODE → MICRO-COOP EVOLUTION                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Stage 1: PERSONAL                                               │
│  • Alice runs node for herself                                  │
│  • Personal apps, personal data                                 │
│                                                                  │
│  Stage 2: HOUSEHOLD                                              │
│  • Alice adds family members                                    │
│  • Shared household ledger (chores, expenses)                   │
│  • Still informal, no legal entity                              │
│                                                                  │
│  Stage 3: INFORMAL COLLECTIVE                                    │
│  • Friends join, maybe 5-10 people                              │
│  • Mutual aid tracking                                          │
│  • Informal governance (consensus chat)                         │
│                                                                  │
│  Stage 4: FORMAL COOP                                            │
│  • Group incorporates as legal coop                             │
│  • Full governance apps enabled                                 │
│  • Can federate with other coops                                │
│  • Node becomes org node                                        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Developer Experience

### Local Development Setup

```bash
# Start a local ICN node for development
icnctl dev start

# Creates:
# - Local node at localhost:8080
# - Test identity (did:icn:dev-alice)
# - Test org (did:icn:dev-coop)
# - Pre-funded test accounts
# - All apps running locally

# Connect wallet to local node
icn-wallet connect localhost:8080

# Run app in development
cd my-app/
icnctl app deploy --local
```

### Test Harness

```rust
#[cfg(test)]
mod tests {
    use icn_testkit::*;

    #[test]
    fn test_my_app() {
        // Spin up test environment
        let env = TestEnv::new()
            .with_orgs(2)           // Two test coops
            .with_members(5)         // Five test members
            .with_federation()       // Federated
            .build();

        // Deploy app under test
        env.deploy_app("my-app", include_bytes!("my-app.wasm"));

        // Simulate user actions
        let alice = env.member("alice");
        alice.wallet().transfer(&bob.did(), 100)?;

        // Assert state
        assert_eq!(env.balance(&alice), 900);
        assert_eq!(env.balance(&bob), 1100);
    }
}
```

---

## Network Topology Summary

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         ICN NETWORK TOPOLOGY                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│    ┌─────────────┐         ┌─────────────┐         ┌─────────────┐         │
│    │  FOOD COOP  │◄───────►│  TECH COOP  │◄───────►│HOUSING COOP │         │
│    │   (3 nodes) │   Fed   │   (5 nodes) │   Fed   │  (2 nodes)  │         │
│    └──────┬──────┘         └──────┬──────┘         └──────┬──────┘         │
│           │                       │                       │                 │
│           │ Member                │ Member                │ Member          │
│           │ connections           │ connections           │ connections     │
│           │                       │                       │                 │
│    ┌──────┴──────┐         ┌──────┴──────┐         ┌──────┴──────┐         │
│    │   WALLETS   │         │   WALLETS   │         │   WALLETS   │         │
│    │  (members)  │         │  (members)  │         │  (members)  │         │
│    └─────────────┘         └─────────────┘         └─────────────┘         │
│                                                                              │
│    ┌─────────────┐         ┌─────────────┐                                  │
│    │  PERSONAL   │         │   PUBLIC    │◄──── Unaffiliated individuals   │
│    │    NODE     │         │   GATEWAY   │      browse, apply, onboard     │
│    │  (Alice's)  │         │             │                                  │
│    └─────────────┘         └─────────────┘                                  │
│                                                                              │
│    ┌─────────────┐                                                          │
│    │    DEV      │                                                          │
│    │   NODE      │◄──── Developers testing locally                         │
│    │  (local)    │                                                          │
│    └─────────────┘                                                          │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Key Principles

1. **Keys live in wallets** - Never on nodes, never exported
2. **Wallets sign, nodes verify** - Operations are signed envelopes
3. **Offline-first** - Queue operations, sync when connected
4. **Progressive trust** - Unaffiliated → Verified → Member → Trusted
5. **Sovereignty gradient** - Light client → Personal node → Org node
6. **Same primitives everywhere** - Phones and servers use same kernel concepts

---

## Related Documents

- [KERNEL_CONTRACTS.md](../spec/KERNEL_CONTRACTS.md) - Kernel primitive specifications
- [COOPOS.md](../vision/COOPOS.md) - Future vision: CoopOS distribution

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-25 | Initial draft |
