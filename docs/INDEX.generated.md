# ICN Documentation Index

Canonical index of all ICN documentation, organized by category.
*Generated: 2026-03-21 00:51:16 UTC*

---

## Quick Navigation

- [Architecture](#architecture)
- [Design](#design)
- [Guide](#guide)
- [Operations](#operations)
- [Reference](#reference)
- [Security](#security)
- [Strategy](#strategy)

---

## Architecture

### 🔒 **Canonical** [ICN Vision Statement](VISION.md)

Core vision, values, and long-term aspiration for InterCooperative Network

**For:** `all` | **Updated:** 2026-02-01

### 🔒 **Canonical** [ICN Architecture Reference](docs/ARCHITECTURE.md)

Single authoritative architecture covering all 8 primitives, kernel-app separation, subsystems, and implementation status

**For:** `developers`, `architects`, `grant-reviewers` | **Updated:** 2026-03-21

### 📝 **Living** [ADR-0010: App Topology](docs/adr/ADR-0010-app-topology.md)

Architectural decision record on app topology

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Architectural Gaps & Remediation Plan](docs/architecture/ARCHITECTURAL_GAPS_AND_FIXES.md)

Analysis of architectural weaknesses and remediation strategies

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Canonical Encoding](docs/architecture/CANONICAL_ENCODING.md)

Specification for deterministic serialization of ICN data structures

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Cells and Scopes Architecture](docs/architecture/CELLS_AND_SCOPES.md)

Design of ICN's cell-based organization model and scope hierarchy

**For:** `architects`, `developers` | **Updated:** 2026-03-15

### 📝 **Living** [Client Model Architecture](docs/architecture/CLIENT_MODEL.md)

Architecture of ICN client models and their relationship to kernel primitives

**For:** `developers`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [Federation Actions](docs/architecture/FEDERATION_ACTIONS.md)

Design of federated action execution across network boundaries

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Federation Interoperability Contract](docs/architecture/FEDERATION_INTEROP_CONTRACT.md)

Specification of contracts and interfaces for federation interoperability

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Governance State Machine Architecture](docs/architecture/GOVERNANCE_STATE_MACHINE.md)

State machine design for governance decision-making and enforcement

**For:** `architects`, `developers` | **Updated:** 2026-03-12

### 📝 **Living** [Identity and Membership Architecture](docs/architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md)

Design of identity primitives, membership verification, and member lifecycle

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Kernel/App Separation Architecture](docs/architecture/KERNEL_APP_SEPARATION.md)

Normative specification of kernel-app boundary, infection vectors, and capability propagation rules

**For:** `developers`, `architects` | **Updated:** 2026-03-17

### 📝 **Living** [Scope Bounded Trust](docs/architecture/SCOPE_BOUNDED_TRUST.md)

Trust model design limiting trust scope to organizational boundaries

**For:** `architects`, `security` | **Updated:** 2026-03-10

### ⏮️ **Archived** [Legacy State Machine Design](docs/architecture/legacy-state-machine.md)

Early design iteration, no longer in use

### 📋 **Draft** [ICN Kernel Contracts Specification](docs/spec/KERNEL_CONTRACTS.md)

Specification of kernel contract primitives

**For:** `architects`, `developers` | **Updated:** 2026-03-10


## Design

### 📝 **Living** [RFC: ICN Commons Evolution](docs/design/COMMONS_EVOLUTION.md)

Design for evolving ICN commons governance and stewardship models over phases 0-3

**For:** `architects`, `stakeholders` | **Updated:** 2026-03-15

### 📝 **Living** [Minimal Viable Coop Track](docs/design/MINIMAL-VIABLE-COOP.md)

Program for shipping one end-to-end cooperative use case for production 6-month validation

**For:** `product`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [Capability-Based Feature Gating](docs/design/capability-based-features.md)

System for graceful version handling via capability advertisement and negotiation

**For:** `developers` | **Updated:** 2026-03-15

### 📝 **Living** [Compute Classes: Legitimacy vs Utility](docs/design/compute-classes.md)

Design distinguishing between legitimacy compute and utility compute subsystems

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Compute Substrate Design](docs/design/compute-substrate-design.md)

Design for ICN's execution environment and compute resource management

**For:** `architects`, `developers` | **Updated:** 2025-11-18

### 📝 **Living** [ICN Deterministic Core Specification](docs/design/deterministic-core.md)

Specification for deterministic computation substrate ensuring reproducible state machines

**For:** `developers`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [ICN Economic Architecture](docs/design/economics/ECONOMIC_ARCHITECTURE.md)

Design of value flows, contribution accounting, and economic incentives

**For:** `architects`, `product` | **Updated:** 2026-01-17

### 📝 **Living** [Economic Vision](docs/design/economics/ECONOMIC_VISION.md)

