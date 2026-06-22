---
name: truth-sync
description: ICN public-claim truth sync. This skill should be used when the user explicitly invokes "/icn-agent-pack:truth-sync", or asks to "truth-sync", "check public claims against truth", "is this docs/website/API claim safe to publish", or "verify readiness claims". Compares public, docs, and API claims against the repository's canonical truth sources and classifies each claim safe / partial / unsafe / needs-local-verification.
disable-model-invocation: true
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
---

Compare a set of public / docs / API claims against ICN's canonical truth sources, then classify each claim. The goal is to catch claims that over-state readiness, liveness, or capability before they reach the public surface.

This skill is user-invoked only. It reads truth sources and reports; it does not edit docs or the website. Apply fixes only when the user asks.

## Inputs

The claims to check come from the user: a doc, a website section, a README, a marketing line, an OpenAPI description, or a pasted paragraph. If the user gives no explicit claims, scan the file(s) they name (or the changed docs/website files) for assertions about state, liveness, scale, security, or capability.

## Truth sources (read these first)

Read whichever exist; absence is itself a signal. See `reference.md` for what each one governs.

- `docs/STATE.md` — declared project state (canonical)
- `docs/PHASE_PROGRESS.md` — phase tracking (canonical)
- `docs/reference/project-index/source-of-truth-map.md`
- `docs/reference/project-index/show-readiness-map.md`
- `docs/reference/project-index/website-truth-map.md`
- `docs/reference/project-index/runtime-surface-map.md`
- `docs/reference/project-index/generated/route-inventory.md` — generated route inventory, if present

## Method

1. Read the truth sources above (use `Glob`/`Read`; they are markdown).
2. For each claim, find the governing truth source and compare.
3. Classify with the four-level rubric in `reference.md`: **safe / partial / unsafe / needs-local-verification**.
4. For anything below `safe`, give the exact corrected or hedged phrasing the canonical state supports.

## Hard constraints

- **Never upgrade a claim's confidence to clear it.** If canonical state does not support "production / live / running for N months / pilot in production," the claim is at best `partial` and usually `unsafe`. Runtime liveness is an ops claim, not source-verifiable — treat it as `needs-local-verification` unless current ops evidence is cited.
- Keep ICN vocabulary disciplined: coordination substrate / digital public infrastructure / constraint engine / mutual credit. Flag "blockchain", "token", "payment", "currency", "wallet" framing as `unsafe`.

## Output

A table: `claim | classification | governing source | corrected/hedged phrasing`. Then a one-paragraph summary of the riskiest claim and what evidence would clear it.

See `reference.md` for the full classification rubric, per-map responsibilities, and worked examples.
