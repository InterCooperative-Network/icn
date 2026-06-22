# truth-sync — reference

Detailed rubric and source responsibilities for the `truth-sync` skill. Load this when actually classifying claims.

## Classification rubric

| Level | Meaning | When to assign |
|-------|---------|----------------|
| **safe** | Claim is directly supported by a canonical truth source as written. | A governing source asserts the same thing at the same confidence. No liveness/scale inflation. |
| **partial** | Core of the claim is supported, but it over-reaches on scope, scale, tense, or certainty. | Source supports a narrower or more-hedged version. Provide the narrower phrasing. |
| **unsafe** | Claim contradicts canonical state, asserts unverifiable liveness/scale as fact, or uses prohibited framing. | No source supports it; or it says "production / live / running N months / N nodes" without cited ops evidence; or it uses blockchain/token/payment/currency/wallet framing. |
| **needs-local-verification** | Claim could be true but cannot be settled from committed docs alone. | Runtime/cluster liveness, perf numbers, deployment status, "the API returns X" — anything that needs a live check, a route-inventory regen, or current ops evidence. Name the exact check. |

Default to the **lower** level when uncertain. Never round up.

## What each truth source governs

- `docs/STATE.md` — the single declared project state. The first place to check any "where is the project" claim.
- `docs/PHASE_PROGRESS.md` — which phases are complete / in-progress / planned. Check phase and milestone claims here, not against marketing docs.
- `source-of-truth-map.md` — which file is authoritative for each topic. Use it to find the *right* governing source for a claim before judging it.
- `show-readiness-map.md` — what is safe to show in a demo / presentation vs. what is still aspirational. The governing source for "we can demo X" claims.
- `website-truth-map.md` — maps public website claims to their backing evidence. The governing source for anything destined for the website / landing page.
- `runtime-surface-map.md` — what runtime surface actually exists (endpoints, services). The governing source for "the system exposes / does X at runtime" claims.
- `generated/route-inventory.md` — mechanical inventory of declared gateway routes. Evidence-limited: a route appearing here means it is *declared*, not that it is wired, authorized, or live. Pair with the route-impact skill.

## Liveness / readiness — the recurring trap

Per the repository's own deployment doctrine, current live runtime status is an **ops claim, not source-verifiable**. `docs/status.toml` historically flags the K3s deployment as needing ops re-confirmation. Therefore:

- "Running in production" / "live federation" / "running for N months" → `unsafe` unless current ops evidence is explicitly cited in the claim's context.
- "Deployed to a cluster" → at most `needs-local-verification` (cite the check: `kubectl get pods -A`, recent CI, or an ops sign-off).
- "Pilot" language → check `docs/PHASE_PROGRESS.md` for the actual pilot posture (partner-bound vs. formally committed) and match its exact hedging.

## Vocabulary discipline (flag as unsafe)

ICN is a coordination substrate / digital public infrastructure / constraint engine / mutual credit coordination system. Flag and rephrase:

| Prohibited framing | Safe reframe |
|--------------------|--------------|
| blockchain | (do not use — wrong architecture) |
| token | credit / allocation |
| payment | settlement |
| currency | unit |
| wallet | member account / account |
| balance | position |

## Worked examples

- Claim: *"ICN has been running in production across four cooperatives for six months."*
  → **unsafe**. No source supports six-month production liveness; liveness is an ops claim. Reframe: *"ICN has K3s/devnet deployment manifests and a partner-bound pilot track (see docs/PHASE_PROGRESS.md); live status requires current ops confirmation."*

- Claim: *"Members vote on proposals and the result is enforced by governance."*
  → **partial / needs-local-verification** depending on `show-readiness-map.md`. If the readiness map marks the governance flow demo-ready, `partial` with the map's exact scope; otherwise `needs-local-verification` (cite the demo check).

- Claim: *"The gateway exposes a /governance/proposals endpoint."*
  → **needs-local-verification**. Check `generated/route-inventory.md` and `runtime-surface-map.md`; a declared route is evidence the handler exists, not that it is live or authorized.
