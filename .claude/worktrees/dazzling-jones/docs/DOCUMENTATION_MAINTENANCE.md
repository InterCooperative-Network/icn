# Documentation Maintenance Guide

This guide provides guidelines for maintaining ICN documentation structure and quality.

**Last Updated**: 2026-02-10  
**Version**: 1.0

---

## Quick Decision Tree: Where to Put Documentation

```
┌─ New documentation? ────────────────────────────────────┐
│                                                          │
├─ Is it architectural/design? ──────────────────────────►├─ docs/architecture/ or docs/design/
│                                                          │
├─ Is it a formal specification? ────────────────────────►├─ docs/spec/
│                                                          │
├─ Is it API/config reference? ──────────────────────────►├─ docs/reference/api/ or docs/reference/config/
│                                                          │
├─ Is it a user or developer guide? ─────────────────────►├─ docs/guides/user/ or docs/guides/developer/
│                                                          │
├─ Is it security-related? ──────────────────────────────►├─ docs/security/
│                                                          │
├─ Is it deployment/operations? ─────────────────────────►├─ docs/guides/operations/ or docs/deployment/
│                                                          │
├─ Is it SDIS-specific? ─────────────────────────────────►├─ docs/sdis/
│                                                          │
├─ Is it historical/completed? ──────────────────────────►├─ docs/archive/YYYY/
│                                                          │
├─ Is it internal planning? ─────────────────────────────►├─ docs/internal/
│                                                          │
├─ Is it a status report? ───────────────────────────────►├─ docs/status/
│                                                          │
└─ When in doubt ────────────────────────────────────────►└─ Ask in PR or create issue
```

---

## Directory Categories

### Core Documents (`docs/`)

**Never add to project root except these files:**
- `README.md`, `CHANGELOG.md`, `CLAUDE.md`, `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `AGENTS.md`

Core documentation files live directly in `docs/`:
- `ARCHITECTURE.md` - Master architecture document
- `GETTING_STARTED.md` - Quick start guide
- `PHASE_HISTORY.md` - Completed development phases
- `STATE.md` - Current project state snapshot
- `TODO.md` - Active work items and priorities
- `INDEX.md` - Documentation index (update when adding major sections)
- `README.md` - Documentation overview
- `glossary.md` - Terminology definitions

### Architecture & Design (`docs/architecture/`, `docs/design/`)

**When to use `architecture/`:**
- Comprehensive architectural reviews
- Design decisions affecting multiple components
- System-wide patterns and conventions
- Architectural Decision Records (ADRs)
- Gap analyses and audit reports

**When to use `design/`:**
- Feature-specific designs
- Evolution plans and roadmaps
- Proposals for new capabilities
- Domain-specific designs (economics, governance, SDIS)

**Subdirectories:**
- `design/economics/` - Economic system designs
- `design/governance/` - Governance system designs
- `design/sdis/` - SDIS-specific designs

### Specifications (`docs/spec/`)

**Use for:**
- Formal protocol specifications
- Wire format definitions
- Contract specifications
- Anything requiring precise, versioned definitions

### Reference Documentation (`docs/reference/`)

**Use for:**
- API documentation (`reference/api/`)
- Configuration references (`reference/config/`)
- Command-line tool references
- Technical glossaries and references

### Guides (`docs/guides/`)

**Subdirectories:**
- `guides/user/` - End-user documentation (cooperative setup, usage)
- `guides/developer/` - Developer documentation (contribution, debugging)
- `guides/operations/` - Operations documentation (deployment, monitoring)

**Guidelines:**
- Keep guides practical and task-oriented
- Include code examples and commands
- Link to reference documentation for details

### Development Resources (`docs/development/`)

**Use for:**
- Sprint planning and tracking (`development/sprints/`)
- Testing documentation (`development/testing/`)
- Development workflows
- Build and release processes

### Security Documentation (`docs/security/`)

**Use for:**
- Security audits
- Threat models
- Vulnerability reports and fixes
- Security guidelines
- Penetration test results

### SDIS Documentation (`docs/sdis/`)

**Use for:**
- SDIS implementation guides
- VUI documentation
- Zero-knowledge proof documentation
- SDIS-specific protocols

### Internal Documentation (`docs/internal/`)

**Use for:**
- Internal planning documents
- Pilot program documentation (`internal/pilots/`)
- Status tracking (`internal/status/`)
- Documents not yet ready for public consumption

### Status Reports (`docs/status/`)

**Use for:**
- Current project state snapshots
- Deployment verification reports
- System health reports
- Periodic status updates

**Naming convention:** `PROJECT_STATE_YYYY-MM-DD.md` or `COMPONENT_STATUS_YYYY-MM-DD.md`

### Archives (`docs/archive/`)

**Organization:**
```
docs/archive/
└── YYYY/
    ├── design/
    ├── guides/
    ├── sprints/
    ├── sessions/
    └── status/
