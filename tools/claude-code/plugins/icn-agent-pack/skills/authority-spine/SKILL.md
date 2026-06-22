---
name: authority-spine
description: ICN authority-spine work. This skill should be used when the user explicitly invokes "/icn-agent-pack:authority-spine", or asks to work on "entity-aware auth", "trusted token authority / trusted issuance", "coop_id to EntityId mapping", "CoopEntityMap", or "treasury observe/cutover readiness". Orients the agent on the entity-authority spine, requires a written plan before any edit, and surfaces files, risks, verification commands, and required truth/doc updates.
disable-model-invocation: true
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
---

Guide work on ICN's **entity-authority spine**: making authority flow from a typed entity model (individuals / cooperatives / federations) rather than from self-asserted identifiers. This spans entity-aware authorization, trusted token issuance, the canonical `coop_id` ↔ `EntityId` mapping, and treasury observe → cutover readiness.

This is sensitive territory: it touches token-minting and authorization paths. **Plan before editing. Do not edit source until the user approves the plan.**

## Doctrine (non-negotiable)

- A capability token is **not** a mandate. DID ownership alone must never authorize a cooperative. Self-asserted `coop_id` issuance is **fail-closed**.
- Any dev/self-serve auth bypass must require BOTH an explicit opt-in AND a loopback bind, and must not be settable from a config file. Never enable a dev posture on a routable (`0.0.0.0`) bind.
- Before any change that could expose token-minting, auth, or write paths, prove it cannot be reached by an untrusted caller — and say so in the plan.

## Step 1 — Discover current state (do not trust this file for status)

Issue/PR status changes constantly. Establish current truth live; do not assert merged/open status from memory or from this skill.

```bash
# Current declared state and the authority-spine doctrine
sed -n '1,80p' docs/STATE.md
grep -rl "RFC-0018" docs        # locate the RFC and its xrefs
find docs -iname '*adr*' -o -iname '*rfc*' | grep -i -E 'auth|entity|rfc-0018' | head
# Open work in this lane (read titles/bodies; do not assume)
gh issue list --repo InterCooperative-Network/icn --search "entity authority coop_id EntityId trusted issuance treasury" --state open --limit 30
```

Read the doctrine anchors:
- `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md` (object-context binding, abuse cases)
- the RFC located above (the entity-authority RFC, currently RFC-0018)

## Step 2 — Map the code surface

The mapping primitive already exists; read it before proposing changes:
- `icn/crates/icn-entity/src/coop_entity_map.rs` — `CoopEntityMap` (coop_id ↔ EntityId store)
- `icn/crates/icn-entity/src/coop_entity_surrogate.rs` — surrogate allocation
- `icn/crates/icn-entity/` — unified entity model
- auth / token issuance seam in the gateway/auth layer (`/auth/verify`, token claims, `issue_*_token`)
- treasury integration points (observe vs. cutover)

## Step 3 — Produce a plan (required before edits)

See `reference.md` for the full plan template and risk checklist. The plan must include, at minimum: scope (one PR), files likely to touch, the untrusted-caller reachability argument, verification commands, and the truth/doc updates the change requires.

## Output

A written plan with these sections: **Scope · Files likely to touch · Authority/abuse risks · Verification commands · Truth & doc updates required · Open questions**. Then stop and wait for approval before editing source.

See `reference.md` for the plan template, the risk checklist, and the standard verification command set.
