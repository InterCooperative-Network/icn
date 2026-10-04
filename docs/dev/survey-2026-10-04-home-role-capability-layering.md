---
Status: descriptive
Authority: session evidence
Canonical: no
Last verified: 2026-10-04
---

> **Provenance.** Read-only survey produced by a subagent of the 2026-10-03/04 Katie/Home
> identity-integration session, against this worktree at `b098396b5` (before N4-D landed). It
> modified nothing and ran no cargo. Every `file:line` is evidence to re-verify against the HEAD
> you are on; line numbers drift. Preserved verbatim here because its conclusions drive the N4-D,
> admission, fact-set, capability-layering, GEN-kind and N7 slices that follow it on this branch.
> Title of record: Survey: where a Home runtime's device roles live relative to N1 device capabilities (icn-authz, kernel Capability, AuthorityGrant/TypedScope, N4 payloads).

# Survey: a generic, extensible role model for a person's Home runtime

**Read-only survey.** Source: icn-dev VM, worktree
`/home/ubuntu/icn-dev/worktrees/icn/katie-home-identity-integration`, branch
`task/katie-home-identity-integration`, HEAD `b098396b5` ("test(identity): the two-device
acceptance invariant through every boundary"). Paths are relative to that repo root. No files
were modified; no cargo was run. `gh` is present on the VM; #2599 and #2603 were read directly.

Two provenance caveats that matter for the recommendation:

- `icn/crates/icn-identity/src/device_channel_binding.rs` and its test are **untracked**
  (`git status --short` prints `??` for both; no commit touches the file). It is cited below as a
  precedent-in-progress, not as a landed contract. `lib.rs:28` already declares
  `pub mod device_channel_binding;`.
- `icn/crates/icn-identity/src/authority_log/mod.rs:216` declares `pub(crate) mod encoding;` and
  `encoding.rs:103` / `:150` declare `pub(crate) struct Writer` / `pub(crate) struct Reader`. Only
  `CodecError` is re-exported (`mod.rs:232`). An app-layer crate outside `icn-identity` cannot reuse
  ICN's canonical length-prefix writer today.

---

## 0. The constraint, restated

N1's device capability set is `{Sign=0x01, Encrypt=0x02, Present=0x03, Recover=0x04}`
(`icn/crates/icn-identity/src/authority_log/body.rs:351-380`). It is a wire format: `tag()` is
documented "The wire tag. Never renumber these." (`body.rs:363`) and `from_tag` returns
`CodecError::UnknownCapability(other)` for anything else (`body.rs:379`), so a new tag makes every
existing decoder refuse the body. The enum is also documented as **app-layer labels the kernel
never interprets**, with the rule that **no capability value confers log-writing or establishment
authority** (`body.rs:342-349`); `Recover` is "An app-layer recovery role. **Inert** with respect to
establishment authority." (`body.rs:358`).

A grant is `AuthorizeBody { header, device: PrincipalKey, capabilities: CapabilitySet,
validity: Option<ValiditySpan> }` (`body.rs:527-536`); revocation is `RevokeBody { header, device }`
(`body.rs:540-545`). The derived view is `devices: BTreeMap<PrincipalKey, DeviceGrant>`
(`authority_log/derive.rs:60`), where `DeviceGrant { capabilities, validity, granted_at }` is
inserted by `Authorize` (`derive.rs:294-302`), removed by `Revoke` (`derive.rs:303-305`), and
cleared wholesale by the establishment-level `AuthorityBody::Recover` (`derive.rs:288-293`).
There is no caveat, resource, amount, or label field anywhere in the N1 body.

The question is therefore where "presentation endpoint / execution provider / storage provider /
administrative device / approval edge / recovery participant / context participant" can live
without touching those bytes.

---

## 1. `icn-authz` — the capability graph crate

**What it models.** `lib.rs:1-15`: "Canonical capability graph model for unifying ICN's
authorization systems"; "This is an **app-layer** crate ... The kernel sees only hashes produced by
this crate, never the capability model itself." The model is:

| Type | Definition | Cite |
|---|---|---|
| `CapabilitySubjectId(String)` | a validated `did:`-prefixed string; explicitly **not** `authority_log::SubjectId` | `model/ids.rs:14-25` |
| `Action(String)` | `domain:verb[:subverb...]`, ASCII alnum/colon/hyphen, lowercased, >=2 segments | `ids.rs:47-86` |
| `ResourceKind` | frozen `{Entity, Scope, Asset, Contract, System}`, tags 0x01-0x05 | `ids.rs:111-127`, `hash.rs:28-32` |
| `ResourceId { kind, id: String }` | | `ids.rs:133-138` |
| `Constraint` | frozen `{RateLimit, CreditMultiplier, MaxTopics, TimeLock, RequiresQuorum, Tag(String)}` | `ids.rs:154-173`, `hash.rs:38-43` |
| `EdgeSource` | frozen `{CclContract, TrustScore, GovernanceVote, Static}` | `ids.rs:179-193`, `hash.rs:49-52` |
| `CapabilityEdge { subject, action, resource, constraints, source, valid_at: Option<BlockHeight> }` | constraints sorted+deduped | `model/edge.rs:18-52` |
| `CapabilityGraph::query(subject, action, resource) -> Decision { allowed, matching_edges }` | **exact equality** on subject/action/resource; "B0 minimal" | `edge.rs:99-123` |
| frozen BLAKE3 type tags 0x10-0x16 | | `hash.rs:16-22` |

**Resource/action scoping.** Yes, as data: an edge carries an `Action` and a `ResourceId`.
**Caveats.** Only as `Constraint` values (including an opaque `Tag(String)`); the query at
`edge.rs:103-123` does not evaluate constraints at all, it returns the matching edges.
**Attenuation / delegation chains.** Not present in the model read in full (`ids.rs`, `edge.rs`,
`graph/builder.rs:1-77`): `EdgeSource` has no delegated variant and there is no narrowing or
chain-walk operation. The task premise ("attenuation, delegation chains") is not borne out by the
crate.

**Runtime callers.** None. Only `icn/Cargo.toml:38` (workspace member) and `icn/Cargo.toml:165`
(path dep declaration) mention it; no other `Cargo.toml` depends on it and `grep -rn icn_authz`
outside the crate returns nothing. `src/adapters/mod.rs:1` is the single line
"Domain-specific adapters (CCL, Trust, JWT) -- implemented in Phase B1", and `graph/builder.rs:4-5,
16-17` say the same. It is a B0 type model with mock-source tests
(`tests/capability_graph_integration.rs`).

**Could it carry Home roles as namespaced actions?** Syntactically yes (`home:present`,
`home:execute`, `home:store` all satisfy `Action::new`), and the `domain:verb` grammar is a good
naming convention to borrow. As a mechanism, no: its subject is a `did:` string rather than
`SubjectId`/`PrincipalKey`; `valid_at` is a `BlockHeight` (`ids.rs:199-203`), not an authority-log
position; it has no signer, no witness, no canonical bytes bound under a signature; and nothing
evaluates it. Using it would be inventing a second authority system beside N1/N4.

---

## 2. `icn-kernel-api` — `authz.rs` and `scope.rs`

**Bearer token.** `Capability { id, resource: String, action: String, constraints: Constraints,
holder: Option<Did>, issuer: Did, expiration: LogicalTimestamp, signature: Vec<u8> }`
(`src/authz.rs:800-818`); `Constraints { max_amount, max_uses, allowed_targets, custom }`
(`:819-830`). The header states non-goals: "Role hierarchies (use capabilities instead)" and
"Predefined capability taxonomies (apps define their own)" (`:32-37`).

**`ActionKind::Custom(String)` is an extension point by design** (`:459-480`, with
`ActionKind::custom()` at `:477-479`). It is used today for `"network_message"`
(`icn-core/src/supervisor/network_policy.rs:297`, `lifecycle.rs:1893`, `icn-net/src/rate_limit.rs:1162`),
`"network_access"` (`icn-gateway/src/rate_limit.rs:561`) and `"connect"`
(`icn-gateway/src/trust_mgr.rs:1632`). But `ActionKind` lives in `PolicyRequestCore { actor: Did,
action, domain }` (`:487-495`) and is routed to a `PolicyOracle` (`:645-723`) whose job is to turn
app semantics into a kernel-enforced `ConstraintSet` (`:212-235`). It is a per-request policy
question keyed by a `Did`, not a durable, signed, position-bound delegation. It also has no
position semantics and no witness.

**Who issues `Capability`?** Nobody in production. `CapabilityEngine` (`:867-904`) has **no
implementor** (`grep -rn "impl CapabilityEngine"` returns nothing). The only construction found is
`icn-core/src/apps/runtime.rs:730-740` `root_capability_set()`: `resource: "*", action: "*",
issuer: "did:icn:root", expiration: u64::MAX, signature: vec![]` — an unsigned wildcard root token.
HOME profile `§6.2` lists a bearer token among the forbidden substitutes for an `Authorize` fact
(`docs/architecture/HOME_RUNTIME_IDENTITY_PROFILE.md:196-198`).

**`ScopeLevel`** is `{Local=0, Cell=1, Org=2, Federation=3, Commons=4}` (`src/scope.rs:41-60`), a
network-reach ordering ("The kernel knows that `Federation` is 'wider' than `Org`, but it does NOT
know what an organization is", `scope.rs:8-10`), consumed by placement/durability code
(`state.rs:687-703`, `receipts.rs`, `icnd/src/compute_wiring.rs:178,291`). It is not an authority
scope and has no personal/household level.

---

## 3. `icn-governance/src/authority.rs` and ADR-0014 / ADR-0019

**Model.** `AuthorityClass` is a **closed** enum `{Representation, Execution, Attestation}`
(`authority.rs:52-73`). `AuthorityGrant { id, class, grantor_entity: GrantorEntityId, grantee:
Grantee, scope: TypedScope, granted_by: Option<DecisionProvenance>, valid_from, valid_until,
revoked_at }` (`:257-297`), all **wall-clock** `Timestamp`s with `is_active_at(now)` (`:300-325`).
`Grantee` is `{Did, Entity}` (`:221-227`).

**The grantor rule is the disqualifier.** "A valid grantor is a **cooperative, community, or
federation** acting under its own charter's process. The ICN runtime, daemon, gateway, or any
non-entity platform component is **never** a valid grantor." (`:190-201`; ADR-0014 `:190-197` and
non-goal 11 at `docs/adr/ADR-0014-constitutional-object-model.md:588-593`). A person is not a
grantor entity; a household "mints no `AuthorityGrant`; its members delegate to their own devices in
their own logs" (`HOME_RUNTIME_IDENTITY_PROFILE.md:165`; also `:141-146` "never an `AuthorityGrant`,
never a steward").

**`TypedScope`** is `{ domain: Option<GovernanceDomainId>, proposal_class: Vec<String>, action_kind:
Vec<String>, amount_ceiling: Option<AmountCeiling>, time_window: Option<TimeWindow> }`
(`authority.rs:135-158`). "The semantic categories are frozen by ADR-0014; exact encoding may
refine" (`:131-134`; ADR-0014 `:224-256`). It is a conjunction of governance-flavoured axes and
`is_empty()` is malformed (`:160-172`). The string bridge `parse_authority_scope_strings` accepts
only `domain:`, `proposal_class:`, `action_kind:`, `amount_ceiling:` (`:336-407`). So: extensible
only by ADR, and along institutional axes. ADR-0014 also says what `AuthorityClass` "explicitly
does **not** cover: general-purpose ACL/RBAC roles, kernel-level capability bearer semantics ...
or ordinary membership status" (`ADR-0014:178-182`).

**Runtime status.** Grants are minted only for steward appointment/reconfirmation (Attestation
class) at the accepted-decision seam (`docs/adr/ADR-0019-...md:39-47, 56-68`), persisted in
`icn-gateway/src/receipt_store.rs:1015-1726`; "Kernel dispatch gated by mandates: NOT
IMPLEMENTED"; "Kernel capability minting from `AuthorityGrant`: NOT IMPLEMENTED" (`ADR-0019:60-68`).
`icn-gateway/src/token_authority.rs:91` lists `AuthorityGrant` as a *future* issuance basis
(`:80-82`: production source "allows nothing"). The types "must never be imported by kernel crates"
(`authority.rs:23-27`; ADR-0014 `:488-497`).

---

## 4. IDENTITY_SEMANTICS §2.7 and N4 (#2599)

`docs/architecture/IDENTITY_SEMANTICS.md:233-247` (Device Principal): "Not a subject, and not a
Principal class of its own"; "**Device-ness is not in the identifier — it is the grant.**";
"Authority — an `authorize` event in the subject's log naming this Principal, with capabilities
and a validity span. **Attenuation is mandatory**: issued ⊆ issuer ∩ flow ∩ requested."; "A device
signature alone never establishes subject authorship." Invariant **I5** (`:638`): "Device
authorization requires explicit subject delegation evidence — never inferred from a valid
signature | Verification returns a **grant**, not a bool." Routing table (`:691`): #2599 owns
"Device authorization/rotation/revocation mechanics".

**#2599 (read via `gh issue view 2599`).** Core model requires N4 to define "capability
attenuation; scope/caveat representation; authorization validity relative to subject
authority-log position". Load-bearing invariants: 2 (attenuation), 3 ("Every security-relevant
authorization field is cryptographically bound"), 6 (evaluation position not signer-chosen),
**7 ("Recovery capability is not establishment authority. A device capability named `Recover` must
not become a branch-selection or log-writing privilege")**, 9 (authorship vs authorization
distinct). Required outputs include "capability/scope model and attenuation rule". Non-goals:
"N7 guardian threshold recovery itself".

**Is adding capability tags a contract change?** Yes, by three independent statements:
N4-A `§7.3` "N1 records a capability set and an optional position span; it has no caveat, amount or
resource field. N4-A checks exactly what N1 recorded. **Richer scope is a contract change for
#2599, not something to improvise here.**" (`docs/architecture/N4A_DEVICE_AUTHORITY_EVALUATION.md:243-247`);
N4-C `§7` scope boundary "any new capability beyond N1's four" (`N4C_DEVICE_ENROLLMENT_REQUEST.md:140-143`;
`device_enrollment.rs:39`); HOME profile row (c) "No caveat/resource field: richer scope is #2599"
(`HOME_RUNTIME_IDENTITY_PROFILE.md:58`). And mechanically: `from_tag` refuses unknown tags
(`body.rs:379`) and GEN-A genesis verification requires the initial grant to equal exactly the
Alpha set (`subject_context.rs:634`, `:690`), so widening the set breaks existing verifiers.

---

## 5. N4-A §7.2 act classes and HIA §9.3

`N4A:233-239`: class 1 deferred-decision (the decision pins `E`; a device revoked after `E` still
counts), class 2 immediately settled ("the acceptor evaluates at its own retained `frontier − 1`
once, at admission; **no validity bound exists** and R5.1 remains unmet"), class 3 authority
events ("not acts; the log orders them itself"). HIA `§9.3` table at
`docs/architecture/HUMAN_IDENTITY_ARCHITECTURE.md:1318-1322`, and the honest statement at
`:1356-1362`: for class 2 "the only real controls are **scope limits** (a device may not settle
above some bound) and a relying party's own freshness bar".

Note what the mapping is **not**: it maps *acts* to evaluation rules, not device *capabilities* to
classes. The only capability-to-act coupling in code is one capability per act
(`N4A:178-179`; `DeviceActV1.capability`, `device_authority.rs:120-121`) and
`CapabilityNotGranted` when the act's capability is outside the grant (`N4A:210`;
`device_authority.rs:275-277`). Which capability an action family uses is the family's choice.

