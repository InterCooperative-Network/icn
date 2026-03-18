# ICN Economic Architecture

**Last Updated**: 2026-01-17
**Status**: Design Document

---

## Overview

This document describes the technical architecture of ICN's economic system. It complements [ECONOMIC_VISION.md](ECONOMIC_VISION.md) which covers the strategic framing.

The economic system is designed around three principles:
1. **Grounded in reality** — tokens represent real things, not financial abstractions
2. **Speculation-resistant** — mechanisms prevent value extraction without contribution
3. **Incrementally deployable** — each layer is independently useful

---

## The Layered Economy

ICN's economic system has three layers, each serving different needs:

```
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 3: FIAT BRIDGES (Controlled Interfaces)                  │
│  • Treasury cooperatives hold fiat reserves                     │
│  • Exchange only through governed gateways                      │
│  • Rate bands, not free markets                                 │
│  • For: taxes, external imports, medical systems                │
└─────────────────────────────────────────────────────────────────┘
                              ↑↓
                    Governed conversion (not free market)
                              ↑↓
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 2: ASSET TOKENS (Transformation-Tracked)                 │
│  • Represent real goods: tools, materials, products             │
│  • Created only via attested transformations                    │
│  • Carry full provenance chains                                 │
│  • Exchange via negotiation within guardrails                   │
│  • Optional demurrage on idle hoarding                          │
└─────────────────────────────────────────────────────────────────┘
                              ↑↓
                    Labor inputs to transformations
                              ↑↓
┌─────────────────────────────────────────────────────────────────┐
│  LAYER 1: MUTUAL CREDIT (Pure Coordination)                     │
│  • Hours or community-defined units                             │
│  • For services, labor, daily exchange                          │
│  • Reputation-gated limits                                      │
│  • No backing required (it's coordination, not money)           │
└─────────────────────────────────────────────────────────────────┘
                              ↑↓
                    Built on
                              ↑↓
┌─────────────────────────────────────────────────────────────────┐
│  SUBSTRATE: ICN Core                                            │
│  Identity • Trust Graph • Ledger • CCL • Governance • Gossip    │
└─────────────────────────────────────────────────────────────────┘
```

### Layer 1: Mutual Credit

**Purpose**: Coordinate service exchange between members

**Characteristics**:
- Unit of account is hours or community-defined
- Zero-sum: every credit creates a matching debit
- No external backing required
- Works entirely within network

**Use Cases**:
- Timebanking (1 hour = 1 hour)
- Service exchange between members
- Labor contribution tracking

**Safety Mechanisms**:
- Reputation-gated credit limits
- Balance transparency
- Dispute/freeze mechanisms

### Layer 2: Asset Tokens

**Purpose**: Track ownership and provenance of real goods

**Characteristics**:
- Each token represents a real physical item
- Creation requires attestation (proof it exists)
- Full provenance chain from origin
- Exchange via negotiation, not markets

**Use Cases**:
- Equipment sharing between coops
- Tool libraries
- Material tracking through supply chains
- Inventory management

**Safety Mechanisms**:
- Attestation requirements for creation
- Bonded witnesses for high-value items
- Challenge windows for disputes

### Layer 3: Fiat Bridges

**Purpose**: Interface with external economy when necessary

**Characteristics**:
- Controlled gateways, not free exchange
- Rate bands prevent speculation
- Governed by treasury cooperatives
- Minimized by design (internal first)

**Use Cases**:
- Tax payments
- External supplier payments
- Medical expenses
- Imports not available internally

**Safety Mechanisms**:
- Governance approval for large conversions
- Rate bands (not free market prices)
- Reserve requirements

---

## Failure Mode Defenses

The economic system must defend against known failure modes of mutual credit and alternative currency systems.

### Failure Mode 1: Soft Budget Constraint

**Problem**: Accounts accumulate large negatives with no real consequence, leading to over-issuance and confidence collapse.

**Defense: Reputation-Gated Credit Limits**

```
credit_limit = f(
    participation_history,    // How long active, how consistent
    attestations_received,    // Vouches from other members
    contracts_fulfilled,      // Track record of commitments
    dispute_record           // History of problems
)
```

