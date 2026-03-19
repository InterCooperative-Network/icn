# ICN UX Language Guide

**Status**: Enforced — Phase 0 of [Regulatory-Safe Verifiable State Architecture](../design/regulatory-safe-verifiable-state.md)

ICN is communications infrastructure for verifiable cooperative coordination — not a financial
intermediary. The surface language in API endpoints, SDK types, documentation, and UI must
reflect this accurately. Mismatched terminology creates regulatory exposure and confuses what
ICN actually is.

This guide lists terms that are **forbidden** in public-facing API surfaces, SDK types, and
user-visible text, alongside the approved alternatives.

---

## Forbidden Terms and Approved Alternatives

### `payment` / `pay`

| Context | Forbidden | Approved |
|---------|-----------|----------|
| API endpoint | `POST /ledger/{id}/payment` | `POST /ledger/{id}/settle` |
| API endpoint | `POST /payments/recurring` | `POST /settlements/recurring` |
| Request type | `CreatePaymentRequest` | `RecordTransferRequest` |
| SDK method | `client.pay()` | `client.settle()` |
| SDK method | `client.batchPay()` | `client.batchSettle()` |
| Event type | `PaymentCreated` | `SettlementCreated` |
| Scope string | `payments:read` / `payments:write` | `settlements:read` / `settlements:write` |
| UI label | "Make a payment" | "Record a settlement" |
| UI label | "Payment history" | "Settlement history" |

**Why**: "Payment" implies the operator is routing value between accounts (Money Services Business
trigger). ICN records user-signed state transitions; it does not process payments.

---

### `currency`

| Context | Forbidden | Approved |
|---------|-----------|----------|
| Request field | `{"currency": "hours"}` | `{"unit": "hours"}` |
| SDK type field | `currency: string` | `unit: string` |
| UI label | "Currency" | "Unit of account" |
| Variable name | `let currency = ...` | `let unit = ...` |

**Why**: "Currency" implies a fiat-like regulated instrument. ICN units are locally-defined measures
of cooperative capacity (hours, compute-units, participation-units). They are not currencies.

**Exception**: The FX oracle (`icn-ledger/src/oracle/`) uses `CurrencyPair` and `ExchangeRate`
internally to model rates between external systems. This is a technical adapter, not a user-facing
concept. Do not expose these types in gateway APIs or SDK types.

---

### `balance`

| Context | Forbidden | Approved |
|---------|-----------|----------|
| API endpoint | `GET /ledger/{id}/balance/{did}` | `GET /ledger/{id}/position/{did}` |
| API endpoint | `GET /treasury/{id}/balance` | `GET /treasury/{id}/position` |
| Response field | `{"balance": 42}` | `{"position": 42}` |
| Response field | `{"balances": {...}}` | `{"positions": {...}}` |
| SDK method | `client.getBalance()` | `client.getPosition()` |
| SDK type field | `balance: number` | `position: number` |
| Handler name | `get_balance()` | `get_position()` |
| UI label | "Balance" | "Net position" |

**Why**: A "balance" is a stored value under operator control — the MSB custody trigger. A
"position" is a derived view computed from the set of signed receipts and accepted obligations.
The protocol primitive is the signed entry; the position is interpretation.

---

### `wallet`

| Context | Forbidden | Approved |
|---------|-----------|----------|
| UI component | "My Wallet" | "My Account" or "My Accounts" |
| SDK type | `Wallet` interface | `Account` interface |
| API concept | "wallet address" | "member account" or "DID" |

**Why**: "Wallet" in a hosted context implies the operator controls the keys and can move balances
unilaterally. ICN accounts are identified by DID; the member controls the signing key.

**Exception**: "Unhosted wallet" in regulatory/legal documentation is acceptable — it describes
ICN's non-custodial architecture relative to the regulatory concept.

---

### `send funds` / `transfer funds`

| Context | Forbidden | Approved |
|---------|-----------|----------|
| UI button | "Send funds" | "Create settlement" or "Record transfer" |
| UI button | "Transfer funds" | "Propose transfer" |
| Documentation | "send funds to..." | "settle an obligation with..." |