Two HIA requirements frame the role question directly: **R3.5 [STRONG]** "Device classes differ by
scope and custody strength, never by principal class" (`HIA:234-235`), and the Meaning Firewall
table at `HIA:1500-1504`: kernel/identity layers hold "a generic **attenuating delegation** between
principals" and never "any notion of person, member, **device-class** or coop"; the **app** layer
holds "`Person`, `member`, **`role`**, standing, recovery policy, act-class rules".

---

## 6. GEN-A `alpha_initial_device_capabilities()`

`icn/crates/icn-identity/src/subject_context.rs:364-371`: "Exactly `{Sign, Present}`. `Encrypt` is
not granted because nothing in this profile addresses content to the Subject yet, and `Recover` is
not granted because an app-layer label named `Recover` must never be mistaken for N1 establishment
authority." Genesis verification pins equality with this set (`:634`, `:690`), and the initial
device must not be the establishment authority (`SubjectContextError::InitialDeviceIsAuthority`,
`:385-387`). N4-C enrollment never requests or grants `Recover` (`device_enrollment.rs:110,
134-135, 293-294`); HOME `§6.2` says approvals use `capabilities ⊆ {Sign, Present, Encrypt}`
(`HOME:192`).

---

## 7. The precedent: N4-D `device_channel_binding.rs` (untracked)

