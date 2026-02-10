# Architecture Documentation

Comprehensive architectural documentation, design reviews, and system specifications.

**Last Updated**: 2026-02-10

## Overview

This directory contains in-depth architectural documentation for the ICN system, including comprehensive reviews, audit reports, gap analyses, and detailed component specifications.

## Quick Start

- **New to architecture?** → Start with [ARCHITECTURE_QUICK_REF.md](ARCHITECTURE_QUICK_REF.md)
- **Visual learner?** → Check [ARCHITECTURE_MAP.md](ARCHITECTURE_MAP.md)
- **Need comprehensive details?** → Read [../ARCHITECTURE.md](../ARCHITECTURE.md)
- **Looking for specific component?** → Use [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md)

## Key Documents

### Overview & Navigation

| Document | Description | Size |
|----------|-------------|------|
| [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) | Complete architecture document index | 18KB |
| [ARCHITECTURE_QUICK_REF.md](ARCHITECTURE_QUICK_REF.md) | Quick reference card for developers | 6KB |
| [ARCHITECTURE_MAP.md](ARCHITECTURE_MAP.md) | Visual architecture guide with diagrams | 197KB |

### Core Architecture Specifications

| Document | Description |
|----------|-------------|
| [CANONICAL_ENCODING.md](CANONICAL_ENCODING.md) | Wire format and encoding specifications (critical) |
| [KERNEL_APP_SEPARATION.md](KERNEL_APP_SEPARATION.md) | Kernel/application boundary design (53KB) |
| [CELLS_AND_SCOPES.md](CELLS_AND_SCOPES.md) | Cell-based federation model |
| [SCOPE_BOUNDED_TRUST.md](SCOPE_BOUNDED_TRUST.md) | Trust scope architecture |

### Component Architectures

| Document | Description |
|----------|-------------|
| [FEDERATION_INTEROP_CONTRACT.md](FEDERATION_INTEROP_CONTRACT.md) | Federation protocol specifications |
| [FEDERATION_ACTIONS.md](FEDERATION_ACTIONS.md) | Federation action definitions |
| [CLIENT_MODEL.md](CLIENT_MODEL.md) | Client architecture and patterns |
| [GOVERNANCE_STATE_MACHINE.md](GOVERNANCE_STATE_MACHINE.md) | Governance flow and state transitions |
| [IDENTITY_MEMBERSHIP_ARCHITECTURE.md](IDENTITY_MEMBERSHIP_ARCHITECTURE.md) | Identity and membership system |

### Architecture Reviews & Audits

| Document | Date | Description |
|----------|------|-------------|
| [COMPLETE_ARCHITECTURE_AUDIT_2025-12-17.md](COMPLETE_ARCHITECTURE_AUDIT_2025-12-17.md) | 2025-12-17 | Complete architecture audit |
| [COMPREHENSIVE_ARCHITECTURE_REVIEW_2025-12-17.md](COMPREHENSIVE_ARCHITECTURE_REVIEW_2025-12-17.md) | 2025-12-17 | Comprehensive review |
| [ARCHITECTURE_AUDIT_2025-12-17.md](ARCHITECTURE_AUDIT_2025-12-17.md) | 2025-12-17 | Architecture audit report |
| [ARCHITECTURE_UPDATE_2025-12-17.md](ARCHITECTURE_UPDATE_2025-12-17.md) | 2025-12-17 | Architecture updates |
| [ARCHITECTURE_REVIEW_SUMMARY.md](ARCHITECTURE_REVIEW_SUMMARY.md) | Various | Review summary |
| [ARCHITECTURE_WIRING_AUDIT.md](ARCHITECTURE_WIRING_AUDIT.md) | Various | Component wiring audit |

### Gap Analyses & Status

| Document | Description |
|----------|-------------|
| [ARCHITECTURAL_GAPS_AND_FIXES.md](ARCHITECTURAL_GAPS_AND_FIXES.md) | Identified gaps and fixes |
| [IMPLEMENTATION_GAP_ANALYSIS.md](IMPLEMENTATION_GAP_ANALYSIS.md) | Implementation gaps |
| [ARCHITECTURE_COMPLETE_CHECKLIST.md](ARCHITECTURE_COMPLETE_CHECKLIST.md) | Completion checklist |

### Session Summaries

