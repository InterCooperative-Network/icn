⚠️ **SUPERSEDED** - This document has been superseded.

See current deployment guide: [HOMELAB_DEPLOYMENT.md](../../operations/deployment/HOMELAB_DEPLOYMENT.md)

---

# ICN Real-World Deployment

**Last Updated**: 2026-01-23
**Status**: Design Document
**Authors**: Claude Code, fahertym

---

## Overview

This document describes how ICN operates at scale in the real world: deployment topology, mobile wallets, day-to-day cooperative use, and federation architecture.

---

## Deployment Topology

### Node Types

ICN uses a **multi-tier node architecture** where different organizations run different types of nodes based on their resources and needs.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        FEDERATION LAYER                                  │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐         │
│  │  Federation     │  │  Federation     │  │  Meta-Federation │         │
│  │  Gateway Node   │  │  Gateway Node   │  │  Coordinator     │         │
│  │  (Regional)     │  │  (Sector)       │  │  (Global)        │         │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘         │
│           │                    │                    │                   │
└───────────┼────────────────────┼────────────────────┼───────────────────┘
            │                    │                    │
┌───────────┼────────────────────┼────────────────────┼───────────────────┐
│           │         COOPERATIVE LAYER               │                   │
│  ┌────────▼────────┐  ┌────────▼────────┐  ┌───────▼─────────┐         │
│  │  Full Node      │  │  Full Node      │  │  Full Node      │         │
│  │  (Coop A)       │  │  (Coop B)       │  │  (Coop C)       │         │
│  │  - Gateway      │  │  - Gateway      │  │  - Gateway      │         │
│  │  - Compute      │  │  - Compute      │  │  - Compute      │         │
│  │  - Ledger       │  │  - Ledger       │  │  - Ledger       │         │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘         │
│           │                    │                    │                   │
└───────────┼────────────────────┼────────────────────┼───────────────────┘
            │                    │                    │
┌───────────┼────────────────────┼────────────────────┼───────────────────┐
│           │           MEMBER LAYER                  │                   │
│  ┌────────▼────────┐  ┌────────▼────────┐  ┌───────▼─────────┐         │
│  │  Mobile Wallet  │  │  Web App        │  │  Desktop App    │         │
│  │  (Light Client) │  │  (Light Client) │  │  (Light Client) │         │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Node Specifications

| Node Type | Who Runs It | Resources | Purpose |
|-----------|-------------|-----------|---------|
| **Full Node** | Cooperatives, Communities | 4+ CPU, 8GB+ RAM, SSD | Full ICN daemon, Gateway API, Compute executor |
| **Gateway Node** | Federations | 2+ CPU, 4GB+ RAM | API routing, cross-coop coordination |
| **Light Client** | Individual members | Mobile/browser | Wallet, signing, local keystore |
| **Compute Node** | Resource contributors | Variable (can include GPU) | Task execution for credit |
| **Steward Node** | Verified members | 2+ CPU, 4GB+ RAM | VUI computation, enrollment ceremonies |

### Recommended Deployment

**Small Cooperative (10-100 members)**:
- 1 Full Node (can be shared VPS or dedicated server)
- Members use mobile wallet or web app

**Medium Cooperative (100-1000 members)**:
- 2-3 Full Nodes (redundancy)
- 1 dedicated Gateway for member traffic
- Optional: Compute nodes for task execution revenue

**Large Federation (10+ cooperatives)**:
- Federation Gateway Node
- Shared steward nodes for SDIS
- Clearing house for inter-coop settlement

---

## Mobile Wallet Architecture

### Overview

The mobile wallet is a **light client** that manages identity, signs transactions, and communicates with cooperative gateways.

