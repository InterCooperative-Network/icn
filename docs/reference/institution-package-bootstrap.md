# Institution Package Bootstrap Contract

Status: draft reference

This document defines the generic bootstrap contract for institution packages inside the ICN monorepo. It is intentionally generic: it describes how package-owned manifests are loaded and planned without introducing institution-specific semantics into ICN core.

## Entry Point

Each package exposes a top-level `bootstrap.yaml` manifest.

Required fields:

```yaml
manifest_version: v0
package_id: nycn
package_kind: institution
charter:
  path: charter/nycn-federation.charter.yaml
seeds:
  - order: 0
    kind: entity-seed
    path: seed/00-nycn-federation.seed.yaml
```

## Loading Semantics

1. Load `bootstrap.yaml`.
2. Resolve the package-owned charter path relative to the package root.
3. Parse and validate the charter as a CCL document.
4. Load seed manifests in strictly increasing `order`.
5. Verify each seed file's internal `kind` matches the `bootstrap.yaml` entry.
6. Build a generic bootstrap operation plan from the loaded seeds.

## Supported Seed Kinds

- `entity-seed`
- `governance-domain-seed`
- `structure-seed`
- `activity-program-seed`
- `milestone-template-seed`
- `role-assignment-seed`

## Ordering Contract

The current stable bootstrap order is:

1. federation/entity seed
2. organizer cooperative seed
3. governance-domain seed
4. structures/committees seed
5. activity/program seed
6. milestone seed
7. role-assignment seed

This order exists to preserve generic dependencies:
- structures need parent entities
- governance domains must be declared before program writes can target them
- activities/programs need parent entities and linked structures
- milestones need target programs
- role assignments need target structures and, for live application, real DIDs

## Current Executor Reality

Today the platform provides:
- generic manifest and seed types in `icn-governance::bootstrap`
- package loading, charter validation, ordering checks, and plan emission through `icnctl institution bootstrap validate|plan`
- a first generic live executor through `icnctl institution bootstrap apply`

## Live Apply Semantics

`icnctl institution bootstrap apply` is intentionally honest about platform gaps.

It currently does:
- validate the package-owned charter locally before any live writes
- create entities through the generic entity gateway API
- provision governance domains through the generic governance API when domain seeds provide usable member DIDs
- create structures through the generic governance gateway API when the seed does not require fields the current write API cannot persist
- create programs through the generic governance gateway API only when the target governance domain already exists and the authenticated caller is allowed to write there
- create activities through the generic governance gateway API including `linked_structures` when referenced structures are live-resolvable
- create milestones through the generic governance gateway API only when the target program already exists as a live program ID
- apply role assignments when a real `person_did` exists and the target structure exists as a live structure ID
- defer role assignments that still have no `person_did`

It does not yet do:
- register or activate package-owned CCL charters through a generic live charter sink
- persist structure `scope` fields through the current generic structure create API
- preserve package-local aliases as runtime IDs for structures, activities, programs, or milestones

`governance-domain-seed` records use the same generic shape as `POST /v1/gov/domains`:
- `id`
- `name`
- `profile`
- `quorum_percent`
- `approval_percent`
- `voting_period_days`
- `members`
- optional `decision_mode`
- optional `max_objections`

For bootstrap convenience, members may include the explicit placeholder
`$bootstrap_subject_did`, which resolves to the DID used for gateway bootstrap authentication at apply time.

The apply report therefore separates:
- `completed`: real writes that succeeded
- `deferred`: intentionally skipped writes such as role assignments without DIDs
- `unsupported`: plan steps whose current generic runtime surfaces cannot faithfully persist the package manifest shape
- `failed`: a concrete live write failed and apply stopped

## NYCN Reality

With the current NYCN package and current platform surfaces:
- entities can be created live
- governance domains can be provisioned live from package seeds
- committee structures can be created live
- the summit program is no longer blocked on out-of-band domain provisioning
- the summit activity is no longer blocked by `linked_structures` contract mismatch
- milestones can proceed when summit program and activity writes succeed
- charter registration/activation remains validation-only

That means NYCN is now **partially live-applicable**, but not yet **fully bootstrappable end-to-end**.