| Document | Date | Description |
|----------|------|-------------|
| [SESSION_SUMMARY_2025-12-17_ARCHITECTURE.md](SESSION_SUMMARY_2025-12-17_ARCHITECTURE.md) | 2025-12-17 | Architecture session |
| [SESSION_SUMMARY_2025-12-17_ARCHITECTURE_AUDIT.md](SESSION_SUMMARY_2025-12-17_ARCHITECTURE_AUDIT.md) | 2025-12-17 | Audit session |
| [ARCHITECTURE_REVIEW_AND_PQ_SESSION.md](ARCHITECTURE_REVIEW_AND_PQ_SESSION.md) | Various | PQ crypto review |

### Historical/Reference

| Document | Description |
|----------|-------------|
| [ARCHITECTURE_VISUAL.md](ARCHITECTURE_VISUAL.md) | Visual architecture reference |
| [ARCHITECTURE_COMPLETION_STATUS_OLD.md](ARCHITECTURE_COMPLETION_STATUS_OLD.md) | Historical status (superseded) |

## Document Types

### Specifications (Normative)
Documents that define **how the system must work**:
- CANONICAL_ENCODING.md
- KERNEL_APP_SEPARATION.md
- FEDERATION_INTEROP_CONTRACT.md
- CELLS_AND_SCOPES.md

### Reviews (Informative)
Documents that **analyze the current state**:
- COMPLETE_ARCHITECTURE_AUDIT_2025-12-17.md
- COMPREHENSIVE_ARCHITECTURE_REVIEW_2025-12-17.md
- ARCHITECTURE_WIRING_AUDIT.md

### References (Informative)
Documents that **help navigate and understand**:
- ARCHITECTURE_INDEX.md
- ARCHITECTURE_QUICK_REF.md
- ARCHITECTURE_MAP.md

### Gap Analyses (Actionable)
Documents that **identify work needed**:
- ARCHITECTURAL_GAPS_AND_FIXES.md
- IMPLEMENTATION_GAP_ANALYSIS.md

## Related Documentation

- **Main Architecture**: [../ARCHITECTURE.md](../ARCHITECTURE.md) - Comprehensive 160KB architecture document
- **Design Documents**: [../design/](../design/) - Feature designs and proposals
- **Specifications**: [../spec/](../spec/) - Formal protocol specifications
- **Security**: [../security/](../security/) - Security architecture and threat models

## Finding What You Need

### By Component
- **Identity**: IDENTITY_MEMBERSHIP_ARCHITECTURE.md
- **Trust**: SCOPE_BOUNDED_TRUST.md
- **Federation**: FEDERATION_INTEROP_CONTRACT.md, CELLS_AND_SCOPES.md
- **Governance**: GOVERNANCE_STATE_MACHINE.md
- **Client**: CLIENT_MODEL.md
- **Kernel Boundaries**: KERNEL_APP_SEPARATION.md
- **Wire Formats**: CANONICAL_ENCODING.md

### By Task
- **Understanding system**: Start with ARCHITECTURE_QUICK_REF.md
- **Visual overview**: ARCHITECTURE_MAP.md
- **Implementation**: Check IMPLEMENTATION_GAP_ANALYSIS.md
- **Protocol compliance**: CANONICAL_ENCODING.md
- **Audit/review**: COMPLETE_ARCHITECTURE_AUDIT_2025-12-17.md

### By Date
- **Latest (2025-12-17)**: Comprehensive audits and reviews
- **Historical**: ARCHITECTURE_COMPLETION_STATUS_OLD.md

## Contributing

When adding architecture documentation:

1. **Specifications**: Use clear normative language (MUST, SHOULD, MAY)
2. **Diagrams**: Include Mermaid or ASCII art where helpful
3. **Cross-references**: Link to related docs
4. **Status**: Mark documents as Draft, Review, or Final
5. **Updates**: Update ARCHITECTURE_INDEX.md when adding new docs

## Questions?

- Check [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) for document summaries
- See [../guides/developer/README.md](../guides/developer/README.md) for development guides
- Review [../CONTRIBUTING.md](../../CONTRIBUTING.md) for contribution guidelines

---

**Navigation**: [Back to Index](../INDEX.md) | [Main Architecture](../ARCHITECTURE.md) | [Design Docs](../design/)
