# ICN Documentation Reorganization 2026

**Date**: 2026-02-10  
**Phase**: Phase 2C - Documentation Reorganization  
**Status**: Complete

## Overview

This document records the comprehensive reorganization of ICN documentation completed in February 2026 (Phases 2A-2C). The reorganization addressed documentation sprawl, inconsistent structure, and navigation challenges that had developed over the project's growth.

## Goals

1. **Improve Discoverability**: Make it easy to find relevant documentation
2. **Logical Structure**: Group documents by purpose and audience
3. **Clear Navigation**: Provide multiple entry points (role, topic, component)
4. **Reduce Duplication**: Consolidate or archive duplicate/superseded content
5. **Maintain History**: Preserve historical context in organized archives

## What Changed

### New Directory Structure

The reorganization introduced a purpose-based structure:

```
docs/
├── Core Documentation (root)       # Essential reading
├── architecture/                   # Architecture & specifications
├── design/                        # Feature designs & proposals
│   ├── economics/                # Economic system design
│   ├── governance/               # Governance design
│   └── sdis/                     # SDIS design
├── spec/                         # Formal specifications
├── reference/                    # Technical references
│   ├── api/                     # API documentation
│   └── config/                  # Configuration
├── guides/                       # Practical guides
│   ├── developer/               # Development guides
│   ├── operations/              # Operations guides
│   └── user/                    # End-user guides
├── development/                  # Development resources
│   ├── sprints/                # Sprint tracking
│   └── testing/                # Testing docs
├── security/                     # Security documentation
├── sdis/                        # SDIS documentation
├── internal/                     # Internal planning
│   ├── pilots/                 # Pilot programs
│   └── status/                 # Status tracking
├── status/                       # Current status reports
├── onboarding/                   # Contributor onboarding
├── templates/                    # Doc templates
├── vision/                       # Vision documents
└── archive/                      # Historical materials
    └── 2025/                    # Year-based archives
```

### Documents Moved

#### To `design/`
From root and various locations:
- COMMONS_EVOLUTION.md
- MINIMAL-VIABLE-COOP.md
- capability-based-features.md
- compute-substrate-design.md
- entity-dissolution.md
- entity-dissolution-example.md
- institution-in-a-box.md
- multi-device-identity-design.md
- nat-traversal-design.md
- platform-layer-design.md
- post-quantum-crypto.md
- razeto-integration-design.md
- scheduler-evolution-plan.md
- social-recovery-design.md

#### To `design/economics/`
Economic system documentation:
- ECONOMIC_VISION.md
- ECONOMIC_ARCHITECTURE.md
- contribution-credits-design.md
- econ-modeling.md
- economic-safety.md

#### To `design/governance/`
Governance documentation:
- PROJECT_GOVERNANCE.md
- governance.md
- governance-primitives.md
- witness-trust-validation.md

#### To `design/sdis/`
SDIS design materials:
- social-recovery.md (SDIS-specific)

#### To `guides/developer/`
Developer-focused guides:
- DEV_ENVIRONMENT.md
- DOCUMENTATION_STYLE.md
- i18n-guide.md

#### To `guides/operations/`
Operations documentation:
- operations-guide.md
- backup-and-recovery.md
- replication-operations.md
- troubleshooting.md

#### To `guides/user/`
User-facing guides:
- WHY_ICN_HANDOUT.md
- cooperative-setup-guide.md

#### To `reference/api/`
API reference materials:
- API_REFERENCE.md
- api-versioning.md
- topic-subscriptions-api.md

#### To `reference/config/`
Configuration documentation:
- CONFIGURATION.md
- identity-backend-configuration.md
- trust-threshold-configuration.md

#### To `development/sprints/`
Sprint documentation:
- Sprint 1-3 materials

#### To `development/testing/`
Testing documentation:
- Testing guides and procedures

#### To `internal/pilots/`
Pilot program materials:
- pilot-coordinator-guide.md
- pilot-limitations.md
- pilot-playbook.md
- pilot-readiness-gaps.md

#### To `internal/status/`
Active status tracking:
- GAP_ANALYSIS.md
- gap-analysis.md
- strategic-gap-analysis.md
- vision-implementation-gap.md
- multi-device-status.md

