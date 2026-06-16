# `icn-app-echo` — example app (NOT runtime-integrated)

**Classification: example/demo only.** This crate is the minimal reference
implementation of the ICN app pattern (a `Reducer` + a `Service`, no
`PolicyOracle`). It exists to prove the app runtime works and to show, by the
smallest possible example, how a real app is structured. See `src/lib.rs`:
*"A minimal test app that validates the app runtime works correctly … This app
exists to prove the runtime works before we extract real apps like trust and
governance."*

## Why it is at top-level `apps/` and not in the `icn/` workspace

- **It has no runtime consumers.** Nothing in the substrate depends on it — not
  `icn-core`, not `icnd`, not `icn-gateway`, no other crate, and no CI workflow.
  (The runtime-integrated app crates *did* have consumers: `icn-ledger-app` ←
  `icn-gateway`/`icnd`, `icn-governance-app` ← `icnd`, `icn-trust-app` ←
  `icn-core`/`icnd`.)
- **[#2064](https://github.com/InterCooperative-Network/icn/issues/2064)'s
  workspace migration applied to *runtime-integrated* top-level app crates** —
  those that influence daemon/core/gateway-visible behavior and therefore must be
  first-class in `cargo test --workspace` / `cargo clippy --workspace`. All of
  them have been migrated under `icn/apps/*`:
  - `icn-ledger-app` → `icn/apps/ledger-app` ([#2070](https://github.com/InterCooperative-Network/icn/pull/2070))
  - `icn-governance-app` → `icn/apps/governance-app` ([#2071](https://github.com/InterCooperative-Network/icn/pull/2071))
  - `icn-trust-app` → `icn/apps/trust-app` ([#2072](https://github.com/InterCooperative-Network/icn/pull/2072))
- **This crate is example-only, so it was deliberately *not* migrated** as a
  runtime crate. It is left at top-level `apps/` and classified here so future
  agents do not treat it as another #2064 coverage gap.

## Honest caveat (no readiness-laundering)

Because `icn-app-echo` is **not** a member of the `icn/` workspace, the standard
CI gates (`cargo test --workspace`, `cargo clippy --workspace`) **do not cover
it** — it compiles only when built directly. It currently builds clean against
the current `icn-core` app API (verified from the **repo root**: `cargo check
--manifest-path apps/echo/Cargo.toml` succeeds — note this resolves relative to
the repo root, not the `icn/` workspace dir), but as an uncovered crate it *can*
drift/bit-rot
if the app-runtime API changes and nobody rebuilds it. That is an accepted
tradeoff for a demo example, not a hidden gap.

If this crate ever gains a real runtime consumer, it stops being example-only and
should be migrated under `icn/apps/` (collision-safe name, e.g. `echo-app`) and
made a workspace member, exactly like the three crates above. See
[`AGENTS.md`](../../AGENTS.md) → "App topology rule".
