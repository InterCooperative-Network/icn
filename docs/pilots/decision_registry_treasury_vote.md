# Decision Registry + Treasury Vote Pilot

This document describes the economic receipt chain implemented in Sprint 8-9, demonstrating ICN's deterministic governance → economics → ledger flow.

## Overview

ICN's pilot program proves that governance decisions produce verifiable economic artifacts:

```
GovernanceDecision → AllocationReceipt → SettlementIntent → LedgerEntry
                           ↓                    ↓               ↓
                    canonical_hash       canonical_hash    provenance
```

**Key Invariant**: Two nodes independently processing the same decision produce identical canonical hashes at every step.

## Economic Receipt Types

### AllocationReceipt

Bridges governance decisions to economic intents.

```rust
pub struct AllocationReceipt {
    pub receipt_id: [u8; 32],      // Node-local (excluded from canonical hash)
    pub decision_hash: [u8; 32],   // Links to governance decision
    pub intents: Vec<SettlementIntent>,
    pub scope: ScopeLevel,
    pub created_at: u64,
}
```

### SettlementIntent

Declarative economic payload - "what should happen".

```rust
pub struct SettlementIntent {
    pub intent_id: Option<String>, // Node-local (excluded from canonical hash)
    pub decision_receipt_id: String,
    pub decision_hash: [u8; 32],
    pub asset: AssetType,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub unit: String,
    pub scope: ScopeLevel,
    pub memo: Option<String>,      // Excluded from canonical hash
    pub created_at: u64,
}
```

### AssetType

Semantic value classification:

| Type | Description | Example |
|------|-------------|---------|
| `Fungible` | Standard mutual credit | Treasury hours |
| `Consumable` | Single-use, may expire | Compute vouchers |
| `Depreciating` | Loses value over time | Equipment licenses |
| `Transformable` | Converts to other assets | Raw materials |
| `Service` | Non-storable value | Labor hours rendered |
| `Claim` | Future performance IOU | Receivables, bonds |

## API Endpoints

### Receipt Queries

```bash
# Get allocation receipt by canonical hash
GET /v1/receipts/allocations/{hash}

# List allocation receipts by decision
GET /v1/receipts/allocations?decision_hash={hex}

# Get settlement intent by canonical hash
GET /v1/receipts/intents/{hash}

# List settlement intents by decision
GET /v1/receipts/intents?decision_hash={hex}

# Get full economic chain for a decision
GET /v1/receipts/chain?decision_hash={hex}
```

### Ledger Provenance Queries

```bash
# Ledger entries with provenance for a decision
GET /v1/ledger/{coop_id}/entries/by-decision?decision_hash={hex}
```

Response includes `decision_receipt_id` and `decision_hash` fields for each transaction:

```json
{
  "transactions": [
    {
      "id": "tx-uuid",
      "timestamp": 1704067200,
      "author": "did:icn:abc123...",
      "accounts": [
        {
          "accountId": "treasury:main",
          "debit": 1000,
          "currency": "HOURS"
        },
        {
          "accountId": "member:alice",
          "credit": 1000,
          "currency": "HOURS"
        }
      ],
      "decisionReceiptId": "decision-001",
      "decisionHash": "2a2a2a2a2a2a2a2a..."
    }
  ],
  "pagination": {
    "page": 1,
    "perPage": 10,
    "total": 1,
    "hasMore": false
  }
}
```

> **Note:** Ledger entries use double-entry accounting with account deltas (credit/debit) rather than simple from/to transfers.

### Receipt Chain Example

```json
{
  "decisionHash": "2a2a2a2a2a2a2a2a...",
  "allocations": [
    {
      "canonicalHash": "8e1779e535438507...",
      "decisionHash": "2a2a2a2a2a2a2a2a...",
      "scope": "Org",
      "createdAt": 1704067200,
      "intentCount": 1,
      "intentHashes": ["dfe9845ac806f40b..."]
    }
  ],
  "intents": [
    {
      "canonicalHash": "dfe9845ac806f40b...",
      "decisionReceiptId": "decision-001",
      "decisionHash": "2a2a2a2a2a2a2a2a...",
      "assetType": "Fungible",
      "from": "treasury",
      "to": "member-alice",
      "amount": 1000,
      "unit": "HOURS",
      "scope": "Org",
      "memo": null,
      "createdAt": 1704067200
    }
  ]
}
```

## Demo Scripts

### Economics Determinism Demo

```bash
./scripts/pilot_economics_demo.sh
```

Runs three test suites:
1. **Two-node determinism**: Proves cross-node hash equality
2. **CanonicalReceipt tests**: Hash stability + serde roundtrip
3. **SettlementIntent tests**: Asset type semantics

Expected output:
```
ECONOMIC DETERMINISM TRIPWIRES
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ TRIPWIRE: Cross-node determinism verified
✅ TRIPWIRE: SettlementIntent hash stability verified
✅ TRIPWIRE: AllocationReceipt tests passed
✅ TRIPWIRE: All test suites passed

═══════════════════════════════════════════════════════════════════
  ECONOMIC DETERMINISM INVARIANT PROVEN
```