Long-term vision for ICN's economic model and cooperative ownership

**For:** `architects`, `stakeholders` | **Updated:** 2026-03-10

### 📋 **Draft** [Contribution Credits Design](docs/design/economics/contribution-credits-design.md)

Design for tracking and accounting for contributions to cooperative entities

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Economic Modeling for Mutual Credit](docs/design/economics/econ-modeling.md)

Simulation and validation of mutual credit economic models

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Economic Safety Design](docs/design/economics/economic-safety.md)

Safety mechanisms preventing economic attacks and misuse of economic primitives

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Economics Truth Contract](docs/design/economics/economics-truth-contract.md)

Truth contract auditing all economics-related code against specification

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Economic Model Validation](docs/design/economics/model-validation.md)

Maps economic operations against implementation state

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Entity Dissolution: Before and After](docs/design/entity-dissolution-example.md)

Practical example of entity dissolution workflow

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Entity Dissolution Design](docs/design/entity-dissolution.md)

Design for graceful shutdown and dissolution of cooperative entities

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Execution Bridge Specification](docs/design/execution-bridge-spec.md)

Authoritative design for bridging between ICN and external execution environments

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Project Governance](docs/design/governance/PROJECT_GOVERNANCE.md)

Governance structure for ICN development and decision-making

**For:** `team`, `stakeholders` | **Updated:** 2026-03-10

### 📋 **Draft** [Governance Primitives](docs/design/governance/governance-primitives.md)

Fundamental governance building blocks and decision-making patterns

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Governance Framework](docs/design/governance/governance.md)

Comprehensive governance framework for cooperative decision-making

**For:** `architects`, `product` | **Updated:** 2026-03-10

### 📝 **Living** [Governance Model Validation](docs/design/governance/model-validation.md)

Maps governance operations against implementation state

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Trust-Graph Integration for Witness Validation](docs/design/governance/witness-trust-validation.md)

Trust-graph integration for witness validation in ledger operations

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Institution-in-a-Box](docs/design/institution-in-a-box.md)

Design pattern for embedding ICN primitives into legacy digital infrastructure with CRDTs and replication

**For:** `architects`, `product` | **Updated:** 2026-03-15

### 📋 **Draft** [IPv6 Endpoint Sets Design](docs/design/ipv6-endpoint-sets-design.md)

Design for managing multiple network endpoints with IPv6

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Multi-Device Identity Design](docs/design/multi-device-identity-design.md)

Design for managing identity across multiple devices within a single agent

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [NAT Traversal Design](docs/design/nat-traversal-design.md)

Design for peer-to-peer connectivity across NAT boundaries

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Platform Layer Design](docs/design/platform-layer-design.md)

Design for platform abstractions and portability across systems

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Post-Quantum Cryptography in ICN](docs/design/post-quantum-crypto.md)

Experimental post-quantum cryptography integration and migration strategy

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📋 **Draft** [Razeto Integration Design](docs/design/razeto-integration-design.md)

Design for integrating external systems via Razeto protocol

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Regulatory-Safe Verifiable State](docs/design/regulatory-safe-verifiable-state.md)

Design maintaining provable state satisfying regulatory audits without exposing ledger internals

**For:** `architects`, `compliance` | **Updated:** 2026-03-10

### 📋 **Draft** [Repository Reality Map](docs/design/repo-reality-map.md)

Mapping between git repository structure and architectural reality

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Scheduler Evolution Plan](docs/design/scheduler-evolution-plan.md)

Plan for evolving ICN's scheduling substrate over multiple phases

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Scope Scheduling Design](docs/design/scope-scheduling.md)

Design for scheduling and resource allocation within organizational scopes

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Social Recovery Guide](docs/design/sdis/social-recovery.md)

M-of-N social recovery mechanism for identity recovery

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📋 **Draft** [Social Recovery Design](docs/design/social-recovery-design.md)

M-of-N social recovery mechanism for lost devices and identity recovery

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📋 **Draft** [Gossip SignedEnvelope Migration](docs/development/gossip-signed-envelope-migration.md)

Migration plan for gossip cryptographic authentication

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [IdentityBundle Refactor Plan](docs/development/identity-bundle-refactor-plan.md)

Hardware-backed signing implementation plan

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module Splitting Analysis](docs/development/module-splitting-analysis.md)

Analysis of large modules for potential splitting

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Mobile Member UX Spec v1](docs/mobile/icn-mobile-ux-spec-v1.md)

Mobile application UX specification

**For:** `developers`, `product` | **Updated:** 2026-03-10

### 📋 **Draft** [SDIS: Secure Distributed Identity System](docs/sdis/SDIS_SYSTEM.md)

SDIS design with implemented flows and planned endpoints

**For:** `architects`, `developers` | **Updated:** 2026-03-10


