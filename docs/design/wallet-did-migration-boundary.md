# Wallet DID Migration Boundary — `wallet_did` / `icn_wallet_did`

**Status**: Accepted (design / deferred-migration plan) — diagnosis and plan for a deferred compatibility slice of [Passport / Keyring / Position / Receipt](./passport-keyring-position-receipt.md). It implements no migration and renames nothing.
**Priority**: Tier 1 — Compatibility-sensitive identity / custody surface
**Companion**: [`./passport-keyring-position-receipt.md`](./passport-keyring-position-receipt.md), [`../architecture/CLIENT_MODEL.md`](../architecture/CLIENT_MODEL.md)
**Scope**: Diagnosis + migration design only. **This document renames no code, no storage keys, and no serialized fields.**

---

## Why this exists

The parent doctrine names two deferred `wallet`-rooted DID surfaces — class **C** (`wallet_did` in
`icn-kernel-api`) and the class **A** key-custody internals (the React Native SDK) — but it does not
yet specify *how* either can be renamed without breaking existing installs or serialized peers. Both
are high-cost compatibility surfaces, and they are **different boundaries** that merely share the word
"wallet". This document catalogs both precisely, fixes the canonical target name per boundary, and
specifies the exact compatibility-preserving sequence a future migration PR must follow.

Key rule, inherited from the doctrine: **a DID is not a wallet.** A Device Keyring may *store or
protect* the key for a DID; a Member Passport may *present* that DID as subject identity. The field /
key name must reflect the actual boundary — not the value the DID happens to carry.

## The two surfaces

### Surface 1 — `icn_wallet_did`: persisted Device-Keyring storage key (React Native SDK)

- **Definition**: the `DID_KEY` constant set to `icn_wallet_did` in `sdk/react-native/src/wallet.ts:30`
  and `sdk/react-native/src/hybrid-wallet.ts:47`.
- **Family**: one member of the legacy persisted secure-storage key family. The key *strings*
  `icn_wallet_did`, `icn_wallet_private_key`, and `icn_wallet_public_key` are referenced by **both**
  modules (in `hybrid-wallet.ts` the classical private/public keys appear under `CLASSICAL_*` constant
  names); `icn_wallet_version` and the `icn_hybrid_wallet_*` keys exist **only** in `hybrid-wallet.ts`:

  | Constant | Legacy key string | Module(s) | Holds |
  |----------|-------------------|-----------|-------|
  | `PRIVATE_KEY_KEY` / `CLASSICAL_PRIVATE_KEY` | `icn_wallet_private_key` | both | Ed25519 private key (classical) |
  | `PUBLIC_KEY_KEY` / `CLASSICAL_PUBLIC_KEY` | `icn_wallet_public_key` | both | Ed25519 public key (classical) |
  | `DID_KEY` | `icn_wallet_did` | both | DID derived from the keyring keypair |
  | `HYBRID_KEYPAIR_KEY` | `icn_hybrid_wallet_keypair` | hybrid only | Hybrid (Ed25519 + ML-DSA-65) keypair |
  | `HYBRID_PUBLIC_KEY_KEY` | `icn_hybrid_wallet_public_key` | hybrid only | Hybrid public key |
  | `WALLET_VERSION_KEY` | `icn_wallet_version` | hybrid only | Keyring storage-format version (classical / hybrid) |

- **What it is**: a **secure-storage key string** under which the SDK persists the DID derived from
  the locally-held Device Keyring keypair. Written via `setItem` on generate / import / hybrid-upgrade,
  read via `getItem` on `getKeyPair()` / `getDid()`, removed on `deleteKeyPair()`.
- **Boundary**: **Device Keyring** (local key custody). The stored *value* is a DID; the *key* is
  keyring-scoped — it names where and why the value is held locally.
- **Classification**: **persisted secure-storage key** — local to the device, compatibility-sensitive,
  not serialized over any network / API.
- **Why it cannot be blindly renamed**: renaming the key string makes `getItem(newKey)` return null on
  every existing install, so the stored DID and keypair of the device appear absent — DID / identity
  continuity is lost and the keyring may regenerate a *different* DID. That is silent data loss for
  installed users.
- **Existing compatibility lock**: `sdk/react-native/src/keyring-aliases.test.ts:68` (titled
  "persisted secure-storage keys are unchanged (no migration)") plus `wallet.test.ts` and
  `hybrid-wallet.test.ts` assert the exact legacy key strings; PRs #1966 / #1967 deliberately preserved
  them.