```
┌─────────────────────────────────────────────────────────────────┐
│                     MOBILE WALLET APP                           │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐ │
│  │   UI Layer      │  │   UI Layer      │  │   UI Layer     │ │
│  │   (Balance)     │  │   (Payments)    │  │   (Governance) │ │
│  └────────┬────────┘  └────────┬────────┘  └───────┬────────┘ │
│           │                    │                   │           │
│  ┌────────▼────────────────────▼───────────────────▼────────┐ │
│  │                    SDK Layer (@icn/client)                │ │
│  │  - ICNClient (REST calls)                                │ │
│  │  - ICNSubscription (WebSocket)                           │ │
│  │  - SignatureProvider (signing interface)                 │ │
│  └────────────────────────────┬─────────────────────────────┘ │
│                               │                               │
│  ┌────────────────────────────▼─────────────────────────────┐ │
│  │                  Keystore Layer                           │ │
│  │  - Ed25519 keypair (Secure Enclave / Keychain)           │ │
│  │  - DID management                                         │ │
│  │  - Device registration                                    │ │
│  │  - Biometric unlock                                       │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                               │
                               │ HTTPS / WSS
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                    COOPERATIVE GATEWAY                          │
│  - DID-based authentication (challenge/response)                │
│  - JWT tokens for session                                       │
│  - Real-time events via WebSocket                               │
└─────────────────────────────────────────────────────────────────┘
```

### Key Features

#### 1. Secure Keystore

**Platform Integration**:
- **iOS**: Keys stored in Secure Enclave, biometric unlock via Face ID / Touch ID
- **Android**: Keys in Android Keystore, biometric via BiometricPrompt
- **Desktop**: OS-specific keychain with optional hardware key (YubiKey)

**Multi-Device Support**:
```typescript
// Register new device from existing device
await client.registerDevice('did:icn:alice', {
  device_id: 'phone-2',
  label: 'New iPhone',
  public_key: newDevicePublicKey,
  capabilities: ['sign', 'encrypt'],
  signing_device_id: 'phone-1',  // Existing device authorizes
  signature: authorizationSignature,
});
```

**Device Recovery**:
- Threshold key shares across steward network
- Social recovery via trusted contacts
- Backup to encrypted cloud storage (optional)

#### 2. Authentication Flow

```
┌──────────┐                    ┌──────────┐                    ┌──────────┐
│  Wallet  │                    │  Gateway │                    │   ICN    │
│  (App)   │                    │          │                    │  Daemon  │
└────┬─────┘                    └────┬─────┘                    └────┬─────┘
     │                               │                               │
     │  POST /auth/challenge         │                               │
     │  { did: "did:icn:alice" }     │                               │
     │ ─────────────────────────────►│                               │
     │                               │                               │
     │  { nonce: "abc123..." }       │                               │
     │ ◄─────────────────────────────│                               │
     │                               │                               │
     │  [Sign nonce with private key]│                               │
     │                               │                               │
     │  POST /auth/verify            │                               │
     │  { did, signature, coop_id }  │                               │
     │ ─────────────────────────────►│                               │
     │                               │  [Verify signature vs DID]    │
     │                               │ ─────────────────────────────►│
     │                               │                               │
     │                               │  [Signature valid]            │
     │                               │ ◄─────────────────────────────│
     │                               │                               │
     │  { token: "jwt...", expires } │                               │
     │ ◄─────────────────────────────│                               │
     │                               │                               │
```

#### 3. Real-Time Updates

```typescript
// Subscribe to cooperative events with auto-reconnect
const subscription = client.subscribe('my-coop', {
  onEvent: (event) => {
    switch (event.type) {
      case 'Payment':
        updateBalance(event.payload);
        showNotification('Payment received!');
        break;
      case 'ProposalOpened':
        showVotingNotification(event.payload);
        break;
    }
  },
  onReconnect: (attempt) => {
    showConnectionStatus('Reconnecting...');
  },
}, {
  autoReconnect: true,
  autoBackfill: true,  // Catch up on missed events
  gapDetection: true,  // Detect missing sequence numbers
});
```

#### 4. Offline Support