### Governance Chain Demo

```bash
./scripts/pilot_chain_demo.sh
```

Proves the complete governance → ledger provenance chain.

### Receipt Chain Demo (Full Visibility)

```bash
./scripts/pilot_receipt_chain_demo.sh
```

Runs determinism tests + queries gateway API (if running):

```
RECEIPT CHAIN INVARIANTS
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ Cross-node determinism: VERIFIED
✅ Intent ordering determinism: VERIFIED
✅ Provenance chain integrity: VERIFIED

════════════════════════════════════════════════════════════════════
  RECEIPT CHAIN DEMO COMPLETE
```

## Proof of Legitimacy Walkthrough

This section shows three ways to verify the economic chain is working correctly.

### Method 1: CLI (icnctl)

```bash
# Query the full chain for a decision
icnctl receipts chain <decision_hash_64hex> --gateway http://localhost:8080 --coop-id my-coop

# Get a single allocation receipt
icnctl receipts allocation <canonical_hash_64hex> --gateway http://localhost:8080

# Get a single settlement intent
icnctl receipts intent <canonical_hash_64hex> --gateway http://localhost:8080

# JSON output for scripting
icnctl receipts chain <hash> --json | jq '.allocations[0].canonicalHash'
```

### Method 2: REST API

```bash
# Query full chain
curl http://localhost:8080/v1/receipts/chain?decision_hash=2a2a2a...

# Query ledger entries by decision
curl http://localhost:8080/v1/ledger/my-coop/entries/by-decision?decision_hash=2a2a2a...

# Get single allocation
curl http://localhost:8080/v1/receipts/allocations/<canonical_hash>

# Get single intent
curl http://localhost:8080/v1/receipts/intents/<canonical_hash>
```

### Method 3: Pilot UI

1. Open Pilot UI in browser
2. Navigate to **Receipts** tab
3. Paste decision hash (64 hex characters)
4. Click **Query Chain**
5. View:
   - Allocation receipts with **canonicalized** badge (sorted verification)
   - Settlement intents with amounts/accounts
   - Ledger entries with provenance links
6. Click **Copy** to get chain summary for bug reports

### Verification Checklist

| Check | How to Verify |
|-------|---------------|
| Cross-node determinism | Run `pilot_receipt_chain_demo.sh` - all tripwires green |
| Order-independence | AllocationReceipt hash unchanged regardless of intent order |
| Provenance linkage | Ledger entries show `decision_hash` field |
| Sorted intents | UI shows "✓ canonicalized" badge on allocation |
| Restart durability | After `icnd` restart, same hashes returned |

## Invariants

### Cross-Node Determinism

Same economic inputs → same canonical hashes:

```
Node A: intent_id="node-a-001" → canonical_hash = dfe9845a...
Node B: intent_id="node-b-777" → canonical_hash = dfe9845a...
                                             ✅ MATCH
```

Node-local identifiers (`intent_id`, `receipt_id`, `memo`) are excluded from canonical hash computation.

### Provenance Chain Integrity

Every ledger entry from economic intents carries:
- `decision_receipt_id`: Links to governance decision
- `decision_hash`: Canonical anchor for cross-node verification

### Hash Stability Across Restart

Receipts persist in durable storage. After node restart:
- Same canonical hashes are computed
- Chain queries return identical results

## Test Coverage

| Test | File | Purpose |
|------|------|---------|
| `test_economic_chain_canonical_hash_determinism` | `economic_chain_two_node.rs` | Two-node hash equality |
| `test_receipt_store_persistence_across_restart` | `receipt_persistence_e2e.rs` | Durability proof |
| `test_allocation_receipt_*` | `receipts.rs` | Hash stability tests |
| `test_settlement_intent_*` | `economics.rs` | Intent canonicalization |

## Architecture

```
┌─────────────────┐
│  Governance     │
│  Decision       │
└────────┬────────┘
         │ ProposalAccepted
         ▼
┌─────────────────┐
│  Effect         │
│  Translator     │
└────────┬────────┘
         │ TreasuryEffect::Spend
         ▼
┌─────────────────┐
│  Kernel         │
│  Executor       │
└────────┬────────┘
         │ SettlementIntent
         ▼
┌─────────────────┐
│  ReceiptStore   │  ← Persists AllocationReceipt + SettlementIntent
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  LedgerService  │  ← Creates entry with provenance
└─────────────────┘
```

## What This Enables

1. **Dispute Resolution**: Explicit intents to contest
2. **Audit Trail**: Complete decision → ledger chain
3. **Federation**: Canonical payloads for gossip
4. **CCL Integration**: Economic clauses with teeth

## Related Documentation

- [ARCHITECTURE.md](../ARCHITECTURE.md) - System architecture
- [governance-primitives.md](../design/governance/governance-primitives.md) - Governance design
- `Sprint 8 Plan`: `.copilot/session-state/*/plan.md` (local planning artifact, not tracked in repo)
