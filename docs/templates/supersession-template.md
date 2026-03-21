# Supersession Banner Template

Use this template when marking a document as superseded. Prepend it to the top of the affected documentation file.

---

> **⚠️ SUPERSEDED**
>
> This document has been superseded by [REPLACEMENT_TITLE](REPLACEMENT_PATH).
>
> **Superseded on:** YYYY-MM-DD
>
> **Reason:** Brief explanation of why this document is no longer canonical
>
> **Historical value:** Yes/No
>
> If historical value is Yes: Explain what is still useful about this document (e.g., "Original problem statement before scope refinement", "Shows evolution of thinking", "Contains technical details not fully migrated").
>
> The canonical reference is now [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

## Examples

### Example 1: Replaced by updated design doc

> **⚠️ SUPERSEDED**
>
> This document has been superseded by [Institution-in-a-Box Design](docs/design/institution-in-a-box.md).
>
> **Superseded on:** 2026-03-21
>
> **Reason:** Original design iteration did not account for CRDT-based replication; new design incorporates lessons from Q1 2026 prototyping
>
> **Historical value:** Yes — This document contains the initial problem analysis that motivated the CRDT approach. Useful for understanding design evolution.
>
> The canonical reference is now [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

### Example 2: Merged into main docs

> **⚠️ SUPERSEDED**
>
> This document has been superseded by [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (sections 4-6).
>
> **Superseded on:** 2026-03-15
>
> **Reason:** Content merged into single canonical architecture reference
>
> **Historical value:** No — All content has been fully migrated and integrated
>
> The canonical reference is now [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

### Example 3: Obsolete roadmap

> **⚠️ SUPERSEDED**
>
> This document has been superseded by [ICN Live Roadmap](docs/strategy/ICN-Roadmap-Live.md).
>
> **Superseded on:** 2026-03-01
>
> **Reason:** Quarterly plan updated for 2026 priorities
>
> **Historical value:** No — Older roadmap no longer reflects current priorities
>
> The canonical reference is now [docs/strategy/ICN-Roadmap-Live.md](docs/strategy/ICN-Roadmap-Live.md).

---

## How to Use

1. Create a new markdown file with only the banner + brief content explanation
2. Link the old file from registry.toml with status = "superseded"
3. Add entries to registry.toml with: superseded_by, reason, date, historical_value
4. Update references in other docs to point to replacement
5. Archive original if needed (move to docs/archive/)

## Metadata Requirements

In registry.toml, superseded entries must include:

```toml
[docs."path/to/old-doc.md"]
category = "..."
status = "superseded"
title = "..."
superseded_by = "path/to/new-doc.md"
reason = "Brief reason"
date = "2026-03-21"
historical_value = true  # or false
```
