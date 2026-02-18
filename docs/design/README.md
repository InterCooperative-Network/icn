# Design Documentation

Design specifications, truth contracts, and architectural decisions for ICN subsystems.

## Truth Contracts (Authoritative)

These documents are the **ground-truth audit** of what ICN can and cannot do today.
They are authoritative for pilot planning, SDK documentation, and implementation prioritization.
Every claim has been verified against the codebase. If a document says "Not Supported", it means no working end-to-end path exists.

| Document | Scope | Date |
|----------|-------|------|
| [Economics Model Validation](economics/model-validation.md) | Gap matrix for 4 cooperative archetypes (worker, consumer, housing, federation) | 2026-02-17 |
| [Economics Truth Contract](economics/economics-truth-contract.md) | Detailed per-operation audit of ledger, treasury, FX, escrow, labor shares, clearing | 2026-02-17 |
| [Governance Model Validation](governance/model-validation.md) | Gap matrix for 5 governance scenarios (elections, budgets, bylaws, emergencies, disputes) | 2026-02-17 |
| [Execution Bridge Spec](execution-bridge-spec.md) | Decision Executor design: governance -> economic action pipeline, idempotency, receipts | 2026-02-17 |

## Subsystem Design

### Economics
- [Economic Architecture](economics/ECONOMIC_ARCHITECTURE.md) - Core economic system design
- [Economic Vision](economics/ECONOMIC_VISION.md) - Long-term economic vision
- [Economic Modeling](economics/econ-modeling.md) - Modeling and simulations
- [Economic Safety](economics/economic-safety.md) - Safety rails and risk management
- [Contribution Credits](economics/contribution-credits-design.md) - Contribution credit system

### Governance
- [Project Governance](governance/PROJECT_GOVERNANCE.md) - ICN project governance structure
- [Governance Concepts](governance/governance.md) - Core governance mechanisms
- [Governance Primitives](governance/governance-primitives.md) - Technical primitives
- [Witness Trust Validation](governance/witness-trust-validation.md) - Trust validation protocols

### Infrastructure
- [NAT Traversal](nat-traversal-design.md) - NAT traversal and TURN relay
- [Platform Layer](platform-layer-design.md) - Platform abstraction layer
- [Multi-Device Identity](multi-device-identity-design.md) - Multi-device identity management
- [Social Recovery](social-recovery-design.md) - Social recovery for key loss
- [Post-Quantum Crypto](post-quantum-crypto.md) - PQ hybrid cryptography

### Compute
- [Compute Substrate](compute-substrate-design.md) - Distributed compute design
- [Compute Classes](compute-classes.md) - Compute task classification
- [Deterministic Core](deterministic-core.md) - Deterministic execution guarantees
- [Scope Scheduling](scope-scheduling.md) - Scope-based task scheduling
- [Scheduler Evolution](scheduler-evolution-plan.md) - Scheduler roadmap

### Cooperative Structure
- [Minimal Viable Coop](MINIMAL-VIABLE-COOP.md) - Minimum viable cooperative
- [Entity Dissolution](entity-dissolution.md) - Entity lifecycle termination
- [Institution in a Box](institution-in-a-box.md) - Cooperative bootstrapping
- [Razeto Integration](razeto-integration-design.md) - Razeto cooperative economics
- [Capability-Based Features](capability-based-features.md) - Feature gating by capability

### SDIS
- [SDIS Design](sdis/) - Stewardship-based Digital Identity System

## Related
- [Architecture](../ARCHITECTURE.md)
- [Specifications](../spec/)
- [API Reference](../reference/api/)
