# Passport / Keyring / Position / Receipt — Identity & Custody Vocabulary Boundary

**Status**: Accepted (doctrine) — refines [Regulatory-Safe Verifiable State Architecture](./regulatory-safe-verifiable-state.md)
**Priority**: Tier 1 — Foundational positioning
**Companion**: [`../dev/language-guide.md`](../dev/language-guide.md), [`../architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md`](../architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md), [`./multi-device-identity-design.md`](./multi-device-identity-design.md), [`../architecture/AUTH_BRIDGE_AND_DID_LOGIN.md`](../architecture/AUTH_BRIDGE_AND_DID_LOGIN.md)

---

## Why this exists

ICN's [language guide](../dev/language-guide.md) already forbids fintech vocabulary on the value
axis: `payment` → `settle`, `balance` → `position`, `currency` → `unit`. The website and the
foundational manual already say, plainly, that ICN **is not a wallet** ("position over wallet,
obligation over debt, settlement over payment, unit over currency" — `ICN_VISUAL_EXPLAINER_BIBLE.md`).

But "wallet" is overloaded, and the *value* meaning is only one of three. The word also gets used for
the **identity app** a member opens ("Open your ICN Wallet app and scan this code") and for the
**key-custody library** in the SDK (`createWallet()`, `HybridWallet`). The existing guide resolves
only the value meaning (`wallet` → `account`). The identity and custody meanings have no canonical
target, so today a single word silently teaches the whole crypto/fintech model:

- assets live in the wallet,
- balances live in the wallet,
- payments move between wallets,
- identity *is* possession of a wallet,
- and therefore the product is crypto-adjacent.

ICN does not teach that model. This document defines the canonical split **before** any broad
migration, so that a future rename has an architectural target instead of becoming a blind
find-and-replace. It is doctrine, not a migration: **it does not rename any code.**

---

## The canonical model

| Concept | What it is | Existing architecture anchor |
|---------|-----------|------------------------------|
| **Member Passport** | The user-facing **identity / membership / role / credential / delegation / capability presentation** surface. What a person *shows* to participate. | `IDENTITY_MEMBERSHIP_ARCHITECTURE.md`, `AUTH_BRIDGE_AND_DID_LOGIN.md` (DID login) |
| **Device Keyring** | Local **cryptographic key custody and signing**. Holds private keys on a device; biometrics/PINs unlock *it*, not identity itself. | `multi-device-identity-design.md` (Keystore v3, per-device keys, rotation), `sdk/react-native` key store |
| **Position** | **Ledger / accounting state** — a derived net view computed from signed entries. Positions are *recorded by the ledger*, never *held inside a wallet*. | `regulatory-safe-verifiable-state.md` Invariant 4; `language-guide.md` |
| **Receipt** | Verifiable **proof that an action/event/transition occurred**. | `regulatory-safe-verifiable-state.md` (`ExecutionReceipt`, `GovernanceDecisionReceipt`); `glossary.md` |
| **Settlement** | The canonical **movement / resolution of obligations and positions**. | `language-guide.md` (`payment` → `settle`) |
| **Credential** | A signed claim a passport presents (membership, role, attestation). Presented, not "held as value". | `glossary.md` (Attestation), `IDENTITY_MEMBERSHIP_ARCHITECTURE.md` |
| **Capability** | An authority grant a passport may exercise / a keyring may sign for. Scoped, delegable, revocable. | `multi-device-identity-design.md` (capability hierarchy), `capability-based-features.md` |

### The architectural rule

> A **Member Passport** does not hold money, tokens, or balances.
> A **Member Passport** presents credentials and authorizes signing.
> A **Device Keyring** protects private keys and produces signatures.
> The **ledger** records **positions**.
> **Receipts** prove events.
> **Settlements** resolve obligations and positions.

Identity is *not* possession of a wallet. Identity is a DID; the member presents it through a
**passport** and proves control of it through a **keyring**. The two are deliberately separate: a
person has one passport (identity) backed by potentially many device keyrings (custody), and losing a
device loses a keyring, not the passport. This separation is already the design in
`multi-device-identity-design.md` — this doctrine only names it.

---

## Deprecated / avoid term: `wallet`

`wallet` is **not** ICN-native user-facing language. It is allowed **only** for:

- third-party ecosystem references (e.g. an external crypto wallet a user already owns),
- legacy compatibility / deprecation notes,
- historical text,
- the regulatory term of art "**unhosted wallet**", which describes ICN's non-custodial posture
  relative to a regulatory concept (see `language-guide.md` and `regulatory-safe-verifiable-state.md`),
- low-level existing code where the concept is *actually key custody* and migration has not happened
  yet (label it, schedule it — see Migration rules).

It must **not** be introduced as the name of a member-facing surface, an SDK type, an identity app,
or anything that holds value.

### Boundary examples

| ❌ Avoid | ✅ Use |
|---------|--------|
| "member wallet balance" | "the ledger records the member's **position**" |
| "send payment from wallet" | "**settlement** records the obligation transition" |
| "wallet holds tokens" | (no token custody exists — describe the actual obligation/position) |
| "connect wallet to vote" | "the member's **passport** presents the credential that authorizes the vote" |
| "on-chain wallet history" | "**receipts** prove each action occurred; the **position** is the derived view" |
| "Open your ICN Wallet app" (sign-in) | "approve sign-in with your **ICN Passport**" (identity/session) |
| "the wallet signs the action" | "the **device keyring** signs the action" (key custody) |
| "reset wallet / wallet not ready" (keys) | "rotate/recover the **device keyring**" |

Note the last three rows: a single "wallet app" interaction usually decomposes into **both** a
passport step (present identity / approve session) **and** a keyring step (sign with a local key).
"Unlock" is ambiguous on purpose — "**unlock passport**" if the action is user-facing identity/session
access; "**unlock keyring**" if the action is producing a cryptographic signature.

---

## Classification of existing `wallet` usage

A full repo sweep (≈1,288 occurrences across ≈226 files) sorts into these classes. The point of the
table is to make the **migration target** explicit per class — not to authorize editing anything now.

| Class | Meaning | Representative hits | Migration target |
|-------|---------|---------------------|------------------|
| **A — Key custody internals** | A real local key store + signer | `sdk/react-native` `createWallet()` / `HybridWallet` / `wallet.sign()`; `multi-device-identity-design.md` Keystore | **Device Keyring** (deferred — breaking SDK API) |
| **B — User-facing identity surface** | The app a member opens to present identity / approve login | "Open your ICN Wallet app and scan this code" (`web/pilot-ui`, `icn-gateway` static); `CoopWallet` login; "member wallet UI" | **Member Passport** (identity/session) + **Device Keyring** (the signing it triggers) |
| **C — Stale crypto metaphor for identity** | identity expressed *as* a wallet | `icn-kernel-api/src/compute.rs` `wallet_did`, "Wallet-rooted identity for all operations" | DID-/passport-rooted identity (deferred — public API field) |
| **D — Ledger state mislabeled** | accounting state called "wallet" | `demo/notes/flow-4-notes.md` "surplus to member wallets"; "member wallet balance" | **Position** |
| **E — Historical / deprecation / non-claim** | "ICN is *not* a wallet"; "unhosted wallet"; archived text | `THE_COMMONS.md` "It is not a wallet."; manual "No wallet balance."; `docs/archive/**`; regulatory "unhosted wallet" | Leave; label historical/deprecated where needed. These are *correct* as-is. |
| **F — Third-party / unavoidable** | external wallets / hardware wallets | `witness-signature-best-practices.md` "hardware wallets or TPM-backed keys"; references to users' own crypto wallets | Mark **external / third-party only** |
| **G — Generated / enforcement / do-not-edit** | generated output, or firewall code that *correctly forbids* the word | `docs/api/openapi.generated.yaml`; the `["payment","wallet","balance",...]` forbidden-term lists in `icn-governance` proofs and `apps/governance` tests; `STATE.md`/`PHASE_PROGRESS.md` log lines | Do not edit. Fix the source, not the generated artifact; never weaken the forbidden-term lists. |
| **H — Ambiguous / needs design** | bundles concepts this doctrine splits | `docs/plans/agent-teams-launch-prompt.md` "node modes: wallet (signing+affiliations)"; `THE_COMMONS.md` "Commons Shell ↔ pilot-ui / CoopWallet" convergence | Resolve using this doctrine: split "signing" → keyring, "affiliations" → passport |

---

## Migration rules

When a future slice does migrate a `wallet` hit, the target follows the *meaning*, not the word:

1. **Do not blindly rename `wallet` → `passport`.** The right target depends on what the hit means.
2. If it refers to **key custody / signing**, migrate toward **keyring** (class A).
3. If it refers to **identity / membership / credentials / login**, migrate toward **passport**
   (class B/C). If the same interaction also signs, name the keyring step too.
4. If it refers to **ledger / accounting state**, migrate toward **position** (class D).
5. If it refers to **proof / history**, migrate toward **receipt** (+ the derived **position** view) (class D/E).
6. If it refers to an **external crypto wallet**, mark it **external / third-party only** — do not
   adopt it as ICN-native language (class F).
7. If it is **historical or a non-claim**, leave it; label it historical/deprecated if it could be
   misread as current (class E).
8. If it is **generated or firewall-enforcement code**, do not edit it; fix the upstream source and
   never weaken a forbidden-term list to make examples pass (class G).

### Disambiguation: "passport" and "keyring" already appear

- **`passport`** today means two things, neither of which is the *Member Passport* product concept,
  so this doctrine does not overwrite an existing meaning — but writers must keep them distinct:
  (a) a *metaphor* for a DID ("a DID — like a passport they alone hold", summit workshop docs), which
  is aligned; and (b) the literal **government passport document** used during identity verification
  (`icn-identity/src/anchor.rs`, `web/pilot-ui/sdis-enrollment`). "Member Passport" is the ICN
  identity-presentation surface; "passport" lowercase in an enrollment/KYC context is a government ID
  document. Do not conflate them.
- **`keyring`** today means the **OS/system keyring** (Keychain, GNOME Keyring) used to store local
  key material. This is *aligned* with **Device Keyring**: the Device Keyring is the ICN concept of
  local key custody and may be *backed by* the OS keyring. No conflict.

### Future migration slices this doctrine should guide

These are **named, not scheduled here** — each is its own future PR with its own review:

1. **SDK key-custody rename** — `createWallet` / `HybridWallet` / `Wallet` (TypeScript + React Native)
   → keyring naming. Breaking public-API change; out of scope for this doctrine.
2. **Login-copy reframe** — "Open your ICN Wallet app", "Scan with Mobile Wallet" (`web/pilot-ui`,
   `icn-gateway` static, OpenAPI QR descriptions) → passport-presentation language.
3. **Example app** — `CoopWallet` example → passport+keyring framing.
4. **`wallet_did` field** (`icn-kernel-api`) → DID-/passport-rooted identity naming. High-cost public
   API rename; deferred.
5. **Demo speaker notes** — "surplus to member wallets" → "member positions".
6. **"wallet" node mode** terminology (plans) → adopt the passport/keyring split.

---

## Non-claims

This document is vocabulary doctrine only. It explicitly does **not** claim that ICN is, or that this
doctrine implements:

- a banking product;
- a crypto wallet product;
- token custody, or any custody of member assets;
- member balances held in a wallet (positions are derived ledger views, not stored value);
- production-ready identity, a live federation, or completed membership/standing governance;
- that passport/keyring/position/receipt migration is implemented anywhere (it is not — this is the
  target, not the state).

It does **not** rename code, change any public API, alter SDK behaviour, or weaken any
meaning/regulatory/firewall check.

---

## Relationship to the firewalls

ICN has two firewalls; this doctrine belongs to the **regulatory/vocabulary** one, not the
architectural one:

- **Regulatory / vocabulary firewall** — `language-guide.md` + `.github/scripts/compliance_linter.py`
  (gateway/SDK/UI surfaces) + `regulatory-safe-verifiable-state.md`. This doctrine refines the `wallet`
  entry of the language guide along the identity/custody axis.
- **Architectural firewall** — kernel/app semantic separation (`meaning-firewall-audit.md`,
  `icn-core/src/meaning_firewall.rs`, `firewall_denylist.py`). Unaffected by this doctrine.

The compliance linter scans only gateway/SDK/UI paths, not `docs/`. The bad-examples above are quoted
illustrations; per the language guide's exception policy, **do not weaken any linter** to accommodate
deprecated-term examples — keep them in `docs/` (out of the linter's scope) instead.

**See also**: [language-guide.md](../dev/language-guide.md) · [regulatory-safe-verifiable-state.md](./regulatory-safe-verifiable-state.md)