## Guide

### 🔒 **Canonical** [Contributing to ICN](CONTRIBUTING.md)

Architectural guardrails, contribution workflow, code standards, and review process

**For:** `contributors` | **Updated:** 2026-03-01

### 🔒 **Canonical** [Getting Started with ICN](docs/GETTING_STARTED.md)

Quick-start guide for new developers to set up ICN in minutes

**For:** `developers`, `contributors` | **Updated:** 2026-03-15

### 📝 **Living** [Deploy Test Network](docs/deployment/DEPLOY_TEST_NETWORK.md)

Guide for setting up multi-node test networks for validation

**For:** `developers`, `testers` | **Updated:** 2026-03-10

### 📝 **Living** [Deploy to K3s](docs/deployment/DEPLOY_TO_K3S.md)

Steps for deploying ICN to Kubernetes using K3s

**For:** `operators`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Quick Deploy Guide](docs/deployment/QUICK_START.md)

Fast path for getting ICN running locally

**For:** `developers`, `operators` | **Updated:** 2026-03-10

### 🔒 **Canonical** [ICN UX Language Guide](docs/dev/language-guide.md)

Enforced communications style guide for regulatory-safe messaging

**For:** `all` | **Updated:** 2026-03-10

### 📝 **Living** [Federation Roadmap Implementation Guide](docs/development/federation-roadmap-implementation.md)

Step-by-step federation feature implementation guide

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Trust Multi-Graph Migration Guide](docs/development/trust-multi-graph-migration.md)

Migration guide for multi-graph architecture

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Witness Signature Best Practices](docs/features/witness-signature-best-practices.md)

Developer guide for witness signature implementation

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Development Environment](docs/guides/developer/DEV_ENVIRONMENT.md)

Setup guide for icn-dev development VM

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Documentation Style Guide](docs/guides/developer/DOCUMENTATION_STYLE.md)

Consistent documentation formatting standards

**For:** `contributors` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Developer Guides](docs/guides/developer/README.md)

Index of guides for developers building on or contributing to ICN

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Internationalization (i18n) Guide](docs/guides/developer/i18n-guide.md)

i18n implementation across Rust, React Native, web

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Operations Guides](docs/guides/operations/README.md)

Operational guides for running ICN deployments

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Backup and Recovery Guide](docs/guides/operations/backup-and-recovery.md)

Operator procedures for node backup and recovery

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Backup and Restore: Operator Recovery](docs/guides/operations/backup-restore.md)

Low-level Sled database and keystore recovery

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Daemon Mode with Governance Receipts](docs/guides/operations/daemon-mode-governance.md)

Phase 0 pilot daemon mode operations

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [NAT Traversal Pilot Test Guide](docs/guides/operations/nat-traversal-pilot-test.md)

Manual testing guide for NAT traversal feature

**For:** `operators`, `testers` | **Updated:** 2026-03-10

### 📝 **Living** [NAT Traversal Operations Guide](docs/guides/operations/nat-traversal.md)

Configuring and operating nodes behind NAT

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Operations Guide](docs/guides/operations/operations-guide.md)

Comprehensive operational procedures and workflows

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Smoke Runbook](docs/guides/operations/pilot-smoke.md)

Deterministic operator check for pilot deployment

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Replication Operations Guide](docs/guides/operations/replication-operations.md)

Replication procedures for Phase 17 storage hardening

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Troubleshooting Runbooks](docs/guides/operations/troubleshooting.md)

Step-by-step procedures for diagnosing operational issues

**For:** `operators` | **Updated:** 2026-03-10

### 🔒 **Canonical** [User Guides](docs/guides/user/README.md)

End-user documentation and tutorials

**For:** `users` | **Updated:** 2026-03-10

### 📝 **Living** [Cooperative Setup Guide](docs/guides/user/cooperative-setup-guide.md)

Instructions for setting up a new cooperative on ICN

**For:** `users` | **Updated:** 2026-03-10

### 📝 **Living** [Summit Decision Registry Demo](docs/guides/user/summit-demo.md)

Demo script for decision registry and treasury spending

**For:** `team`, `demo-users` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Coordinator Guide](docs/internal/pilots/pilot-coordinator-guide.md)

Practical guide for cooperative coordinators deploying ICN

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Deployment Playbook](docs/internal/pilots/pilot-playbook.md)

Step-by-step guide for pilot cooperative deployment

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Keystore Version Migration](docs/migration-guides/keystore-versions.md)

Guide for keystore format version migration

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Version Upgrade Guide](docs/migration-guides/version-upgrades.md)

Guide for upgrading between ICN versions

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Tail-Based Sampling Configuration](docs/observability/tail-based-sampling.md)

Tail-based sampling setup for traces