**Queued Transactions**:
```typescript
// Queue payment for later submission
if (!isOnline) {
  await queueTransaction({
    type: 'payment',
    to: 'did:icn:bob',
    amount: 2.5,
    currency: 'hours',
    memo: 'Garden help',
  });
  showMessage('Payment queued - will submit when online');
}
```

**Cached Data**:
- Balance cached locally (synced on connect)
- Recent transaction history
- Member directory
- Proposal summaries

**Sync on Reconnect**:
1. Submit queued transactions
2. Request backfill of missed events
3. Refresh balances
4. Update cached data

---

## Identity → Membership → Authorization (In Practice)

**How a person goes from "I downloaded the app" to "I can pay and vote":**

### Step 1: Create Identity (in wallet)

```typescript
// Happens automatically on first app launch
const keypair = await SecureEnclave.generateEd25519();
const did = `did:icn:${base58Encode(keypair.publicKey)}`;
// DID stored locally, never leaves device
```

**User sees**: "Your identity has been created. Back up your recovery phrase."

### Step 2: Apply for Membership

```typescript
// User taps "Join Cooperative" and enters invite code
const application = await client.applyForMembership({
  coopId: 'coop:icn:food-coop-123',
  inviteCode: 'ABC123',
  name: 'Alice Smith',
  email: 'alice@example.com',
});
// Status: "pending" until approved
```

**User sees**: "Application submitted. A member will review your application."

### Step 3: Receive Membership Credential

```typescript
// After approval (governance vote or admin action)
// Gateway issues Verifiable Credential:
const membershipVC = {
  type: 'MembershipCredential',
  issuer: 'entity:icn:coop:food-coop-123',
  subject: 'did:icn:alice...',
  roles: ['member'],  // Can be upgraded later
  issuedAt: '2026-01-23T...',
  expiresAt: '2027-01-23T...',
  signature: '...',
};
// VC stored in wallet, presented to gateway on auth
```

**User sees**: "Welcome to Food Co-op! You're now a member."

### Step 4: Authenticate to Gateway

```
Every API call includes this flow:

1. Wallet: POST /auth/challenge { did: "did:icn:alice..." }
2. Gateway: { nonce: "random-challenge-bytes" }
3. Wallet: Signs nonce with private key
4. Wallet: POST /auth/verify { did, signature, membershipVC }
5. Gateway: Verifies signature + VC + non-revocation
6. Gateway: { token: "jwt...", expiresIn: 3600 }
7. Wallet: Uses JWT for subsequent requests
```

### Step 5: Authorization Check (every request)

```
Gateway checks (in order):

1. JWT valid and not expired?
2. DID in JWT matches request context?
3. Membership VC valid for this coop?
4. VC not in revocation accumulator?
5. Role in VC permits this action?
6. Trust score meets threshold for this endpoint?

If ANY check fails → 401/403 error
```

### Role Progression

```
New member joins:
  └── Roles: [member]
      ├── Can: view balance, make payments, vote
      └── Cannot: create proposals, approve members

After 6 months + good standing:
  └── Roles: [member, proposer]
      └── Can: create governance proposals

Elected to board:
  └── Roles: [member, proposer, moderator, treasurer]
      └── Can: manage budgets, approve members

Trained as steward:
  └── Roles: [member, steward]
      └── Can: conduct VUI ceremonies, participate in key recovery
```

---

## Identity Lifecycle (Practical Scenarios)

### Scenario: Lost Phone

```
1. Alice loses her phone (device: "iphone-12")

2. Alice uses her laptop (device: "macbook-1") to:
   DELETE /devices/{alice-did}/iphone-12
   { reason: "lost", signature: <signed-by-macbook-1> }

3. Gateway adds "iphone-12" to revoked devices list

4. If someone finds Alice's phone:
   - They can't auth (device credential revoked)
   - They can see cached data (encrypt sensitive local data!)
   - They can't sign new transactions

5. Alice buys new phone, registers it:
   POST /devices/{alice-did}
   { device_id: "iphone-14", public_key: "...", signature: <signed-by-macbook-1> }

6. Alice's membership is unaffected—her DID didn't change
```

