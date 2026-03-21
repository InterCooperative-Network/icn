# ADR-0004: ICN Base Layer Models Obligations and Attestations, Not Payment Rails

**Date**: 2026-03-01
**Status**: accepted
**Tags**: economics, governance, compliance, kernel, orchestration

## Context

ICN tracks the exchange of labor, resources, and cooperative capacity among peers. This creates regulatory classification risk if the architecture resembles financial infrastructure: FinCEN MSB classification (triggered by intermediation + custody), IRS barter exchange classification (triggered by operating as a brokered marketplace), and regulatory drift (future features accidentally introducing custodial patterns). The kernel-api already states ICN's thesis ("money is not value, it's a claim on value") and AssetType::Claim already exists. The settlement engine is already a neutral kernel enforcer. ExecutionReceipt and GovernanceDecisionReceipt already exist. The surface language (payment, currency, balance) creates unnecessary regulatory exposure that does not reflect the actual architecture.

## Decision

The ICN base layer models obligations (signed acknowledgements of owed cooperative capacity) and attestations (signed claims about delivered services/events), not payment rails. Balances and net positions are derived views computed from the attestation/obligation graph — they are interpretations, not protocol primitives. Seven architecture invariants are enforced: (1) user-signed transitions only, (2) no hosted balances, (3) no operator routing of value, (4) derived views are not authoritative, (5) no embedded convertibility, (6) matching/market features are opt-in and governance-scoped, (7) execution receipts close the governance loop. The AssetType::Claim variant in icn-kernel-api is the kernel primitive for obligations. JournalEntry provenance (decision_receipt_id) is made required rather than optional. API surface is renamed from payment/currency/balance semantics to settlement/unit/position semantics.

## Consequences

Becomes easier: positioning ICN as Digital Public Infrastructure in grant applications (Mozilla Democracy x AI, NSF CIVIC, NLnet); avoiding FinCEN MSB classification by eliminating intermediation trigger behaviors from the protocol surface; avoiding IRS barter exchange classification by operating as a governance container and attestation graph rather than a brokered marketplace; NY Corp Law Section 93 compliance through PatronageTracker aligning with internal capital account model. Becomes harder: developers must understand the distinction between 'the ledger enforces mechanics' and 'apps interpret meaning'; derived balance views must not be confused with protocol state; any future feature that introduces custody or operator-mediated transfer requires explicit invariant review.

## Alternatives Considered

Alternative 1: Token transfer primitive — model economics as token transfers with a native token. Rejected because it directly triggers MSB/securities classification and eliminates the software provider exemption. Alternative 2: Operator-run clearing — maintain a central clearinghouse for netting obligations. Rejected because clearing is intermediation; it is the definition of what FinCEN regulates. Alternative 3: Terminology-only change with no structural work — just rename fields without fixing structural gaps (optional provenance, formula in kernel, no obligation lifecycle). Rejected because terminology without structure is regulatory theater, not architectural safety.