**For:** `operators` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Developer Onboarding Curriculum](docs/onboarding/README.md)

Structured learning path for new ICN developers

**For:** `developers`, `contributors` | **Updated:** 2026-03-15

### 📝 **Living** [Onboarding Assessments](docs/onboarding/assessments.md)

Module checkpoints and self-review questions

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Capstone: Local Two-Node ICN + Ledger Flow](docs/onboarding/capstone.md)

Final capstone project demonstrating end-to-end ICN

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 01: Workspace Setup](docs/onboarding/labs/lab-01-workspace/README.md)

Learning lab for workspace organization

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 02: Error Receipts and Tracing](docs/onboarding/labs/lab-02-error-receipt/README.md)

Learning lab for structured errors and tracing

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 03: Mini Actor Runtime](docs/onboarding/labs/lab-03-mini-actor/README.md)

Learning lab for actor model implementation

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 04: Firewall Oracle (Keystone Lab)](docs/onboarding/labs/lab-04-firewall-oracle/README.md)

Keystone lab proving the Meaning Firewall boundary

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 05: Mini Ledger with Double-Entry](docs/onboarding/labs/lab-05-mini-ledger/README.md)

Learning lab for ledger implementation

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 06: Signed Envelopes with Replay Protection](docs/onboarding/labs/lab-06-signed-envelope/README.md)

Learning lab for cryptographic signatures

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 07: Gossip Sync with Vector Clocks](docs/onboarding/labs/lab-07-gossip-sync/README.md)

Learning lab for eventual consistency

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 08: Governance Flow](docs/onboarding/labs/lab-08-governance-flow/README.md)

Learning lab for governance proposal to constraint update

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [01: Environment Setup & Repository Navigation](docs/onboarding/path/phase-1-foundations/01-environment.md)

Foundational module on environment and repo layout

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [02: Rust Through ICN's Lens](docs/onboarding/path/phase-1-foundations/02-rust-through-icn.md)

Foundational module on Rust patterns used in ICN

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [03: Errors and Tracing](docs/onboarding/path/phase-1-foundations/03-errors-and-tracing.md)

Foundational module on error handling and distributed visibility

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Actors and Concurrency](docs/onboarding/path/phase-2-architecture/04-actors-and-concurrency.md)

Learning module on ICN's actor model and concurrent execution

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [The Meaning Firewall](docs/onboarding/path/phase-2-architecture/05-the-meaning-firewall.md)

Learning module on ICN's type-safe boundary enforcement

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Persistence and Ledger](docs/onboarding/path/phase-2-architecture/06-persistence-and-ledger.md)

Learning module on state persistence and event logging

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [07: Identity and Cryptography](docs/onboarding/path/phase-3-systems/07-identity-and-crypto.md)

Systems module on identity and cryptographic operations

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [08: Network and Gossip](docs/onboarding/path/phase-3-systems/08-network-and-gossip.md)

Systems module on network communication and eventual consistency

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [09: Governance and Contracts](docs/onboarding/path/phase-3-systems/09-governance-and-contracts.md)

Systems module on governance and contracts

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [10: Federation and Operations](docs/onboarding/path/phase-4-ownership/10-federation-and-ops.md)

Ownership module on federation and production deployment

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [11: Maintainer Skills](docs/onboarding/path/phase-4-ownership/11-maintainer.md)

Ownership module on reshaping architecture safely

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Maintainer Capstone](docs/onboarding/path/phase-4-ownership/capstone.md)

Final capstone gate for Maintainer tier

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Onboarding Reading Map](docs/onboarding/reading-map.md)

Links between modules and high-signal source files

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 0: Setup and Tooling](docs/onboarding/reference/module-00-setup.md)

Reference module on development environment setup

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 1: Rust Fundamentals](docs/onboarding/reference/module-01-rust-fundamentals.md)

Reference module on Rust concepts for ICN

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 02: Architecture Overview](docs/onboarding/reference/module-02-architecture-overview.md)

Reference module providing high-level architecture overview

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 3: Runtime and Actor Model](docs/onboarding/reference/module-03-runtime-actors.md)

Reference module on actor model and constraint engine

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 4: Identity and Trust](docs/onboarding/reference/module-04-identity-trust.md)

Reference module on identity and trust primitives

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 5: Network and Gossip](docs/onboarding/reference/module-05-network-gossip.md)

Reference module on network communication and state sync

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 6: Ledger and Contracts](docs/onboarding/reference/module-06-ledger-contracts.md)

Reference module on ledger and CCL programming

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 8: Web UI Integration](docs/onboarding/reference/module-08-web-ui.md)

Reference module on web UI integration

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 9: Operations and Deployment](docs/onboarding/reference/module-09-ops-deploy.md)

Reference module on production deployment

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 10: Contributor Workflow](docs/onboarding/reference/module-10-contributor-workflow.md)