### Scenario: Phone Stolen AND Unlocked (Worst Case)

```
1. Attacker has Alice's phone, unlocked (biometrics bypassed)

2. Alice calls trusted contact (Bob) who has emergency auth:
   Bob: POST /identity/emergency-revoke
   { target_did: "did:icn:alice...", voucher_did: "did:icn:bob...", reason: "theft" }

3. Gateway immediately:
   - Revokes ALL of Alice's device credentials
   - Marks Alice's DID as "recovery mode"
   - Blocks all transactions from Alice's DID

4. Alice schedules recovery ceremony with stewards:
   - In-person verification (video call with 3 stewards)
   - Stewards use threshold shares to reconstruct recovery key
   - New keypair generated, new DID created

5. Alice's old DID is linked to new DID via recovery proof:
   { old_did: "did:icn:alice-old...", new_did: "did:icn:alice-new...",
     recovery_proof: <signed-by-steward-threshold>, timestamp: "..." }

6. Alice's memberships are migrated:
   - Each entity re-issues membership VC to new DID
   - Trust graph transfers attestations (with verification)
   - Balance transfers (via governance-approved migration)
```

### Scenario: Key Rotation (Proactive Security)

```
1. Alice decides to rotate keys (annual security hygiene)

2. Alice generates new keypair on her device:
   const newKeypair = await SecureEnclave.generateEd25519();
   const newDid = `did:icn:${base58Encode(newKeypair.publicKey)}`;

3. Alice publishes rotation announcement (signed by OLD key):
   POST /identity/rotate
   { old_did: "did:icn:alice-old...", new_did: "did:icn:alice-new...",
     reason: "scheduled_rotation", signature: <signed-by-old-key> }

4. 7-day grace period:
   - Both DIDs accepted by gateway
   - Other members notified of pending rotation
   - Entities prepare new membership VCs

5. After grace period:
   - Old DID deprecated (auth rejected)
   - New DID is canonical
   - All credentials now reference new DID
```

### Scenario: Member Removed (Governance Action)

```
1. Governance proposal: "Remove Alice for code of conduct violation"

2. Proposal passes with required majority

3. Gateway executes:
   - Alice's membership VC added to revocation accumulator
   - Alice's trust attestations from this entity zeroed
   - Alice's credit limit set to zero, outstanding balance flagged

4. Alice's app shows:
   - "Your membership has been revoked"
   - "Reason: Governance decision #1234"
   - "Appeal deadline: 30 days"

5. Alice can appeal:
   POST /governance/appeals
   { proposal_id: "1234", grounds: "procedural error", evidence: "..." }

6. If appeal succeeds:
   - Membership VC removed from revocation accumulator
   - Trust/credit restored
```

---

## Day-to-Day Cooperative Use

### User Journey: New Member Onboarding

```
1. INVITATION
   ├── Existing member sends invite link/QR code
   └── Contains: coop_id, invite_code, gateway_url

2. APP INSTALLATION
   ├── Download mobile wallet from app store
   └── Or: Access web app at coop's subdomain

3. IDENTITY CREATION
   ├── Generate Ed25519 keypair (in Secure Enclave)
   ├── Create DID: did:icn:<pubkey>
   └── Set up biometric unlock

4. STEWARD VERIFICATION (for full membership)
   ├── Schedule enrollment ceremony
   ├── Meet with steward (in person or video)
   ├── Complete VUI generation (proof-of-personhood)
   └── Receive personhood anchor credential

5. COOPERATIVE MEMBERSHIP
   ├── Apply with invite code
   ├── Existing members vouch (builds initial trust)
   ├── Membership approved (governance or auto)
   └── Initial credit limit assigned based on sponsorship

6. FIRST TRANSACTION
   ├── Receive welcome credits from cooperative fund
   └── Or: Log first contribution, earn credits
```

