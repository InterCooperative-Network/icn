# icn.zone

The technical and operational surface of the InterCooperative Network.

**Short for the software, long for the explanation.** If somebody needs to *type* a URL to get somewhere on ICN as a piece of working software — code, dashboards, ops, dev resources — it's `icn.zone`. If somebody is *learning about* ICN as an idea — what it is, why it exists, the political/economic/cooperative-organizing case for it — that's [`intercooperative.network`](https://intercooperative.network). The latter is `website/` in this repo.

icn.zone assumes you already know what ICN is, or that you can click through to learn. It does not re-explain. It is the working domain — shortlinks, dashboards, dev tools, contributor coordination.

## Three things `icn.zone` does

### 1. Short URLs (primary)

Most traffic to `icn.zone` is short paths that resolve to ICN resources. The whole point of the short domain.

## What `icn.zone` is, in order of how it's used

### 1. A shortlink hub (primary)

Most traffic to `icn.zone` is short paths that resolve to ICN resources. Examples:

| Short | Resolves to |
|---|---|
| `icn.zone/board` | GitHub Project #15 |
| `icn.zone/repo` | `github.com/InterCooperative-Network/icn` |
| `icn.zone/site` | `intercooperative.network` |
| `icn.zone/state` | `docs/STATE.md` (rendered) |
| `icn.zone/phase` | `docs/PHASE_PROGRESS.md` |
| `icn.zone/dash` | The public dashboard page |
| `icn.zone/pr/NNNN` | The matching PR on GitHub |
| `icn.zone/issue/NNNN` | The matching issue |
| `icn.zone/adr/NNNN` | ADR `NNNN` |
| `icn.zone/idea/NNNN` | idea-`NNNN` framing |
| `icn.zone/handoff/YYYY-MM-DD` | Matching handoff doc |
| `icn.zone/spec/NAME` | A spec doc by slug |

Shortlink table lives in [`src/data/shortlinks.ts`](src/data/shortlinks.ts) and is resolved first by middleware. Anything that doesn't match a shortlink falls through to the page router.

### 2. A technical landing page (with no path)

When somebody arrives at `icn.zone/` directly, they land on a substantive technical home base for interested-but-uninvested nerds, contributors, and evaluators:

| Page | Public, no login |
|---|---|
| `icn.zone/` | Dashboard front door — phase, workstream heat, repo status |
| `icn.zone/architecture` | Kernel/app boundary, meaning firewall, constraint engine model |
| `icn.zone/roadmap` | Public phase roadmap |
| `icn.zone/spec-ladder` | Navigable view of architecture-spec docs |
| `icn.zone/repos` | Cross-repo map |
| `icn.zone/feed` | Public activity stream (PRs, commits, discussions) |
| `icn.zone/wiki` | Concept explainers, design rationale, FAQ, ADR navigator |
| `icn.zone/contribute` | Public on-ramp |
| `icn.zone/glossary` | ICN vocabulary |
| `icn.zone/sign-in` | Sign-in |

### 3. The development kingdom (logged in)

Signing in unlocks the inside. The team's coordination surface — what's on this week, who's blocking on what, deploy ops, your audit trail. The inside also adds private inside-shortlinks (`icn.zone/me`, `icn.zone/standup`, `icn.zone/today`).

| Page | Scope |
|---|---|
| `icn.zone/inside` | `staff` — your team-specific dashboard |
| `icn.zone/inside/notices` | `staff` — pinned team context |
| `icn.zone/inside/standups` | `staff` — async written updates |
| `icn.zone/inside/feed` | `staff` — full feed including private repos |
| `icn.zone/inside/deploy` | `steward` — K3s status, image SHA, smoke history |
| `icn.zone/inside/audit-log` | `staff` (own data) — your access trail |
| `icn.zone/inside/contributors` | `staff` — team directory |
| `icn.zone/me` | shortcut → your inside dashboard |
| `icn.zone/standup` | shortcut → today's standup |

## Doctrine: how access works

> "Public by default" in ICN means **every action and decision leaves an auditable trace that affected parties can legitimately access through process they consent to**. It does not mean "everything is readable by anyone."

See [`content/scope-doctrine.md`](content/scope-doctrine.md) for the full statement. Scopes: `public` · `staff` · `contributor` · `cooperative-member` · `steward`. Most landing pages are `public` (no auth). Everything under `/inside/*` requires auth. Scope-gated pages emit audit receipts to a structured log (interim implementation; ADR-0026 receipts when the substrate is ready).

## Resolution order

Each incoming request runs through middleware in this order:

1. **Shortlink table**: does the path match a shortlink? If yes, 302 to the destination.
2. **Page router (public)**: does the path match a public page? If yes, render.
3. **Page router (`/inside/*`)**: does the path match an inside page? If yes, check session.
   - No session → redirect to `/sign-in` (with `?next=<path>`)
   - Session → check scope via policy oracle. Allow / Deny / Audit.
4. **404**.

## Stack

- **Astro 5** SSR mode (server endpoints, sessions, middleware). Same Astro as `website/`.
- **TypeScript** for `src/lib/`, `src/data/`, endpoints.
- **No SPA framework**. Astro server-renders. Islands of interactivity only on pages that need them.
- **Auth bootstrap**: GitHub OAuth, allowlisted to InterCooperative-Network org members. Session via signed cookie.
- **Eventual**: ICN identity (DIDs), ADR-0026 receipts for audit, governance-mediated access decisions.

## Add a shortlink

Edit [`src/data/shortlinks.ts`](src/data/shortlinks.ts):

```ts
export const shortlinks: ShortlinkTable = {
  "board": { kind: "external", to: "https://github.com/orgs/InterCooperative-Network/projects/15" },
  "pr/:n": { kind: "external", to: "https://github.com/InterCooperative-Network/icn/pull/{n}" },
  "me":    { kind: "internal", to: "/inside",    scope: "staff" },
  // ...
};
```

Internal shortlinks can be scope-gated; external shortlinks resolve regardless of scope.

## Add a page

```astro
---
import ScopedPage from "../layouts/ScopedPage.astro";
const scope = "public"; // public | staff | contributor | cooperative-member | steward
---
<ScopedPage title="My page" scope={scope}>
  ...content...
</ScopedPage>
```

Add a nav entry in `src/components/Nav.astro` if appropriate.

## Data sources

| What | Lives in | Refreshed |
|---|---|---|
| Phase status | `src/data/phases.ts` (mirrors `docs/PHASE_PROGRESS.md`) | Hand-edit when phases tick |
| Repo metadata | `src/data/repos.ts` | Hand-edit when a repo changes role |
| Workstream heat | `src/data/workstreams.ts` | Hand-edit when activity shifts |
| Shortlinks | `src/data/shortlinks.ts` | Hand-edit |
| Standing (who has what scope) | `src/data/standing.ts` (interim) | Hand-edit until ICN identity ready |
| Open PRs / CI / activity | server endpoint hitting GitHub API | Live on page load, cached briefly |
| Spec ladder | `docs/spec/` via build-time read | Rebuild when docs change |
| Dashboard data | `docs/status/icn-system-dashboard.md` | Hand-edit, rebuild |

## Develop

```bash
cd web/dev-portal
npm install
npm run dev      # localhost:4321
npm run build    # SSR build
```

Mock auth in dev: a handler at `src/pages/api/auth/mock` lets you pretend to be any GitHub login. Real OAuth is enabled when `GITHUB_OAUTH_CLIENT_ID` is set.

## Deploy

- **Target**: `icn.zone`. New domain, distinct from `intercooperative.network`.
- **First cut**: SSR Astro deployed to a small Node host. K3s ingress route alongside the daemon. One instance is enough at the team's current scale.
- **Auth**: GitHub OAuth app registered to `InterCooperative-Network` org. Allowlist enforced via org-membership check.
- **Sessions**: signed cookie, HTTPS-only, SameSite=Lax. Short-lived (a day or two) since this isn't a consumer product.

## Status

Scaffold. The shell, scope primitives, middleware, shortlink mechanism, and a few pages (landing, repos, sign-in) are real. Auth is mocked but the plug points are wired and documented. Add shortlinks and pages as the team's actual coordination needs surface.

## Related (different things)

- `website/` — public-facing marketing site at intercooperative.network. The outside.
- `web/dashboard/` — single-ICN-node admin dashboard. Different audience (node operators).
- `web/pilot-ui/` — pilot demo UI for organizer rehearsals.
- `web/api-docs/` — REST API reference.
- `docs/status/icn-system-dashboard.md` — Markdown source-of-truth for the Dashboard page.