Reference module on contribution processes

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 11: Federation](docs/onboarding/reference/module-11-federation.md)

Reference module on federation and inter-cooperative agreements

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 12: Observability and Metrics](docs/onboarding/reference/module-12-observability.md)

Reference module on monitoring

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 13: Security and Privacy](docs/onboarding/reference/module-13-security-privacy.md)

Reference module on security and privacy in ICN

**For:** `developers`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Module 14: Governance and CCL Deep Dive](docs/onboarding/reference/module-14-governance-ccl-deep-dive.md)

Reference module on governance and contract language

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Curriculum Gap Analysis](docs/onboarding/review-plan.md)

Analysis of curriculum gaps and iteration plan

**For:** `team` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Onboarding Syllabus](docs/onboarding/syllabus.md)

Overall course structure for Foundations and Accelerated tracks

**For:** `developers` | **Updated:** 2026-03-15

### 🔒 **Canonical** [Accelerated Track (4 weeks)](docs/onboarding/tracks/accelerated.md)

Fast-track onboarding for Rust-intermediate developers

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [ICN Contributor Ladder](docs/onboarding/tracks/contributor-ladder.md)

Tier-based contributor skill progression (Observer → Contributor → Maintainer → Architect)

**For:** `contributors` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Foundations Track (8 weeks)](docs/onboarding/tracks/foundations.md)

Comprehensive onboarding track for Rust beginners

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Curriculum Update Process](docs/onboarding/update-process.md)

Process for keeping onboarding aligned with codebase

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 0: Local Build and Repo Orientation](docs/onboarding/workshops/workshop-00-setup.md)

Workshop on local build and repository structure

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 1: Rust Fundamentals in Practice](docs/onboarding/workshops/workshop-01-rust-fundamentals.md)

Workshop applying Rust patterns to ICN code

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 2: Architecture Mapping Exercise](docs/onboarding/workshops/workshop-02-architecture.md)

Workshop for building architecture mental models

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 3: Runtime and Actor Lifecycle](docs/onboarding/workshops/workshop-03-runtime.md)

Workshop on actor model and lifecycle

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 4: Identity and Trust Hands-On](docs/onboarding/workshops/workshop-04-identity-trust.md)

Workshop on identity and trust operations

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 5: Network and Gossip Deep Dive](docs/onboarding/workshops/workshop-05-network-gossip.md)

Workshop on network and gossip systems

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 6: Ledger and Contract Flow](docs/onboarding/workshops/workshop-06-ledger-contracts.md)

Workshop on ledger and contract operations

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 8: Web UI Exploration](docs/onboarding/workshops/workshop-08-web-ui.md)

Workshop on web UI and gateway integration

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 9: Local Deployment and Observability](docs/onboarding/workshops/workshop-09-ops.md)

Workshop on deployment and monitoring

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 10: Contributor Workflow](docs/onboarding/workshops/workshop-10-contributor.md)

Workshop on contribution workflow

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 11: Federation Hands-On](docs/onboarding/workshops/workshop-11-federation.md)

Workshop on federation code paths

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 12: Observability and Metrics](docs/onboarding/workshops/workshop-12-observability.md)

Workshop on observability

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 13: Security and Privacy](docs/onboarding/workshops/workshop-13-security-privacy.md)

Workshop on security and privacy layers

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 14: Governance and CCL Deep Dive](docs/onboarding/workshops/workshop-14-governance-ccl.md)

Workshop on governance and contract language

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Pilot Proposal Template](docs/pilots/pilot-proposal-template.md)

Template for approaching potential pilot communities

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [SDIS User Guide](docs/sdis/SDIS_USER_GUIDE.md)

End-user guide for credential presentation system

**For:** `users` | **Updated:** 2026-03-10

### 📝 **Living** [Mobile App Testing Guide](docs/testing/MOBILE_APP_TESTING_GUIDE.md)

Testing guide for mobile application

**For:** `testers` | **Updated:** 2026-03-10


## Operations

### 📝 **Living** [ICN Deployment Guide](docs/deployment/DEPLOYMENT_GUIDE.md)

Comprehensive guide for deploying ICN to production environments

**For:** `operators`, `developers` | **Updated:** 2026-03-15

### 📝 **Living** [Code Quality Improvement Tracker](docs/development/code-quality-improvements.md)

Tracks error handling and code quality audits

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Network Policy Design — K3s](docs/guides/operations/network-policies.md)

K3s network policy design (deferred)

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Phase 0 Operational Monitoring](docs/guides/operations/phase-0-monitoring.md)

Monitoring configuration for Phase 0 pilot

**For:** `operators` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Operations Directory README](docs/operations/README.md)

Navigation for operations and deployment documentation

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Operations Deployment Guide](docs/operations/deployment/deployment-guide.md)