### User Journey: Daily Transaction

```
ALICE PAYS BOB FOR 2 HOURS OF GARDENING HELP

1. ALICE opens wallet app
   └── Authenticates with Face ID

2. ALICE taps "Pay"
   ├── Scans Bob's QR code (or selects from directory)
   ├── Enters: 2 hours, memo "Garden help"
   └── Reviews: "Pay 2.00 hours to Bob"

3. ALICE confirms
   └── Signs transaction with private key

4. TRANSACTION SUBMITTED
   ├── SDK calls POST /ledger/{coop}/payment
   └── Gateway validates, forwards to ICN daemon

5. LEDGER PROCESSING
   ├── ICN daemon creates double-entry
   │   ├── Debit: Alice -2 hours
   │   └── Credit: Bob +2 hours
   ├── Entry added to Merkle-DAG
   └── Gossip broadcasts to other nodes

6. REAL-TIME UPDATE
   ├── WebSocket pushes event to Bob's wallet
   └── Bob sees notification: "Received 2 hours from Alice"

7. BOTH WALLETS UPDATED
   ├── Alice sees new balance
   └── Bob sees new balance

TOTAL TIME: ~2 seconds
```

### User Journey: Governance Vote

```
COOPERATIVE PROPOSAL: "Increase credit limit for new members"

1. PROPOSAL CREATED
   ├── Board member creates proposal via app
   ├── Type: Economic, requires 2/3 majority
   └── Discussion period: 7 days

2. NOTIFICATION
   ├── All members receive push notification
   └── "New proposal: Review and vote within 7 days"

3. MEMBER REVIEWS
   ├── Opens proposal in app
   ├── Reads description, sees current discussion
   └── Can add comments

4. MEMBER VOTES
   ├── Options: For / Against / Abstain
   ├── Signs vote with private key
   └── Can delegate vote to trusted member

5. VOTING CLOSES
   ├── Automatic after deadline
   ├── Or: Manual close by authorized member
   └── Results calculated

6. OUTCOME APPLIED
   ├── If passed: Parameters updated automatically
   └── If failed: Proposal archived

7. AUDIT TRAIL
   └── Full vote history preserved in governance log
```

---

## Application Integration Patterns

### Pattern 1: Web Application

```typescript
// React app with ICN SDK
import { createClient, ICNClient } from '@icn/client';

function CoopDashboard() {
  const [client, setClient] = useState<ICNClient | null>(null);
  const [balance, setBalance] = useState<number>(0);

  useEffect(() => {
    let sub: any = null;

    async function init() {
      const icn = createClient({
        baseUrl: 'https://gateway.mycoop.org',
        autoRefresh: true,
      });

      // Authenticate with wallet
      await icn.authenticate(did, walletSigner, 'my-coop');
      setClient(icn);

      // Subscribe to real-time updates
      sub = icn.subscribe('my-coop', {
        onEvent: (event) => {
          if (event.type === 'Payment') {
            refreshBalance();
          }
        },
      });
    }

    init();
    return () => { if (sub) sub.close(); };
  }, []);

  async function refreshBalance() {
    const bal = await client.getBalance('my-coop', did);
    setBalance(bal.balance);
  }

  return (
    <div>
      <h1>Balance: {balance} hours</h1>
      <PaymentForm client={client} />
    </div>
  );
}
```

### Pattern 2: Point-of-Sale Terminal

```typescript
// POS terminal for in-person payments
class POSTerminal {
  private client: ICNClient;
  private merchantDid: string;

  async processPayment(amount: number, currency: string) {
    // Display QR code for customer to scan
    const paymentRequest = {
      to: this.merchantDid,
      amount,
      currency,
      memo: `POS Sale - ${new Date().toISOString()}`,
    };

    displayQRCode(encodePaymentRequest(paymentRequest));

    // Wait for payment confirmation via WebSocket
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject('Payment timeout'), 60000);

      this.client.subscribe('my-coop', {
        onEvent: (event) => {
          if (event.type === 'Payment' &&
              event.payload.to === this.merchantDid &&
              event.payload.amount === amount) {
            clearTimeout(timeout);
            printReceipt(event.payload);
            resolve(event.payload);
          }
        },
      });
    });
  }
}
```

