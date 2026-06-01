# Wallet DID Migration Boundary — `wallet_did` / `icn_wallet_did`

**Status**: Accepted (design / naming plan) — diagnosis and naming plan for two `wallet`-rooted DID surfaces. It implements no rename and changes no code, storage keys, or serialized fields.
**Priority**: Tier 1 — identity / custody naming boundary
**Companion**: [`./passport-keyring-position-receipt.md`](./passport-keyring-position-receipt.md), [`../architecture/CLIENT_MODEL.md`](../architecture/CLIENT_MODEL.md)
**Scope**: Diagnosis + naming/migration design only. **This document renames no code, no storage keys, and no serialized fields.**

---

## Why this exists

The parent doctrine names two deferred `wallet`-rooted DID surfaces — class **C** (`wallet_did` in
`icn-kernel-api`) and the class **A** key-custody internals (the React Native SDK) — but does not say
what each actually is, what its correct canonical name is, or whether a rename carries any
compatibility cost. This document answers all three. The two surfaces merely **share the word
"wallet"**; they are different boundaries with different blast radii, and they must be evaluated
**separately**.

Key rule, inherited from the doctrine: **a DID is not a wallet.** A Device Keyring may *store or
protect* the key for a DID; a Member Passport may *present* that DID as subject identity. The field /
key name must reflect the actual boundary — not the value the DID happens to carry.

## Deployment reality (installed-base assumption)

**There is no prior installed React Native client base whose persisted secure-storage keys must be
preserved.** `@icn/react-native` is pre-release (`0.1.0`) and there are no real-world installs holding
`icn_wallet_*` / `icn_hybrid_wallet_*` keys. Consequently:

- **Surface 1 (RN secure storage) does not need installed-client migration.** No lazy migration, no
  dual-read, and no cross-version downgrade promise are required to protect a nonexistent installed
  base. The key-string constants can simply be **renamed to canonical `icn_keyring_*` names before
  release.**
- This assumption is **specific to Surface 1.** It does **not** decide Surface 2: the Rust `wallet_did`
  field must still be judged on its own source / wire / test exposure (below).
- The earlier "preserve legacy storage keys for existing installs" framing is therefore **dropped** for
  Surface 1. Legacy storage-key support should be added **only** if some reason *other than* installed
  clients requires it — and then justified as development convenience, not user migration.

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
- **Classification**: **persisted secure-storage key** — local to the device. **Pre-release: no
  installed data depends on these key strings** (see Deployment reality), so the strings can be
  canonicalized directly.