The pattern the Home slice needs already has a worked example in the worktree, uncommitted:

- "This module adds **no kernel primitive**. A channel binding is a [`DeviceActV1`] with capability
  [`DeviceCapability::Present`] whose payload is a domain-separated [`ChannelBindingV1`]"
  (`device_channel_binding.rs:9-11`).
- Own domain and version: `CHANNEL_BINDING_DOMAIN = b"icn.n4.channel-binding"`, version 1
  (`:52-55`); payload struct `ChannelBindingV1 { channel: [u8;32], nonce: [u8;32] }` (`:59-63`)
  with strict `canonical_bytes`/`decode` (`:74-96`).
- `bind_channel` builds `DeviceActV1::new(subject, device, DeviceCapability::Present, position,
  payload)` and signs it (`:116-132`); `verify_channel_binding` refuses a non-`Present` act
  (`:145`, `NotAPresentation`), then runs N4-A unchanged, then one equality check.
- The layering table (`:17-24`) separates transport / binding / N4-A / the human.

This is exactly "an action family that signs through this envelope places its own
domain-separated canonical bytes here; N4-A never parses them" (`N4A:180-182`;
`device_authority.rs:67-68`). The Home role object should follow the same shape — but at the
**app layer**, see §9.

---

## 8. Answers

### (a) Where should Home roles live?