```

**When to archive:**
- Completed work that is superseded
- Historical sprint plans
- Old design documents no longer relevant
- Outdated guides or references
- Session notes older than 6 months (unless frequently referenced)

**When NOT to archive:**
- Living documents (ARCHITECTURE.md, STATE.md, TODO.md)
- Current guides and references
- Active design documents
- Frequently referenced historical documents

---

## Archival Policy

### When to Archive

**Archive documentation when:**
1. **Superseded by newer versions** - Old design replaced by new implementation
2. **Completed work** - Sprint plans, session notes for completed phases
3. **Historical value only** - Documents useful for context but not current guidance
4. **Age > 6 months** and no longer actively referenced

**Keep in active structure when:**
1. **Frequently referenced** - Even if historical, if referenced often keep active
2. **Living documents** - Continuously updated (ARCHITECTURE.md, STATE.md, etc.)
3. **Current guidance** - Active guides, references, specifications
4. **Recent status** - Status reports from last 3 months

### Archival Process

1. **Create year directory** if it doesn't exist:
   ```bash
   mkdir -p docs/archive/YYYY/
   ```

2. **Move files to appropriate subdirectory**:
   ```bash
   # Example: Archive old sprint plans
   mv docs/development/sprints/sprint-2025-01-* docs/archive/2025/sprints/
   
   # Example: Archive superseded design
   mv docs/design/old-feature-design.md docs/archive/2025/design/
   ```

3. **Update links**:
   - Search for references to moved files: `grep -r "old-file.md" docs/`
   - Update links to point to archive location or remove if no longer relevant
   - Add redirect note in prominent locations

4. **Document in archive README**:
   ```bash
   # Update docs/archive/YYYY/README.md
   echo "- old-file.md - Reason for archival, date moved" >> docs/archive/YYYY/README.md
   ```

5. **Update INDEX.md** if major sections moved

---

## Naming Conventions

### File Names

**Use lowercase with hyphens:**
- ✅ `multi-device-identity-design.md`
- ✅ `api-reference.md`
- ❌ `MultiDeviceIdentityDesign.md`
- ❌ `API_REFERENCE.md`

**Exceptions (all caps):**
- Core docs: `ARCHITECTURE.md`, `GETTING_STARTED.md`, `README.md`, `INDEX.md`
- ADRs: `ADR-001-decision-name.md`

**Prefixes for special types:**
- ADRs: `ADR-NNN-` (Architecture Decision Records)
- Status reports: `PROJECT_STATE_` or `COMPONENT_STATUS_`

### Directory Names

**Use lowercase singular or plural as appropriate:**
- ✅ `architecture/`, `guides/`, `reference/`
- ✅ `api/`, `config/`, `security/`
- ❌ `Architecture/`, `Guides/`

---

## README Update Process

### Category READMEs

**Every category directory must have a README.md:**

Template:
```markdown
# [Category Name]

Brief description of what belongs in this category.

## Contents

- `document.md` - Brief description
- `subdirectory/` - Brief description

## Related Documentation

