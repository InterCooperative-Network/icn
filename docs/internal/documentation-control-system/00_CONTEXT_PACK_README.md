# ICN Docs System Migration — Context Pack

This directory contains the **minimum high-leverage markdown context pack** to give an agent alongside the migration prompt.

## Recommended inclusion order

1. `01_FORMAL_DOC_SYSTEM_SPEC.md`
2. `02_DOCUMENT_REGISTRY_SPEC.md`
3. `03_BOOTSTRAP_REGISTRY_AND_CANON_MAP.md`
4. `04_DOC_MIGRATION_PR_SPEC.md`
5. `05_DOC_PLACEMENT_AND_TRUTH_CLASS_RULES.md`
6. `06_ISSUE_TEMPLATE.md`
7. `07_PR_TEMPLATE.md`
8. `08_ADR_TEMPLATE.md`
9. `09_RFC_TEMPLATE.md`
10. `10_SPEC_TEMPLATE.md`
11. `11_STATUS_NOTE_TEMPLATE.md`
12. `12_AGENT_PROMPT_TEMPLATES.md`

## What this pack is for

This pack is meant to help an agent:
- read and classify the existing docs corpus
- formalize the new docs system
- migrate docs safely
- generate a reviewable PR
- leave behind templates and automation hooks for future work

## Existing repo material intentionally absorbed into this pack

These repo docs were reviewed and their strongest ideas were incorporated:
- `docs/DOCUMENTATION_MAINTENANCE.md`
- `docs/planning/agent-knowledge-architecture.md`
- `docs/registry.toml`
- `docs/freshness.toml`
- `docs/status.toml`
- `docs/templates/supersession-template.md`
- `docs/guides/developer/DOCUMENTATION_STYLE.md`
- `docs/ARCHITECTURE.md`
- `docs/strategy/ADR-001-What-ICN-Is.md`

## Guidance

The pack is designed to prevent creation of a **third competing documentation doctrine**.  
It should help the agent consolidate and formalize what already exists, while upgrading the repo into a cleaner control system.