| Option | Verdict | Why |
|---|---|---|
| New kernel-level `DeviceCapability` variants | **No** | wire change (`body.rs:363, 379`); breaks GEN-A verify (`subject_context.rs:634, 690`); "contract change for #2599" (`N4A:246-247`); role is app-layer meaning (`HIA:1500-1504`, R3.5 `HIA:234-235`) |
| Namespaced app capability / role object carried in `DeviceActV1.payload` | **Yes — the answer** | payload is opaque by contract (`N4A:180-182`); precedent N4-D (`device_channel_binding.rs:9-11`); evaluated by N4-A as the N1 capability the role requires; zero contract change |
| `icn-authz` resources/actions | **Borrow the `domain:verb` grammar only** (`ids.rs:47-56`) | no callers, no adapters (`adapters/mod.rs:1`), `did:` string subjects (`ids.rs:14-25`), `BlockHeight` not log position (`ids.rs:199-203`), exact-match query (`edge.rs:103-123`), no attenuation |
| `TypedScope` / `AuthorityGrant` | **No** | grantor must be a chartered entity (`authority.rs:190-201`); wall-clock validity (`:286-296`); institutional axes frozen by ADR (`:131-134`); explicitly excludes RBAC roles (`ADR-0014:178-182`); HOME `:165` |
| kernel `Capability` bearer / `ActionKind::Custom` | **No** | no `CapabilityEngine` impl; only an unsigned wildcard root (`runtime.rs:730-740`); per-request policy keyed by `Did` (`authz.rs:487-495`); forbidden substitute (`HOME:196-198`) |
| Attenuation caveat on N1 `Authorize` under #2599 | **Not for this slice; file it if needed later** | it is the legitimate path for *verifier-enforced* role attenuation across relying parties, but it changes N1 canonical bytes, the GEN-A preimage and the N4-B record layout; the Home slice does not need it because the Home runtime is its own relying party |