### Pattern 3: Scheduled Contributions

```typescript
// Automated monthly contributions via CCL contract
const monthlyContributionContract = {
  name: "MonthlyMembershipDues",
  participants: memberDids,
  currency: "hours",
  rules: [
    {
      name: "collect_dues",
      requires: [
        // Only callable on first of month
        "day_of_month(now()) == 1"
      ],
      body: [
        // Transfer 1 hour from each member to coop treasury
        {
          type: "For",
          var: "member",
          iterable: "participants",
          body: [
            {
              type: "LedgerTransfer",
              from: { type: "Var", name: "member" },
              to: { type: "Literal", value: "did:icn:coop-treasury" },
              amount: { type: "Literal", value: 1 },
              currency: { type: "Literal", value: "hours" },
            }
          ]
        }
      ]
    }
  ],
  triggers: [
    {
      name: "monthly_dues",
      schedule: "0 0 1 * *",  // First of each month
      action: [{ type: "Call", name: "collect_dues", args: [] }]
    }
  ]
};
```

---

## Federation at Scale

### Topology: Regional Food Co-op Federation

```
                     ┌─────────────────────────────────┐
                     │     NATIONAL FOOD COOP          │
                     │     FEDERATION                  │
                     │     (Meta-Federation)           │
                     │     - Policy coordination       │
                     │     - Bulk purchasing pools     │
                     │     - Advocacy                  │
                     └──────────────┬──────────────────┘
                                    │
          ┌─────────────────────────┼─────────────────────────┐
          │                         │                         │
┌─────────▼─────────┐    ┌─────────▼─────────┐    ┌─────────▼─────────┐
│  MIDWEST REGION   │    │  NORTHEAST REGION │    │  WEST COAST       │
│  FEDERATION       │    │  FEDERATION       │    │  FEDERATION       │
│  - Regional       │    │  - Regional       │    │  - Regional       │
│    clearing       │    │    clearing       │    │    clearing       │
│  - Shared stewards│    │  - Shared stewards│    │  - Shared stewards│
│  - Compute pool   │    │  - Compute pool   │    │  - Compute pool   │
└─────────┬─────────┘    └─────────┬─────────┘    └─────────┬─────────┘
          │                         │                         │
    ┌─────┼─────┐            ┌─────┼─────┐            ┌─────┼─────┐
    │     │     │            │     │     │            │     │     │
┌───▼─┐ ┌─▼───┐ ┌▼──┐    ┌───▼─┐ ┌─▼───┐ ┌▼──┐    ┌───▼─┐ ┌─▼───┐ ┌▼──┐
│Coop │ │Coop │ │...│    │Coop │ │Coop │ │...│    │Coop │ │Coop │ │...│
│  A  │ │  B  │ │   │    │  C  │ │  D  │ │   │    │  E  │ │  F  │ │   │
└─────┘ └─────┘ └───┘    └─────┘ └─────┘ └───┘    └─────┘ └─────┘ └───┘
```

### Cross-Cooperative Transactions

**Scenario**: Alice (Coop A) pays Bob (Coop B) for consulting

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     CROSS-COOP PAYMENT FLOW                             │
└─────────────────────────────────────────────────────────────────────────┘

1. ALICE initiates payment
   ├── Alice's wallet → Coop A Gateway
   └── "Pay 5 hours to did:icn:bob (Coop B)"

2. COOP A processes
   ├── Debit Alice's account: -5 hours
   ├── Create clearing entry: +5 hours owed to Coop B
   └── Sign cross-coop payment message