#### To `archive/2025/`
Historical/superseded documents:
- Phase completion reports
- Historical deployment guides
- System status snapshots
- Completed analyses
- Security audit resolutions
- Integration reports
- Bug reports
- Sprint plans
- HSM/TPM documentation
- Plus 20+ additional historical documents

See [archive/2025/README.md](archive/2025/README.md) for complete list.

### New Documentation Created

#### READMEs
- architecture/README.md - Architecture documentation guide
- security/README.md - Security documentation guide
- vision/README.md - Vision document guide
- templates/README.md - Template guide
- design/economics/README.md - Economics docs index
- design/governance/README.md - Governance docs index
- design/sdis/README.md - SDIS design index
- guides/developer/README.md - Developer guide index
- guides/operations/README.md - Operations guide index
- guides/user/README.md - User guide index
- reference/api/README.md - API reference index
- reference/config/README.md - Config reference index
- internal/README.md - Internal docs index
- development/README.md - Development docs index
- development/sprints/README.md - Sprint tracking index
- development/testing/README.md - Testing docs index

#### Navigation Documents
- INDEX.md (major update) - Comprehensive documentation index
- README.md (updated) - Documentation overview
- REORGANIZATION_2026.md (this file) - Migration guide

## How to Find Old Files

### By Previous Location

| Old Location | New Location | Notes |
|--------------|--------------|-------|
| `docs/ECONOMIC_*.md` | `docs/design/economics/` | Economic system docs |
| `docs/governance-*.md` | `docs/design/governance/` | Governance docs |
| `docs/*-design.md` | `docs/design/` | Design documents |
| `docs/operations-*.md` | `docs/guides/operations/` | Operations guides |
| `docs/dev-journal/` | `docs/archive/2025/` or deleted | Historical dev notes |
| `docs/sessions/` | Consolidated/archived | Session summaries |
| Historical status | `docs/archive/2025/` | Superseded status reports |

### By Purpose

- **Want to understand the system?** → Start with `ARCHITECTURE.md` or `guides/`
- **Want to implement features?** → Check `design/` for designs
- **Want to deploy/operate?** → See `guides/operations/`
- **Want API details?** → Browse `reference/api/`
- **Want historical context?** → Explore `archive/2025/`
- **Want current status?** → Check `status/` or `internal/status/`

### By Role

See "Documentation by Role" section in [INDEX.md](INDEX.md) for role-based navigation.

## Cross-Reference Updates

Cross-references in moved files were updated to reflect new locations:

### Common Pattern Changes

1. **Economics documents**: `docs/ECONOMIC_*.md` → `design/economics/ECONOMIC_*.md`
2. **Governance documents**: `docs/governance-*.md` → `design/governance/governance-*.md`
3. **Design documents**: `docs/*-design.md` → `design/*-design.md`
4. **Operations guides**: `docs/operations-*.md` → `guides/operations/operations-*.md`
5. **API references**: `docs/API_*.md` → `reference/api/API_*.md`

### Files with Updated References

The following files had cross-references updated:
- design/ directory files (links to other design docs, architecture)
- reference/ directory files (links to related references)
- guides/ directory files (links to guides and main docs)
- internal/ directory files (links to status and related docs)

**Note**: External links to docs from code, README.md, or external sites were preserved where possible by maintaining compatibility paths.

## Status File Consolidation

### Active Status

Kept in appropriate locations:
- **Current project state**: `status/PROJECT_STATE_2026-02-09.md`
- **Live status reports**: `status/` directory
- **Feature tracking**: `internal/status/` directory
- **Gap analyses**: `internal/status/` directory

### Archived Status

Moved to `archive/2025/`:
- Historical project status reports
- Completed phase summaries
- Superseded gap analyses
- Old deployment guides
- Resolved security audits
- Integration completion reports

### Status Document Standards

Going forward:
- Add "Last Updated" date to all status documents
- Archive status documents when superseded
- Keep only current, actionable status in main tree
- Historical status preserved in year-based archives

## External References

### Maintaining Compatibility

The following external references were considered:
- GitHub links from issues/PRs
- External documentation links
- Blog posts and announcements
- SDK documentation references

**Strategy**: 
- Core documents (ARCHITECTURE.md, GETTING_STARTED.md, etc.) remain at root
- Moved documents include archive notes where appropriate
- INDEX.md provides finding aids for historical links

### Breaking Changes