Composition, not enumeration: each Home role **reduces to one of the three grantable N1
capabilities plus app data**, and the roles that do not reduce are not device roles at all:

| Home role | Reduces to | Cite |
|---|---|---|
| presentation endpoint | `Present` act with Home payload; the channel it is on via N4-D | `body.rs:357`; `device_channel_binding.rs:9-11` |
| execution provider (hosted VM, PC) | `Sign` act with Home payload; the VM is a device Principal | `body.rs:353`; `HOME:164, 206` |
| storage provider | `Encrypt` if it must decrypt content addressed to the Subject; **no capability at all** if it only stores public N1 facts | `body.rs:355`; `HOME:204` |
| administrative / management device | a `Sign` device whose Home act family is "admin"; it can never author N1 facts | `body.rs:345-349` |
| approval / authority edge | **not a device role** — it is the Subject's establishment authority; refused as a device | `N4A:251-259` (§7.4); `subject_context.rs:385-387`; `N4C:98` |
| recovery participant | **not a device role** — see (b); N7 object, MISSING | `HOME:61`; #2603 |
| context participant | **not a device role** — a Subject per context (GEN) with per-context device keys; household membership is mutual recognition | `HOME:141-150, 163`; `N4C:131-134` |

### (b) `Recover` vs "participates in a recovery scheme"