- **Why it was not already renamed**: the names are merely legacy, and three tests currently *lock* the
  exact strings (`sdk/react-native/src/keyring-aliases.test.ts:68`, titled "persisted secure-storage
  keys are unchanged (no migration)", plus `wallet.test.ts` and `hybrid-wallet.test.ts`). PRs #1966 /
  #1967 preserved the strings while only *aliasing* the public class / factory names. The storage-key
  rename was deferred to its own slice — which, given no installed base, is now a **direct rename**, not
  a migration.

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
- **Classification**: **public Rust API field + serde-serializable field.** A unit test
  (`compute.rs:744`) locks the serialized `wallet_did` string. **Exposure check (this revision):** the
  only code that serializes `OperatorMode` is its own unit tests (`compute.rs:738`, `:753`); it is
  **not** in any OpenAPI document, HTTP handler, persisted store, gossip path, or the TypeScript SDK,
  and no external crate consumer was found (`icn-kernel-api` is `0.1.0`, used in-repo only). So the
  serde shape is a *potential* wire contract but has **no actual serialized consumer today**.
- **Why a rename still needs care**: a rename changes (a) the public Rust field name — source-breaking
  for any consumer of the struct (today only in-repo: `operator_id()`, `is_compatible_with()`,
  `services.rs`, and the lock test) — and (b) the serde field name in **any** field-named format (JSON,
  YAML, and so on). Whether (b) matters depends on whether ICN adopts a wire / persistence contract for
  `OperatorMode` *before* such a consumer exists.

### Side-by-side

| | Surface 1 — `icn_wallet_did` | Surface 2 — `wallet_did` |
|---|---|---|
| Location | `sdk/react-native` (`wallet.ts`, `hybrid-wallet.ts`) | `icn-kernel-api` (`compute.rs`) |
| Kind | Persisted secure-storage **key string** | Serialized public Rust **struct field** |
| Boundary | **Device Keyring** (local custody) | **Operator / subject identity** (passport-rooted) |
| Installed / external dependents | **None** — pre-release, no installs | **None found** — in-repo only; serialized only in tests |
| Canonical target | `icn_keyring_did` (`icn_keyring_*` family) | `operator_did` |
| Recommended mechanism | **Direct rename** of storage-key constants; tests assert canonical-only | **Direct rename** *or* serde alias — decide per API-stability policy |

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

## Implementation plans (for FUTURE, separately-reviewed PRs — not performed here)

### Surface 1 — direct canonicalization (no installed-base migration)

Because there is no installed client base (see Deployment reality), Surface 1 is a **direct rename**,
not a migration:

1. Rename the RN secure-storage key constants to canonical `icn_keyring_*` / `icn_hybrid_keyring_*`
   strings (`icn_keyring_did`, `icn_keyring_private_key`, `icn_keyring_public_key`,
   `icn_hybrid_keyring_keypair`, `icn_hybrid_keyring_public_key`, `icn_keyring_version`). No
   `icn_wallet_*` storage key remains in the Device Keyring layer.
2. **Fresh installs write only canonical keyring storage keys.** With no installed base, no dual-read
   and no lazy migrate-write are needed.
3. Update the locking tests (`keyring-aliases.test.ts`, `wallet.test.ts`, `hybrid-wallet.test.ts`) to
   assert the **canonical** key strings, and add an assertion that **no `wallet`-named storage key** is
   used by the RN key-custody layer.
4. `deleteKeyPair()` must purge the canonical keyring keys the implementation writes (after the rename
   there is a single canonical namespace to clear). Add a test that an explicit deletion leaves **no**
   key behind.
5. **No legacy fallback or downgrade promise is required** — there is no installed base to read from or
   to downgrade onto. If a legacy `icn_wallet_*` read path is nonetheless kept, justify it explicitly as
   a **development convenience** (not user migration), and then `deleteKeyPair()` must purge **both** the
   canonical and the legacy families so a "deleted" identity cannot resurface via the fallback reader.

### Surface 2 — evaluate direct rename vs. serde alias (separate decision)

This is an operator/subject-identity field in the kernel API, **not** a React Native storage migration.
Decide it on its own API-stability policy:

1. **Confirm exposure first.** Re-verify (as of this revision) that no OpenAPI / HTTP / TypeScript-SDK /
   persisted / gossiped consumer of `OperatorMode` exists and that `icn-kernel-api` is not relied on as
   a published wire contract. The only serializer today is the crate's own tests.
2. **If there is no external / wire contract (current finding):** a future PR may **directly rename** the
   Rust field `wallet_did` → `operator_did`, updating the in-repo consumers (`operator_id()`,
   `is_compatible_with()`, `services.rs`) and the lock test to assert `operator_did`. Treat it as a
   semver-relevant change to `icn-kernel-api` even though no external consumer exists today.
3. **If / when a wire or persistence contract is adopted** for `OperatorMode`: do **not** direct-rename
   the emitted field. Use a serde `rename` + `alias` and keep emitting the legacy `wallet_did` until an
   explicit payload / version transition, so old readers and writers interoperate during the window.
4. Either way, **do not treat this as a React Native storage migration.**

## Recommended slice ordering

A. **RN storage-key canonicalization** — rename `icn_wallet_*` / `icn_hybrid_wallet_*` →
   `icn_keyring_*` / `icn_hybrid_keyring_*`. **No installed-client migration required**; tests assert
   canonical-only key names and that no `wallet`-named storage key remains in the key-custody layer.
B. **Rust operator-DID field** — evaluate a **direct rename** `wallet_did` → `operator_did` (no external
   consumer found) versus a serde alias, per the crate's API-stability policy; separate code PR.
C. **Docs / examples** — refresh once the new names land.

## Compatibility requirements

- **Surface 1**: **none for installed clients — there are none.** The only requirement is internal
  consistency: fresh installs and the test suite use the canonical key strings, and no `wallet`-named
  storage key remains in the Device Keyring layer.
- **Surface 2**: **no compatibility obligation today** (no external / wire consumer). *If* a wire or
  persistence contract is later adopted, a rename must go behind a serde `rename` + `alias` with legacy
  output retained until an explicit version transition.
- No change to DID generation, signing semantics, or cryptography in any slice.

## Non-claims

This document is design / naming planning only. It does **not**:

- rename or remove any storage key, field, type, or export;
- perform any installed-client migration (there is no installed base to migrate);
- change any persisted secure-storage key string;
- change any serialized API / wire payload or serde shape;
- promise to preserve pre-release legacy storage-key names;
- change DID generation, signing, or cryptography;
- imply token custody, a wallet balance, or any banking / payment product;
- make any production, live-federation, or governance readiness claim;
- weaken any meaning / regulatory / firewall check.
