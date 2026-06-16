# NYCN Institution Package

NYCN lives here as an institution package inside the monorepo.

This directory is intentionally shaped as if it were already an external repository. Its job is to hold NYCN-specific vocabulary, seeds, templates, summit operations, views, migration glue, and workflow definitions without contaminating ICN core.

## What Belongs Here

- Charter configuration and institution-level policy data
- Bootstrap seeds for NYCN entities, structures, activities, programs, milestones, and role assignments
- NYCN-specific and summit-specific templates, docs, and operational runbooks
- Local dashboards, app/view configuration, and institution-owned UI wiring
- Migration/import placeholders for `ny-coop-net`, spreadsheets, Google Workspace, and other local systems
- Institution-specific workflows such as sponsor handling, session intake, registration ops, and venue-selection operations

## What Must Stay In ICN Core

- Generic primitives such as `Entity`, `Structure`, `Activity`, `Program`, `Milestone`, `Meeting`, `ActionItem`, `RoleAssignment`, provenance/receipt objects, and generic scope/work views
- Kernel and app-layer mechanics for storage, transport, authorization, receipts, event emission, and generic CRUD/state transitions
- Generic APIs that a second institution could reuse unchanged

## Why Summit Logic Belongs Here

The summit is a NYCN activity and program domain, not a platform primitive. ICN core should know how to store and govern generic activities, programs, milestones, and meetings. It should not know NYCN's sponsor tiers, session catalog shape, registration workflow, venue rubric, branding, or summit-year milestone names. Those semantics belong in this package because another institution would rename or reshape them.

## Package Layout

- `charter/` — charter documents and institution-level policy material
- `config/` — package metadata and local configuration defaults
- `seed/` — bootstrap manifests for initial instantiation
- `definitions/` — institution-owned entity/structure/activity/program definitions
- `summit/` — summit templates, docs, and summit-only operational artifacts
- `views/` — dashboards and local app/view config
- `migrations/` — import placeholders and crosswalks
- `workflows/` — institution-specific workflow definitions
- `docs/` — package-local indexes and runbooks

## Conditions For A Later Split Into Its Own Repo

- Package artifacts stop depending on ad hoc relative paths back into this repo
- Bootstrap manifests can be consumed by a stable institution-instantiation flow
- Package-owned views/workflows can target stable ICN APIs without patching core
- Migration/import logic is isolated to package-owned scripts or manifests
- Canonical platform docs point to package boundaries instead of embedding NYCN semantics
- Ownership and release cadence for NYCN artifacts are separable from ICN core release cadence

## Current Status

- Canonical platform boundary remains [`docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](../../docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md)
- Conceptual ICN/NYCN package-boundary clarification is in [`docs/strategy/ICN_NYCN_INSTITUTION_PACKAGE_BOUNDARY.md`](../../docs/strategy/ICN_NYCN_INSTITUTION_PACKAGE_BOUNDARY.md)
- This package is the operational home for institution-owned artifacts going forward