3. FEDERATION ROUTING
   ├── Coop A Gateway → Midwest Federation Gateway
   ├── Federation routes to Coop B (same region)
   │   OR
   ├── If different region: → National Federation → Target Region → Coop B
   └── Message includes: signed payment, Alice's trust attestation

4. COOP B processes
   ├── Verify Alice's signature and trust score
   ├── Credit Bob's account: +5 hours
   ├── Create clearing entry: -5 hours from Coop A
   └── Notify Bob via WebSocket

5. FEDERATION SETTLEMENT (periodic)
   ├── Clearing house nets all inter-coop balances
   ├── Net settlement via:
   │   ├── Labor hour transfers
   │   ├── Asset token swaps
   │   └── Or: Fiat bridge (last resort)
   └── Clearing entries zeroed out
```

### Trust Propagation

```
Trust flows transitively through federation:

Alice (Coop A) → Bob (Coop B) direct trust: NONE
Alice (Coop A) → Coop A trust: 0.8 (Partner)
Coop A → Midwest Federation trust: 0.7 (Federated)
Midwest Federation → Coop B trust: 0.6 (Partner)

Transitive trust: Alice → Bob = 0.8 × 0.7 × 0.6 = 0.336 (Known)

Bob can accept Alice's payment because transitive trust > MIN_TRUST (0.1)
Bob's credit limit for Alice is gated by this transitive trust score.
```

### Federation Services

| Service | Purpose | How It Works |
|---------|---------|--------------|
| **Clearing House** | Net inter-coop balances | Periodic settlement, multi-modal (labor/assets/fiat) |
| **Steward Pool** | Shared identity verification | Distributed steward network, threshold ceremonies |
| **Compute Pool** | Shared task execution | Federated executors, cross-coop workloads |
| **Bulk Purchasing** | Collective buying power | Aggregate orders, negotiate as federation |
| **Dispute Resolution** | Cross-coop conflicts | Arbitration panels from multiple coops |

---

## Scaling Considerations

### Horizontal Scaling

**Gateway Layer**:
```
┌─────────────────────────────────────────────────────────────┐
│                    LOAD BALANCER                            │
│                    (HAProxy / Nginx)                        │
└──────────────────────────┬──────────────────────────────────┘
                           │
     ┌─────────────────────┼─────────────────────┐
     │                     │                     │
┌────▼─────┐         ┌─────▼────┐         ┌─────▼────┐
│ Gateway  │         │ Gateway  │         │ Gateway  │
│ Instance │         │ Instance │         │ Instance │
│    1     │         │    2     │         │    3     │
└────┬─────┘         └─────┬────┘         └─────┬────┘
     │                     │                     │
     └─────────────────────┼─────────────────────┘
                           │
                    ┌──────▼──────┐
                    │  ICN Daemon │
                    │  (Single)   │
                    └─────────────┘
```

**Gossip Layer**:
- Adaptive fanout based on network size
- Bloom filters reduce bandwidth
- Sharded topics for high-volume coops

### Database Scaling

**Sled (embedded)**:
- Single-writer, multi-reader
- Good for: <10,000 transactions/day
- Runs in ICN daemon process

**Distributed Storage** (large deployments):
- PostgreSQL + read replicas for gateway state
- Redis for session/cache
- S3-compatible for large blobs

### Compute Scaling

**Auto-scaling Executor Pool**:
```yaml
# Kubernetes HPA for compute nodes
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: icn-compute-pool
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: icn-compute
  minReplicas: 2
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
```

### Performance Targets

**Note**: These are design targets, not measured benchmarks. Actual performance depends on hardware, network conditions, and load. See [docs/performance/](performance/) for benchmark methodology and results.

| Metric | Target | Notes |
|--------|--------|-------|
| Payment latency | < 2 seconds | End-to-end, same coop |
| Cross-coop payment | < 5 seconds | Regional federation |
| WebSocket event delivery | < 100ms | After ledger commit |
| Gateway API p99 | < 200ms | Most endpoints |
| Mobile app startup | < 3 seconds | Cold start to usable |

**Benchmarking resources**:
- `docs/performance/BENCHMARKS.md` - Benchmark suite and results
- `docs/performance/PROFILING.md` - CPU/memory profiling guides
- `docs/performance/TARGETS.md` - Detailed target specifications

---

## Security at Scale

### Defense in Depth

```
Layer 1: Transport
├── TLS 1.3 everywhere
├── Certificate pinning in mobile apps
└── mTLS for inter-node communication