Deep operational guide for production deployments

**For:** `operators` | **Updated:** 2026-03-15

### 📋 **Draft** [Distributed Tracing Setup](docs/operations/deployment/distributed-tracing.md)

Configuration for distributed tracing and observability

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Incident Response Plan](docs/operations/deployment/incident-response.md)

Procedures for responding to production incidents

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Emergency Node Restart](docs/ops/runbooks/01-emergency-restart.md)

Runbook for emergency node restart procedures

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Data Recovery Procedure](docs/ops/runbooks/02-data-recovery.md)

Runbook for node data recovery from backup

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Version Upgrade Procedure](docs/ops/runbooks/03-version-upgrade.md)

Runbook for upgrading daemon to new version

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Security Incident Response](docs/ops/runbooks/04-security-incident.md)

Runbook for security incident response

**For:** `operators`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Troubleshooting Guide](docs/ops/runbooks/05-troubleshooting.md)

Common issues and solutions for node operations

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Secrets Rotation Procedure](docs/ops/runbooks/06-secrets-rotation.md)

Runbook for rotating cryptographic secrets

**For:** `operators`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Vertical Slice Smoke](docs/ops/runbooks/07-pilot-vertical-slice-smoke.md)

Runbook for pilot deployment verification

**For:** `operators` | **Updated:** 2026-03-10


## Reference

### 📝 **Living** [Agent Registry](AGENTS.md)

Catalog of authorized AI agents and their capabilities for ICN development

**For:** `agents`, `team` | **Updated:** 2026-03-01

### 📝 **Living** [Changelog](CHANGELOG.md)

Release notes and version history

**For:** `contributors`, `public` | **Updated:** 2026-03-21

### 🔒 **Canonical** [Claude Agent Onboarding](CLAUDE.md)

Guidance for Claude Code sessions working with ICN codebase

**For:** `agents`, `developers` | **Updated:** 2026-03-21

### 🔒 **Canonical** [Code of Conduct](CODE_OF_CONDUCT.md)

Community guidelines and expected behavior for contributors

**For:** `contributors`, `community` | **Updated:** 2025-12-01

### 🔒 **Canonical** [ICN - InterCooperative Network](README.md)

Main project README with overview, quick start, and CI/CD status badge

**For:** `contributors`, `public` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Golden Development Prompt](docs/GOLDEN_PROMPT.md)

Master context and instructions for AI-assisted development on ICN

**For:** `agents`, `developers` | **Updated:** 2026-03-18

### 🔒 **Canonical** [ICN Documentation Index](docs/INDEX.md)

Master navigation and directory of all documentation with cross-references

**For:** `all` | **Updated:** 2026-03-15

### 🔒 **Canonical** [Docs Directory README](docs/README.md)

Overview of documentation structure and navigation guide

**For:** `contributors` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Gateway API Documentation](docs/api/OPENAPI.md)

HTTP API specification and usage guide for ICN Gateway

**For:** `developers`, `integrators` | **Updated:** 2026-03-15

### 🔒 **Canonical** [API Documentation Index](docs/api/README.md)

Navigation and overview for API-related documentation

**For:** `developers`, `integrators` | **Updated:** 2026-03-10

### 📝 **Living** [OpenAPI Specification](docs/api/openapi.yaml)

Machine-readable API specification in OpenAPI 3.0 format

**For:** `developers`, `tools` | **Updated:** 2026-03-15

### 🔒 **Canonical** [Architecture Directory README](docs/architecture/README.md)

Navigation guide for architecture documentation

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [CI Documentation](docs/ci/README.md)

CI status, gates, and ratchet plan

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Demo System Documentation](docs/demo/README.md)

Documentation for ICN demonstration system

**For:** `team`, `demo-users` | **Updated:** 2026-03-15

### 🔒 **Canonical** [Design Directory README](docs/design/README.md)

Navigation guide for design documentation

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Economics Directory README](docs/design/economics/README.md)

Navigation for economics and value flow documentation

**For:** `architects` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Governance Directory README](docs/design/governance/README.md)

Navigation guide for governance documentation

**For:** `architects` | **Updated:** 2026-03-10

### 🔒 **Canonical** [SDIS Design Documentation](docs/design/sdis/README.md)

Navigation guide for Sovereign Digital Identity System design

**For:** `architects` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Development Documentation](docs/development/README.md)

Navigation guide for development activities

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Testing Documentation](docs/development/testing/README.md)

Navigation guide for testing guides

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Policy Examples](docs/examples/policies/README.md)

Example governance policies for cooperative organizations

**For:** `users`, `architects` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Glossary](docs/glossary.md)

Reference document with ICN terminology and definitions

**For:** `all` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Internal Documentation](docs/internal/README.md)

