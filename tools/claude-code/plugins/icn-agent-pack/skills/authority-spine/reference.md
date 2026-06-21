# authority-spine — reference

Plan template, risk checklist, and verification set for the `authority-spine` skill.

## Plan template

```
## Authority-spine plan: <short title>

### Scope
- One coherent change, one PR. State the single seam being moved.
- Out of scope: <list adjacent concerns noted but not acted on>

### Files likely to touch
- icn/crates/icn-entity/...        # mapping / surrogate / entity model
- icn/crates/<gateway|auth>/...    # issuance / verify seam (if in scope)
- docs/...                          # truth + doctrine updates

### Authority / abuse risks
- Can this change let an untrusted caller mint or escalate authority? Prove not.
- Does it widen a token's meaning (key control vs. institutional authority)?
- Does it add a write/mint path reachable before entity binding is resolved?
- Fail-closed preserved? Dev bypass still loopback-only + opt-in + non-config?

### Verification commands
- (see "Verification set" below)

### Truth & doc updates required
- docs/STATE.md / docs/PHASE_PROGRESS.md if phase/lane state changes
- the entity-authority RFC (RFC-0018) migration record, if behavior changes
- docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md if a new abuse case is closed
- ADR if a kernel trait or wire format changes (locate the ADR dir via discovery)

### Open questions
- <anything that needs the user's decision before editing>
```

## Risk checklist (authority spine)

- [ ] The change keeps self-asserted `coop_id` issuance **fail-closed**.
- [ ] No new code path reaches token minting or a cooperative write without a resolved, trusted entity binding.
- [ ] Any dev/bypass posture requires explicit opt-in AND a loopback bind, and is not settable from a config file (`#[serde(skip)]`, not `serde(default)`).
- [ ] Token claims keep the distinction between **DID-key control** (proves key ownership) and **institutional authority** (proves the holder may act for an entity).
- [ ] `coop_id` ↔ `EntityId` mapping changes go through `icn-entity` (`CoopEntityMap`), not ad-hoc maps scattered across crates.
- [ ] Non-mappable / default coop ids remain the common, safe path (binding is additive and non-authoritative until explicitly cut over).
- [ ] Treasury cutover stays gated: observe-only until the binding source is trusted and reconciled.

## Verification set

```bash
# Rust workspace lives in icn/ inside the monorepo root
cd "$(git rev-parse --show-toplevel)/icn"
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test -p icn-entity                 # mapping/surrogate unit + integration
cargo test --workspace                   # if the change crosses crates
```

If the change touches the gateway/auth route surface, also run the route-impact skill
(`/icn-agent-pack:route-impact`) — issuance/verify endpoints feed OpenAPI and the TS SDK.

## Why the spine matters (one paragraph)

Authority in ICN must derive from a typed entity model so that "who may act for this cooperative"
is answered by a trusted, reconciled binding — not by whoever holds a DID key or asserts a `coop_id`.
The mapping (`CoopEntityMap`) and surrogate allocation are the substrate; trusted issuance and the
treasury cutover are the consumers. Move one seam at a time, keep the default path non-mappable and
fail-closed, and never let a token's key-control meaning silently become institutional authority.
