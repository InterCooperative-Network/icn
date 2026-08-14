---
Status: descriptive
Canonical: no
Last Reviewed: 2026-08-14
---

# The Principal Model

Who ICN can honestly say did something: which actors are independently
identifiable, what each may sign for, and why an identity must never silently
become another identity merely because it runs on, controls, hosts, or belongs
to it.

> **Truth status.** §2 (*What ICN implements today*) is verified against
> `74c832f1` with file:line citations and is descriptive. Everything from §4
> onward is **proposed architecture** — design, not implementation. No claim
> here asserts production operation, pilot readiness, or institutional
> adoption. Where a proposal contradicts shipping code, §11 names the conflict
> rather than papering over it.

**Related, deferred to, not restated:**
[Passport / Keyring / Position / Receipt](../design/passport-keyring-position-receipt.md)
(vocabulary doctrine) ·
[ADR-0014](../adr/ADR-0014-constitutional-object-model.md) (authority types) ·
[ADR-0019](../adr/ADR-0019-authority-grant-minting-and-mandate-persistence-seam.md)
(grant minting) ·
[ADR-0020](../adr/ADR-0020-institutional-bootstrap-activation-and-standing-read-model.md)
(institution bootstrap) ·
[ADR-0035](../adr/ADR-0035-entity-aware-request-authorization.md) (what a
principal proves today) ·
[institutional-structure-spec](../design/institutional-structure-spec.md)
(Layer A/B/C) · [MEMBER_STANDING](MEMBER_STANDING.md) (the read model) ·
[authenticated-governance-replication](../design/authenticated-governance-replication.md)
(#2469) · [KERNEL_APP_SEPARATION](KERNEL_APP_SEPARATION.md) (Meaning Firewall)

---

## 1. The invariant

    Person != Device != Node != Institution

Each is independently identifiable. Relationships between them are explicit,
scoped, and revocable — never implicit inheritance.

Five consequences, stated as tests any design must pass:

1. Replacing a phone must not create a new human.
2. Replacing a node must not create a new cooperative.
3. Hosting a cooperative must not make the host the cooperative.
4. Assigning a personal node to a cooperative must not rewrite the node's identity.
5. Running one institution's workloads on a node must not change the node's species.

This document exists because **ICN currently fails test 1 and test 2 in
shipping code**, and cannot express tests 3–5 at all. §2 shows where.

### 1.1 Principles

- **The DID is the anchor; institutions are changing contexts.** Cooperatives,
  communities and federations are co-equal — never a ladder. (Already binding
  doctrine: `.claude/rules/design.md`.)
- **Authority is proven, not asserted.** A bearer token proves key control at
  mint time, not institutional authority. *A token is not a mandate.*
- **Proof travels with the claim.** A receiver must be able to verify authorship
  from the bytes in front of it, without consulting distributed state whose own
  authenticity it cannot establish. (Derived from #2469 §14.1.)
- **Custody defines identity.** Whoever holds a key *is* that principal. Any
  design that hands principal A's key to principal B has merged them, whatever
  the documentation says.
- **The kernel holds keys, DIDs and opaque scopes. It never holds meanings.**
  `Person`, `member`, `cooperative`, `role` are app-layer concepts and must stay
  above the Meaning Firewall.

---

## 2. What ICN implements today

Verified at `74c832f1`. This section is descriptive.

### 2.1 One opaque, key-derived identifier for everything

`pub struct Did(String)` (`icn/crates/icn-identity/src/lib.rs:177`).
`Did::from_str` (`:210-235`) hard-requires `did:icn:` + multibase base58btc +
exactly 32 bytes + a valid Ed25519 key.

**A DID *is* an Ed25519 public key.** There is no method namespace, no role tag,
and structurally no room for one. The same `Did` names a node
(`icn-core/src/node.rs:339`), a member (`icn-coop/src/types.rs:354`), a recovery
trustee (`icn-identity/src/multi_device.rs:108`), and a ledger author
(`icn-ledger/src/entry.rs:62`).

Genesis is already self-sovereign and offline: `KeyPair::generate()` (`:341`) →
`Did::from_public_key` (`:191`). No registry, no network call. **This part of
the goal is already met.**

### 2.2 A device roster, not a device authority model

`icn/crates/icn-identity/src/multi_device.rs` (772 lines) implements
`DidDocument`(:19), `VerificationMethod`(:43), `Capability`(:78),
`RecoveryConfig`(:100), `RotationEvent`(:135), `RecoveryProof`(:237),
`can_sign`(:308), `add_device`(:320), `revoke_device`(:426), `rotate_key`(:450).

`Capability = { Sign, AddDevice, RevokeDevice, RotateKey, Recover, Encrypt }`.

Device add/revoke is signature-gated: `identity_mgr.rs:161` checks
`Capability::AddDevice` and `:188-197` verifies an Ed25519 signature from an
existing method before mutating the roster (`:295` for revoke; `icnctl` mirrors at
`main.rs:6254,6383`).

> **But the signature does not cover what is enrolled.**
> `build_add_device_message` (`identity_mgr.rs:488-490`) signs exactly
> `ICN_ADD_DEVICE:{did}:{device_id}:{label}`. The `public_key`,
> `encryption_public_key` and `capabilities` from the request are parsed and
> applied *after* verification (`:200-228`) **without being covered by that
> signature**. A captured or replayed authorization can therefore be altered to
> enrol a *different key* with *different capabilities* under the same device id.
> Revoke has the same shape (`:493`, covering only did + device id).
>
> Raised in review on PR #2586 and verified at `74c832f1`. **This is a live
> defect in shipping code, not merely a design gap**, and slice B must fix the
> signed message to cover the full enrolment payload before treating this
> machinery as sufficient for §5.2.

**But:**

- `DidDocument::can_sign` has **zero production callers.** All 18 references are
  tests.
- `Capability::Sign` is **checked nowhere in production.** The only production
  capability checks are `AddDevice`, `RevokeDevice` (4 sites) and
  `RotationEvent::verify` (`multi_device.rs:550`).
- A device is a **string id** inside one person's document
  (`multi_device.rs:259`), not an identity. It cannot be a token subject.

So devices can be enrolled and revoked, but **no device key can sign anything
the network verifies.** The capability model governs roster mutation only.

One thing already works in our favour: the `Did` is derived from the *original*
key and is **stable across rotation** (`multi_device.rs:21-22`). Identity
continuity is designed in.

### 2.3 Two contradictory definitions of "who may sign as DID X"

| Definition | Site | Rule | Status |
|---|---|---|---|
| Document-mediated, multi-key | `multi_device.rs:308` `can_sign` | any non-revoked method with `Capability::Sign` | **dead code** |
| Self-certifying, single-key | `replication.rs:373` `verify` → `author.to_verifying_key()` | only the key the DID derives from | **live** |

`SignedGovernanceOp::sign` (`replication.rs:300-315`) returns `AuthorKeyMismatch`
unless `Did::from_public_key(signing_key) == author`.

**Therefore a device key can never produce a valid `SignedGovernanceOp` for its
person.** This is the precise mechanism behind #2469 §7.0.2's custody boundary.

### 2.4 Authorship is assumed, not proven

- `CastVote { voter: Did, .. }` (`apps/governance/src/actor.rs:171-180`) carries
  a doc-comment reading "DID of the voter (authenticated caller)". The handler
  (`:2151-2168`) does `Vote::new(pid, voter, choice)` → `save_vote` → `publish`
  with **no check that the submitter controls `voter`** and no signature. `Vote`
  (`icn-governance/src/vote.rs:23-40`) has no signature field.
- Authorship is the token subject: `voter_did = parse_did(&claims.sub, ..)`
  (`apps/governance/src/http/handlers.rs:1822`); same for proposer (`:851`) and
  delegator (`:2530`); same on RPC (`icn-rpc/src/server.rs:654-660`).
- What binds a token to a DID is an **HMAC over the gateway's symmetric secret**
  (`icn-gateway/src/auth.rs:184`), not the DID's key. The intended trusted
  issuance gate `TokenAuthority` is `DenyUntilWired` with "no production caller"
  (`token_authority.rs:32,150-176`).
- Only the challenge/response path proves key custody (`auth.rs:279,315`).
  **Invite redemption does not**: `api/invites.rs:256-282` parses a
  client-supplied DID from the request body, validates only its *form*, and
  mints a bearer token on it. Granted scopes are `coop:read`, `coop:write`,
  `ledger:read`, `ledger:transact` — **not** governance scopes, so this is not a
  vote-forgery path, but it does permit acting on settlement authority under an
  arbitrary DID. Flagged, not addressed here.
- `GovernanceProof` binds *what was recorded*, not *who authorized it*;
  `icn-governance/src/verify.rs:22-27` states authorization is explicitly out of
  scope.

### 2.5 The node already stands in for the person — in production

`manager.rs:3480` accepts `proposer: Did`. In the actor-backed branch
(`:3486-3497`) it calls `create_proposal_with_actions(..)` and **never passes
it** — the argument is silently dropped. Downstream, `actor.rs:1925-1927` does
`Proposal::new(domain_id, self.did.clone(), ..)`, stamping **the node's DID as
the proposal author**. The non-actor fallback (`manager.rs:3512`) uses the human
caller.

**The same endpoint attributes authorship differently by deployment mode.** This
is invariant test 2 failing today, not hypothetically.

### 2.6 An institution is a slug, and never signs

`EntityId(String)` = `entity:icn:<type>:<slug>` (`icn-entity/src/entity.rs:43,61`).
`to_did()` returns `Some` **only for individuals** (`:159`). Organization slugs
are human-chosen and unbound to any key.

Institutional signing is modelled but unwired: `federation/types.rs:22`
(`public_did`, "signs on behalf of the coop") and `federation/gossip.rs:625`
exist, but `gossip.rs:548` records in-source that
`FederationGossip::set_keypair` **has no caller in the workspace**.
`coop/lifecycle.rs:22` `derive_treasury_did` mints
`did:icn:treasury:{bs58(hash(coop_id))}` — a string with **no keypair behind
it**, and one that would not satisfy `Did::from_str` (it is typed
`Option<String>`, which is why it compiles).

The only wired signer is the node keypair (`init_federation.rs:314`).

Institutional authority is instead **authorization-record shaped**:
`AuthorityGrant` (`icn-governance/src/authority.rs:269`) with
`GrantorEntityId(String)` (`:201`), `Mandate` (`mandate.rs:175`), enforced by
`DefaultMandateGate::require` (`apps/governance/src/mandate_gate.rs:441,612`).
`icn-authz::SubjectId` must start with `did:` (`model/ids.rs:19,24`), so **an
institution cannot currently be an authorization subject at all**; entities
appear only as resources (`ids.rs:110`).

Membership *is* already a first-class relationship with a bidirectional index:
`membership:{parent}:{member}` + `member_of:{member}:{parent}`
(`icn-entity/src/sled_registry.rs:9-12`). `RoleAssignment`
(`icn-governance/src/structure.rs:173`) states in-source that "Roles carry
delegated authority scopes but do NOT grant sovereignty."

There is **no verifiable-credential layer** anywhere in the entity or governance
crates.

### 2.7 Nodes are self-issued; no claiming exists

`icnd --init` generates a keypair locally and refuses only if a keystore already
exists (`icn/bins/icnd/src/main.rs:398-419`). **Nothing authorizes a node into
existence**; the network treats self-issued DIDs as cheap by design
(`icn-net/src/handlers/mod.rs:272-275`).

Transport identity is TOFU at the TLS layer (`icn-net/src/tls.rs:276-279`);
the DID becomes real only when the Hello handshake's three-fact cert binding
passes (`handlers/hello.rs:62-97`), and the result lives in
`ConnectionContext::authenticated_peer` — **per connection, not per peer**
(`handlers/mod.rs:76-78`). Configured bootstrap DIDs are *expectations, not
pins*: divergence warns and counts, keeps the connection, penalizes nobody
(`handlers/mod.rs:296-340`).

`operator_did` exists (`icn-core/src/node.rs:341`) but is wired to the node's own
DID: `create_node_profile(did.clone(), did.clone(), // For now, operator is same
as node DID` (`supervisor/lifecycle.rs:371-374`).

**Node claiming, pairing, and enrollment are ABSENT.** (SDIS `enrollment.rs` is
*person* enrollment.) An administrator today is a DID holding a JWT scope minted
by the node's own gateway (`middleware.rs:84-95`).

`SignedEnvelope` authenticates the **hop, not the author** — stated verbatim at
`icn-governance/src/replication.rs:16-18`.

### 2.8 Hosting: the data model allows it, the identity model forbids it

Node config carries exactly one `[federation].coop_id: String`
(`icn-core/src/config/federation.rs:18`); `init_federation.rs:71-85` builds one
`own_coop_info` bound to the node DID. But the entity store is multi-entity
(`entity_mgr.rs:65,196`), and `docs/spec/institutional-domain.md:39` asserts "a
single node may serve many domains."

**Runtime is singular; spec is plural. They conflict.**

Authorization is per-*token*, not per-node (`middleware.rs:150-158` compares
`claims.coop_id`); the `entity_id` claim is documented non-enforcing
(`auth.rs:120`). A hosting operator's gateway could therefore mint tokens for any
tenant today with no structural check.

Multi-tenancy is otherwise ABSENT — the only `tenant` token in the codebase is a
prohibition (`icn-governance/src/program.rs:97`). The doctrinal firewall is
already stated verbatim: *"Operator scope is not institutional authority. The
appliance image must never collapse them."*
(`deploy/appliance/appliance.manifest.example.yaml:161`;
`DEBIAN_APPLIANCE_MODEL.md:187`).

Institution genesis is an API call authorized by a **bearer token**, and nothing
signs the genesis content at all. `icnctl institution bootstrap apply` obtains a
JWT via challenge/response using whichever identity `--data-dir` selects, then
`POST /v1/entities` sends a plain JSON body (`identifier`, `name`, `description`,
`parent_id`) under that token
(`icn/bins/icnctl/src/institution_bootstrap.rs:965-972`); with `--local-mint` even
the challenge is skipped and the gateway secret mints the token directly.

*Correction (2026-08-14, raised in review on PR #2586): an earlier revision of this
section said the node's key signs institution genesis. It does not. The node key
may complete a challenge to obtain a token, but no key signs the genesis payload.*
**The consequence is stronger, not weaker: institutional genesis carries no
authorship binding whatsoever**, so the §7.3 remedy — genesis signed by the
founding Persons — is closing a hole that is currently fully open.

### 2.9 Clients

| Surface | Holds a private key? | CI | Status |
|---|---|---|---|
| `sdk/typescript` | No — JWT only; `SignatureProvider` interface whose only in-repo impl is a mock (`examples/basic-auth.ts:35`) | yes | shipping |
| `sdk/react-native` | **Yes** — Ed25519 (`src/wallet.ts:93`) + hybrid ML-DSA-65 (`hybrid-crypto.ts:145`), Keychain/Keystore custody | **none** | unshipped |
| `web/member-shell` | No — the member pastes a bearer credential (`shell.js:1566-1578`) | yes | rehearsal-grade |
| `web/pilot-ui` | Yes — WebCrypto Ed25519 (`crypto.js:118`) | yes | superseded |

`SignedGovernanceOp` appears in **zero** client files. Device registration exists
in both SDKs, but `sdk/typescript/src/index.ts:2059` requires the request be
signed by an existing device holding `AddDevice` — so **the first device is
unbootstrappable from client code**.

The RN surface already names itself correctly: "Device Keyring implementation
(legacy-named 'wallet')" (`wallet.ts:55`).

---

## 3. Historical design intent, classified

| Artifact | Claim | Classification |
|---|---|---|
| `design/passport-keyring-position-receipt.md` | Splits the client god-object into Member Passport / Device Keyring / Position / Receipt; "a DID is not a wallet" | **CURRENT** (Accepted doctrine; governs vocabulary here) |
| `design/wallet-did-migration-boundary.md` | Canonical names for two `wallet`-rooted DID surfaces | **CURRENT — executed** (RN → `icn_keyring_*` #1970; Rust `wallet_did` → `operator_did` 2026-06-02) |
| `design/multi-device-identity-design.md` | One DID, many devices, capability hierarchy, rotation, social recovery | **PARTIALLY CURRENT** — implemented in `multi_device.rs`; registry still `draft`; the *signing* half was never wired (§2.2) |
| `design/social-recovery-design.md` | M-of-N trustee recovery with delay and cancellation | **PARTIALLY CURRENT** — `icn-identity/src/recovery.rs` implements the exact 7-step flow |
| `architecture/CLIENT_MODEL.md` | Full / **Personal** / Light node types | **PARTIALLY CURRENT** (self-labelled target-state; best repo source for *personal node* intent) |
| `architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md` | Individual identity independent of infrastructure nodes | **PARTIALLY CURRENT** (registry `living`, 1265 lines, dated 2025-12-25) |
| `design/institutional-structure-spec.md` | Layer A sovereign entities / Layer B structures / Layer C activities | **PARTIALLY CURRENT** — Layer A matches `EntityType`, Layer B matches `structure.rs`; self-marked DRAFT |
| `mobile/icn-mobile-ux-spec-v1.md` | "organized around a persistent member, not an organization"; on-device Ed25519 | **PARTIALLY CURRENT / largely ASPIRATIONAL** — strongest statement of member-first multi-org identity |
| `ADR-0014 / 0019 / 0020 / 0035` | Authority types, grant minting, bootstrap, entity-aware authz | **PARTIALLY CURRENT** — all four self-report `implementation_status: partial` |
| `architecture/DEBIAN_APPLIANCE_MODEL.md` | Node lifecycle `unclaimed → claimed` | **ASPIRATIONAL** — states the dev image has *no claim flow*; the only substantive repo source on node claiming |
| `ADR-0083` | Institutional domain as runtime authority root | **ASPIRATIONAL** (`proposed` / `not-started`) |
| `design/MINIMAL-VIABLE-COOP.md` | 13-week co-op launch program | **SUPERSEDED** — timeline elapsed; `VISION.md` names the Rehearsal Node as the current wedge |

**Not recoverable from repo sources** — stated explicitly rather than invented:
personal/household cells; QR-based *device enrollment* (only QR proof
*presentation* exists); org-owned vs member-contributed infrastructure as a
designed distinction; offline signing (referenced as a property, no design doc).

---

## 4. The principal taxonomy (proposed)

Four principal classes. A fifth is deliberately **not** created.

| | Person | Device | Node | Institution |
|---|---|---|---|---|
| Has a `Did` | yes | yes | yes (today) | **no — `EntityId`** |
| Root authority | the human | the Person that authorized it | the Person(s) administering it | its charter + governance process |
| Holds a key | yes (root) | yes (device key) | yes (node key) | **never** |
| Survives hardware replacement | yes | no (a new device is a new principal) | identity is per-instance | yes |
| Authenticates by | root-key signature | device-key signature + carried authorization | DID-TLS binding at Hello | it does not authenticate; it is *invoked* via mandate |
| May delegate | to Devices; to Institutions via membership | nothing (leaf) | nothing (leaf) | scoped authority via `AuthorityGrant` |
| Must never impersonate | — | its Person | any Person; any Institution | any Person |

### 4.1 Why an Institution gets no DID and no key

This is the load-bearing asymmetry, and it is not aesthetic.

`Did` is structurally an Ed25519 public key (§2.1). Giving an institution a DID
therefore means giving it a **keypair**, which means someone holds that keypair.
Whoever holds it *is* the institution — which is precisely invariant test 3
("hosting a cooperative must not make the host the cooperative") failing at the
cryptographic level. A hosted cooperative whose key sits on the host's disk has
been merged with its host, whatever the org chart says.

An institution is not a thing that signs. It is a thing that **decides**. Its
acts are legitimate because a governed procedure produced them — recorded as a
`Mandate` carrying `AuthorityGrant`s, executed by a named Person or Node
principal — not because a key attested them. ICN already models this
(ADR-0014/0019, §2.6); the unwired `public_did` fields are the vestige of a
different, weaker idea.

**Consequence.** Cross-context proof of an institutional act is a *chain*, not a
signature: charter → governed decision → mandate → grant → executing principal's
signature. This is heavier than a single institutional signature, and honestly
so. §11.3 records what this costs.

What an institution *does* need is a **non-squattable, stable identifier**.
Today's human-chosen slug (§2.6) is neither. §7.1 proposes binding it to its
genesis decision.

### 4.2 Why Device gets a DID

A device already holds an Ed25519 keypair (§2.2, `wallet.ts:93`), and a
`did:icn:` *is* a base58 Ed25519 public key — so a device DID costs nothing new
cryptographically. It buys three things a string id cannot:

1. a nameable **subject** for a delegation proof (§6);
2. a nameable **target** for revocation independent of document rewriting;
3. a correlation boundary (§10) — a device identifier that is not the person's.

The device DID **never appears as a governance author.** It appears as the
*acting* principal alongside the authoring Person.

### 4.3 Why there is no separate Service/Agent principal

A background service, scheduler, or automation runs on a Node and acts under a
scoped `AuthorityGrant`. It needs no new identity class: the Node DID names *who
is running*, and the grant names *what it may do*. Creating a fifth class would
add a key custody problem without adding an authority distinction.

---

## 5. Lifecycles

### 5.1 Person

```
generate root keypair locally  ──►  Person DID exists
        (offline, no network)          (self-certifying)
                                            │
                                            ▼
                              authorize first Device (§5.2)
                                            │
                                            ▼
                          join Institutions as a relationship (§7.2)
```

**Genesis is offline and complete.** No network interaction is required for the
Person DID to exist — this already holds (§2.1) and must be preserved. The
network *recognizes* a person; it never manufactures one. There is deliberately
no identity registrar: a Person DID becomes resolvable to others only as a
side-effect of the person acting (membership, authorization proofs), never by
registration.

**Root key custody.** The root key is **recovery-oriented, not operational.**
It authorizes devices, rotates, and participates in recovery. It should not be
the key used to sign day-to-day operations, and on a phone-only setup it should
be backed up (recovery phrase) and otherwise kept out of the hot path. The
device key does the routine work.

*Open:* whether the root key may live on the first phone at all, or must be
generated to backup material from the outset. See §13.

### 5.2 Device

```
device generates its own keypair  ──►  Device DID
              │
              ▼
Person signs a DeviceAuthorization{person, device, scopes, not_before, not_after}
              │
              ▼
   device may act for Person, within scopes, until expiry or revocation
```

- The device **never receives the Person's root key.** This is the whole point.
- Authorization is scoped and time-bounded by default (least authority).
- Adding a second device: the existing roster machinery (§2.2) lets a device
  holding `AddDevice` mutate the **roster**. But issuing a `DeviceAuthorization`
  is a different act: §6.1 requires `sig_P` verifiable under
  `P.to_verifying_key()`, which an existing device cannot produce.
  **For v1, issuing a device authorization is root-only**; roster add/revoke
  stays device-capable. Allowing an existing device to authorize another would
  require a carried delegation chain, which is the same unresolved problem as
  O8 (§6.3). *(Contradiction raised in review on PR #2586: §5.2 previously said
  an existing device could sign the authorization, which §6.1 forbids.)*
- Revocation: roster revoke (`revoke_device`) **plus** authorization expiry.
  See §6.3 for the freshness problem this creates.
- Hardware backing (Secure Enclave / Android Keystore / passkey) is a property of
  the device key, not a new principal class. Absent in Rust today (§2.1).

### 5.3 Node

```
first boot ──► node generates its own keypair ──► Node DID (per instance)
                                                        │
                                                        ▼
                                        claim: Person P proves administration
                                                        │
                                                        ▼
                              P holds NodeAdmin grant; may delegate to others
```

Node DID genesis already works (§2.7). What is missing is **claiming**: the step
that binds a running node instance to an administering Person.

The claim must bind **the running instance**, not the image — an image is copied,
an instance is not. A claim ceremony should therefore be challenge-response over
the node's own key, not a bearer secret baked into a build.

A restored or migrated node is a **new instance**. Whether it keeps its Node DID
is a genuine open question (§13) with a real security cost either way: keeping it
makes backup-restore-twice indistinguishable from a clone; rotating it breaks
every peer expectation and hosting assignment.

### 5.4 Institution

```
people agree  ──►  genesis decision (signed by founding Persons)
                              │
                              ▼
              Institution EntityId bound to that decision (§7.1)
                              │
                              ▼
                    charter adopted; governance active
                              │
                              ▼
   founder authority EXPIRES into ordinary governed authority
```

The final arrow is the one that matters: no founder's device or node may remain
the institution's sovereign. Today, entity genesis requires only the JWT scope
`entity:write` and auto-installs the creator as `Founder`
(`icn-gateway/src/api/entity.rs:296,404-408`) — with no mandate, charter, or
governed decision. That is the gap.

---

## 6. Member-origin signing

This is the load-bearing mechanism, and it is what unblocks the honest version of
#2469 slice 7.

### 6.1 The shape

```
Person P                                (root key — offline / backup)
   │ signs once
   ▼
DeviceAuthorization{ person: P, device: D, scopes, not_before, not_after, sig_P }
   │ carried with every op
   ▼
Device D signs the governance operation with D's key
   │
   ▼
gateway / node RELAYS  ── holds no key of P, asserts no authorship ──►  peers
   │
   ▼
receiver verifies, using ONLY the bytes received:
   1. sig_P over the DeviceAuthorization, under P.to_verifying_key()
   2. op signature under D.to_verifying_key()
   3. now within [not_before, not_after]
   4. scope admits this operation class
```

**Author remains the Person DID.** The device is the *acting* principal, carried
alongside. A gateway can relay this without ever holding P's key — which is
exactly what §7.0.2 of #2469 says is impossible today.

### 6.2 Why carried, not resolved

The alternative — having the receiver resolve P's DID document and call
`can_sign` — recreates precisely the flaw #2469 §14.1 rejected when it declined to
add a `standing_hash`: the receiver would compare against distributed state whose
own authenticity it cannot establish, producing *"the appearance of a
deterministic check over non-deterministic data — worse than having no field,
because it would look like the hole was closed."*

A carried proof is self-certifying. Both signatures verify against keys recovered
from the DIDs in the envelope itself. No resolution, no external state, no
ingress-path dependency.

### 6.3 The rotation problem — a blocking flaw in this design

*Raised in review on PR #2586; it is real and it is load-bearing.*

Because a DID **is** its genesis public key (§2.1), `P.to_verifying_key()` always
recovers the **original** key, even after the Person rotates or recovers. A verifier
that checks the carried authorization against `P.to_verifying_key()` therefore:

- **rejects** authorizations signed by the *replacement* root, and
- **keeps accepting** authorizations signed by a *compromised original* root, indefinitely.

That inverts the recovery guarantee in §7: rotation would break the person's ability
to authorize devices while leaving the attacker's ability intact. A carried,
self-contained proof cannot resolve this on its own, because deciding *which* root is
current is exactly a question about state the receiver does not hold. Both obvious
repairs are unsatisfying:

- **A carried rotation chain** (each new root signed by its predecessor, walked from
  the DID-embedded genesis key) restores the new root — but an attacker holding the
  compromised original can mint a *competing* chain, and "longest chain wins" is not a
  security argument.
- **An authenticated current-root source** resolves it correctly but reintroduces the
  external-state dependency §6.2 exists to avoid.

**This is unresolved (O8, §13) and blocks slice A**, which must not freeze an
authorization format that cannot express rotation. Whatever this document says about
identity continuity in §5.1 and §7 holds only for the pre-rotation case until O8 is
answered.

### 6.4 Expiry is not order-independent — a second constraint on slice D

*Raised in review on PR #2586.*

`not_after` evaluated against each receiver's local clock is **not** a
convergent predicate: an operation delivered before the deadline at peer A and
after it at peer B is accepted by A and permanently rejected by B. That directly
contradicts #2469, which gives wall-clock no ordering authority and carries no
timestamp field precisely so that late delivery stays safe (§5.1 of that design).

So the expiry window in §5.2 is sound as a *custody* control on the signing
device, but it **must not be used as an ingress validity gate** in a replicated
operation without a deterministic rule. Candidate directions, none adopted here:
bind validity to a monotonic, replicated quantity (for example the domain's
membership epoch) rather than wall-clock; or evaluate expiry only where the
verdict is local and non-replicated. Recorded as **O9** and as a slice-D test
requirement (delayed delivery, clock skew).

### 6.5 What this also does not solve — revocation freshness

A carried authorization is self-contained, and that cuts both ways: **a revoked
device's previously-issued authorization still verifies.** This is inherent to
carried proofs, not a defect of this particular design.

Mitigations, in order of strength:
- short `not_after` with routine re-issue (bounded exposure window; the primary control);
- revocation propagation as gossiped state, consulted where freshness matters more than availability;
- high-consequence operation classes excluded from device scopes entirely, requiring root-key signature.

This residual risk must be stated in the envelope's own documentation rather than
implied away. It is the honest cost of not depending on receiver-local state.

### 6.6 Implications for `SignedGovernanceOp` — **not changed here**

Per the session constraint, nothing in `SignedGovernanceOp` is modified. Recording
implications only:

- The current field set cannot express this. `verify()` recovers exactly one key,
  from `author` (`replication.rs:373`), and `sign()` refuses any key that does not
  derive `author` (`:308`).
- #2469 §14 designates the extension point: unknowns "resolve into either a **new
  `ReplicationAuthority` variant** or a **version bump**." A carried device
  authorization needs the **version bump** (`GOV_OP_V2`), because the *signature
  verification branch* changes, not merely the authority description.
- `op_id = SHA-256(canonical_body())` must cover the authorization, or a device
  proof could be swapped between ops.
- The canonical encoding's length-prefixed discipline (§5.2) extends naturally;
  the authorization is one more length-prefixed field.
- **Migration:** v1 and v2 must coexist; v1 remains valid for node-authored ops
  (where the node genuinely is the principal). This is additive, not a rewrite.

### 6.7 Layer boundary

Device authorization is a **principal-authentication** primitive, not a governance
one. It belongs in `icn-identity` (§6.7) and must be usable by any subsystem that needs
"principal A authorized principal B to act in class X" — settlement, membership,
compute. #2469 consumes it; #2469 does not own it.

---

## 7. Institutions, membership, and hosting

### 7.1 Institution identity

An institution keeps `EntityId`, but the identifier must become **bound rather
than squattable**. Proposed: bind the `EntityId` to the hash of its genesis
decision, so that claiming a slug requires producing the decision that created
it. The human-readable slug remains a label; the binding is what makes it
non-forgeable.

*This is a proposal, and it is incomplete until O11 is answered:* binding an id
to a decision hash requires a **domain-separated canonical encoding** and explicit
normalization, or two nodes serializing the same decision with different map or
founder-signature ordering will derive different ids for the same institution.
The hashed field set must also be stated, so a decision cannot be rebound to a
different slug. The current `EntityId` is an unbound human-chosen string (§2.6),
and `derive_treasury_did` compounds this by minting DID-shaped strings with no key
and no `Did` validation.

### 7.2 Membership is a relationship, never inheritance

Person P being a member of Institution I grants P *standing within I's context*.
It does not merge identities, and it does not require a separate account.

ICN's existing membership record with its bidirectional index
(`sled_registry.rs:9-12`) is the right shape and needs no redesign. A person is
simultaneously a member of Coop A, a member of Coop B, a participant in Community
C, and a delegate from A to Federation F by holding **four relationship records
against one Person DID** — which the current schema already supports.

Cooperatives, communities and federations are **co-equal contexts**, not a
hierarchy. This is binding doctrine, not a proposal.

What is missing is **portability**: relationships live in a local sled registry
with no credential form, so standing cannot be proven to a party that does not
already hold the registry (§2.6). A membership credential — a signed, verifiable
statement of standing — is the gap. It is *not* required for slice-7 governance,
because membership there is evaluated against the receiver's own domain state.

### 7.3 Hosted and shared infrastructure

The symmetry that makes this tractable:

> Person identity survives device replacement.
> Institution identity survives node replacement.

Both hold for the same reason: **neither principal's identity is its key
custody.** A person is not their phone; an institution is not its server.

Proposed separation:

| Concern | Held by | Never held by |
|---|---|---|
| Institution identity + charter | the institution's governed record | the hosting operator |
| Node keys | the node instance | the institution |
| Hosting assignment | an explicit, revocable record naming node + institution + scope + term | implied by config |
| Institutional acts | mandate chain executed by authorized Persons | the host's node key |

A malicious host is removed by **revoking the hosting assignment and
re-materializing state elsewhere** — possible only if institutional state is
portable and institutional authority never depended on the host's key.

This requires changing one thing in particular: institution genesis is currently
authorized by a **bearer token and signed by nobody** (§2.8) — the request body is
plain JSON. Under this model, genesis must be signed by the founding **Persons**,
with the node merely relaying — the same relay/authorship split as §6. Note the
remedy is not about taking custody away from the node key, because the node key
never signed the genesis payload in the first place; it is about introducing an
authorship binding where there is none.

Five deployment shapes must all work without changing anyone's identity: phone
only; person + personal node; institution on a hosted node; institution on its own
node; federation across many nodes. Moving between them is a change of *hosting
assignment*, not of identity.

---

## 8. Mobile onboarding and the NYCN acceptance vertical

The reference flow, and what each step needs that does not exist yet:

| Step | Needs | Status |
|---|---|---|
| install app, "create my identity" | local keygen | **exists** (`wallet.ts:93`) — unshipped, no CI |
| phone becomes authorized device | first-device bootstrap | **missing** — `index.ts:2059` requires an existing `AddDevice` device |
| receive and accept an invitation | invite → membership without DID assertion | **exists but unsound** (§2.4) |
| see coop + federation context | `/me/standing` | **partial** (shipped subset, PR #1627) |
| create / vote on a proposal | member-origin signing | **missing** (§6) |
| device signs locally; node relays | v2 envelope | **missing** (§6.4) |
| network verifies; state converges | slice 7 + authority | **blocked on the above** |

NYCN is used here strictly as an **acceptance test for generic primitives** — a
federation with member cooperatives, organizers, stewards, delegates, committees,
and later-joining coops, some hosted and some self-hosted. Nothing about NYCN may
enter the kernel; per `.claude/rules/design.md`, NYCN vocabulary belongs only in
institution packages. If the generic primitives cannot express NYCN, the
primitives are wrong — not NYCN.

---

## 9. Threat model

| # | Threat | Required invariant | Mitigation | Residual |
|---|---|---|---|---|
| T1 | Stolen phone | device compromise ≠ person compromise | device key ≠ root key; revoke + short expiry | ops signed before revocation stand (§6.3) |
| T2 | Compromised device | blast radius bounded by scope | least-authority scopes; high-consequence classes excluded | in-scope ops until expiry |
| T3 | Malicious node operator | node key cannot author member acts | authorship = carried device proof; relay asserts nothing | operator can withhold/delay relay |
| T4 | Malicious hosting provider | host key ≠ institution authority | mandate chain; revocable hosting assignment | host can deny service; state portability required |
| T5 | Compromised institution admin | admin ≠ sovereign | roles carry scopes, not sovereignty (`structure.rs:167-170`) | scoped damage until governed revocation |
| T6 | Lost root key | recovery preserves Person P | social recovery (`recovery.rs`), delay + cancel | trustee collusion ≥ threshold |
| T7 | Revoked device operating offline | revocation must bound in time | `not_after`; revocation gossip | the §6.3 window — accepted, documented |
| T8 | Cloned node / restore-twice | one instance, one identity | unresolved — see §13 | **open** |
| T9 | Gateway impersonates a member | a relay cannot author | §6; and today, #2469 §7.0.2 fallback already prevents *signed* forgery | today the unsigned legacy path carries it |
| T10 | Federation overreach | member sovereignty | co-equal scopes, not a ladder | requires authority checks not yet enforced |
| T11 | Correlation / profiling | institutional life not trivially linkable | **unmitigated** — see §10 | **open** |
| T12 | Token minted for an arbitrary DID | authorship = key control | challenge/response path does this; invite path does not (§2.4) | live gap, flagged |

---

## 10. Privacy and correlation

**Honest statement of the tradeoff, as the brief requires.**

A single permanent Person DID used everywhere makes a person's entire
institutional life trivially linkable: the same 32-byte identifier appears in
every membership record, every vote key
(`gov:vote:{pid}:{voter}` — the one author-bound storage key, #2469 §1.4 fact 13),
and every gossiped operation.

`Did::from_str` (§2.1) permits exactly one identifier form, so **pairwise or
derived identifiers are not expressible today** without changing the `Did`
type itself — which every crate depends on.

Directions, none free:
- **Pairwise Person DIDs per institution**, linked by a private linkage secret.
  Strong unlinkability; breaks the "one anchor" model, complicates recovery
  (each pairwise identity needs recovery), and requires membership proofs that
  do not reveal the anchor.
- **Device DIDs as the public surface** (§4.2) — reduces root-key exposure and
  gives a rotation boundary, but does not unlink institutional memberships.
- **Selective disclosure over membership credentials** (§7.2) — proves standing
  without revealing the full relationship set; needs the credential layer that
  does not exist.

ICN has `icn-privacy` and `icn-zkp` crates and a `Vui` commitment primitive that
may be relevant. **This document does not resolve the privacy model.** It records
that the current architecture optimizes for verifiability over unlinkability, and
that this was not an explicit decision — it is a consequence of `Did` being a raw
public key.

---

## 11. Architectural conflicts discovered

1. **Two contradictory signing authorities** (§2.3). `can_sign` (multi-key,
   document-mediated) is dead; `to_verifying_key()` (single-key, self-certifying)
   is live. Resolve by making the carried authorization the one sanctioned
   multi-key path and either wiring or deleting `can_sign`.
2. **Two `DidDocument` types.** `icn-identity/src/multi_device.rs:19` (live) and
   `icn-kernel-api/src/identity.rs:33` (spec-only; `IdentityService` has **zero
   implementations**). One must be retired or explicitly scoped.
3. **Institutional signing is modelled but unwired** (§2.6). Under §4.1 it should
   be *removed*, not completed — but that is a decision, and `public_did` +
   `federation_accept` verification currently assume the opposite.
4. **Runtime is single-institution; spec is multi-institution** (§2.8).
5. **Authorship differs by deployment mode** (§2.5) — a correctness bug, not just
   an architectural gap.
6. **`derive_treasury_did` mints DID-shaped strings with no key** and would fail
   `Did::from_str`; it survives only because the field is `Option<String>`.
7. **Three entity models** (`icn-entity`, `icn-coop`, `icn-federation`) plus
   verbatim duplicates in `apps/membership/*_core/`, glued by ~3,900 lines of
   reconciliation code.
8. **`icn-authz::SubjectId` requires `did:`**, so institutions cannot be
   authorization subjects — consistent with §4.1, but currently by accident.

---

## 12. Explicit non-properties

This model does **not** provide, and must not be read as providing:

- anonymity or unlinkability (§10);
- institutional signing keys (§4.1 — deliberately);
- protection against a device compromised *before* authorization expiry (§6.3);
- consensus, finality, or double-vote prevention beyond #2469's own mechanisms;
- proof that a hosting operator will not deny service (only that it cannot
  *become* the institution);
- a solution to node cloning (§13);
- support for Person root-key rotation or recovery in the carried authorization
  format — see §6.3; until O8 is resolved, rotation breaks device authorization
  while leaving a compromised original root able to authorize (§6.3);
- any change to #2470 containment, which remains in force.

---

## 13. Open design decisions

| # | Question | Why it is open |
|---|---|---|
| O1 | May the Person root key live on the first phone, or must it be generated to backup material? | Usability vs. custody; determines whether phone-only onboarding is complete or degraded |
| O2 | Does a restored/migrated node keep its Node DID? | Keeping it makes restore-twice indistinguishable from a clone; rotating it breaks peer expectations and hosting assignments (T8) |
| O3 | Pairwise identifiers — adopt, and at what cost to recovery and the single-anchor model? (§10) | Requires changing `Did`, on which every crate depends |
| O4 | Is `EntityId`-bound-to-genesis-hash sufficient, or does an institution need a resolvable record? (§7.1) | Affects federation-scale name resolution |
| O5 | Remove or wire `public_did` institutional signing? (§11.3) | §4.1 implies remove; existing federation-accept verification implies wire |
| O6 | Does the membership credential layer belong in this arc or a later one? (§7.2) | Not required for slice 7; required for portable standing |
| O7 | Which operation classes may a device sign, and which require the root key? | Determines the default scope set in §5.2 |
| **O8** | **How does a verifier learn the *current* root key for a Person after rotation or recovery, given that the DID encodes only the genesis key?** (§6.3) | **BLOCKS slice A.** A carried chain is forgeable by a compromised original root; an authenticated current-root source breaks the no-external-state property of §6.2 |
| **O9** | **What deterministic rule bounds an authorization's validity, given that wall-clock expiry is not order-independent?** (§6.4) | **Constrains slice D.** Local-clock evaluation makes the same operation accepted by one peer and permanently rejected by another, contradicting #2469 |
| O10 | What is the canonical byte encoding of `DeviceAuthorization` — field order, version, domain separator? (§6.1) | Independent Rust and SDK implementations will otherwise produce incompatible proofs, and the signature has no cross-protocol separation boundary |
| O11 | What is the canonical encoding and normalization of a genesis decision before hashing to an `EntityId`? (§7.1) | Different map or founder-signature ordering yields different ids for the same institution, defeating the stable binding; the field set must also prevent rebinding a decision to another slug |

---

## 14. Implementation gap matrix

Legend: **I** implemented · **P** partial · **M** missing · **C** conflicting ·
**D** needs design decision

| Capability | Current repo | Intended | Gap | Sev | Owner |
|---|---|---|---|---|---|
| Person DID | **I** — self-sovereign, offline (`lib.rs:177,191,341`) | unchanged | none | — | icn-identity |
| Device keypair + roster | **I** — `multi_device.rs`, gateway + icnctl | unchanged | none | — | icn-identity |
| Device DID as principal | **M** — device is a string id (`:259`) | first-class DID | new type usage | high | icn-identity |
| Device authorization proof | **M** — `Capability::Sign` never checked; no nested signature type | carried, signed, scoped | new primitive | **critical** | icn-identity |
| Device enrolment integrity | **C** — add/revoke signature covers only did+device_id+label; key and capabilities are unsigned (`identity_mgr.rs:488-493`) | signature covers full payload | live defect | **critical** | icn-gateway |
| Member-origin signing | **M** — `SignedGovernanceOp` is root-key-only (`:308,373`) | device-signed, person-authored | v2 envelope | **critical** | icn-governance |
| Client-side governance signing | **M** — zero client references | device signs locally | SDK + CI | high | sdk/react-native |
| First-device bootstrap | **M** — `index.ts:2059` needs a prior device | self-bootstrapping | new flow | high | sdk + gateway |
| Mobile identity genesis | **P** — real, but zero CI (`wallet.ts:93`) | shipped + tested | CI + release | med | sdk/react-native |
| Multi-device | **P** — roster only (§2.2) | roster + authority | wire signing | high | icn-identity |
| Recovery | **P** — `recovery.rs` 7-step flow implemented | reconcile + surface | client + audit trail | med | icn-identity |
| Key rotation preserving DID | **I** — stable across rotation (`:21-22`) | unchanged | none | — | icn-identity |
| Node DID | **I** — first-boot generated | unchanged | none | — | icnd |
| Node claiming | **M** — ABSENT; `operator_did = node_did` (`lifecycle.rs:371-374`) | Person claims instance | ceremony + grant | high | icn-core |
| Node admin delegation | **M** — admin is a JWT scope | NodeAdmin grant | authority wiring | med | icn-gateway |
| Hosted nodes | **M** — only a *prohibition* on tenancy | explicit assignment | new record | high | icn-core |
| Institution identity | **C** — unbound slug; keyless `derive_treasury_did` | genesis-bound `EntityId` | binding | high | icn-entity |
| Institution signing | **C** — modelled, unwired, contradicts §4.1 | never signs | decision O5 | med | icn-federation |
| Institution genesis | **P** — needs only `entity:write`; body is unsigned JSON under a bearer token, no key signs it | governed, person-signed | authorship binding + relay split | high | icn-gateway |
| Institution migration | **M** — restoration is an ADR-0086 open blocker | state portable | portability | high | deploy |
| Membership relationship | **I** — bidirectional index | unchanged | none | — | icn-entity |
| Membership credential | **M** — no VC layer at all | portable standing | new layer (O6) | med | icn-entity |
| Coop / community / federation identity | **P** — three models + glue | one primitive | consolidation | med | icn-entity |
| Pairwise / privacy identifiers | **M** — `Did` permits one form | undecided | decision O3 | med | icn-identity |
| Offline operation | **P** — property claimed, no design | explicit model | design | med | — |
| Gateway relay semantics | **P** — §7.0.2 fallback prevents signed forgery; unsigned path unaffected | relay asserts nothing | v2 + slice 7 | high | icn-governance |
| Token → principal binding | **C** — HMAC-based; invite path unproven (§2.4) | proven key control | fix invite path | high | icn-gateway |
| Authorship by deployment mode | **C** — node DID overwrites proposer (§2.5) | caller is author | correctness fix | high | apps/governance |

---

## 15. Proposed implementation slices

Derived from source dependencies. **None are implemented in this session.**

Ordering rationale: A is the primitive everything else consumes; B and C are
independent of each other once A lands; D is the #2469 join point; E–G are the
node/institution arc, which does not block D.

| Slice | Invariant | Source seam | Tests | Non-goals |
|---|---|---|---|---|
| **A. Device principal + authorization** | A device key may act for a Person only within a signed, scoped authorization, over a **specified canonical encoding** | `icn-identity` — new type beside `multi_device.rs` | sign/verify round trip; tamper each field; scope mismatch; wrong signer; revoked device; **pre- and post-rotation authorization**; **encode/decode round trip and cross-implementation vectors** | no governance wiring; no envelope change. **Blocked on O8 (§6.3);** must also settle O10 (canonical bytes, version, domain separator) before the format is frozen. v1 issues authorizations **root-only** (§5.2) |
| **B. Device enrollment + revocation, end to end** | First device self-bootstraps; revocation is provable; **the enrolment signature covers the enrolled key and capabilities** | `icn-gateway/src/identity_mgr.rs` (incl. `build_add_device_message`), `api/devices.rs`, SDKs | first-device path; add second; revoke; revoked device rejected; **tampering with `public_key` or `capabilities` under a valid signature is rejected** | no QR (inherits #2569 rules — separate) |
| **C. Person genesis on mobile, shipped** | Genesis is local, offline, and CI-covered | `sdk/react-native` | keygen determinism; secure-storage custody; CI added | no new crypto — it exists |
| **D. Member-origin signing (`GOV_OP_V2`)** | Authorship is provable without the relay holding the author's key, **without a non-convergent validity predicate** | `icn-governance/src/replication.rs` — version bump per #2469 §14 | v1/v2 coexistence; device-signed op verifies; wrong device rejected; `op_id` covers the authorization; **delayed delivery and clock-skew tests showing two honest peers reach the same verdict** | **does not lift #2470 containment**; does not change v1. **Constrained by O9 (§6.4)** — wall-clock expiry must not become an ingress gate |
| **E. Node claim + admin grant** | A Person administers a node instance; node ≠ operator | `icn-core` claim ceremony; `operator_did` unwired from `node_did` | claim binds instance not image; re-claim rejected; admin delegation | no hosting semantics |
| **F. Institution genesis, governed** | Genesis is person-signed and governed, not `entity:write`, and the `EntityId` binds to a **canonically encoded** decision | `icn-gateway/src/api/entity.rs`; `institution_bootstrap.rs` | founder authority expires; genesis body carries an authorship binding; **same decision yields the same id under reordered maps and founder signatures** | no institutional signing key (§4.1). Requires O11 |
| **G. Hosting assignment** | Hosting is explicit, scoped, revocable; host ≠ institution | node config → assignment record | host cannot act as institution; revoke and re-host | no billing/accounting |
| **H. Mobile member vertical** | The §8 flow works end to end | SDK + member shell | full vertical | not a UI project |
| **I. NYCN acceptance** | Generic primitives express a real federation | institution package only | no NYCN semantics in kernel | — |

### Dependency on #2469

| #2469 slice | Relationship |
|---|---|
| **4** quarantine | **Independent.** Bounded store + steward release valve needs nothing here. Should proceed. |
| **5** `op_id` / order state | **Independent**, with one forward constraint: `op_id` must be derived over the full canonical body so that adding a v2 authorization field changes it. |
| **6** lifecycle guard | **Independent.** Unconditional monotonicity is orthogonal to authorship. |
| **7** verify + authority + apply | **Depends on slice A + D.** Slice 7 restores `VoteCast` application, but per §7.0.2 gateway deployments *emit nothing to converge on* — the node cannot sign a member's vote. Slice 7 without member-origin signing is honest only for the composition where each voter runs their own daemon. **To truthfully claim member-governance convergence, A and D must land first.** |

Slices 4–6 are explicitly **not blocked** by this work.

---

## 16. Migration and evolution

- **Additive, never a rewrite.** `GOV_OP_V1` stays valid for node-authored
  operations, where the node genuinely is the acting principal.
- **Containment holds.** Nothing here lifts #2470; slice D is a signing capability,
  not an application capability.
- **One primitive, many consumers.** Device authorization lands in `icn-identity`
  (§6.5) so settlement, membership and compute can consume it without depending
  on governance.
- **Retire dead paths deliberately.** `can_sign` and the unwired institutional
  signing surfaces should be either wired or deleted, with the decision recorded
  — not left as ambiguous half-truths.
- **Order:** A → (B ∥ C) → D → (E → F → G) → H → I.