Internal-only documentation for team coordination

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Legal Considerations](docs/internal/legal-considerations.md)

Legal questions and considerations for cooperative communities

**For:** `compliance` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Deployment Limitations](docs/internal/pilots/pilot-limitations.md)

Known limitations and constraints in pilot phase

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Readiness Gaps](docs/internal/pilots/pilot-readiness-gaps.md)

Critical gaps between implementation and pilot readiness

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [Storage Metrics Reference](docs/observability/storage-metrics.md)

Prometheus metrics for storage and database monitoring

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Module Template](docs/onboarding/module-template.md)

Template for creating onboarding modules

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Common Patterns Reference](docs/onboarding/patterns.md)

Quick reference for recurring code patterns in ICN

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [ICN Operations Runbooks](docs/ops/runbooks/README.md)

Navigation guide for production runbooks

**For:** `operators` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Performance Documentation](docs/performance/README.md)

Performance requirements, benchmarks, and optimization guidance

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Trust Score Benchmark Results](docs/performance/trust-score-benchmark-results.md)

Benchmark results for trust score performance

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Trust Service Performance Characteristics](docs/performance/trust-service-performance.md)

Performance characteristics and optimization guidance

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Decision Registry + Treasury Vote Pilot](docs/pilots/decision_registry_treasury_vote.md)

Economic receipt chain implementation in pilot

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [Hosted Pilot Approach](docs/pilots/hosted-approach.md)

Approach for hosted cooperative pilot deployments

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [Agent Knowledge Architecture](docs/planning/agent-knowledge-architecture.md)

Design for AI agent knowledge bases and context management

**For:** `agents`, `architects` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Crate Reference](docs/planning/icn-crate-reference.md)

Authoritative inventory of workspace crates

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Governance Demo One-Pager](docs/planning/icn-demo-one-pager.md)

One-page demo explanation and value proposition

**For:** `stakeholders` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Ecosystem Map](docs/planning/icn-ecosystem-map.md)

System component interconnection map

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [API Reference](docs/reference/api/API_REFERENCE.md)

Detailed API endpoint reference with examples and error codes

**For:** `developers` | **Updated:** 2026-03-15

### 🔒 **Canonical** [API Reference Documentation](docs/reference/api/README.md)

Navigation guide for API reference documents

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [API Versioning Strategy](docs/reference/api/api-versioning.md)

Versioning scheme and compatibility policy for ICN APIs

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Topic Subscriptions API](docs/reference/api/topic-subscriptions-api.md)

API for subscribing to and consuming topic streams

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Configuration Management Guide](docs/reference/config/CONFIGURATION.md)

Complete configuration reference for ICN nodes

**For:** `operators`, `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Configuration Reference](docs/reference/config/README.md)

Navigation guide for configuration documentation

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Identity Backend Configuration](docs/reference/config/identity-backend-configuration.md)

Identity keystore backend configuration guide

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Trust Threshold Configuration](docs/reference/config/trust-threshold-configuration.md)

Trust score threshold configuration guide

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [SDIS API Guide](docs/sdis/SDIS_API_GUIDE.md)

Complete API guide for Sovereign Digital Identity System

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [SDIS + Steward System Status](docs/sdis/SDIS_STATUS.md)

Snapshot of SDIS deployment status

**For:** `team` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Grant Application Artifacts](docs/strategy/grants/README.md)

Navigation for grant application templates and materials

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Budget Skeleton](docs/strategy/grants/budget-skeleton.md)

Grant budget template

**For:** `team` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Documentation Templates](docs/templates/README.md)

Navigation guide for documentation templates

**For:** `contributors` | **Updated:** 2026-03-10

### 📋 **Draft** [Development Journal Template](docs/templates/dev-journal.md)

Template for development session journals

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Vision & Strategy](docs/vision/README.md)

Navigation guide for vision and strategy documents

**For:** `all` | **Updated:** 2026-03-10


## Security

### 📝 **Living** [Gateway Content Security Policy](docs/security/GATEWAY_CSP.md)

CSP configuration for ICN gateway ensuring safe web integration

**For:** `security`, `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Security Documentation Index](docs/security/README.md)

Navigation and overview of security-related documentation

**For:** `security`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [Secret Management](docs/security/SECRET_MANAGEMENT.md)

Policy and design for managing cryptographic secrets and sensitive data

**For:** `security`, `operators` | **Updated:** 2026-03-10

### 📝 **Living** [TOFU (Trust-On-First-Use) Security Model](docs/security/TOFU_SECURITY_MODEL.md)

Trust establishment for first-time peer contact without certificates or CAs

