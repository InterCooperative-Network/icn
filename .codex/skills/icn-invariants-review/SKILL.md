---
name: icn-invariants-review
description: "Review ICN code changes for invariant regressions across trust boundaries, determinism, canonical encodings, panic safety, and kernel/app separation. Use when reviewing PRs, planning protocol changes, or validating risky edits in icn/crates, gateway, governance, ledger, gossip, and deploy paths."
---

# ICN Invariants Review

Review changes with a fail-closed mindset.

## Workflow

1. Identify change surface.
- Map touched files to subsystem labels: `rust-core`, `gateway-api`, `gossip-net`, `ledger-econ`, `governance-ccl`, `trust-identity`, `sdk-web`, `deploy-devnet`, `docs-spec`, `ci-tests`.
- Flag boundary crossings: network edge, persistence edge, auth edge, protocol encoding edge.

2. Evaluate non-negotiable invariants.
- Adversarial-by-default: reject implicit trust or auth bypass.
- Determinism: reject nondeterministic state/proof transitions.
- Canonical encoding: reject silent wire/proof format drift.
- No panics in protocol paths: reject `unwrap`/`expect` in runtime/network/deserialization paths.
- Kernel/app separation: reject domain semantics in kernel crates.

3. Check failure behavior.
- Verify fail-closed behavior on invalid input/proof/signature.
- Verify idempotency and replay safety where events can duplicate.
- Verify error paths preserve state integrity and auditability.

4. Verify with scoped commands from the touched area.
- Rust baseline:
```bash
cd icn
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib
```
- Gateway API touched:
```bash
cd icn
cargo test -p icn-gateway --features sled-storage
cargo build -p icnctl
./target/debug/icnctl api export-openapi -o ../docs/api/openapi.generated.yaml
cd ../sdk/typescript
npm ci && npm run generate-types && npm run check-types
```

## Fast Triage Greps

```bash
rg "fn main\(|clap::" -S
rg "router|route|axum|warp|actix" -S icn/crates/icn-gateway
rg "jwt|authorize|authn|authz|claims|scope" -S icn/crates
rg "gossip|broadcast|fanout|topic" -S icn/crates
rg "journal|entry|posting|balance|double[-_ ]entry" -S icn/crates
rg "proposal|vote|quorum|proof|receipt" -S icn/crates
```

## Output Contract

Produce findings first, ordered by severity.
- Each finding includes: severity, file path, violated invariant, concrete failure mode, minimal fix.
- State explicitly if no findings are discovered.
- List residual risk/test gaps separately.