- `Link to related category` (`./other-category/`)
- `Link to INDEX.md` (`./INDEX.md`)
```

**Update when:**
- Adding new documents to category
- Reorganizing category structure
- Changing document locations

### INDEX.md Updates

**Update `docs/INDEX.md` when:**
- Adding new top-level categories
- Adding significant new documents
- Reorganizing major sections
- Changing navigation structure

**Don't update for:**
- Minor documents within existing categories
- Category README changes
- Small content updates

**Process:**
1. Add entry in appropriate section
2. Update "Last Updated" date at top
3. Maintain alphabetical ordering within sections
4. Keep descriptions concise (one line)

---

## Link Checking Guidelines

### Internal Links

**Use relative paths:**
- ✅ `[Architecture](./architecture/ARCHITECTURE_MAP.md)`
- ✅ `[Getting Started](./GETTING_STARTED.md)`
- ❌ Example absolute-path link syntax (avoid): `/docs/architecture/ARCHITECTURE_MAP.md`
- ❌ `[Architecture](https://github.com/.../ARCHITECTURE_MAP.md)` (external)

**Check links when:**
- Moving or renaming files
- Reorganizing directories
- Before major documentation releases
- Periodically (monthly recommended)

**Tools:**
```bash
# Find markdown links in docs (fallback heuristic)
rg -n "\\]\\([^)]*\\.md\\)" docs/

# Check for markdown link targets ending in .md
find docs/ -name "*.md" -exec rg -n "\\]\\([^)]*\\.md\\)" {} \;
```

### External Links

**Check external links:**
- Before major releases
- Periodically (quarterly recommended)
- When referenced projects release major versions

**Document known issues:**
- If external links are temporarily broken, add note in document
- Consider Internet Archive links for important references

---

## Cross-References Best Practices

### When to Cross-Reference

**Always link when:**
- Mentioning another document's topic
- Referring to concepts defined elsewhere
- Directing users to more detail
- Building documentation hierarchy

**Example:**
```markdown
ICN uses Tokio for its actor runtime. For details on actor communication 
patterns, see [Architecture Overview](./architecture/README.md).

For API usage examples, see the [Developer Guide](./guides/developer/README.md).
```

### Cross-Reference Format

**Use descriptive link text:**
- ✅ `[Architecture Overview](./architecture/README.md)`
- ❌ `[here](./architecture/README.md)`
- ❌ `[click here](./architecture/README.md)`

**Include section anchors when relevant:**
- ✅ `[Trust Graph Computation](./ARCHITECTURE.md#trust-graph)`
- ✅ `[API Versioning Strategy](./reference/api/api-versioning.md#semver-approach)`

---

## Documentation Quality Checklist

Before committing new documentation:

- [ ] **Location**: File is in correct category directory
- [ ] **Naming**: Follows naming conventions (lowercase-with-hyphens.md)
- [ ] **README**: Category README updated if needed
- [ ] **INDEX**: docs/INDEX.md updated if major document
- [ ] **Links**: All internal links use relative paths
- [ ] **Links**: All internal links verified to exist
- [ ] **Cross-refs**: Key concepts link to detailed docs
- [ ] **Code examples**: Tested and working
- [ ] **File paths**: Rust paths verified (icn/crates/...)
- [ ] **Commands**: Build/test commands verified
- [ ] **Metadata**: Date and version at top if relevant
- [ ] **Formatting**: Markdown properly formatted
- [ ] **Spelling**: Spell-checked
- [ ] **Grammar**: Proofread

---

## Metadata Standards

### Document Headers

**Include at top of significant documents:**

```markdown
# Document Title

Brief one-line description.

**Last Updated**: YYYY-MM-DD  
**Version**: X.Y  
**Status**: Draft | Active | Superseded | Archived

---
```

**Fields:**
- **Last Updated**: Date of last significant change (YYYY-MM-DD)
- **Version**: Semantic version or simple incrementing number
- **Status**: Current status (Draft, Active, Superseded, Archived)

### Status Values

- **Draft**: Work in progress, not yet authoritative
- **Active**: Current, authoritative documentation
- **Superseded**: Replaced by newer document (link to replacement)
- **Archived**: Historical, no longer applicable

---

## Review Cycles

### Quarterly Reviews

**Every 3 months, review:**
1. Core documentation (ARCHITECTURE.md, GETTING_STARTED.md, etc.)
2. Guide accuracy and completeness
3. API reference alignment with code
4. Broken links and outdated references
5. Archive candidates

### Before Major Releases

**Documentation tasks:**
1. Update all version references
2. Verify all guides and examples
3. Check API documentation completeness
4. Update CHANGELOG.md
5. Review and update INDEX.md
6. Archive completed phase documentation

---

## Common Mistakes to Avoid

❌ **Don't:**
- Save docs to project root (except core files)
- Use absolute paths in links
- Leave broken internal links
- Create orphaned documents (no links to them)
- Duplicate information without cross-refs
- Use ambiguous file names (e.g., `notes.md`)
- Forget to update category READMEs

✅ **Do:**
- Use docs/ category structure
- Use relative paths in links
- Verify links before committing
- Link new docs from INDEX.md or category README
- Cross-reference instead of duplicating
- Use descriptive, specific file names
- Keep READMEs up to date

---

## Contact & Questions

For questions about documentation organization:
- **Create an issue**: Label with `documentation`
- **Ask in PR**: Tag reviewers for guidance
- **Update this guide**: Submit PR with improvements

---

## Version History

- **1.0** (2026-02-10): Initial maintenance guide created during Phase 2D reorganization
