# ICN Documentation Index

Welcome to the ICN (Intercooperative Network) documentation! This index provides clear navigation to all documentation.

**Last Updated**: 2026-02-10  
**Version**: 2.0 (Post-Phase 2C Reorganization)

---

## 📋 Quick Navigation

- **New to ICN?** → Start with [Getting Started Guide](GETTING_STARTED.md)
- **Understanding the system?** → Read [Architecture Overview](ARCHITECTURE.md)
- **Building features?** → Check [Developer Guides](#developer-guides)
- **Deploying ICN?** → See [Operations Guides](#operations-guides)
- **Looking for APIs?** → Browse [API Reference](#api-reference)
- **Historical context?** → Explore [Archives](#archives)

---

## 📚 Core Documentation

Essential reading for all ICN users and contributors.

| Document | Description |
|----------|-------------|
| [README.md](README.md) | Documentation overview and structure |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Comprehensive system architecture (160KB+) |
| [GETTING_STARTED.md](GETTING_STARTED.md) | Quick start guide for developers |
| [PHASE_HISTORY.md](PHASE_HISTORY.md) | Development phase completion history |
| [STATE.md](STATE.md) | Current project state snapshot |
| [TODO.md](TODO.md) | Active work items and priorities |
| [glossary.md](glossary.md) | ICN terminology and definitions |

---

## 🏗️ Architecture & Design

In-depth architectural documentation and design specifications.

### Architecture Documentation (`architecture/`)

Comprehensive architectural reviews and design decisions:

- [ARCHITECTURE_INDEX.md](architecture/ARCHITECTURE_INDEX.md) - Architecture document index
- [ARCHITECTURE_MAP.md](architecture/ARCHITECTURE_MAP.md) - Visual architecture guide (197KB)
- [ARCHITECTURE_QUICK_REF.md](architecture/ARCHITECTURE_QUICK_REF.md) - Quick reference card
- [CANONICAL_ENCODING.md](architecture/CANONICAL_ENCODING.md) - Wire format specifications
- [CELLS_AND_SCOPES.md](architecture/CELLS_AND_SCOPES.md) - Cell-based federation model
- [SCOPE_BOUNDED_TRUST.md](architecture/SCOPE_BOUNDED_TRUST.md) - Trust scope architecture
- [KERNEL_APP_SEPARATION.md](architecture/KERNEL_APP_SEPARATION.md) - Kernel/app boundary design
- [FEDERATION_INTEROP_CONTRACT.md](architecture/FEDERATION_INTEROP_CONTRACT.md) - Federation protocols
- [CLIENT_MODEL.md](architecture/CLIENT_MODEL.md) - Client architecture patterns
- [GOVERNANCE_STATE_MACHINE.md](architecture/GOVERNANCE_STATE_MACHINE.md) - Governance flow design
- [IDENTITY_MEMBERSHIP_ARCHITECTURE.md](architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md) - Identity & membership
- Plus audit reports and gap analyses

### Design Documents (`design/`)

Feature designs, proposals, and evolution plans:

**Core Systems:**
- [COMMONS_EVOLUTION.md](design/COMMONS_EVOLUTION.md) - Commons-based governance evolution
- [MINIMAL-VIABLE-COOP.md](design/MINIMAL-VIABLE-COOP.md) - MVC specification
- [capability-based-features.md](design/capability-based-features.md) - Capability system design
- [compute-substrate-design.md](design/compute-substrate-design.md) - Distributed compute layer
- [scheduler-evolution-plan.md](design/scheduler-evolution-plan.md) - Task scheduler design
- [multi-device-identity-design.md](design/multi-device-identity-design.md) - Multi-device identity
- [nat-traversal-design.md](design/nat-traversal-design.md) - NAT traversal strategy
- [post-quantum-crypto.md](design/post-quantum-crypto.md) - PQ cryptography design
- [platform-layer-design.md](design/platform-layer-design.md) - Platform abstraction
- [razeto-integration-design.md](design/razeto-integration-design.md) - Razeto model integration
- [social-recovery-design.md](design/social-recovery-design.md) - Social recovery mechanisms
- [entity-dissolution.md](design/entity-dissolution.md) & [entity-dissolution-example.md](design/entity-dissolution-example.md) - Entity lifecycle
- [institution-in-a-box.md](design/institution-in-a-box.md) - Organizational templates

**Economics (`design/economics/`):**
- [README.md](design/economics/README.md) - Economics documentation index
- [ECONOMIC_VISION.md](design/economics/ECONOMIC_VISION.md) - Strategic economic vision
- [ECONOMIC_ARCHITECTURE.md](design/economics/ECONOMIC_ARCHITECTURE.md) - Layered economy design
- [contribution-credits-design.md](design/economics/contribution-credits-design.md) - Credit system
- [economic-safety.md](design/economics/economic-safety.md) - Economic safety rails
- [econ-modeling.md](design/economics/econ-modeling.md) - Economic simulations

**Governance (`design/governance/`):**
- [README.md](design/governance/README.md) - Governance documentation index
- [PROJECT_GOVERNANCE.md](design/governance/PROJECT_GOVERNANCE.md) - Project governance model
- [governance.md](design/governance/governance.md) - Governance system design
- [governance-primitives.md](design/governance/governance-primitives.md) - Governance building blocks
- [witness-trust-validation.md](design/governance/witness-trust-validation.md) - Witness validation

**SDIS (`design/sdis/`):**
- [README.md](design/sdis/README.md) - SDIS design documentation index
- [social-recovery.md](design/sdis/social-recovery.md) - SDIS social recovery design

### Specifications (`spec/`)

Formal protocol and contract specifications:

- [KERNEL_CONTRACTS.md](spec/KERNEL_CONTRACTS.md) - Kernel contract specifications

---

## 📖 Reference Documentation

Technical reference materials, APIs, and configuration.

### API Reference (`reference/api/`)

REST API, WebSocket, and SDK documentation:

- [README.md](reference/api/README.md) - API documentation index
- [API_REFERENCE.md](reference/api/API_REFERENCE.md) - Complete API reference
- [api-versioning.md](reference/api/api-versioning.md) - API versioning strategy
- [topic-subscriptions-api.md](reference/api/topic-subscriptions-api.md) - Subscription API
- Also see: [api/OPENAPI.md](api/OPENAPI.md) and [api/README.md](api/README.md)

### Configuration Reference (`reference/config/`)

Configuration files and environment settings:

- [README.md](reference/config/README.md) - Configuration documentation index
- [CONFIGURATION.md](reference/config/CONFIGURATION.md) - Configuration guide
- [identity-backend-configuration.md](reference/config/identity-backend-configuration.md) - Identity backends
- [trust-threshold-configuration.md](reference/config/trust-threshold-configuration.md) - Trust thresholds

### Other References

- [glossary.md](glossary.md) - Terminology and definitions
- [examples/policies/README.md](examples/policies/README.md) - Policy examples

---

## 📚 User & Developer Guides

Practical guides for using and building with ICN.

### User Guides (`guides/user/`)

End-user documentation:

- [README.md](guides/user/README.md) - User guide index
- [WHY_ICN_HANDOUT.md](guides/user/WHY_ICN_HANDOUT.md) - ICN value proposition
- [cooperative-setup-guide.md](guides/user/cooperative-setup-guide.md) - Setting up a cooperative

### Developer Guides (`guides/developer/`)

Technical guides for contributors:

- [README.md](guides/developer/README.md) - Developer guide index
- [DEV_ENVIRONMENT.md](guides/developer/DEV_ENVIRONMENT.md) - Development environment setup
- [DOCUMENTATION_STYLE.md](guides/developer/DOCUMENTATION_STYLE.md) - Documentation conventions
- [i18n-guide.md](guides/developer/i18n-guide.md) - Internationalization guide

### Operations Guides (`guides/operations/`)

Deployment and operations documentation:

- [README.md](guides/operations/README.md) - Operations guide index
- [operations-guide.md](guides/operations/operations-guide.md) - General operations
- [backup-and-recovery.md](guides/operations/backup-and-recovery.md) - Backup procedures
- [replication-operations.md](guides/operations/replication-operations.md) - Storage replication
- [pilot-smoke.md](guides/operations/pilot-smoke.md) - Deterministic pilot linkage smoke runbook
- [troubleshooting.md](guides/operations/troubleshooting.md) - Common issues and solutions

### Quick References

- [FAQ.md](guides/FAQ.md) - Frequently asked questions
- [QUICK_REFERENCE.md](guides/QUICK_REFERENCE.md) - Command cheat sheet

---

## 🛠️ Development Documentation

Internal development resources, testing, and CI/CD.

### Development Process (`development/`)

- [README.md](development/README.md) - Development documentation index

**Sprints (`development/sprints/`):**
- [README.md](development/sprints/README.md) - Sprint planning and tracking
- Sprint completion records

**Testing (`development/testing/`):**
- [README.md](development/testing/README.md) - Testing documentation index
- Integration and E2E testing guides
- Mobile app testing procedures

### CI/CD & Automation (`ci/`)

Continuous integration and delivery:

- [GATE_RATCHET_PLAN.md](ci/GATE_RATCHET_PLAN.md) - CI check graduation schedule
- CI configuration and status reports

### Performance (`performance/`)

- [README.md](performance/README.md) - Performance documentation index
- Benchmarks and optimization guides

---

## 🔒 Security & Compliance

Security documentation, threat models, and audit reports.

### Security Documentation (`security/`)

**Production Security:**
- [FINAL_SECURITY_STATUS.md](security/FINAL_SECURITY_STATUS.md) - Production readiness assessment ✅
- [COMPREHENSIVE_SECURITY_IMPROVEMENTS.md](security/COMPREHENSIVE_SECURITY_IMPROVEMENTS.md) - Security overview
- [SECURITY_FIXES_2025-12-18.md](security/SECURITY_FIXES_2025-12-18.md) - Detailed vulnerability fixes
- [SECURITY_TESTING_GUIDE.md](security/SECURITY_TESTING_GUIDE.md) - Testing procedures
- [production-hardening.md](security/production-hardening.md) - Production hardening measures

**Threat Models & Audits:**
- [threat-model.md](security/threat-model.md) - Comprehensive threat analysis
- [security-roadmap.md](security/security-roadmap.md) - Security roadmap
- [SECURITY_AUDIT_REPORT.md](security/SECURITY_AUDIT_REPORT.md) - Audit findings
- [SECURITY_AUDIT_RESULTS.md](security/SECURITY_AUDIT_RESULTS.md) - Audit results

**SDIS Security:**
- [SDIS_THREAT_MODEL.md](security/SDIS_THREAT_MODEL.md) - SDIS-specific threats
- [SDIS_CRYPTO_REVIEW.md](security/SDIS_CRYPTO_REVIEW.md) - Cryptographic review
- [SDIS_AUDIT_CHECKLIST.md](security/SDIS_AUDIT_CHECKLIST.md) - SDIS audit checklist

**Specialized Topics:**
- [TOFU_SECURITY_MODEL.md](security/TOFU_SECURITY_MODEL.md) - Trust-On-First-Use model
- [GATEWAY_CSP.md](security/GATEWAY_CSP.md) - Content Security Policy
- [SECRET_MANAGEMENT.md](security/SECRET_MANAGEMENT.md) - Secret management practices
- [EDUCATIONAL_GUIDE_SECURITY_FIXES.md](security/EDUCATIONAL_GUIDE_SECURITY_FIXES.md) - Learning resource

### SDIS Documentation (`sdis/`)

Sovereign Digital Identity System:

- [SDIS_SYSTEM.md](sdis/SDIS_SYSTEM.md) - System overview
- [SDIS_STATUS.md](sdis/SDIS_STATUS.md) - Implementation status
- [SDIS_IMPLEMENTATION_PLAN.md](sdis/SDIS_IMPLEMENTATION_PLAN.md) - Implementation roadmap
- [SDIS_IMPLEMENTATION_COMPLETE.md](sdis/SDIS_IMPLEMENTATION_COMPLETE.md) - Completion report
- [SDIS_QUICK_START.md](sdis/SDIS_QUICK_START.md) - Quick start guide
- [SDIS_USER_GUIDE.md](sdis/SDIS_USER_GUIDE.md) - User guide
- [SDIS_API_GUIDE.md](sdis/SDIS_API_GUIDE.md) - API documentation
- [SDIS_BUILD_PLAN.md](sdis/SDIS_BUILD_PLAN.md) - Build planning
- [SDIS_STEWARD_ROADMAP.md](sdis/SDIS_STEWARD_ROADMAP.md) - Steward network roadmap
- [SDIS_DEPLOYMENT_STATUS.md](sdis/SDIS_DEPLOYMENT_STATUS.md) - Deployment status

---

## 🎯 Specialized Topics

### Onboarding (`onboarding/`)

Contributor onboarding materials:

- [README.md](onboarding/README.md) - Onboarding program overview
- [manual.md](onboarding/manual.md) - Complete onboarding manual
- [syllabus.md](onboarding/syllabus.md) - Learning syllabus
- [reading-map.md](onboarding/reading-map.md) - Documentation reading order
- [patterns.md](onboarding/patterns.md) - Code patterns guide
- Plus modules, tracks, and workshops

### Deployment & Operations

**Deployment Guides (`deployment/`):**
- Deployment configuration and guides

**Operations (`operations/`):**
- [README.md](operations/README.md) - Operations documentation
- Monitoring, runbooks, and operational procedures

**Ops Runbooks (`ops/runbooks/`):**
- [README.md](ops/runbooks/README.md) - Runbook index
- Incident response procedures

### Mobile & Observability

**Mobile (`mobile/`):**
- Mobile app documentation

**Observability (`observability/`):**
- Metrics, logging, and tracing

### Pilot Programs (`internal/pilots/`)

Real-world pilot deployment documentation:

- [pilot-playbook.md](internal/pilots/pilot-playbook.md) - Pilot deployment guide
- [pilot-coordinator-guide.md](internal/pilots/pilot-coordinator-guide.md) - Coordinator handbook
- [pilot-readiness-gaps.md](internal/pilots/pilot-readiness-gaps.md) - Readiness assessment
- [pilot-limitations.md](internal/pilots/pilot-limitations.md) - Known limitations

---

## 🔬 Internal Documentation

Internal planning, status tracking, and project management.

### Internal Overview (`internal/`)

- [README.md](internal/README.md) - Internal documentation index
- [legal-considerations.md](internal/legal-considerations.md) - Legal notes

### Status Tracking (`internal/status/`)

Active project status and gap analyses:

- [GAP_ANALYSIS.md](internal/status/GAP_ANALYSIS.md) - Current gaps
- [gap-analysis.md](internal/status/gap-analysis.md) - Gap tracking
- [strategic-gap-analysis.md](internal/status/strategic-gap-analysis.md) - Strategic gaps
- [vision-implementation-gap.md](internal/status/vision-implementation-gap.md) - Vision gaps
- [multi-device-status.md](internal/status/multi-device-status.md) - Multi-device status

### Current Status Reports (`status/`)

Live status reports and deployment verification:

- [PROJECT_STATE_2026-02-09.md](status/PROJECT_STATE_2026-02-09.md) - Current project state ⭐
- [CURRENT_SYSTEM_STATUS.md](status/CURRENT_SYSTEM_STATUS.md) - System status
- [DEPLOYMENT_VERIFICATION.md](status/DEPLOYMENT_VERIFICATION.md) - Deployment verification
- [FINAL_CI_RESOLUTION.md](status/FINAL_CI_RESOLUTION.md) - CI resolution status
- [FINAL_DEMO_SUCCESS.md](status/FINAL_DEMO_SUCCESS.md) - Demo success report
- [FINAL_SESSION_STATUS.md](status/FINAL_SESSION_STATUS.md) - Session status
- [MOBILE_APP_DEMO.md](status/MOBILE_APP_DEMO.md) - Mobile app demo
- [TESTS_FIXED_STATUS.md](status/TESTS_FIXED_STATUS.md) - Test fix status
- [2025-12-25-sprint-status.md](status/2025-12-25-sprint-status.md) - Sprint status

### Architecture Decision Records (`adr/`)

Formal architectural decisions:

- [ADR-0010-app-topology.md](adr/ADR-0010-app-topology.md) - App topology decision
- Additional ADRs as numbered documents

### Templates (`templates/`)

Documentation and code templates for consistency.

### Vision Documents (`vision/`)

High-level vision and strategic direction.

### AI & Automation (`ai/`)

- [CODEX_WORKFLOW.md](ai/CODEX_WORKFLOW.md) - AI workflow and agent rules

---

## 📦 Archives

Historical documentation, completed phases, and superseded materials.

### Archive Structure (`archive/2025/`)

Organized by year, containing completed work and historical records:

- [README.md](archive/2025/README.md) - Archive index
- [SUMMARY.md](archive/2025/SUMMARY.md) - Year summary

**Key Archived Documents:**
- Phase completion reports (PHASE_18_COMPLETE.md, etc.)
- Historical deployment guides (KUBERNETES_DEPLOYMENT.md, PRODUCTION_DEPLOYMENT_GUIDE.md)
- System status snapshots (PROJECT_STATUS_2025-12-06.md, SYSTEM_GAPS_2025-12-06.md)
- Completed analyses (FOUNDATIONAL_REVIEW_2025-12-16.md, CODE_REVIEW_COMPLETE.md)
- Security audit resolutions (security-audit-resolution-2025-12-18.md, security-hardening-2025-12-18.md)
- Integration reports (GOVERNANCE-LEDGER-INTEGRATION-COMPLETE.md, GOVERNANCE-LEDGER-BUGS-FOUND.md)
- Evolution documents (COMMONS_EVOLUTION_SUMMARY.md, ICN_COMPLETE_ARCHITECTURE_SYNTHESIS.md)
- HSM/TPM documentation (hsm-tpm-roadmap.md, tpm-setup.md, tpm-implementation-plan.md)
- Bug reports and sprint plans
- Plus 20+ additional historical documents

**Migration Notes:** Documents are moved to archives when superseded, completed, or no longer actively referenced. See [REORGANIZATION_2026.md](REORGANIZATION_2026.md) for migration details.

---

## 🗂️ Documentation by Role

### For New Contributors
1. [GETTING_STARTED.md](GETTING_STARTED.md) - Quick start
2. [onboarding/README.md](onboarding/README.md) - Onboarding program
3. [guides/developer/README.md](guides/developer/README.md) - Developer guides
4. [ARCHITECTURE.md](ARCHITECTURE.md) - Architecture overview

### For Core Developers
1. [ARCHITECTURE.md](ARCHITECTURE.md) - Full architecture
2. [architecture/ARCHITECTURE_MAP.md](architecture/ARCHITECTURE_MAP.md) - Visual guide
3. [design/](design/) - Design documents
4. [development/testing/README.md](development/testing/README.md) - Testing guides

### For Operations Engineers
1. [guides/operations/README.md](guides/operations/README.md) - Operations guides
2. [security/FINAL_SECURITY_STATUS.md](security/FINAL_SECURITY_STATUS.md) - Security status
3. [deployment/](deployment/) - Deployment configs
4. [operations/README.md](operations/README.md) - Operational procedures

### For Security Engineers
1. [security/FINAL_SECURITY_STATUS.md](security/FINAL_SECURITY_STATUS.md) - Security overview
2. [security/threat-model.md](security/threat-model.md) - Threat analysis
3. [security/SECURITY_TESTING_GUIDE.md](security/SECURITY_TESTING_GUIDE.md) - Testing
4. [security/production-hardening.md](security/production-hardening.md) - Hardening

### For Product/Community
1. [guides/user/WHY_ICN_HANDOUT.md](guides/user/WHY_ICN_HANDOUT.md) - Value proposition
2. [design/economics/ECONOMIC_VISION.md](design/economics/ECONOMIC_VISION.md) - Economic vision
3. [design/MINIMAL-VIABLE-COOP.md](design/MINIMAL-VIABLE-COOP.md) - MVC specification
4. [internal/pilots/pilot-playbook.md](internal/pilots/pilot-playbook.md) - Pilot guide

---

## 📊 Documentation Statistics

- **Total Markdown Files**: 200+
- **Core Documentation**: 7 files
- **Architecture Docs**: 25+ files
- **Design Documents**: 35+ files
- **Reference Materials**: 15+ files
- **Guides**: 20+ files
- **Security Documents**: 20+ files
- **Internal/Status**: 15+ files
- **Archived Documents**: 30+ files
- **Total Documentation Size**: ~2MB+ of structured knowledge

---

## 🔍 Search Tips

### Find by Topic
- Use your IDE's search (Ctrl/Cmd+Shift+F) across `docs/` directory
- Check specific subdirectories for focused results
- Search [glossary.md](glossary.md) for terminology

### Find by Date
- Recent work: Check `status/` directory
- Historical: Browse `archive/2025/`
- Development timeline: See [PHASE_HISTORY.md](PHASE_HISTORY.md)

### Find by Component
- **Identity**: Search `identity` in architecture/ and design/
- **Trust**: Check `trust` in architecture/ and security/
- **Ledger**: Search `ledger` or `economic` in design/economics/
- **Governance**: Browse design/governance/
- **Federation**: Search `federation` in architecture/
- **SDIS**: Check sdis/ directory

---

## 📞 Getting Help

- **Documentation Issues**: File an issue on GitHub
- **Architecture Questions**: Check architecture/ or ask in dev channels
- **Operations Support**: See guides/operations/troubleshooting.md
- **Contributing**: Read CONTRIBUTING.md in project root
- **Security Issues**: Follow responsible disclosure process

---

## 🔄 Documentation Maintenance

### Keeping This Index Current

This index should be updated when:
- New major documents are added
- Directory structure changes
- Documents are archived
- Significant reorganizations occur

### Last Major Updates
- **2026-02-10**: Phase 2C reorganization (this version)
- **2026-01-17**: Previous organization
- See [REORGANIZATION_2026.md](REORGANIZATION_2026.md) for detailed change history

---

**Navigation**: [Top](#icn-documentation-index) | [Core Docs](#-core-documentation) | [Architecture](#️-architecture--design) | [Reference](#-reference-documentation) | [Guides](#-user--developer-guides) | [Archives](#-archives)