Layer 2: Authentication
├── DID-based challenge/response
├── JWT tokens (short-lived, 1 hour)
├── Refresh tokens (longer-lived, secure storage)
└── Device-bound credentials

Layer 3: Authorization
├── Cooperative membership verification
├── Role-based access control
├── Capability tokens for scoped access
└── Trust-gated rate limits

Layer 4: Application
├── Input validation at gateway
├── CCL fuel metering
├── Compute result verification (BFT)
└── Byzantine fault detection

Layer 5: Data
├── Signed messages (Ed25519)
├── Merkle-DAG auditability
├── Encrypted topic metadata
└── Optional E2E encryption
```

### Rate Limiting by Trust

```
Isolated (< 0.1):   10 requests/second, 1KB max payload
Known (0.1-0.4):    50 requests/second, 10KB max payload
Partner (0.4-0.7):  100 requests/second, 100KB max payload
Federated (0.7+):   200 requests/second, 1MB max payload
```

### Monitoring & Alerting

**Prometheus Metrics**:
- `icn_gateway_requests_total` - Request count by endpoint
- `icn_ledger_transactions_total` - Transaction count
- `icn_gossip_messages_total` - Gossip message count
- `icn_trust_violations_total` - Security violations
- `icn_compute_tasks_total` - Compute task count

**Alerting Thresholds**:
- Gateway error rate > 1%
- Payment latency p99 > 5 seconds
- Byzantine violations > 10/hour
- Node unreachable > 5 minutes

---

## Deployment Checklist

### New Cooperative Setup

```
□ Provision infrastructure
  □ Server(s) for Full Node
  □ Domain name and SSL certificate
  □ Backup storage

□ Deploy ICN daemon
  □ Install icnd binary
  □ Generate node keypair
  □ Configure node.toml
  □ Start daemon, verify peer discovery

□ Deploy Gateway
  □ Configure gateway.toml
  □ Connect to ICN daemon
  □ Enable HTTPS
  □ Set up rate limiting

□ Configure cooperative
  □ Create cooperative entity
  □ Set governance parameters
  □ Configure credit policy
  □ Set up currency (hours, etc.)

□ Member onboarding
  □ Deploy mobile app / web app
  □ Create invite system
  □ Document onboarding process
  □ Train initial stewards

□ Federation (optional)
  □ Apply to regional federation
  □ Configure trust bridge
  □ Test cross-coop payments
  □ Set up clearing participation
```

---

## Related Documents

- [ICN_COMPLETE_ARCHITECTURE_SYNTHESIS.md](ICN_COMPLETE_ARCHITECTURE_SYNTHESIS.md) - Technical architecture
- [ICN_MINIMUM_VERTICAL_SLICE.md](ICN_MINIMUM_VERTICAL_SLICE.md) - End-to-end operation traces
- [ECONOMIC_ARCHITECTURE.md](ECONOMIC_ARCHITECTURE.md) - Economic system details
- [HOMELAB_DEPLOYMENT.md](HOMELAB_DEPLOYMENT.md) - Example deployment
- [dev-journal/ROADMAP.md](dev-journal/ROADMAP.md) - Implementation roadmap

---

## Document History

- 2026-01-23: Added Identity→Membership→Authorization, Identity Lifecycle scenarios
- 2026-01-23: Initial creation from SDK/Gateway analysis