**Why**: "Send funds" describes an operator-mediated transfer. ICN broadcasts user-signed state
transitions — the gateway does not move anything on the user's behalf.

---

### `exchange rate` (in user-facing contexts)

| Context | Forbidden | Approved |
|---------|-----------|----------|
| API field | `exchange_rate: f64` | _(omit entirely; use governance proposals)_ |
| UI label | "Exchange rate" | _(not applicable to internal units)_ |
| SDK type | `exchangeRate` field | _(not applicable)_ |

**Why**: Embedding exchange rates in the protocol implies convertibility — a securities/MSB trigger.
Unit conversion between internal cooperative units is a governance decision, not a protocol primitive.
If a CCL policy defines a conversion, it is policy output, not a gateway API field.

---

### `redeem`

| Context | Forbidden | Approved |
|---------|-----------|----------|
| API endpoint | `POST /redeem` | _(not applicable)_ |
| UI button | "Redeem credits" | _(not applicable; describe the actual action)_ |
| Documentation | "redeem your hours for..." | _(reframe as fulfillment of an obligation)_ |

**Why**: "Redeem" implies convertibility to an external asset (cash, fiat). ICN units fulfill
cooperative obligations — they are not redeemable instruments.

---

### `money` / `funds`

| Context | Forbidden | Approved |
|---------|-----------|----------|
| Documentation | "move money" | "settle obligations" |
| Documentation | "hold funds" | "track obligations" |
| UI | "your money" | "your participation credits" or "your net position" |
| Commit message | "payment of funds" | "settlement of obligation" |

**Why**: "Money" and "funds" trigger financial-services classification. ICN tracks obligations and
attestations — the economic relationship between participants — not money.

---

## Terms That Are Explicitly Approved

These terms are architecturally accurate and should be used freely:

| Term | Notes |
|------|-------|
| `settlement` / `settle` | Recording a bilateral obligation acknowledgement |
| `position` | Derived net view computed from signed entries |
| `unit` (of account) | Locally-defined cooperative measure |
| `obligation` | A signed commitment between participants |
| `attestation` | A signed statement by one DID about another |
| `receipt` | Cryptographic proof of execution |
| `journal entry` / `JournalEntry` | Standard double-entry bookkeeping term |
| `debit` / `credit` | Standard bookkeeping mechanics — keep as-is |
| `account` | A member's ledger context within a cooperative |
| `transfer` | The act of recording a bilateral entry (not operator routing) |
| `allocation` | A governance-authorized resource assignment |
| `proposal` | A governance action requiring member vote |
| `provenance` | The authorization chain behind a state change |

---

## Scope of This Guide

**In scope** (enforced):
- Gateway REST API endpoints (`/v1/...`)
- WebSocket event type names
- OAuth scope strings
- SDK method names and type fields (`sdk/typescript/`, `sdk/react-native/`)
- User-visible UI text in `web/pilot-ui/`
- Public documentation in `docs/`

**Out of scope** (internal, not enforced here):
- `icn-ledger` internals (`get_balance()`, `recompute_balances()`) — standard bookkeeping
- FX oracle types (`CurrencyPair`, `ExchangeRate`, `is_same_currency()`) — technical adapter
- Bond accounting in `labor_shares.rs` (`BondPayment`, `record_payment()`)
- Test fixture names and internal variable names that never surface to users

---

## Enforcement

1. **CI gate** — the `compliance-linter` job (`.github/scripts/compliance_linter.py`) scans
   gateway, SDK, and UI files for forbidden terms on every PR. Currently `warning` phase
   (non-blocking); will ratchet to `blocking` once pre-existing violations are resolved.
2. **PR checklist** — the CONTRIBUTING.md regulatory invariants checklist includes a language check.
3. **Code review** — reviewers should flag forbidden terms in any changed API, SDK, or UI file.

**See also**: [Regulatory-Safe Verifiable State Architecture](../design/regulatory-safe-verifiable-state.md)