### Surface 2 — `wallet_did`: serialized public Rust field (`icn-kernel-api`)

- **Definition**: `wallet_did: Did` in `OperatorMode::Individual { wallet_did, contributes_to_commons }`,
  `icn/crates/icn-kernel-api/src/compute.rs:78`.
- **Exposure**: `OperatorMode` derives `Serialize` / `Deserialize` with snake_case field renaming and
  is re-exported as crate public API (`icn/crates/icn-kernel-api/src/lib.rs:78`). Consumed by
  `operator_id()` (`compute.rs:123`), `is_compatible_with()` (`compute.rs:137`), and the cell-join
  checks in `services.rs` (`cell_operator_mode` / `can_join_cell`).
- **What it is**: the DID of the **individual who operates** a compute node — the operator identifier
  used for cell-join compatibility (operator boundary E5 / E6). No keys are stored in this field.
- **Boundary**: **operator / subject identity** (a Passport-rooted subject DID presented in an operator
  role). This is **not** device-keyring custody and **not** local storage.
- **Classification**: **public Rust API field + serialized (serde) field.** A wire-shape unit test
  (`compute.rs:744`) locks the serialized `wallet_did` string. It is **not** currently present in any
  OpenAPI document, HTTP handler, or the TypeScript SDK, but the serialized shape is part of the public
  contract of the crate — any persisted or gossiped `OperatorMode` depends on it.
- **Why it cannot be blindly renamed**: a rename changes (a) the public Rust field name —
  source-breaking for any consumer of the struct — and (b) the serde field name in **any** field-named
  format (JSON, YAML, and so on) — breaking any persisted or gossiped serialized `OperatorMode`, plus
  the lock test.

### Side-by-side

| | Surface 1 — `icn_wallet_did` | Surface 2 — `wallet_did` |
|---|---|---|
| Location | `sdk/react-native` (`wallet.ts`, `hybrid-wallet.ts`) | `icn-kernel-api` (`compute.rs`) |
| Kind | Persisted secure-storage **key string** | Serialized public Rust **struct field** |
| Boundary | **Device Keyring** (local custody) | **Operator / subject identity** (passport-rooted) |
| Breakage on naive rename | Existing installs lose stored keys (silent data loss) | Source + serde wire break; lock test fails |
| Canonical target | `icn_keyring_did` (keyring-rooted key family) | `operator_did` |
| Safe mechanism | Dual-read + lazy migrate-write + version marker, for **every** key | serde alias + dual output until a version transition, semver-coordinated |

## Canonical target naming (and why)

- **Surface 1 to `icn_keyring_did`** within an `icn_keyring_*` / `icn_hybrid_keyring_*` family. The
  persisted key denotes *where and why* the value is held — device-local key custody — which is the
  **Device Keyring**. It is deliberately **not** `passport_did`: storage here is custody, not identity
  presentation.
- **Surface 2 to `operator_did`** (with `subject_did` as an acceptable alternative). The field is the
  subject DID of the node operator; the enclosing type is `OperatorMode` and its accessor is
  `operator_id()`, so `operator_did` is the most boundary-accurate. It is **not** `keyring_did` (no key
  custody occurs here) and **not** a bare `passport_did` (it is specifically the *operator role* of a
  subject identity).

## Migration sequences (for a FUTURE, separately-reviewed PR — not performed here)

### Surface 1 — dual-read / lazy migrate-write (zero install breakage)

1. Introduce canonical constants (`icn_keyring_did`, `icn_keyring_private_key`, and so on) **alongside**
   the legacy ones; do not remove the legacy constants.
2. **Read order — for *every* key in the family, not only the DID**: try the canonical key first, then
   **fall back to the legacy** key (`icn_wallet_did`, `icn_wallet_private_key`, `icn_wallet_public_key`,
   the `icn_hybrid_wallet_*` keypair / public key, and the `icn_wallet_version` marker). Signing,
   presence checks, and hybrid detection read keys beyond the DID (`wallet.ts:135` public, `wallet.ts:173`
   private for signing, `wallet.ts:198` presence; the hybrid reads throughout `hybrid-wallet.ts`), so a
   DID-only fallback would leave an upgraded install unable to load or sign.
3. On a successful legacy-fallback read, **write the canonical key** for that same key (lazy migration).
   Keep the legacy key in place; any eventual removal is a later, separately-reviewed cleanup.
