---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Tool Commons Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), accepted RFCs, and [`source-of-truth-map.md`](source-of-truth-map.md). This map routes the Tool Commons design layer; it is not an implementation claim.

## Purpose

Tool Commons is the design direction for reusable, institution-controlled tools over institution-owned state.

This map prevents three mistakes:

1. treating Tool Commons as a live app store;
2. treating tools as the source of institutional truth;
3. treating package-local NYCN tool shapes as generic ICN runtime implementation.

## Core rule

```text
tools do not own institutional truth
the institution/domain/node owns institutional truth
```

A tool may render, edit, summarize, import, publish, search, or assist. It should not become the sovereign database of the institution.

## Source paths

| Area | Path |
|---|---|
| Tool Commons architecture | `docs/architecture/COOPERATIVE_TOOL_COMMONS.md` |
| Domain infrastructure dependency | `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` |
| Tool install RFC | `docs/rfcs/RFC-0017-tool-install-infrastructure.md` |
| Service hosting dependency | `docs/architecture/SERVICE_HOSTING_MODEL.md` |
| NYCN package-side examples | external `InterCooperative-Network/nycn`, in-repo `institutions/nycn/` |
| Runtime/API dependencies | `icn-gateway`, governance, identity, authz, receipt/provenance paths |

## Tool lifecycle concepts

| Concept | Meaning | Current posture |
|---|---|---|
| ToolManifest | Declares what a tool is and what it needs. | RFC/design-direction unless implemented in a current path. |
| ToolInstallRequest | Request to install/bind a tool for a domain. | RFC/design-direction. |
| ToolBinding | Specific tool version bound into a specific institutional domain. | RFC/design-direction. |
| CapabilityRegistryEntry | Records what scopes a tool actually holds. | RFC/design-direction. |
| ServiceIdentityAuditEntry | Audits service/tool actions through evidence. | RFC/design-direction. |

## Candidate tool families

| Tool | Role | Status band |
|---|---|---|
| `icn-domain-admin` | Manage institutional domain configuration and boundaries. | design-direction |
| `icn-member-directory` | Member/role/standing directory. | design-direction / overlaps with current standing surfaces |
| `icn-governance` | Proposals, votes, decisions, governance records. | partially grounded in current governance runtime |
| `icn-meetings` | Agendas, attendance, minutes, meeting receipts. | partially grounded in current meeting/attendance paths |
| `icn-action-cards` | Member action queue and completion paths. | partially grounded in current action-card runtime |
| `icn-drive` / artifact vault | Institution-owned artifact storage and imports. | package-local/design-direction |
| `icn-docs` | Collaborative docs over institution-owned state. | design-direction |
| `icn-tables` | Structured records and operating tables. | design-direction |
| `icn-forms` | Intake/review forms. | design-direction |
| `icn-calendar` | Events, meetings, deadlines. | design-direction |
| `icn-publish` | Public publishing from governed artifacts. | design-direction |
| `icn-search` | Permission-aware search. | design-direction |
| `icn-agreements` | Membership/service/data/compute/resource agreements. | design-direction |
| `icn-budget` | Resource/treasury/budget views using safe vocabulary. | design-direction / economics-dependent |
| `icn-signals` | Institutional feedback and escalation signals. | design-direction |
| `icn-compute-jobs` | Bounded compute workload submission and evidence. | compute-dependent/design-direction |
| `icn-directory` | Public/community/institution directory. | design-direction |

## Status rule

A tool family can be described as runtime-backed only if there is current evidence for:

- source paths;
- API or app surface;
- identity/capability boundary;
- data ownership boundary;
- receipt/provenance behavior;
- tests or runbook evidence;
- current state-doc support.

Otherwise, describe it as design-direction, package-local, or future work.

## Relationship to NYCN

NYCN may contain package-local tool shapes, fixtures, templates, or workflow artifacts. Those are useful proof-of-shape materials, but they do not automatically mean the generic ICN Tool Commons runtime exists.

Correct frame:

```text
NYCN package tools = first institution-package rehearsal shapes
Tool Commons = reusable generic tool layer still being designed/built
```

## Relationship to service hosting

Tools may depend on services. Services still need their own governance model:

- service identity;
- scoped authority;
- backups and restore;
- event streams;
- receipt-producing transitions;
- exit/export path.

A tool UI without those boundaries is not enough.

## Safe claims

Safe, current claims:

- ICN has a Tool Commons design direction.
- Some current runtime surfaces already support future tools: governance, meetings, action cards, standing, receipts.
- NYCN provides package-local examples and rehearsal shapes.
- The generic install/binding lifecycle is not something to claim as complete unless runtime evidence exists.

## Claims to avoid

Avoid claiming:

- ICN has a live app store;
- Tool Commons is implemented as a complete ecosystem;
- NYCN package-local tools are generic ICN runtime tools;
- tools own institutional truth;
- service hosting is automatically solved by Tool Commons;
- tools replace governance, facilitation, care, legal judgment, or institutional accountability.

## Follow-ups

- Add concrete rows when `ToolManifest`, `ToolBinding`, or tool install lifecycle code lands.
- Map each candidate tool to existing runtime primitives before implementation.
- Keep public website copy from implying a live tool ecosystem.
- Cross-link future NYCN package maps to this file without duplicating NYCN-specific details here.