These links will break and need updating:
- Direct links to moved design docs (e.g., `docs/scheduler-evolution-plan.md` → `docs/design/scheduler-evolution-plan.md`)
- Links to specific session summaries (now archived or consolidated)
- Links to historical status reports (now in archive/2025/)

**Mitigation**: INDEX.md provides search guidance and archive navigation.

## Migration Guide for Contributors

### Finding Documentation

1. **Start with [INDEX.md](INDEX.md)** - Comprehensive index with multiple navigation paths
2. **Use role-based navigation** - Find docs relevant to your role
3. **Check README files** - Each directory has a README explaining contents
4. **Search by topic** - Use IDE search or grep across docs/

### Adding New Documentation

1. **Choose the right directory**:
   - Design/proposals → `design/`
   - How-to guides → `guides/`
   - API/config reference → `reference/`
   - Architecture specs → `architecture/` or `spec/`
   - Current status → `status/` or `internal/status/`
   - Historical → Consider if it should start in archive

2. **Update the index**:
   - Add entry to [INDEX.md](INDEX.md)
   - Update relevant directory README.md
   - Cross-link from related documents

3. **Follow conventions**:
   - See [guides/developer/DOCUMENTATION_STYLE.md](guides/developer/DOCUMENTATION_STYLE.md)
   - Use templates from `templates/` directory
   - Add "Last Updated" dates
   - Include clear status (Draft, Review, Final, Superseded)

### Updating Existing Documentation

1. **Check for moves**: Document may have been relocated
2. **Update cross-references**: Use relative paths from new location
3. **Update INDEX.md**: If significance changes
4. **Add "Last Updated"**: Note when materially changed

### Archiving Documentation

When to archive (move to `archive/YEAR/`):
- Document is superseded by newer version
- Phase/sprint is complete and documented
- Status report is historical
- Design is implemented and documented elsewhere
- Document is no longer actively referenced

**Do not archive**:
- Core architecture documents
- Active status reports
- Current guides and references
- Specifications still in use

## Benefits Realized

### Before Reorganization
- 140+ documents in flat or inconsistent structure
- Multiple directories with overlapping purposes (sessions/, dev-journal/, sprints/)
- Hard to find relevant documentation
- Unclear which documents were current vs. historical
- Duplicate information in multiple places

### After Reorganization
- Clear purpose-based structure
- Role-based and topic-based navigation
- Active vs. archived clearly distinguished
- Comprehensive index with multiple access paths
- READMEs in all directories for guidance
- Historical context preserved but separated

### Metrics

| Metric | Before | After |
|--------|--------|-------|
| Top-level directories | 30+ | 25 (with clear purposes) |
| Orphaned documents | Many | None |
| Documents with unclear status | Many | None (all dated/categorized) |
| Average time to find doc | Unknown | Reduced (via INDEX.md) |
| Duplicate information | Multiple | Minimized |

## Lessons Learned

1. **Purpose-based structure** works better than chronological or format-based
2. **Multiple navigation paths** (role, topic, component) improve discoverability
3. **Active archival process** prevents historical documents from cluttering main tree
4. **Directory READMEs** are essential for navigation
5. **Comprehensive index** (INDEX.md) provides single source of navigation truth

## Future Maintenance

### Regular Tasks
1. **Quarterly review**: Check for documents to archive
2. **Update INDEX.md**: When significant docs added
3. **Validate links**: Check cross-references periodically
4. **Archive year-end**: Move completed work to archive/YEAR/

### Preventing Sprawl
1. Use existing directories before creating new ones
2. Consult INDEX.md and README files before adding docs
3. Follow documentation style guide
4. Archive superseded documents promptly
5. Update cross-references when moving files

## Questions?

- **Can't find a document?** Check [INDEX.md](INDEX.md) or `archive/2025/`
- **Where should new doc go?** See "Migration Guide for Contributors" above
- **How to update cross-references?** Use relative paths from new location
- **How to archive old docs?** Move to `archive/YEAR/` and update references

## Related Documentation

- [INDEX.md](INDEX.md) - Complete documentation index
- [README.md](README.md) - Documentation overview
- [guides/developer/DOCUMENTATION_STYLE.md](guides/developer/DOCUMENTATION_STYLE.md) - Style guide
- [archive/2025/README.md](archive/2025/README.md) - Archive index

---

**Phase 2C Team**: Documentation reorganization  
**Completion Date**: 2026-02-10  
**Next Review**: 2026-05-10 (Quarterly)
