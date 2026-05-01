# Licensing

This document summarizes current repository license metadata and open licensing questions. **It is a maintainer-facing project note, not legal advice.** It does not relicense any code, modify any license declaration, define legal obligations for downstream consumers, or settle a licensing architecture by itself.

If repository metadata appears inconsistent with this document, the metadata in [`LICENSE`](LICENSE) and per-crate `Cargo.toml` files governs over any prose here. When inconsistencies surface, resolve them in a dedicated licensing decision (PR or RFC), not by treating this document as doctrine.

## What the repository currently declares

The metadata signals below are **observations of the present state**, not claims about how the signals should ultimately interact. Their relationship is an [open question](#open-questions) for a dedicated licensing decision.

### Root license file

[`LICENSE`](LICENSE) at the repository root contains the GNU Affero General Public License, Version 3 (AGPL-3.0).

### Rust workspace license metadata

[`icn/Cargo.toml`](icn/Cargo.toml) declares a workspace-level package license:

```toml
[workspace.package]
license = "MIT OR Apache-2.0"
```

Most crates in the canonical Rust workspace declare this license explicitly or inherit it via `license.workspace = true`; a small number declare no `license` field at all. As of this writing, of the 49 canonical `Cargo.toml` files (excluding `target/` and `.claude/worktrees/`):

- 13 declare `license = "MIT OR Apache-2.0"` explicitly. These are: `icn/Cargo.toml` (workspace root), the four `apps/*` crates (`apps/governance`, `apps/trust`, `apps/ledger`, `apps/echo`), the four `icn/apps/*` crates (`membership`, `charter`, `governance`, `ledger`), and four `icn/crates/*` crates (`icn-kernel-api`, `icn-http-kit`, `icn-zkp`, `icn-crypto-pq`).
- 34 inherit via `license.workspace = true`. These are the rest of `icn/crates/*` and all three `icn/bins/*` binaries (`icnd`, `icnctl`, `icn-console`).
- 2 declare no `license` field at all and do not opt into the workspace inheritance: `examples/wasm-compute/Cargo.toml` (an example crate) and `icn/crates/icn-ccl/fuzz/Cargo.toml` (a fuzz test harness). What license applies to these two `Cargo.toml`s on their own is one of the [Open Questions](#open-questions) below.

**No `Cargo.toml` in the canonical set declares AGPL-3.0.** The AGPL-3.0 declaration lives only in the root `LICENSE` file.

### Cargo `license` field caveat

The Cargo `license` field is **intent metadata**. It is not a license document and does not by itself capture every notice, exception, or accompanying file that may apply to a crate. Downstream consumers should consult the `license` field together with any `LICENSE` or `NOTICE` file in the relevant directory before relying on it.

### Other directly relevant artifacts

- [`docs/internal/legal-considerations.md`](docs/internal/legal-considerations.md) — maintainer-facing notes on regulatory questions communities deploying ICN may face. Like this document, it disclaims being legal advice.
- `docs/api/`, `docs/reference/api/`, `web/api-docs/`, `deploy/README.md`, and per-SDK READMEs reference `MIT OR Apache-2.0` as the API/SDK metadata. These reflect Cargo workspace metadata, not a separate decision.

## Open questions

The following questions are **not resolved by this document**. They are recorded here so a future licensing decision (PR, RFC, or maintainer/legal review) can address them explicitly:

- **Relationship between root `LICENSE` and the Rust workspace license.** The root file declares AGPL-3.0; the workspace metadata declares `MIT OR Apache-2.0`. The intended relationship between these two signals — whether the AGPL-3.0 governs the source distribution or specific deliverables, whether the Rust workspace metadata reflects an intentional permissive posture for reusable libraries, or whether one of the two signals is stale — is not currently captured by a canonical maintainer decision.
- **Scope of AGPL-3.0 if it is intended to govern the project as a whole.** Which artifacts (source distribution, daemon binaries, deployment manifests, documentation, generated artifacts) the AGPL-3.0 obligations are intended to cover.
- **Per-component license posture.** Whether reusable Rust crates should remain `MIT OR Apache-2.0` (as the workspace currently declares) or move to a different license; whether runtime / tool layers should adopt a network copyleft license (AGPL, CAL, or another) deliberately rather than by default.
- **Network copyleft scope.** If AGPL-style obligations are intended for any component, which components and which deployment shapes trigger them.
- **Data and autonomy protections.** Whether any component requires data-rights or autonomy-rights protections beyond what `MIT OR Apache-2.0` or AGPL-3.0 provide on their own.
- **Trademark and certification policy.** Whether the project intends to maintain a separate trademark or certification policy distinct from its source license, and where such a policy would live in the repository.
- **SPDX header policy.** Whether per-file SPDX identifiers should be added across the source tree, and to what scope.
- **Crates with no `license` field.** What license applies on its own to `examples/wasm-compute/Cargo.toml` and `icn/crates/icn-ccl/fuzz/Cargo.toml`, both of which omit the `license` field entirely and do not opt into workspace inheritance. The repository-level `LICENSE` file applies to the source tree as a whole; whether each of these two crates should also declare `license.workspace = true` (or an explicit license) is a separate decision.

These questions are deliberately phrased as questions, not tentative answers. Answering any of them is a maintainer/legal decision and should land as an explicit, dedicated PR or RFC clearly titled and reviewed for the licensing implication.

## Hard rules for changes

- **License metadata changes must be explicit.** Do not silently change a crate's `license` field, the workspace `license` declaration, or the root `LICENSE` file as a side effect of feature work. License changes land in a dedicated PR clearly titled and reviewed for the licensing implication.
- **License text is authoritative over prose.** When a downstream question arises about a specific file or crate, the relevant `LICENSE`, `NOTICE`, and `Cargo.toml` `license` fields are the source of truth; this document and any other prose are summaries that may lag behind.
- **Not legal advice.** This document is a maintainer-facing inventory. It does not give legal advice, define obligations for downstream consumers, or authorize any change to the present licensing posture.

## What this document does not do

- It does not relicense any existing code.
- It does not modify [`LICENSE`](LICENSE), [`icn/Cargo.toml`](icn/Cargo.toml), or any per-crate `Cargo.toml` license field.
- It does not add SPDX headers to any source file.
- It does not authorize, justify, or settle a multi-license architecture. Any such decision must come from a dedicated maintainer/legal review and a separate PR or RFC.
- It does not assert that the present metadata signals are intentional, harmonized, or final. They are observations of the current state, recorded so a future licensing decision has a starting point.
