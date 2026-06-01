# ICN Client Architecture: Passport, Keyring, Position, Receipt

**Version**: 0.2.0
**Status**: Living — describes the *target* client architecture; not a completed code rename
**Last Updated**: 2026-06-01
**Doctrine**: [Passport / Keyring / Position / Receipt](../design/passport-keyring-position-receipt.md) · [UX Language Guide](../dev/language-guide.md)

This document describes how end users interact with ICN through a **member client** — the app or
device a person uses to participate. A member client is **not** a "wallet". It is composed of
separate concerns:

- **Member Passport** — user-facing identity, membership, roles, credentials, delegations, and capabilities.
- **Device Keyring** — local private-key custody and signing. Signs actions locally; holds no value.
- **Position** — derived ledger/accounting state recomputed from signed entries; not a balance held in a wallet.
- **Receipt** — verifiable proof that an action/event/transition occurred.
- **Settlement** — the recording of an obligation/position transition; not "sending payment from a wallet".

> **Why this rewrite?** An earlier version of this document defined "The Wallet" as a single client
> object that "manages identity, credentials, memberships, and economic interactions" — a
> `pub trait Wallet` exposing `did()`, `sign()`, and `transfer()`. That god-object collapses identity,
> key custody, and ledger state into one crypto/fintech metaphor. The
> [Passport / Keyring / Position / Receipt doctrine](../design/passport-keyring-position-receipt.md)
> decomposes it, and this document adopts that split. This is the architectural **target**; it does
> **not** assert that the code has been renamed (see [Migration note](#migration-note) and
> [Non-claims](#non-claims)).

---

## Overview

The ICN architecture distinguishes between three node types:

1. **Full Nodes** - Run by organizations, complete state, participate in consensus
2. **Personal Nodes** - Run by individuals, subset of state, offline-capable
3. **Light Clients** - Personal devices, minimal/no state, connect to full nodes

A **member client** is the primary interface for an individual. It is not a single "wallet" object:
it presents the member's **passport** (identity), signs with a **device keyring** (key custody),
displays **positions** derived from the ledger, surfaces **receipts** (proof), and records
**settlements** (obligation transitions). It manages identity presentation, credentials, and
memberships — but it does **not** hold money, tokens, or balances.

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
│  │ • Example: Phone client app, browser extension                       │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## The Member Client

The **member client** is the primary interface for individuals interacting with ICN. It is **not**
just for money — and it is **not** a single "wallet" object. It decomposes into distinct concerns,
each with a different trust and regulatory meaning.

### Why not "Wallet"?

"Wallet" collapses three unrelated things into one crypto/fintech metaphor: **identity**, **key
custody**, and **ledger state**. The word silently teaches a model ICN does not implement — that
assets live in the wallet, that balances live in the wallet, that payments move between wallets, and
that identity *is* possession of a wallet.

ICN teaches none of that. Identity is a **DID**, which the member *presents* through a **passport**
and *proves control of* through a **keyring**. The two are deliberately separate: a person has one
passport (identity) backed by potentially many device keyrings (custody); losing a device loses a
keyring, not the passport. Accounting is a **position** — a derived view recomputed from signed
entries, never stored value held inside a client. Obligation movements are **settlements**, not
payments. History is a trail of **receipts**. See the
[Passport / Keyring / Position / Receipt doctrine](../design/passport-keyring-position-receipt.md)
for the full boundary.

### What a member client holds

```
MEMBER CLIENT (a member's app or device)
│
├── MEMBER PASSPORT  (identity — presents, never holds value)
│   ├── Primary DID (did:icn:z6MkAlice…) — control proven via the keyring
│   ├── Anchor reference (survives key rotation)
│   ├── Social-recovery contacts (recover the passport/identity if all devices are lost)
│   ├── Membership credentials (signed by orgs)
│   │   └── "Alice is a member of FoodCoop since 2024-01-15"
│   ├── Capability grants / delegations (scoped, delegable, revocable)
│   │   └── "Alice may record a transfer up to 100 units"
│   └── Verification & reputation attestations (portable)
│       └── "Alice completed identity verification"; "0.75 trust at TechCoop"
│
├── DEVICE KEYRING  (local key custody — signs, never holds value)
│   ├── Private keys (NEVER exported; may be backed by an OS keyring / TPM / hardware device)
│   ├── Signing of envelopes and votes
│   └── Key rotation (rotate this device's keys; losing the device loses the keyring, not the passport)
│
├── POSITION VIEWS  (derived accounting state, cached from connected nodes)
│   ├── Net positions across orgs — recomputed from signed entries, NOT stored balances
│   └── Pending / proposed obligation transitions
│
├── RECEIPTS  (verifiable proof trail of actions/events that occurred)
│
└── OUTBOX  (signed envelopes queued while offline — transport, not value)
```

### Canonical split

Old `Wallet` responsibilities map onto the canonical ICN concepts as follows:

| Old `Wallet` responsibility | Canonical ICN concept |
|-----------------------------|-----------------------|
| `did()`, anchor, recovery contacts, identity | **Member Passport** (DID *control* proven by the keyring) |
| `sign()`, `rotate_keys()`, private keys | **Device Keyring** |
| `list_memberships()`, `list_capabilities()`, `present_credential()`, credentials | **Member Passport** |
| `get_balances()` / "account balances" | **Position** — a derived ledger view, not stored value |
| `create_transfer()` / "send / transfer funds" | **Settlement** — records an obligation transition |
| transaction history / on-chain history | **Receipt** (proof) + the derived **Position** view |
| `cast_vote()` / governance participation | Passport presents the authorizing credential; keyring signs the vote |
| offline queue / `sync()` | Outbox of signed envelopes — transport, neither identity nor value |

### Conceptual interfaces

> **Conceptual architecture — not an implemented API.** The traits below do **not** exist in code.
> They name responsibilities to show how the former `Wallet` god-object decomposes; they are not a
> proposed Rust surface and nothing should be generated from them. `SignedEnvelope` *does* exist
> (in `icn-net`) but as a distinct node-to-node wire type — not as any of these conceptual shapes (see [Client operation envelope](#client-operation-envelope)).

```rust
// Identity / membership / credential presentation. Holds no value.
trait MemberPassport {
    fn subject(&self) -> Did;                 // the DID this passport presents
    fn memberships(&self) -> Vec<MembershipCredential>;
    fn credentials(&self) -> Vec<Credential>;
    fn capabilities(&self) -> Vec<Capability>; // scoped, delegable, revocable
}

// Local key custody and signing. Holds no value; may be backed by OS keyring / TPM / hardware.
trait DeviceKeyring {
    fn sign(&self, payload: &[u8]) -> Signature; // keys never leave the device
    fn rotate(&self) -> Result<()>;
}

// Derived accounting / proof views recomputed from signed entries. Read-only; nothing is "held".
trait LedgerView {
    fn position(&self, org: &OrgId) -> Position; // derived view, NOT a stored balance
    fn receipts(&self, filter: ReceiptFilter) -> Vec<Receipt>;
}

// Records an obligation/position transition. Returns a signed client envelope (signed by the keyring).
trait SettlementClient {
    fn settle(&self, with: &Did, amount: Amount, org: &OrgId) -> ClientEnvelope;
}
```

Governance is not a fifth object: a vote is **authorized** by a credential the passport presents and
**signed** by the device keyring, then submitted as a signed client envelope like any other operation.

### Rejected anti-pattern: the `Wallet` god-object

The following interface appeared in earlier drafts of this document. It is retained here **only as a
rejected, historical anti-pattern** so that anyone migrating code recognises what is being replaced.
**Do not treat it as the canonical client model.** It conflates identity (`get_did`), key custody
(`sign`, `rotate_keys`), credentials, ledger state (`get_balances`), settlement (`create_transfer`),
and governance into one object — exactly the collapse this architecture rejects.

```rust
// ❌ REJECTED — historical god-object. Decomposed into Passport / Keyring / Position / Settlement.
pub trait Wallet {
    fn get_did(&self) -> Did;                                  // → Member Passport (+ Keyring control)
    fn sign(&self, payload: &[u8]) -> Signature;               // → Device Keyring
    fn rotate_keys(&self) -> Result<()>;                       // → Device Keyring
    fn list_memberships(&self) -> Vec<MembershipCredential>;   // → Member Passport
    fn list_capabilities(&self) -> Vec<Capability>;            // → Member Passport
    fn get_balances(&self) -> Vec<(OrgId, Balance)>;           // → Position (derived view)
    fn create_transfer(&self, to: &Did, amount: Amount, org: &OrgId) -> SignedEnvelope; // → Settlement
    fn cast_vote(&self, proposal: &ProposalId, vote: Vote) -> SignedEnvelope; // → passport-authorized, keyring-signed
    // ...offline queue / sync — transport, not a concern of identity or value
}
```

### Client form factors

A member client can run on many devices. Form factor is independent of the passport/keyring split —
every form factor presents a passport and signs with a keyring.

| Form factor | Description | Use case |
|-------------|-------------|----------|
| Mobile app | iOS/Android | Most common, always available |
| Browser extension | Web integration | Web-based interactions |
| Desktop app | Full-featured | Power users, developers |
| Hardware-backed keyring | YubiKey, Ledger, TPM | High-security key custody (third-party hardware backing the **Device Keyring**) |
| CLI client | Command line | Developers, automation |

> The "Hardware-backed keyring" row references **third-party hardware** (e.g. a hardware wallet a user
> already owns). "Hardware wallet" is an external product name, not ICN-native vocabulary; in ICN it
> *backs* a Device Keyring — it does not hold ICN value.

---

## Client–Node Connection

Light clients connect to full nodes. The connection model:

```
┌─────────────────────────────────────────────────────────────────┐
│                    CLIENT CONNECTION MODEL                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  CLIENT                                                          │
│    │                                                             │
│    ├──► PRIMARY NODE (org membership)                           │
│    │    • Alice is FoodCoop member                              │
│    │    • Client connects to FoodCoop node by default           │
│    │    • Full access to FoodCoop apps and state                │
│    │                                                             │
│    ├──► SECONDARY NODES (other memberships)                     │
│    │    • Alice is also TechCoop member                         │
│    │    • Client can connect to TechCoop when needed            │
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
# ~/.icn/client-config.yaml  (illustrative)
identity:
  did: did:icn:z6MkAlice...
  keystore: local  # or: hardware, tpm   # backs the Device Keyring

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

## Client–Node Protocol

### Client operation envelope

A client operation is carried in a signed envelope whose author DID is presented via the member's
**passport** and which is signed by the **device keyring**. The shape below is **conceptual**.

> **Not the wire type.** `icn-net` defines a real `SignedEnvelope`, but it is a *node-to-node* wire
> envelope with a different shape (`from`, `sequence`, `timestamp`, `payload_type`, `payload`,
> `signature_type`, `signature`, `pq_signature`). The illustrative `ClientEnvelope` below is a
> distinct, conceptual client-operation model — do **not** assume its fields match the `icn-net`
> wire format or its canonical encoding.

```rust
// Conceptual client-operation envelope (NOT the icn-net SignedEnvelope wire type)
struct ClientEnvelope {
    operation: Operation,        // the operation being requested
    author: Did,                 // author DID (presented via the passport; control proven by the keyring)
    timestamp: LogicalTimestamp,
    nonce: u64,                  // replay protection
    signature: Signature,        // produced by the device keyring
}

// Operations a client envelope can carry
enum Operation {
    Transfer(TransferRequest),   // records a bilateral entry (a settlement), not operator routing
    Vote(VoteRequest),
    Proposal(ProposalRequest),
    MembershipRequest(MembershipRequest),
    Custom { app: AppId, payload: Vec<u8> },
}
```

### Protocol Interface

> **Conceptual** — the trait name and shape below illustrate the protocol; they are not a frozen API.

```rust
/// A client connects to a node via WebSocket or HTTP
trait ClientNodeProtocol {
    // === Authentication ===

    /// Authenticate to a node via challenge-response (passport presents the DID; keyring signs)
    fn authenticate(&self, did: &Did, challenge_response: &Signature) -> Session;

    // === State Queries (node serves from its state; all derived, read-only) ===

    /// Get derived positions (NOT stored balances)
    fn get_positions(&self, session: &Session) -> Vec<Position>;

    /// Get active proposals
    fn get_proposals(&self, session: &Session, filter: ProposalFilter) -> Vec<Proposal>;

    /// Get recent activity / receipts
    fn get_activity(&self, session: &Session, since: Timestamp) -> Vec<ActivityItem>;

    // === Submit Operations (keyring signs, node processes) ===

    /// Submit a signed envelope for processing; returns a verifiable receipt
    fn submit_envelope(&self, session: &Session, envelope: ClientEnvelope) -> Result<Receipt>;

    // === Sync (for offline support) ===

    /// Get pending operations from the outbox
    fn get_pending(&self, session: &Session) -> Vec<PendingOperation>;

    /// Sync the offline outbox with the node
    fn sync_outbox(&self, session: &Session, envelopes: Vec<ClientEnvelope>) -> SyncResult;

    // === Subscriptions (real-time updates) ===

    /// Subscribe to topics for real-time updates
    fn subscribe(&self, session: &Session, topics: Vec<Topic>) -> Subscription;
}
```

### Offline Operation Flow

```
1. CLIENT CREATES OPERATION
   └── User initiates a settlement while offline
   └── Device keyring signs a client envelope
   └── Envelope stored in local outbox

2. CLIENT RECONNECTS
   └── Network becomes available
   └── Client calls sync_outbox()
   └── Sends all queued envelopes

3. NODE PROCESSES
   └── Verifies signatures (valid)
   └── Checks timestamps (not too old, configurable)
   └── Checks nonces (no replay)
   └── Processes operations in order

4. NODE RESPONDS
   └── Success: operation applied; receipt issued
   └── Conflict: obligation/position constraint violated (state changed while offline)
   └── Expired: "timestamp too old" (configurable policy)

5. CLIENT UPDATES
   └── Marks successful envelopes as synced
   └── Surfaces conflicts to the user for resolution
```

---

## Unaffiliated Individuals

**Question:** Can someone use ICN without being a member of any coop?

**Answer:** Yes, with limitations.

### Capabilities

| Can Do | Cannot Do (without membership) |
|--------|--------------------------------|
| Create a DID (self-sovereign identity) | Record settlements (no account) |
| Hold credentials in a passport (portable) | Vote in governance (not a member) |
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
1. INSTALL A MEMBER CLIENT
   └── Create DID locally (no network needed); the device keyring generates and holds the keys

2. CONNECT TO PUBLIC GATEWAY
   └── Browse coop directory
   └── Find coops accepting members

3. OPTIONAL: VERIFY IDENTITY
   └── Complete steward ceremony (proves unique human)
   └── Raises trust from 0.0 to ~0.2

4. REQUEST MEMBERSHIP
   └── Submit application to chosen coop
   └── Passport presents verification credentials

5. ORG APPROVES (per their governance)
   └── Membership credential issued to the passport
   └── Account created in org's ledger
   └── Can now participate fully

6. BUILD TRUST
   └── Participate in governance
   └── Complete settlements
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
  - name: food-coop-client
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
# - Pre-provisioned test accounts
# - All apps running locally

# Connect a member client (passport + device keyring) to the local node at localhost:8080

# Run app in development
cd my-app/
icnctl app deploy --local
```

### Test Harness

> **Illustrative** — the `icn_testkit` shape below is conceptual; method names are not a frozen API.
> Note the framing: the keyring **signs**, the gateway **records a settlement**, and **positions** are
> derived views asserted against — nothing "moves a balance from a wallet".

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

        // Simulate user actions. The settlement client records a bilateral obligation
        // transition; the device keyring signs the envelope locally — it does not move a balance.
        let alice = env.member("alice");
        alice.settlement().settle(&bob.did(), 100)?; // keyring signs the envelope under the hood

        // Positions are derived views recomputed from signed entries, not stored balances.
        assert_eq!(env.position(&alice), 900);
        assert_eq!(env.position(&bob), 1100);
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
│    │   CLIENTS   │         │   CLIENTS   │         │   CLIENTS   │         │
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

## Migration note

This document defines the **target** client architecture. Existing ICN code, SDK types, CLI names,
config examples, and prose elsewhere in this repo still use `wallet` naming. **Adopting this model
renames no code and changes no public API** — it gives a future rename an architectural target
instead of becoming a blind find-and-replace.

- `icn-net` defines a **real** node-to-node `SignedEnvelope` wire type (`from` / `sequence` /
  `timestamp` / `payload_type` / `payload` / `signature_type` / `signature` / `pq_signature`); this
  PR does **not** touch it. The `ClientEnvelope` illustrated above is a separate **conceptual**
  client-operation model with a different shape — not that wire type.
- The conceptual `MemberPassport` / `DeviceKeyring` / `LedgerView` / `SettlementClient` /
  `ClientNodeProtocol` / `ClientEnvelope` shapes above **do not exist in code**.
- The following migrations are **named, not scheduled here** — each is its own future, separately
  reviewed PR (see the doctrine's [migration rules](../design/passport-keyring-position-receipt.md#migration-rules)):
  - **SDK key-custody rename** — `createWallet` / `HybridWallet` (TypeScript + React Native) → keyring
    naming. Breaking public-API change; deferred.
  - **`wallet_did` field** (`icn-kernel-api`) → DID-/passport-rooted naming. Public-API rename; deferred.
  - **Example app** `CoopWallet` → passport + keyring framing.
  - **Residual `wallet`-named CLI / config** in illustrative examples (this doc shows target naming).
- Where the word "wallet" still appears in this document it is either (a) the **rejected
  anti-pattern** shown for contrast, (b) a **third-party / hardware wallet** (external, not
  ICN-native), or (c) the regulatory term of art "**unhosted wallet**".

---

## Non-claims

This document is architecture doctrine for the client surface. It explicitly does **not** claim that
ICN is, or that this architecture implements:

- **token custody** or any custody of member assets;
- a **wallet balance** — positions are derived ledger views, not stored value;
- a **banking / payment / wallet** product;
- **production-ready identity**, a **live federation**, or completed **membership / standing
  governance**;
- that passport / keyring / position / receipt has been **implemented or renamed in code** — this is
  the target, not the current state.

Adopting this model does **not** rename code, change any public API, alter SDK behaviour, or weaken
any meaning / regulatory / firewall check.

---

## Key Principles

1. **Identity is a DID, not a wallet** - presented via a passport, proven via a keyring
2. **Keys live in the device keyring** - Never on nodes, never exported
3. **Keyrings sign, nodes verify** - Operations are signed envelopes
4. **Positions are derived** - accounting state is recomputed from signed entries, never held as value
5. **Offline-first** - Queue operations in the outbox, sync when connected
6. **Progressive trust** - Unaffiliated → Verified → Member → Trusted
7. **Sovereignty gradient** - Light client → Personal node → Org node
8. **Same primitives everywhere** - Phones and servers use the same kernel concepts

---

## Related Documents

- [Passport / Keyring / Position / Receipt](../design/passport-keyring-position-receipt.md) - Identity & custody vocabulary doctrine (canonical split)
- [UX Language Guide](../dev/language-guide.md) - Forbidden fintech terms and approved alternatives
- [IDENTITY_MEMBERSHIP_ARCHITECTURE.md](IDENTITY_MEMBERSHIP_ARCHITECTURE.md) - Identity, membership, and the passport surface
- [AUTH_BRIDGE_AND_DID_LOGIN.md](AUTH_BRIDGE_AND_DID_LOGIN.md) - DID login and session bridging
- [KERNEL_CONTRACTS.md](../spec/KERNEL_CONTRACTS.md) - Kernel primitive specifications
- [COOPOS.md](../vision/COOPOS.md) - Future vision: CoopOS distribution

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-25 | Initial draft ("ICN Client and Wallet Architecture"; `Wallet` god-object) |
| 0.2.0 | 2026-06-01 | Replaced the `Wallet` god-object with the Passport / Keyring / Position / Settlement / Receipt split; added why-not-Wallet, canonical split table, migration note, and non-claims. Docs-only truth-sync — no code or API change. |
