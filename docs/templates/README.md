# Documentation Templates

Templates for consistent documentation across the ICN project.

**Last Updated**: 2026-03-26

## Overview

This directory contains templates for creating new documentation following ICN conventions.

## Available Templates

### Control-plane authoring (imported)

Use these for durable decisions and specs (see also `docs/internal/documentation-control-system/`):

| Template | Use |
|----------|-----|
| [adr-template.md](adr-template.md) | Architecture Decision Records |
| [rfc-template.md](rfc-template.md) | Request for comments / design proposals |
| [spec-template.md](spec-template.md) | Normative specifications |
| [status-note-template.md](status-note-template.md) | Short status / checkpoint notes |

**Specs for the doc system itself:** [../internal/documentation-control-system/00_CONTEXT_PACK_README.md](../internal/documentation-control-system/00_CONTEXT_PACK_README.md)

### Project templates (existing)

| Template | Use |
|----------|-----|
| [dev-journal.md](dev-journal.md) | Session / dev journal entries |
| [supersession-template.md](supersession-template.md) | Marking superseded docs |
| [BETA_FEEDBACK_TEMPLATE.md](BETA_FEEDBACK_TEMPLATE.md) | Beta feedback |

Other areas (feature designs, security, API) should still follow [../guides/developer/DOCUMENTATION_STYLE.md](../guides/developer/DOCUMENTATION_STYLE.md).

## Using Templates

1. Copy the relevant template
2. Fill in all required sections
3. Follow naming conventions (see [../guides/developer/DOCUMENTATION_STYLE.md](../guides/developer/DOCUMENTATION_STYLE.md))
4. Place in appropriate directory
5. Link from relevant indexes

## Template Guidelines

- Include metadata (status, date, authors)
- Use consistent formatting
- Provide clear section descriptions
- Include examples where helpful
- Mark optional sections clearly

## Related Documentation

- **Documentation Style Guide**: [../guides/developer/DOCUMENTATION_STYLE.md](../guides/developer/DOCUMENTATION_STYLE.md)
- **Contributing Guidelines**: [../../CONTRIBUTING.md](../../CONTRIBUTING.md)
- **Architecture Index**: [../architecture/ARCHITECTURE_INDEX.md](../architecture/ARCHITECTURE_INDEX.md)

---

**Navigation**: [Back to Index](../INDEX.md) | [Developer Guides](../guides/developer/)