Credit limits are not fixed numbers but living variables computed from the trust graph. The deeper someone's positive participation, the higher their limit.

**Defense: Parameter Guardrails**

Constitutional ranges enforced at protocol level:
- Minimum/maximum credit limits per trust class
- Maximum rate of limit increase
- Mandatory review thresholds

### Failure Mode 2: Hit-and-Run

**Problem**: Someone joins, consumes value, disappears.

**Defense: Identity Continuity**

DIDs are portable but reputation follows. You cannot "delete your account" — your identity and history persist. Leaving one community with bad standing affects your reputation across the federation.

**Defense: Federated Reputation Propagation**

Misbehavior attestations propagate via gossip. Bad actors in one coop face reduced trust network-wide.

**Defense: Graduated Onboarding**

New members may have:
- Lower initial credit limits
- Sponsorship requirements
- Probationary periods before full privileges

### Failure Mode 3: Balance Asymmetry

**Problem**: One coop becomes perpetual importer, another perpetual exporter, creating unsustainable imbalances.

**Defense: Clearing Bands** (Federation Level)

Each coop maintains target balance ranges with the federation. Exceeding triggers:
- Automatic demurrage on surplus
- Contribution requirements for deficit
- Mediated rebalancing conversations

**Defense: Multi-Modal Settlement**

Imbalances can be settled via:
- Labor hours
- Asset tokens
- Fiat (last resort)
- Service commitments

### Failure Mode 4: Valuation Chaos

**Problem**: If everything is priced arbitrarily, trust evaporates.

**Defense: Reference Units**

Communities maintain reference exchange rates as social agreements:
- "1 hour general labor ≈ X credits"
- "1 table ≈ Y hours carpentry"
- Updated periodically by governance

These are not market prices but community baselines for negotiation.

**Defense: Negotiation Within Guardrails**

Parties negotiate directly, but:
- Exchanges must be within ±N% of reference (configurable)
- Outlier transactions flagged for review
- Disputes go to community arbitration

### Failure Mode 5: Governance Capture

**Problem**: Large actors dominate issuance rules.

**Defense: Quadratic Voice**

Changes to monetary parameters require weighted consensus where voice scales sub-linearly with stake (quadratic voting or similar).

**Defense: Protocol-Level Guardrails**

Some parameters have hard limits that governance cannot override:
- Maximum individual credit limit
- Minimum attestation requirements
- Challenge window durations

---

## The Transformation Model

When materials are transformed into products (lumber → table), the system tracks this cryptographically.

### Transformation Structure

```
Transformation {
    inputs: [
        (token_id: lumber_batch_42, quantity: 10, unit: "board_feet"),
        (token_id: labor_hours, quantity: 4, unit: "hours")
    ],
    outputs: [
        (token_type: "furniture/table", quantity: 1, grade: "standard")
    ],
    transformer: did:icn:carpenter_alice,
    recipe: "furniture/table/basic",
    attestations: [
        (witness: did:icn:bob, claim: "observed transformation"),
        (inspector: did:icn:quality_guild, claim: "grade verified")
    ],
    provenance_root: <merkle_root_of_input_histories>,
    timestamp: 2026-01-17T14:30:00Z
}
```

### Key Properties

1. **Inputs are consumed** (UTXO-style) — lumber tokens are "spent" when transformed
2. **Outputs carry provenance** — the table token contains a merkle root linking to all inputs
3. **Recipes are governed** — valid transformations must match approved patterns
4. **Attestations prevent fraud** — witnesses verify transformation actually happened

### Provenance Chains

Every asset token carries its history:

```
forest_plot_7 → lumber_batch_42 → table_0891
     ↑              ↑                ↑
  harvest       milling          carpentry
  attestation   attestation      attestation
```

Anyone can verify: "This table came from sustainably harvested lumber from plot 7, milled by Coop A, built by Alice."

### Recipe Governance

Recipes define valid transformation patterns:

```
Recipe {
    id: "furniture/table/dining",
    version: 3,
    inputs: [
        (material: "lumber/hardwood", min: 8, max: 12, unit: "board_feet"),
        (labor: "carpentry/journeyman+", min: 3, max: 6, unit: "hours")
    ],
    outputs: [
        (asset: "furniture/table/dining", quantity: 1)
    ],
    quality_grades: ["standard", "fine", "heirloom"],
    required_attestations: ["transformer_capability", "material_inspection"],
    approved_by: governance_proposal_847
}
```

**Tiered Approval**:

| Recipe Type | Approval Path |
|-------------|---------------|
| Variant of existing | Guild review or auto-approve |
| New category | Community vote + expert review |
| Cross-domain | Multi-guild coordination |
| Safety-critical | Stringent review + testing period |

**Experimental Recipes**: Allow transformation but mark outputs as "experimental" with limited transferability until recipe graduates to approved status.

---

## Physical Verification

How do you know the lumber actually exists before tokenizing it?

### Layered Verification by Value

| Value Level | Verification Required |
|-------------|----------------------|
| Low (vegetables, small repairs) | Self-attestation + reputation stake |
| Medium (lumber, tools, bulk materials) | Peer witness (2-of-3 attestations) |
| High (vehicles, housing, equipment) | Inspector guild + bonded collateral |

### Attestation Structure

```
Attestation {
    attester: did:icn:witness_bob,
    claim: "lumber_batch_42 exists, 200 board_feet, grade A",
    stake: 50 reputation_points,
    challenge_window: 30 days,
    evidence: [photo_hashes...],

    // If challenged and found false:
    penalty: stake * 3 + attestation_privileges_suspended
}
```

### Collusion Mitigation

- Attestation graph analysis flags suspicious patterns
- Same small group always attesting each other → audit trigger
- High-value goods require diverse attestation sources
- Economic bonds (not just reputation) for highest-value items

---

## Partial Consumption and Fungibility

### Discrete vs. Fungible Goods

Different goods need different models:

**Discrete goods** (tools, vehicles, furniture):
- Individual tokens, UTXO-style
- Clear ownership, no splitting
- Full provenance per item

**Bulk fungibles** (fuel, grain, lumber by board-foot):
- Pool tokens with quantity tracking
- Provenance tracked by batch proportions
- First-in-first-out for consumption

**Consumables** (food, supplies):
- Simple quantity tokens
- Batch-level provenance optional

### Resource Type Definition

```
ResourceType {
    id: "lumber/pine/construction",
    fungibility: Fungible { unit: "board_feet" },
    provenance_tracking: BatchLevel,
    depreciation: None,
    quality_grades: ["construction", "select", "premium"]
}

ResourceType {
    id: "furniture/table/dining",
    fungibility: Discrete,
    provenance_tracking: Full,
    depreciation: ConditionBased,
    quality_grades: ["standard", "fine", "heirloom"]
}
```

---

## Quality Grades and Depreciation

### Grade-at-Transformation

Quality is attested when the item is created:

```
GradeAttestation {
    token: table_token_0891,
    grade: "fine",
    grader: did:icn:furniture_guild,
    evidence: [photo_hashes...],
    challenge_window: 14 days
}
```

**Dispute Resolution**:
1. Receiver challenges within window
2. Evidence submitted by both parties
3. Arbitration by guild panel or community vote
4. Outcome: grade adjustment, reputation impact, possible compensation

### Depreciation Tracking

For high-value assets:

```
AssetToken {
    id: truck_token_42,
    resource_type: "vehicle/truck/box",
    created: 2020-01-01,
    condition_history: [
        (2024-01-01, "good", attester: mechanic_guild, notes: "new tires"),
        (2025-06-01, "fair", attester: mechanic_guild, notes: "transmission wear")
    ],
    maintenance_log: [
        (2024-03, "oil change", attester: garage_coop),
        (2024-06, "brake pads", attester: garage_coop)
    ]
}
```

**Retirement Events**:

```
RetirementEvent {
    token: truck_token_42,
    reason: "totaled in accident",
    attestation: [insurance_coop, owner],
    // Token marked retired, cannot be transferred
    // History preserved for provenance queries
}
```

---

## Cross-Community Exchange

### Hierarchical Type Ontology

```
Global Layer: Basic resource ontology (wood, metal, food, shelter...)
    ↓
Regional Layer: Refined types (lumber/hardwood, lumber/softwood...)
    ↓
Community Layer: Local specifics (lumber/hardwood/bob's_mill)
```

Each layer maps to the one above. Cross-community exchange happens at shared layer.

### Equivalence Mapping

Communities use local naming. When they trade, they register equivalences:

```
TypeEquivalence {
    community_a: "palm_city/furniture/dining_table",
    community_b: "river_coop/woodwork/table_4seat",
    equivalence: Equivalent,
    attested_by: [federation_council, both_communities]
}
```

### Cross-Community Transfer

```
CrossCommunityTransfer {
    token: table_token_0891,
    from_community: palm_city_coop,
    to_community: river_valley_federation,

    source_type: "palm_city/furniture/table/dining",
    target_type: "river_valley/woodwork/table",
    mapping: registered_equivalence_47,

    exchange_terms: {
        offered: table_token_0891,
        received: 35 river_valley_hours,
        negotiated_by: [alice, bob],
        within_guardrails: true  // ±20% of reference
    },

    export_attestation: palm_city_trade_committee,
    import_attestation: river_valley_trade_committee
}
```

---

## Implementation Staging

The economic system is built incrementally. Each stage is independently useful.

### Stage A: Mutual Credit (Current)

- Hours-based exchange
- Reputation-gated credit limits
- Basic governance
- Dispute mechanisms

**What this enables**: Timebanks, service exchange, labor tracking

### Stage B: Simple Asset Tokens

- Tokens represent "this thing exists"
- Attestation-backed creation
- Ownership transfer
- No transformation yet

**Minimum Viable Listing**:

```
Listing {
    id: unique,
    type: "offer" | "want",
    item: "Commercial convection oven, Vulcan brand",
    description: "Doesn't fit our doorways. Original cost $1200.",
    photos: [hash1, hash2],
    offered_by: did:icn:abundance_coop,
    seeking: "Credits, labor exchange, or make an offer",
    visible_to: ["ny_food_coop_federation"],
    expires: 2026-03-01,
    status: "active" | "matched" | "completed"
}
```

**What this enables**: Tool libraries, equipment sharing, cooperative marketplace

### Stage C: Transformation System

- Recipes as CCL contracts
- Input consumption, output creation
- Provenance tracking
- Quality grading

**What this enables**: Maker spaces, manufacturing coops, supply chain transparency

### Stage D: Cross-Community Exchange

- Bilateral equivalence mappings
- Federation-level clearing
- Shared type vocabulary (emergent)

**What this enables**: Regional cooperative economies, inter-federation trade

### Stage E: Fiat Bridges

- Treasury cooperatives with reserves
- Governed conversion gateways
- Rate bands (not free market)

**What this enables**: External interface when internal options exhausted

---

## Relation to ICN Crates

| Economic Concept | ICN Component |
|-----------------|---------------|
| Mutual credit | `icn-ledger` |
| Credit limits | `icn-ledger` + `icn-trust` |
| Asset tokens | `icn-ledger` (extension) |
| Attestations | `icn-trust` + `icn-gossip` |
| Recipes | `icn-ccl` (contract templates) |
| Governance | `icn-governance` |
| Cross-community | `icn-federation` |
| Listings/exchange | Gateway extension (new) |

---

## Open Questions

1. **Collusion resistance**: Is reputation penalty + economic bonds sufficient? What else is needed?

2. **Recipe complexity**: Should recipes be CCL contracts or a separate registry?

3. **Type ontology governance**: Who defines and maintains the global layer?

4. **Depreciation curves**: How arbitrary is "standard vehicle depreciation"? Should we avoid it?

5. **Fiat bridge legal structure**: What legal entity holds reserves?

---

## Document History

- 2026-01-17: Initial creation from design brainstorming session