**For:** `security`, `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Phase 10C Security Analysis](docs/security/phase-10c-security-analysis.md)

Security analysis and hardening for multi-party contracts

**For:** `security` | **Updated:** 2026-03-10

### 📝 **Living** [Production Hardening](docs/security/production-hardening.md)

Hardening measures protecting against DoS, resource exhaustion, and operational failures

**For:** `operators`, `security` | **Updated:** 2026-03-15

### 📝 **Living** [ICN Security Roadmap](docs/security/security-roadmap.md)

Security architecture and phased hardening approach

**For:** `security`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [ICN Threat Model](docs/security/threat-model.md)

Comprehensive threat model covering attack vectors, adversary capabilities, and mitigations

**For:** `security`, `architects` | **Updated:** 2026-03-15


## Strategy

### 📋 **Draft** [SDIS Complete Build Plan](docs/sdis/SDIS_BUILD_PLAN.md)

Detailed SDIS build plan from API to mobile

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [SDIS & Steward Completion Roadmap](docs/sdis/SDIS_STEWARD_ROADMAP.md)

Roadmap for SDIS and steward system completion

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [ADR-001: What ICN Is](docs/strategy/ADR-001-What-ICN-Is.md)

Architectural Decision Record defining ICN scope, non-goals, and boundary conditions

**For:** `developers`, `architects`, `stakeholders` | **Updated:** 2026-02-28

### 📝 **Living** [What ICN Is](docs/strategy/ICN-Definition.md)

Canonical definition: problem statement, solution approach, scope and non-goals

**For:** `all` | **Updated:** 2026-03-17

### 📝 **Living** [ICN Evolution Arc](docs/strategy/ICN-Evolution-Arc.md)

Long-term vision across phases 0-3, from MVP to mature cooperative ecosystem

**For:** `architects`, `stakeholders` | **Updated:** 2026-03-08

### 📝 **Living** [ICN Gap Analysis March 2026](docs/strategy/ICN-Gap-Analysis-March-2026.md)

Comprehensive implementation assessment across 10 subsystems with evidence and gaps

**For:** `grant-reviewers`, `architects`, `product` | **Updated:** 2026-03-17

### 📝 **Living** [ICN Pitch](docs/strategy/ICN-Pitch.md)

Elevator pitch, one-pagers, and public communication messaging framework

**For:** `stakeholders`, `public` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Live Roadmap](docs/strategy/ICN-Roadmap-Live.md)

Current sprint and quarterly priorities, immediate milestones, and dependencies

**For:** `team`, `stakeholders` | **Updated:** 2026-03-21

### 📝 **Living** [ICN Roadmap Strategy](docs/strategy/ICN-Roadmap-Strategy.md)

Strategic roadmap phases 0-3 with dependencies, milestones, and long-term vision

**For:** `architects`, `stakeholders`, `grant-reviewers` | **Updated:** 2026-03-12

### 📝 **Living** [ICN Scenarios](docs/strategy/ICN-Scenarios.md)

Use case narratives and operational scenarios demonstrating ICN in practice

**For:** `product`, `marketing` | **Updated:** 2026-03-09

### 📝 **Living** [ICN Sprint March 17](docs/strategy/ICN-Sprint-March17.md)

Specific sprint plan and tactical objectives for week of March 17, 2026

**For:** `team` | **Updated:** 2026-03-17

### 📝 **Living** [ICN Technical Whitepaper](docs/strategy/ICN-Technical-Whitepaper.md)

Formal technical specification for grants, regulatory review, and architectural validation

**For:** `grant-reviewers`, `architects`, `compliance` | **Updated:** 2026-03-15

### 📋 **Draft** [ICN Compliance Architecture](docs/strategy/grants/compliance-architecture.md)

Regulatory-safe design rationale for grants

**For:** `compliance`, `grant-reviewers` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Grant Narrative Core](docs/strategy/grants/grant-narrative-core.md)

Reusable grant narrative sections

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Grant One-Pager](docs/strategy/grants/grant-one-pager.md)

One-page ICN summary for grant applications

**For:** `grant-reviewers` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Milestones](docs/strategy/grants/milestones.md)

Project timeline through pilot deployment

**For:** `grant-reviewers` | **Updated:** 2026-03-10

### 📋 **Draft** [Pilot Readiness Assessment](docs/strategy/grants/pilot-readiness.md)

Assessment of pilot readiness and gaps

**For:** `team` | **Updated:** 2026-03-10

### ❌ **Superseded** [Old Roadmap 2025](docs/strategy/old-roadmap-2025.md)

Historical roadmap from late 2025

> Superseded by [docs/strategy/ICN-Roadmap-Live.md](docs/strategy/ICN-Roadmap-Live.md)
> Reason: Roadmap updated for 2026 priorities


---

## Summary

**Total documents:** 241

**By status:**
- Archived: 1
- Canonical: 38
- Draft: 47
- Living: 154
- Superseded: 1