4. Record migration state explicitly via the existing `icn_wallet_version` marker (or a new
   keyring-version marker) so the format is self-describing.
5. **Never** delete a legacy key, and **never** rename a key in place, *as part of the lazy migration*.
   **Exception — explicit user deletion**: `deleteKeyPair()` must purge **both** the canonical and the
   legacy namespaces (today each delete path clears only one namespace — `wallet.ts:151` and
   `hybrid-wallet.ts:394`). Otherwise a "deleted" identity can reappear and still sign via the fallback
   reader. Add a test asserting that **no** key in either namespace survives an explicit deletion.
6. **Tests**: (a) a fresh install writes the canonical keys; (b) a legacy-only install still reads its
   DID **and signs** with its existing key, and hybrid detection still reports the correct version;
   (c) after one read, the canonical keys are present and every migrated value is unchanged; (d) keypair
   / DID continuity is preserved across the upgrade (the device keeps the same DID and can still sign);
   (e) an explicit `deleteKeyPair()` leaves no key in either namespace.
7. **Downgrade safety**: a downgraded SDK writes **only** the legacy namespace (`wallet.ts:88` / `:115`,
   `hybrid-wallet.ts:120` / `:151`), so a canonical-first reader could ignore a legacy keypair that an
   older binary wrote *after* migration and silently resurrect the pre-downgrade identity. The migration
   must therefore either (a) treat the two namespaces as versioned and, when both are present and differ,
   prefer the one the marker says is newer — reconciling, or failing closed on an unreconcilable conflict
   — or (b) explicitly declare post-migration downgrades unsupported. Until a reconciliation strategy is
   chosen, this plan declares post-migration downgrades **unsupported**, and adds a test for the
   both-namespaces-present conflict.

### Surface 2 — forward-compat alias, then rename with dual output (semver-coordinated)

1. **Forward-compat step (changes no emitted output)**: on **today's** `wallet_did` field, add a serde
   `alias` for `operator_did`, so the *new* name is already accepted as input, and add a test for that
   input. Output is still `wallet_did`, so no peer breaks.
2. **Rename step (separate, semver-coordinated PR)**: rename the Rust field to `operator_did`. The field
   rename is itself a **semver-breaking** change to a public crate type — coordinate it with a crate
   version bump and update all in-repo consumers.
3. **Preserve legacy output during the compatibility window**: emitting only `operator_did` would break
   an older reader that still requires `wallet_did` (and would block downgrade recovery). Because
   `OperatorMode` may be persisted or gossiped, keep emitting the legacy `wallet_did` — via a dual-field
   or custom-serde strategy — until an explicit payload / version transition lets all peers move
   together. A read-side `alias` covers reading old input but is **not** sufficient for old readers of
   new output.
4. Update the serde lock test to assert both names are accepted on input **and** that legacy `wallet_did`
   remains present in output during the window. Before dropping legacy output, re-confirm no
   OpenAPI / TypeScript-SDK / HTTP surface has begun consuming `OperatorMode`; if one has, treat the
   emitted name as a wire contract and version it.

## Compatibility requirements (both surfaces)

- Existing installs must keep reading **all** of their keys with **zero user action**.
- No legacy key or field is removed within a migration slice (except an explicit user deletion, which
  must clear both namespaces).
- No change to DID generation or signing semantics.
- No change to cryptography.
- Serialized output may change **only** behind an explicit payload / version transition: keep emitting
  the legacy field name (alongside the canonical one) until all peers can move together; a read-side
  `alias` alone does not protect old readers of new output.

## Recommended slice ordering

1. **(this document)** boundary design — diagnosis + plan only.
2. Surface 1 dual-read / migrate-write for every key + tests — contained, React-Native-only, no wire impact.
3. Surface 2 forward-compat alias + test — accept the new name as input now (no output change) — *before* any field rename.
4. Surface 2 field rename to `operator_did`, preserving legacy output until a version transition — semver-coordinated, separate PR.

## Non-claims

This document is design / vocabulary planning only. It does **not**:

- rename or remove any storage key, field, type, or export;
- migrate any persisted secure-storage data, or change any persisted key string;
- change DID generation, signing, or cryptography;
- change any serialized API / wire payload;
- imply token custody, a wallet balance, or any banking / payment product;
- make any production, live-federation, or governance readiness claim;
- weaken any meaning / regulatory / firewall check.
