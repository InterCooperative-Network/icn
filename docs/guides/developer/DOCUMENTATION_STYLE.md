# Documentation Style Guide

This guide ensures consistent documentation across ICN and related projects (homelab-inventory, deployment repos).

## YAML Frontmatter

All documentation files should begin with YAML frontmatter for machine-parseable metadata:

```yaml
---
date: 2025-12-03
title: "Phase 16C Week 4: Integration & Documentation"
type: dev-journal
phase: 16c
topics: [compute, locality, scheduling, testing]
status: complete
priority: high
related:
  - docs/dev-journal/2025-11-23-phase-16b-placement-negotiation.md
  - docs/compute-substrate-design.md
---
```

### Required Fields

| Field | Description | Values |
|-------|-------------|--------|
| `date` | Document creation date | `YYYY-MM-DD` |
| `title` | Descriptive title | String |
| `type` | Document category | See types below |
| `status` | Current state | `active`, `complete`, `deprecated`, `blocked` |

### Optional Fields

| Field | Description | Values |
|-------|-------------|--------|
| `phase` | ICN development phase | `1`, `16c`, `federation`, etc. |
| `topics` | Subject tags | Array of keywords |
| `priority` | Importance level | `critical`, `high`, `medium`, `low` |
| `related` | Related documents | Array of paths |
| `systems` | Infrastructure affected | Array (for ops docs) |
| `duration` | Time spent | `~1 hour`, `2 days`, etc. |

### Document Types

**Development (`type: dev-journal`)**
- Phase implementation records
- Session logs
- Bug investigation notes

**Architecture (`type: architecture`)**
- Design documents
- Protocol specifications
- API documentation

**Operations (`type: runbook`)**
- Deployment procedures
- Incident response
- Maintenance tasks

**Planning (`type: planning`)**
- Roadmaps
- Sprint plans
- Gap analysis

**Reference (`type: reference`)**
- API docs
- Configuration guides
- FAQs

## File Naming

### Dev Journal
```
docs/dev-journal/YYYY-MM-DD-phase-N-description.md
docs/dev-journal/YYYY-MM-DD-feature-name.md
```

### Other Documentation
```
docs/category-name.md           # e.g., deployment-guide.md
docs/api/endpoint-name.md       # e.g., api/health.md
docs/decisions/YYYY-MM-topic.md # Architecture decisions
```

## Content Structure

### Dev Journal Entry

```markdown
---
date: 2025-12-03
title: "Phase 17: Storage Replication"
type: dev-journal
phase: 17
topics: [storage, replication, durability]
status: complete
duration: ~4 hours
---

# Phase 17: Storage Replication

## Overview

Brief description of what was accomplished.

## Implementation

### 1. Component Name

**Location**: `file.rs:L1-L100`

Description of implementation...

## Testing

- Test description and results
- Validation approach

## Challenges & Solutions

Document any issues encountered and how they were resolved.

## Next Steps

- [ ] Pending work
- [x] Completed items
```

### Architecture/Design Document

```markdown
---
date: 2025-12-03
title: "Compute Substrate Design"
type: architecture
topics: [compute, scheduling, trust]
status: active
priority: high
---

# Compute Substrate Design

## Overview

High-level description.

## Architecture

```
[ASCII diagram or link to diagram]
```

## Components

### Component 1

Description...

## API

### Endpoint/Method

Description...

## Security Considerations

...

## Known Limitations

...
```

## Cross-Repository Consistency

### homelab-inventory (Infrastructure)

Uses additional fields for infrastructure context:

```yaml
systems: [hyperion, atlas, node-1]  # Physical/virtual hosts
people: [matt, claude]              # Contributors
```

Types include: `deployment`, `incident`, `maintenance`, `session`

### ICN (Development)

Uses additional fields for development context:

```yaml
phase: 17              # ICN phase number
duration: ~4 hours     # Time spent
```

Types include: `dev-journal`, `architecture`, `security-analysis`

## Searching Documentation

### ICN (future)
```bash
# Search by topic
grep -r "topics:.*compute" docs/

# Find all complete phase docs
grep -l "status: complete" docs/dev-journal/*.md
```

### homelab-inventory
```bash
make query ARGS="--topic kubernetes"
make query ARGS="--status active"
make show-active
```

## Migration Notes

Existing ICN documents without frontmatter can be updated incrementally. The minimum required frontmatter is:

```yaml
---
date: 2025-MM-DD
title: "Document Title"
type: dev-journal
status: complete
---
```
