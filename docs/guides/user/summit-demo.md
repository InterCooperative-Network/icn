# Summit Decision Registry Demo

This demo script demonstrates how ICN replaces Google Drive + bank statements for cooperative meeting decisions. It creates a meeting, records decisions (including a treasury spend), and shows the provenance chain.

## Quick Start

```bash
# Run in demo mode (no server required)
./scripts/summit_demo.sh

# Run against a live gateway
export ICN_GATEWAY="http://localhost:8000"
export ICN_TOKEN="your-auth-token"
./scripts/summit_demo.sh
```

## What It Demonstrates

1. **Meeting Creation** - Creates a summit planning meeting
2. **Text Decisions** - Records three governance decisions:
   - "Approved: Use downtown venue for summit"
   - "Approved: Catering budget cap at 500 credits"
   - "Approved: Invite partner cooperatives"
3. **Treasury Decision** - Records a 200 credit disbursement with full provenance
4. **Decision Registry** - Displays all decisions with canonical hashes
5. **Provenance Trace** - Shows how treasury spend links to the decision

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ICN_GATEWAY` | Gateway REST API URL | `http://localhost:8000` |
| `ICN_TOKEN` | JWT authentication token | (none, enables demo mode) |
| `ICN_COOP_ID` | Cooperative DID | (auto-generated demo DID) |

## Sample Output

```
╔═══════════════════════════════════════════════════════════════╗
║              ICN SUMMIT DECISION REGISTRY DEMO                ║
╚═══════════════════════════════════════════════════════════════╝

▶ Setting up cooperative...
  ✓ Using demo coop: did:icn:demo-summit-1234567890

▶ Creating meeting: "Summit Planning 2026-02-10"...
  ✓ Meeting ID: mtg-2026-02-10-summit-demo

▶ Recording decisions...

  ✓ Decision: "Approved: Use downtown venue for summit" (hash: abc123def456...)
  ✓ Decision: "Approved: Catering budget cap at 500 credits" (hash: 789ghi012jkl...)
  ✓ Decision: "Approved: Invite partner cooperatives" (hash: mno345pqr678...)
  ✓ Decision: "Disburse 200 credits to venue deposit" (hash: stu901vwx234...) [TREASURY]

═══════════════════════════════════════════════════════════════
                     DECISION REGISTRY
═══════════════════════════════════════════════════════════════

┌────┬─────────────────────────────────────────┬───────────────┬──────────────┐
│ #  │ Title                                   │ Type          │ Hash         │
├────┼─────────────────────────────────────────┼───────────────┼──────────────┤
│ 1  │ Approved: Use downtown venue for summit │ text          │ abc123def... │
│ 2  │ Approved: Catering budget cap at 500 cr │ text          │ 789ghi012... │
│ 3  │ Approved: Invite partner cooperatives   │ text          │ mno345pqr... │
│ 4  │ Disburse 200 credits to venue deposit   │ treasury_spend│ stu901vwx... │
└────┴─────────────────────────────────────────┴───────────────┴──────────────┘

═══════════════════════════════════════════════════════════════
                   TREASURY PROVENANCE TRACE
═══════════════════════════════════════════════════════════════

Decision: "Disburse 200 credits to venue deposit"
  └─ Decision Hash: stu901vwx234...
  └─ Decision Receipt ID: receipt-demo-stu901vwx234
  └─ Effect: TreasuryEffect::Spend
       ├─ Amount: 200 credits
       ├─ From: treasury:did:icn:demo-summit-1234567890
       └─ To: did:icn:venue-downtown-deposit
  └─ Ledger Entry: entry-stu901vw (linked via decision_receipt_id)

───────────────────────────────────────────────────────────────

This is what you'd have to reconstruct from:
  - Shared Drive folder (meeting notes v3 final FINAL.docx)
  - Email threads ("did we approve this?")
  - Bank statement ("what was this $200 for?")

With ICN: One query. Canonical. Cryptographically linked.
```

## API Endpoints Used

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/v1/registry/meetings` | Create a meeting |
| POST | `/v1/registry/decisions` | Index a decision |
| GET | `/v1/registry/decisions` | List decisions (with filters) |
| GET | `/v1/registry/decisions/{id}/trace` | Get provenance trace |

## Running Against Live Gateway

1. **Start the ICN gateway**:
   ```bash
   cd icn && cargo run --bin icnd
   ```

2. **Get an authentication token**:
   ```bash
   ./scripts/generate-test-token.sh
   ```

3. **Run the demo**:
   ```bash
   export ICN_GATEWAY="http://localhost:8000"
   export ICN_TOKEN="<your-token>"
   ./scripts/summit_demo.sh
   ```

## Comparison: ICN vs Traditional Tools

| Aspect | Google Drive + Bank | ICN |
|--------|--------------------|----|
| Decision record | Scattered documents | Single canonical entry |
| Treasury link | Manual cross-reference | Automatic provenance |
| Audit trail | Reconstruct from logs | Query by hash |
| Integrity | Trust the platform | Cryptographic proof |
| Search | Full-text guess | Tag + filter + hash |

## Technical Details

### Decision Index Entry Structure

```json
{
  "decisionReceiptId": "receipt-abc123",
  "decisionHash": "abc123def456789...",
  "coopId": "did:icn:my-coop",
  "meetingId": "mtg-2026-02-10-summit",
  "title": "Approved: Use downtown venue",
  "tags": ["summit", "approved"],
  "status": "approved",
  "proposalType": "text",
  "createdAt": 1739203200
}
```

### Treasury Effect Structure

```json
{
  "treasuryEffect": {
    "amount": 200,
    "from": "treasury:did:icn:my-coop",
    "to": "did:icn:venue-account",
    "currency": "credits"
  }
}
```

### Provenance Trace Response

```json
{
  "decisionHash": "abc123...",
  "parentReceipts": [],
  "effects": [
    {
      "type": "treasury_disbursement",
      "details": { "effectId": "effect-xyz123" }
    }
  ],
  "ledgerEntryId": "entry-123",
  "provenanceChain": ["receipt-abc123"]
}
```

## See Also

- [Cooperative Setup Guide](cooperative-setup-guide.md)
- [Why ICN Handout](WHY_ICN_HANDOUT.md)
- [Gateway API Reference](../../reference/api/README.md)