They are **not the same thing**, and nothing in the current contracts links them.

- `DeviceCapability::Recover` is a bit in a *device's* grant inside the Subject's log. It is
  documented inert (`body.rs:345-349, 358`), never requested or granted by enrollment
  (`device_enrollment.rs:134-135, 293-294`), never granted by Alpha genesis
  (`subject_context.rs:366-368`), and #2599 invariant 7 forbids it ever becoming a log-writing or
  branch-selection privilege. No code path reads it: `grep -n "DeviceCapability::Recover"` in
  `derive.rs`/`device_authority.rs` finds only the refusals above. It is a reserved tag with no
  consumer.
- Recovery participation (N7, #2603) is establishment-level. A guardian group is "a **threshold**
  key set held by M-of-N guardians" (`HIA:1700-1703`) whose aggregate is **one** `PrincipalKey` in
  the pre-committed authority set (#2603 invariant 2: "Threshold key = one principal. Individual
  guardian keys are custody shares/participants; the N1 establishment authority sees one logical
  group principal"). It lands in the log as `AuthorityBody::Recover(EstablishmentBody)`
  (`body.rs:515-523, 562`) and **clears every device grant** (`derive.rs:288-293`; #2603
  invariant 9). The guardian descriptor, commitment and signing session are #2603 "Required
  protocol artifacts" and are **MISSING** (`HOME:61`).
- So a "recovery participant" is a participant in a *different* object (an N7 group behind an
  establishment principal), typically another human's principal (`HIA:1612`), and must never be
  represented as a device grant carrying `Recover`. The Home runtime may display "intended
  guardian" as its own UI state; that confers nothing and must say so.

### (c) The smallest primitive

**`HomeRoleV1`: an app-layer, domain-separated payload object carried in `DeviceActV1.payload`,
signed by the device, verified by N4-A exactly as any act, and interpreted by the Home runtime as
data.** No kernel or N1 change.

Shape (illustrative; the bytes are the Home layer's own, which `N4A:180-182` permits):

```text
home_role_v1 := LP("home.role") || u16be(1)
             || LP(role)            -- ASCII "domain:verb" label, e.g. "home:present" | "home:execute" | "home:store" | "home:admin"
             || b32(service)        -- digest of the Home service/context descriptor this act is for
             || b32(nonce)          -- relying-party challenge or device-chosen bytes; replay is the family's business
             || LP(intent)          -- opaque Home bytes (what the device wants to do in that role)
```

Composition, type by type:

1. `DeviceActV1 { subject, device, capability, evaluation_position, payload }`
   (`device_authority.rs:115-128`) with `capability = required_capability(role)`:
   `home:present → Present`, `home:execute | home:admin → Sign`, `home:store → Encrypt`.
   `sign_device_act` (`:220`) refuses a key that is not `act.device`.
2. The Home runtime (relying party) chooses `E` per `N4A:217-231`, supplies the Subject, and runs
   `verify_device_act` (`:384`) → `DeviceAuthorityEvidence { subject, device, capability,
   evaluation_position, grant, generation }` (`:281-296`) or a refusal.
3. **Role ⊆ grant**: the runtime checks `evidence.capability == required_capability(role)` and
   decodes the payload strictly (wrong domain/version/trailing bytes → refuse), mirroring
   `verify_channel_binding`'s `NotAPresentation` check (`device_channel_binding.rs:145`). A role
   can never widen a grant because the verifier only ever confirms a capability N1 recorded
   (`N4A:210`, `CapabilityNotGranted`).
4. The runtime stores `(act bytes, signature, evidence)` as HOME `§6.3` already prescribes
   (`HOME:205`): "evidence is reproducible from facts + act + `E`; it is a log, not an oracle".
5. Durable *assignment* ("device B is the desk's presentation endpoint") is Home-runtime policy. If
   the person wants it signed, it is another Home act family (`home:admin`, payload names the
   assignee and role) by one of their own `Sign` devices, verified the same way. It is never an N1
   fact.
6. Carriage to a relying party reuses N4-B `DeviceAuthorityBundleV1` unchanged (`HOME:64`); the
   `icnctl device-authority verify|inspect` verb already answers authorized/refused/malformed for
   any payload (`HOME:65`).

**What changes.** No contract. Kernel: nothing. N1 bytes: nothing. N4-A/B/C: nothing. GEN: nothing
(the role object is context-agnostic; the production Private/Household context kind remains
#2602's, `HOME:132-146`). Two *optional* non-contract items:

- Visibility: `pub(crate) mod encoding` (`authority_log/mod.rs:216`) means a Home crate cannot reuse
  `Writer`/`Reader`. Either the Home crate writes its own 30-line length-prefix encoder for its
  own payload (allowed: the payload is opaque to ICN and the MUST NOT at `HOME:242-243` is about
  reimplementing *ICN's* encodings, fold and grant semantics), or `icn-identity` re-exports
  `Writer`/`Reader` as a small API change with no semantic content.
- Placement: a *generic* "labelled act" family could live in `icn-identity` beside N4-D, but
  `role` is app-layer vocabulary (`HIA:1504`) and the handoff boundary says ICN must not "encode
  Katie-, Pi-, VM- or RDP-specific workflow" (`docs/dev/handoff-2026-10-03-katie-home-identity-integration.md:99`).
  Keep `HomeRoleV1` in the Home runtime crate.

**Honest limitation.** Without an N1 caveat, a device granted `Sign` can sign a Home act claiming
any `Sign`-class role; role attenuation is enforced by the Home runtime's policy, not by the
verifier, and does not travel to other relying parties. For a personal Home whose runtime is the
only relying party that is acceptable and matches `HIA:1356-1362` (class-2 controls are scope
limits and the acceptor's own rule). If role-level attenuation must be verifier-enforced across
relying parties, that is "scope/caveat representation" under #2599 and must be filed there.

### (d) What must NOT be done

1. Do not add, rename or renumber `DeviceCapability` variants (`body.rs:363, 379`;
   `N4C:140-143`; `N4A:246-247`).
2. Do not make `Recover` mean recovery participation, and never request or grant it to a device
   (`body.rs:345-349, 358`; `device_enrollment.rs:134-135, 293-294`; #2599 inv. 7).
3. Do not reuse `AuthorityGrant` / `TypedScope` / `Mandate` for personal or household roles
   (`authority.rs:190-201`; `ADR-0014:178-182, 588-593`; `HOME:141-146, 165`).
4. Do not use kernel `Capability` bearer tokens, JWT sessions, `ActionKind::Custom` policy
   requests, SSH keys, OS accounts, RDP logins or TLS client certs as device roles
   (`HOME:196-198`; `N4A:380-385`; `authz.rs:730-740` root token has no signature).
5. Do not use `icn-authz` as a runtime authority source; borrow its `domain:verb` grammar at most
   (`adapters/mod.rs:1`; `ids.rs:14-25`).
6. Do not model the approval/authority edge as a device role; the establishment key is refused as
   a device (`N4A:251-259`; `N4C:98`).
7. Do not model "context participant" as a device role; it is a per-context Subject and
   per-context device key (GEN, `HOME:148-150, 163`; `N4C:131-134`), and a household is mutual
   recognition, not a grant (`HOME:141-146`).
8. Do not let a role widen a grant: role ⊆ grant, always checked against
   `DeviceAuthorityEvidence.capability`.
9. Do not choose `E` from the act, cache verdicts across positions, or fall back to the frontier
   (`N4A:383-384`; `HOME:245-246`).
10. Do not repurpose node `BindingInfo` for devices (`HOME:224-227`); use the N4-D object once it
    is committed.
11. Do not put Home/Katie vocabulary into `icn-identity` or `icn-kernel-api` (`handoff:99`;
    `HIA:1500-1504`).
12. Do not reimplement ICN encodings, the fold, grant semantics or refusal logic outside the Rust
    library (`HOME:242-243`); defining the Home payload's own bytes is not that.
13. Do not derive device keys from `SubjectId` or reuse one device key across contexts
    (`HIA:1614-1620`; `HOME:163`).
14. Do not represent personal roles with `ScopeLevel` or a `GovernanceDomainV1` context
    (`scope.rs:8-10`; `HOME:131`).

---

## 9. Checks that ran

Read-only `grep`/`sed`/`cat` over the worktree at `b098396b5`; `git status --short` and `git log`
for the channel-binding files; `gh issue view 2599` and `2603`. No cargo, no edits. Line numbers
are from that checkout and will drift.
