# Licensing

This project uses two distinct license postures by design. This document clarifies the structure so contributors and downstream consumers can answer "which terms apply to this part of the repo?" without guessing.

## Repository-level license: AGPL-3.0

The ICN project — the source distribution as a whole, the deliverable, the running daemon, and any artifact built from this repository where no narrower crate license applies — is licensed under the **GNU Affero General Public License, version 3** (AGPL-3.0).

The full license text lives at [`LICENSE`](LICENSE). The README license badge points to the same file.

This is the governing license for the project as a whole. Anything in this repository that is not covered by a more specific crate-level declaration falls under AGPL-3.0.

## Per-crate license metadata: MIT OR Apache-2.0 (where explicitly declared)

Selected Rust crates in `icn/crates/` and `icn/apps/` declare their own license in their `Cargo.toml`:

```toml
[package]
license = "MIT OR Apache-2.0"
```

This is intentional. These crates are reusable technical primitives — not deliverable application code — and the dual MIT/Apache-2.0 posture lets downstream Rust projects pick them up under permissive terms, in the same idiom Rust itself uses.

Where a crate's `Cargo.toml` explicitly declares a license, **that declaration governs that crate's published metadata and any third-party reuse of that crate as a library**. Where a crate inherits from `workspace.package.license` in `icn/Cargo.toml`, that workspace declaration governs.

## Which terms apply to what

| What you are looking at | Which license applies |
|---|---|
| The ICN project as a whole — source tree, deliverable, daemon binaries built from this repo | [`LICENSE`](LICENSE) (AGPL-3.0) |
| A Rust crate with `license = "..."` in its `Cargo.toml` | The crate's declared license |
| A Rust crate inheriting `license.workspace = true` | The workspace declaration in `icn/Cargo.toml` |
| A documentation file, deployment manifest, or other non-code artifact under no narrower declaration | [`LICENSE`](LICENSE) (AGPL-3.0) |

## For contributors

When you add or substantially rework a Rust crate, do not silently change its `license` field. License changes are a deliberate decision and should land in a dedicated PR clearly titled and reviewed for the licensing implication, not bundled with feature work.

When you add a non-code artifact (docs, scripts, examples, ops manifests) without a narrower declaration, it falls under the repository-level AGPL-3.0 by default — no per-file header is required for that to hold.

## For downstream consumers

If you are reusing one of this project's Rust crates as a library dependency, look at that crate's `Cargo.toml` `license` field. The license you see there governs your reuse of that crate.

If you are forking, redistributing, or running the ICN project as a whole — including the daemon, the gateway, the deployment, or any composed artifact built from this repository — AGPL-3.0 governs your obligations.

If you are uncertain whether your use case falls under a crate-level declaration or the repository-level license, the safe default is to assume AGPL-3.0 and ask before redistributing.

## What this document does not do

- It does not relicense any existing code.
- It does not modify `LICENSE`, `icn/Cargo.toml`, or any per-crate `Cargo.toml` license field.
- It does not add SPDX headers to source files (that is a separate decision tracked elsewhere).
- It does not authorize a future change to either license posture. Any such change must come from a dedicated, explicit decision and PR.
