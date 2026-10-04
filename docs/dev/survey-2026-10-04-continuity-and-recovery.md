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
> Title of record: Survey: what a person's durable continuity is in N1 (ContinuityRoot, pre-rotation, Rotate/Recover), loss scenarios, and the exact N7 gaps.

# Survey — recovery and continuity for a Home runtime under ICN N1 (read-only, 2026-10-03)

Source: `the icn-dev VM:/home/ubuntu/icn-dev/worktrees/icn/katie-home-identity-integration`,
branch `task/katie-home-identity-integration` @ `b098396b5`. All paths relative to that root.
No files modified, no cargo run. `gh` is available on the VM (2.99.0); issue #2603 was read with it.

Abbreviations: IS = `docs/architecture/IDENTITY_SEMANTICS.md`; HIA = `docs/architecture/HUMAN_IDENTITY_ARCHITECTURE.md`;
HRIP = `docs/architecture/HOME_RUNTIME_IDENTITY_PROFILE.md`; GEN = `docs/architecture/GEN_SUBJECT_CONTEXT_GENESIS.md`;
N4C = `docs/architecture/N4C_DEVICE_ENROLLMENT_REQUEST.md`; `construct.rs`/`derive.rs`/`body.rs`/`mod.rs` =
`icn/crates/icn-identity/src/authority_log/<file>`; `derived.rs` = `icn/crates/icn-identity/tests/authority_log_derived.rs`;
`orders.rs` = `icn/crates/icn-identity/tests/authority_log_adversarial_orders.rs`.

---

## 1. What object IS the durable human continuity

### 1.1 The contract

- IS §2.5 (L207-224): `ContinuityRoot` is "private secret material from which subject-context authority
  material may be derived, plus the person's own index of their subjects." Domain: "opaque client-side secret.
  No public identifier form exists or may be defined" (L212-213). Genesis "client-side at first onboarding;
  phone-only onboarding must produce this by default" (L214-215). It "does not sign protocol acts. It derives
  and recovers Principals that do" (L219-220). Admitted shortfall: "compromise of the continuity secret is a
  takeover and is unrecoverable under the default recovery path. Recorded, not solved" (L221-224).
- IS §10 (L581-628): the root is "private continuity / recovery material", never in the canonical inception
  body, names nothing, obligation = secrecy (L585-590). "`ContinuityRoot` must never enter canonical or public
  encoding. Protected local persistence, and future encrypted transfer, recovery or threshold sharing, are
  permitted — N7 (#2603) owns that protocol" (L612-615). The live helper "bundles the secret with the public
  `ContextNonce`" and that is "implementation debt, not unresolved semantics" (L617-628).
- IS §2.2 (L140-147): the Subject's lifetime "survives key rotation, device replacement, total device loss and
  algorithm change" architecturally, but "surviving total device loss in practice additionally requires the
  person's continuity material to be recoverable, and the protected backup / recovery / threshold-share
  protocol that would deliver that is N7 (#2603) — not built".
- HIA R1.2 [HARD] (L202-203): "No institution, node, gateway or operator holds a key that lets it author acts
  as a human. *Custody defines identity.*" R1.3 [HARD] (L204-205): loss of any infrastructure must not destroy
  the identity. R2.4 [STRONG] (L221-222): continuity "must not require a permanent master secret whose
  compromise is unrecoverable" — and §12.3 (L1770-1777) states path (a) **does not meet R2.4/R9.5**, trading
  them for R10.2.
- HIA §12.1 (L1677-1696): "Pre-rotation is the recovery mechanism. The inception event commits to a digest of
  the next key set. If that next key set is derived from the continuity root, held in backup and not on any
  device, then total device loss is recovered by restoring the root, deriving the pre-committed keys, and
  rotating." And: "it does not solve 'I lost my devices *and* my backup.' ... a person who chose path (a) and
  lost both has lost the subject" (L1689-1694).
- HIA §12.2 (L1698-1745): (a) self-recovery commits to "keys derived from the continuity root (recovery
  phrase)", total-loss = "restore phrase → rotate", root compromise = "takeover, and unrecoverable", produced
  offline = yes; (b) guardian-gated commits to "a threshold key set held by M-of-N guardians", needs an online
  quorum, and FROST-class N7 is "a hard prerequisite" (L1700-1719). "Default is (a), upgradeable to (b) ...
  the *operational* keys live on the phone; the *pre-rotation* material is backed up off-device at onboarding"
  (L1741-1745).

### 1.2 The code: what the root is and what pre-rotation does

`construct.rs`:

- L82-87: `pub struct ContinuityRoot { secret: Zeroizing<[u8; 32]>, context_nonce: ContextNonce, plan: Vec<EstablishmentKind> }`.
  `plan[g - 1]` is the kind armed for generation `g` (L85). No `Clone`, no `serde`, no encoder; only a `Debug`
  impl that never renders the secret (L89-97). There is **no accessor for the secret** at all; only
  `context_nonce()` (L128-130), `horizon()` (L123-125) and `kind_at(g)` (L133-138) are readable.
- L28-34: `pub enum EstablishmentKind { Rotate, Recover }` — "The kind is inside the commitment, so exactly one
  is armed per generation. Path (a) and path (b) can never both be armed at the same position" (L25-27).
- L101-107: `pub fn new(secret: [u8; 32], context_nonce: ContextNonce, horizon: usize) -> Self` arms `horizon`
  generations, **all `Rotate`**. L110-120: `pub fn with_plan(secret: [u8; 32], context_nonce: ContextNonce, plan: Vec<EstablishmentKind>) -> Self`.
- L153-161 (private `authority_seed`): `seed = SHA-256(LP(KDF_DOMAIN) || b32(context_nonce) || u64be(generation) || b32(root))`.
  Inputs are the **pre-subject nonce and the generation, never the SubjectId** (L149-152). `KDF_DOMAIN = b"icn.authority-log.kdf"` (`mod.rs` L253).
- L164-166: `pub fn authority_signing_key(&self, generation: u64) -> SigningKey` — the per-generation Ed25519
  key, exportable as a `SigningKey` (this is the "operational key" the phone holds).
- L173-176: `pub fn authority_set(&self, generation: u64) -> PrincipalSet` — exactly one key per generation
  ("preserves the single-writer property", L170-172).
- L183-203: `pub fn commitment(&self, generation: u64) -> Commitment` — backward fold from the horizon:
  `C_{horizon+1} = TERMINAL`, `C_g = H(kind_g ‖ PS(A_g) ‖ C_{g+1})` (L178-182), using `derive::commitment_for`
  (`derive.rs` L120-135: `SHA-256(LP(COMMITMENT_DOMAIN) || u8(kind) || PS(authority) || b32(next_commitment))`).
  Beyond the horizon the commitment is `Commitment::TERMINAL` (all-zero, `body.rs` L68-75), which "authorizes
  nothing" and freezes the authority set "for the remaining life of the subject" (`body.rs` L72-75).
- L209-223: `pub fn incept(&self) -> Result<SignedAuthorityEvent, ConstructError>` — inception body =
  `{signer: A_0, context_nonce, initial_authority: A_0, next_commitment: C_1}`, signed with key 0.
  L226-228: `pub fn subject_id(&self) -> Result<SubjectId, ConstructError>` = `event_id(inception body)`.
- L235-273: `pub fn establish(&self, generation: u64, subject: SubjectId, position: u64, prev_digest: EventId) -> Result<SignedAuthorityEvent, ConstructError>`
  — refuses generation 0 (L242-244) and generations outside the horizon (L245-250); builds
  `EstablishmentBody { header{subject, position, prev_digest, signer: A_g}, revealed_authority: A_g, next_commitment: C_{g+1} }`
  (L258-267); wraps as `Rotate` or `Recover` **per the plan**, not per caller choice (L268-271); signs with
  key `g` (L272). Deterministic: "no entropy parameter" (L232-234).
- L281-296: `pub fn establish_from_state(&self, state: &AuthorityState, subject: SubjectId, position: u64, prev_digest: EventId) -> Result<SignedAuthorityEvent, ConstructError>`
  — refuses if `state.next_commitment.is_terminal()` (`ChainExhausted`, L288-290); sets
  `generation = state.generation + 1` (L291); refuses if `self.commitment(generation) != state.next_commitment`
  (`CommitmentMismatch`, L292-294); else delegates to `establish`. "This is the safe entry point" (L278-280).
- L47-76: `ConstructError::{GenerationOutOfRange, GenerationZero, CommitmentMismatch, ChainExhausted, Codec}`.

`derive.rs`:

- L49-61: `pub struct AuthorityState { pub authority: BTreeSet<PrincipalKey>, pub next_commitment: Commitment, pub generation: u64, pub devices: BTreeMap<PrincipalKey, DeviceGrant> }`.
- L161-186 `authorized_by_state`: `Rotate`/`Recover` are authorized **iff** `state.next_commitment` is not
  terminal (L165-168), the recomputed commitment over `(kind, revealed_authority, next_commitment)` equals
  `state.next_commitment` (L169-176), and the signer is the sole revealed principal (L177). `Authorize`/`Revoke`
  are authorized iff `signer ∈ state.authority` and `|authority| == 1` (L179-184). "No capability, no flag,
  nothing that a compromised current key could use to claim establishment priority" (L155-157).
- L218-227 `supersede`: establishment events, if any, supersede non-establishment; "never by content"
  (L213-217). L248-259 `resolve`: `0 → Exhausted`, `1 → Advance`, `≥2 → Halt`.
- L277-309 `apply_transition`: **`Rotate`** (L283-287) sets `authority = revealed`, `next_commitment = body.next_commitment`,
  `generation += 1`, **devices untouched**. **`Recover`** (L288-293) does the same **and `next.devices.clear()`** (L292).
  `Authorize` inserts a `DeviceGrant{capabilities, validity, granted_at: position}` (L294-303); `Revoke` removes it (L304-306).
  Doc: "`rotate` and `recover` both replace the authority set with the revealed set, so a superseding
  establishment actually removes a compromised key rather than extending around it ... They differ only in their
  effect on device grants: `rotate` preserves them, `recover` clears them" (L273-276).

`body.rs`: `EstablishmentBody { header, revealed_authority, next_commitment }` (L516-524); `AuthorityBody::{Inception, Rotate, Authorize, Revoke, Recover}` (L552-563); wire tags `ROTATE = 0x02`, `RECOVER = 0x05` (`mod.rs` L263-269); `DeviceCapability::Recover` is "an app-layer recovery role. **Inert** with respect to establishment authority" (L351-359).

`mod.rs` L74-105 states the scheme once: "Commitments are indexed by establishment generation `g`, not by log
position"; "`Rotate` and `Recover` can never both be armed at the same position". L121-125 is the event table
(Rotate: devices preserved; Recover: devices cleared).

**Summary of pre-rotation in this code.** The whole chain is a pure function of `(secret, nonce, plan)`. The
secret is the only private input; the nonce is public (it sits in the inception body, `body.rs` L507, and is
re-derivable from the GEN descriptor+salt, GEN §6.1 L158-168); the plan is *not* public but is pinned by
`C_1` inside the inception body — a different plan/horizon yields a different `SubjectId`
(`tests/gen_subject_context.rs` L310-324 `a_different_plan_horizon_changes_the_subject`; GEN §6.2 L182-193;
`src/subject_context.rs` L115-128). A `Rotate` or `Recover` at generation `g` reveals `A_g`, commits `C_{g+1}`,
and bumps `generation`; the old key leaves the authority set; `Recover` additionally wipes every device grant.

### 1.3 What "the durable human continuity" therefore is, concretely

Three distinct things, which must not be conflated:

| Object | Public? | Where it lives | What it does |
|---|---|---|---|
| `SubjectId` (per context) | yes, to the context | in every relying party's store | the durable name; fixed at inception, never changes under rotate/recover (IS §2.2 L130-139; HIA §12.3 L1781-1784) |
| `Bodies(σ)` + witnesses (`AuthorityStore`) | yes | any replica; grow-only, join = union (`mod.rs` L153-162) | the authoritative *state* — every honest holder of the same set derives the same `AuthorityState` (R4.3/R4.5) |
| `(secret, nonce, plan)` → `ContinuityRoot` | **no** (secret) | person's client + protected backup only | the only thing that can *advance* the establishment chain |

The "durable human continuity" the question asks about is the pair: the public `SubjectId` as the name, and the
private `(secret, plan)` (plus the recoverable nonce) as the sole capability to re-establish authority over
that name. The person's "index of their subjects" (IS §2.5 L209; GEN §5.1 L116-120: the salt "must be retained
in the person's private subject index and protected recovery material") is the third, unbuilt, component.

---

## 2. Issue #2603 (N7) — what is specified, what is `needs-design`

`gh issue view 2603 --repo InterCooperative-Network/icn` (OPEN; labels `area:identity, area:security,
epic:arch-invariants, security-review, status:needs-design, tier:1-correctness, type:impl`).

**Specified (purpose/invariants):** "the threshold/quorum recovery protocol for a human subject"; "a quorum can
authorize one logical recovery transition without giving every qualifying quorum a subject-wide kill switch";
"N7 is therefore a protocol around threshold cryptography, not a synonym for FROST". Load-bearing invariants
1-10: one logical transition per position; threshold key = one principal; canonical body before signing; one
next-authority commitment; no second-quorum fork; no arrival-order selector; recovery is prospective;
participant anti-equivocation survives restart; "Recovery does not silently preserve compromised device grants.
N1 `Recover` semantics clear device grants unless a later explicitly authorized flow re-establishes them";
historical signature ≠ surviving authorization.

**`needs-design` (the Scope and Required-artifacts lists, none of which exist in code):** guardian/group
enrollment and rotation; threshold/group public principal representation; canonical recovery request/context
(`RecoveryIntent`/session id); deterministic successor derivation; FROST/signing-session binding; participant
durable anti-equivocation state; nonce/session lifecycle; offline guardian behaviour; guardian replacement/loss;
lost-device vs stolen-device paths; compromised-current-authority vs compromised-recovery-group; competing
attempts; observation via N3; mapping of the threshold result to the N1 `Recover` body/witness;
post-recovery device-grant clearing/re-establishment; evidence/receipt shape; migration from legacy trustees.
Acceptance criteria are all unchecked. Non-goals: guardians never own the subject; no global recovery registry;
no independent M-of-N as final design; no production cutover. "#2591 is not N7 completion."

**Docs referencing #2603:** IS L147, L224, L615, L696 (N7 row: N2 establishes "the `ContinuityRoot` contract,
including that protected export and threshold sharing are permitted (§10)"; downstream = "the protected backup
/ recovery / threshold-share protocol, and the continuity-secret-compromise shortfall"); N4C L125-127
("Recovery. Out of scope here (N7, #2603) ... losing the *edge* is an establishment-authority event, handled
by N1 pre-rotation, not by enrollment"); HRIP §1 row (e) L61 ("LIB-TESTED (establishment transitions); MISSING
(guardian/threshold protocol, backup)"); handoff `docs/dev/handoff-2026-10-03-katie-home-identity-integration.md`
L218 ("recovery (N7 #2603)" listed under "Not done, named").

**What the guardian/threshold protocol would add.** Under path (b) the pre-committed `A_g` for some generation
is a *group* key (one `PrincipalKey`, `mod.rs` L141-143 "A future threshold/group key remains one
`PrincipalKey`"), so the reveal at that generation is a FROST-class group signature over the one canonical
body. N1 already enforces the two rules that make this safe without halting: canonical derivation is *checked*
(`derive.rs` L169-177) and exactly one kind+authority is armed per generation (`construct.rs` L25-27,
`mod.rs` L103-105; HIA §9.2.1 constraint 3 L1183-1213). `mod.rs` L202-207: "Guardian recovery is **not**
implemented here. N7 ... is necessary but not sufficient ... the guardian workflow itself is out of scope."
What N7 would add is therefore: (i) a way for the root to *arm* a group principal at a generation instead of a
root-derived key (today `authority_set(g)` is always root-derived, `construct.rs` L173-176 — there is no hook
for a foreign `A_g`), (ii) the ceremony that produces one signature over the one canonical body, (iii)
guardian custody of shares rather than of the secret.

**What "root compromise is unrecoverable under the default path" (IS L221-223) means.** HIA §12.2 L1721-1733:
an attacker holding the root "derives the pre-committed keys, rotates first, and commits a fresh `next` digest
the victim does not hold ... pre-rotation makes root compromise *more* decisive, not less. It is an excellent
defence against *device* compromise and no defence at all against *backup* compromise." Note a precision
against the live code: in `construct.rs` the next commitment is **not** freely chosen — it is
`self.commitment(generation + 1)`, derived from the same root and plan (L266). So an attacker with the secret
(and the plan, which is pinned by the public `C_1` and brute-forceable for small horizons) produces the
*byte-identical* canonical rotation the victim would, and both parties hold every generation's key forever.
The live outcome is a **symmetric, permanent contest**: either side forks any non-establishment position
(`derived.rs` L261-305 obligation 11; `orders.rs` L205-234 case 9) and a superseding establishment never
excludes the other (both derive `A_{g+1}`); each round burns one generation until `TERMINAL`, after which no
establishment is authorized (`derive.rs` L165-168) and a fork is a permanent halt. Either reading
("lock-out" per the doc, "shared authority to exhaustion" per the code) is unrecoverable; the doc's sentence
about a "fresh `next` digest the victim does not hold" does not describe `construct.rs`. HIA §9.6 item 7
(L1545-1547): "its compromise is takeover of every context *plus* full deanonymization, and under path (a) it
is unrecoverable." R9.6's notice-and-veto window (HIA §12.3 L1757-1769) is the only mitigation and is a
relying-party wall-clock policy, not code.

---

## 3. Where authoritative continuity state may live vs what is a client/cache

**Authoritative state = the public body set.** `mod.rs` L14-17: durable replicated state is "two grow-only
sets joined by union"; the derived view is "a pure function of the durable body set". HIA R4.5 (L246-253):
authority may be evaluated against receiver-local replicated state iff it "is itself authenticated and
converges deterministically". So *any* replica — phone, Home VM, relying party — holding `Bodies(σ)` is equally
authoritative about the derived `AuthorityState`; none is "the" source. The caveat is completeness, not
authority: HIA §18 O17 (L2140-2141) "A truncated-but-valid prefix verifies perfectly and yields a stale
authority state"; N4-A therefore fails closed when the retained prefix does not reach `E`
(`src/device_authority.rs` L312-323 `PrefixIncomplete`; `derive.rs` L452-454). A Home VM's store is thus a
**cache of facts** whose verdicts are exact for the prefix it holds and unknowable beyond it.

**Secret material: client only.** IS §2.5 L212 ("opaque client-side secret"), §5 item 4 (L337-339): "No
protocol party may hold the linkage; the only linkage that exists lives in the client-held `ContinuityRoot`";
§10 L612-615. HIA R1.2 L202-203; R3.1 L225-226 ("A device holds its own key and never receives another
principal's key"); R8.4 L289 ("A relay relays without holding any key of the author"); §11.1 L1609 (node
operator/gateway "does not learn ... the continuity root"); §8 scenarios 11-13 (L913-917: gateway, personal
node, malicious host "holds no subject key and cannot author").

**HRIP §5/§6.3 verified against those contracts.** HRIP §5 L163-164 (continuity root "on the person's own
client — the phone is the authority edge — with pre-rotation material backed up off-device at onboarding ...
A VM-hosted Home runtime holds *public* N1 facts and the VM's own device keys; it is a **device Principal**
... not a custodian ... the default is refuse, and a host operated for someone else never qualifies") and
§6.3 L200-207 (stores public facts, device acts + evidence, its own per-context device keys; "never: any
`ContinuityRoot`, any other device's secret, any human establishment key, any cross-context mapping") are
consistent with IS §2.7 L238-240 (a device "generates its own key. A device never receives another
Principal's key"), IS §2.5, IS §10, HIA R1.2/R3.1/R8.4, and I10 (IS L648). §6.6 L236-249 MUST NOT list
("hold or derive a `ContinuityRoot` on infrastructure; ... build an enrollment, revocation or recovery mechanism
of its own") matches IS §13 L707-709 ("Explicitly not smuggled into N2 ... protected continuity backup/recovery").
Nothing in the contracts contradicts the profile. One sharpening the code supports: the phone need not hold the
root after onboarding — `authority_signing_key(g)` is an exportable `SigningKey` (`construct.rs` L164-166) and
`approve_enrollment` takes only `establishment_key: &SigningKey` (`src/device_enrollment.rs` L279-287), so the
"operational key on the phone, pre-rotation material off-device" split (HIA L1741-1745) is realisable with
today's API: phone = `SigningKey` for the current generation; backup = `(secret, plan)`.

---

## 4. Loss scenarios, from the code's semantics

### (a) Loss of an ordinary enrolled device → `Revoke`

`construct.rs` L339-356 `revoke_event(signer_key: &SigningKey, subject, position, prev_digest, device) -> SignedAuthorityEvent`,
signed by the current establishment key; authorized iff signer ∈ `state.authority` (`derive.rs` L182-184);
effect `devices.remove(&device)` (L304-306). The Subject is untouched: `tests/device_lifecycle_acceptance.rs`
L194-211 pins `authority`, `next_commitment` and `generation` (still 0) identical before and after, and
`tests/device_authority.rs` (25 tests) pins refusal at `E ≥` revoke and acceptance at earlier `E`. HRIP §6.4
L209-220; N4C L120-123; IS §2.7 L246 ("Losing a device does not touch the subject"). Replacement = `Revoke(old)`
+ a new N4-C request/approval with a new key. No recovery authority is lost because enrollment never grants
`Recover` (`src/device_enrollment.rs` L293-295 `RecoverNeverGranted`; N4C L125-126).

### (b) Loss of the phone (authority edge) with the root material backed up

A replacement edge rebuilds `ContinuityRoot::with_plan(secret, nonce, plan)` (`construct.rs` L110-120) — the
nonce from the public inception body or from the GEN descriptor+salt — then calls
`establish_from_state(&state, subject, frontier, chosen(frontier−1).event_id())` where `state` is the
`AuthorityView::Live { state, frontier }` derived from any complete replica. By construction:

- same `SubjectId` — the identifier is `event_id(inception)` and no later event rewrites it (`body.rs` L593-596; HIA §12.3 L1781-1784);
- `generation + 1`, authority = `A_1`, `next_commitment = C_2` (`derive.rs` L283-287);
- device grants **preserved** if the armed kind is `Rotate` (`derive.rs` L283-287; `derived.rs` L1176-1203
  `rotate_preserves_devices_and_recover_clears_them` proves a grant at position 1 survives a `Rotate` at 2 with
  `generation == 1`), cleared if `Recover`;
- the old edge key (`A_0`) is no longer in `authority`, so its later `Authorize`/`Revoke` are unauthorized
  (`derived.rs` L349-366: "the rotated-out key cannot author again", proven after a superseding `Rotate`);
- the new edge authors with `authority_signing_key(1)` (`orders.rs` L264-268 `authorize_at(&alpha, 1, …)` advances the frontier).

Determinism means a crash-and-retry at the replacement edge re-emits identical bytes (`derived.rs` L786-803
obligation 15, including "A freshly rebuilt root with the same secret and nonce reproduces the same bytes"
L804-810). **Gap:** no existing test exercises the *positive* `establish_from_state` path — its only call
site in tests is the negative `CommitmentMismatch` check (`derived.rs` L820-830); every positive establishment
in the suite goes through `establish(generation, …)` with a caller-supplied generation. No test composes
"rebuild root from the triple → `establish_from_state` at the frontier → grants intact → old key refused →
new key approves an N4-C enrollment → N4-B relying party reports `evidence.generation == 1`".

Constraint: the *kind* is fixed by the plan at genesis. With `ContinuityRoot::new` every generation is `Rotate`
(`construct.rs` L105); a Home that ever wants a grant-clearing `Recover` must arm it via `with_plan` **before
inception**, because changing the plan changes `C_1` and therefore the Subject (GEN §6.2 L191-193).

### (c) Compromise of the current establishment key (not the root)

What the compromised key can do: fork any non-establishment position (`derived.rs` L261-283 obligation 11;
`orders.rs` L205-234 case 9: "current-key compromise is an instant DoS; the model converges ON A HALTED
SUBJECT"); it **cannot** rotate because it holds no pre-committed material (`derived.rs` L286-305; HIA §9.2
L993-995; §8 scenario 1 L905). A `Recover` *capability* on a device grants nothing (`derived.rs` L688-740).

What the root holder does: author the next armed establishment **at the disputed position** (`derived.rs`
L328-334 "Recovery must rotate AT the disputed position, not the next one"). Proven: it supersedes both
non-establishment candidates, the frontier advances (L336-341), the victim's own event at that position is
also discarded ("a real cost of recovery", L343-347), the compromised key leaves the authority set and its
replay at the next position is refused (L349-366); every delivery order converges (`orders.rs` L237-268
case 10; L270-331 mixed scenario). A fork behind the frontier orphans the whole suffix including honest
grants, which remain durable facts but lose authorization (`derived.rs` L914-996 obligation 17; O-N9).

Whether it is a `Rotate` or a `Recover` is decided by the plan, not at the moment of compromise. The
distinction matters for a *stealthy* attacker who appends `Authorize(attacker_device)` in-chain at the frontier
without forking: a `Rotate` keeps that grant (the new edge must then `Revoke` it with key `g+1`), a `Recover`
clears it with every other grant (`derive.rs` L288-293; proven generically by `derived.rs` L1176-1203). Issue
#2603 invariant 9 restates this. N1's own limit: "O16/O16′ (a single compromised current key halts a subject
unilaterally; that is reproduced as a test, not prevented)" (`mod.rs` L180-181).

### (d) Loss of every device including the edge AND the backup

Nothing in the code can advance the chain: `establish`/`establish_from_state` need the secret; there is no
other constructor of an authorized establishment body (`derive.rs` L169-177 requires the exact pre-image).
What survives: the `SubjectId` and its public history (R2.3 for pre-fork positions); the frozen
`AuthorityState` at the frontier with `A_g` in `authority` and every device grant still in force — a grant
with `validity: None` is in force at every later position (`derive.rs` L36-44), and since positions only
advance when the authority key signs, position-denominated spans never lapse either. So enrolled devices keep
acting within their grants indefinitely and nobody can `Revoke` them (HIA §9.3 L1364-1372 for class 2 is the
same shape). The Subject is **orphaned, not halted**: `AuthorityView::Live` forever at the same frontier. HIA
§12.1 L1689-1694 ("has lost the subject"); §8 scenario 8 L917 ("R9.1 [HARD] is not fully met"); §9.6 item 8
L1548-1550; §18 O14 row L2136 ("the custody question remains"). Starting over means a new genesis per context,
which GEN §5.3 L149-154 says a client MUST refuse "unless a separately specified recovery or migration flow
authorizes it" — that flow does not exist, and institutional re-recognition is FACT B / GEN (#2602), not N1.

---

## 5. The exact missing pieces for a Home

| Piece | State in code | Owner |
|---|---|---|
| `ContinuityRoot` persistence/export format | **None.** No `Clone`, no `serde`, no canonical encoder (`encoding.rs` has no mention of the root); the secret is `Zeroizing` and has no accessor (`construct.rs` L82-87, L89-97). What *can* be exported is the caller's own copy of the inputs `(secret: [u8;32], plan: Vec<EstablishmentKind>)` captured before construction, plus the public nonce (inception body / GEN salt). `horizon()` and `kind_at(g)` let a holder of the live root read the plan back; the secret cannot be read back. Any bundle format, encryption, and I4a enforcement on the real codec are N2-F′ (IS §14 L747) and N7 | N7 #2603 (format/protection); N2-F′ (codec boundary) |
| Backup of pre-rotation material "off-device at onboarding" (HIA L1741-1745) | DOC-ONLY. No onboarding flow, no backup object, no phone client. The *shape* is realisable: phone keeps `authority_signing_key(g)`, backup keeps `(secret, plan, nonce-or-salt)` | N7 #2603; "Slice C (mobile genesis) ... must now also produce ... off-device pre-rotation backup" (HIA §18 L2143) |
| Per-context subject index (which contexts, which salt, which plan) | MISSING (GEN §5.1 L116-120; IS §2.5 L209) | GEN #2602 / N7 |
| Guardian threshold shares, group principal, ceremony | MISSING entirely; no hook to arm a non-root-derived `A_g` (`construct.rs` L173-176) | N7 #2603 (`needs-design`) |
| Re-establishment of a replacement edge | LIB-TESTED mechanism (`with_plan` + `establish_from_state`), **untested as a composed scenario**; the positive `establish_from_state` path has zero test coverage (`derived.rs` L820-830 is the only call, negative) | this session can add a fixture test; the ceremony/transport is N4/N3 |
| Delivery of the rotation fact to the Home and relying parties | MISSING (HRIP §6.2 L195-196; N1-D #2800; N3 #2598) | #2800, #2598 |
| Contest window (R9.6 notice-and-veto) | DOC-ONLY policy, wall-clock, relying-party side (HIA §12.3 L1757-1769); no issue found that owns it | none found |
| "Lost everything" re-genesis / migration flow and institutional re-recognition | MISSING (GEN §5.3 L149-154) | GEN #2602 (FACT B) |
| Historical authorization evidence after superseding recovery (O-N9) | OPEN; no evidence format (`mod.rs` L196-200) | no issue number cited in IS §13; O-N7 is #2606 |

Tests that exist for `establish_from_state`: exactly one, negative — `derived.rs` L820-830 (a root applied to a
stranger's state returns `Err`). Tests for the pieces it composes: `establish` determinism L786-840; generation
skip/replay L518-548; superseding at a disputed position L308-367, L914-996; `rotate_preserves_devices_and_recover_clears_them` L1176-1203; wrong-plan ⇒ different Subject `tests/gen_subject_context.rs` L310-324.

---

## 6. Recommendation — the narrowest executable piece, and what is missing

### 6.1 Recommended: one fixture test, `tests/edge_replacement_acceptance.rs` (or appended to `device_lifecycle_acceptance.rs`)

"**Phone lost; a replacement edge rebuilt from the backed-up `(secret, plan)` and the public nonce rotates to
generation 1 with the same `SubjectId` and intact device grants; the old edge key can no longer author; the
new edge approves a new device through N4-C; an N4-B relying party accepts it with `evidence.generation == 1`;
verdicts are order-independent.**" Everything it needs exists and is `pub`; it invents no custody, transport,
backup format or recovery protocol, and it is the first positive exercise of `establish_from_state`.

Steps and the exact functions:

1. Genesis as today: `authority_log_support::subject_with_plan(seed, vec![EstablishmentKind::Rotate; 4])`
   (support L36-49) — keep the raw `[seed;32]` secret and the plan *outside* the root as the "backup".
2. Edge key 0 = `root.authority_signing_key(0)`; enrol A and B exactly as `device_lifecycle_acceptance.rs`
   L45-61 does (`sign_enrollment_request` → `verify_enrollment_request` →
   `approve_enrollment(&signed, &edge0, subject, position, prev, grant, None)`).
3. Simulate loss: drop `root` and `edge0`. Rebuild
   `ContinuityRoot::with_plan(secret: [u8; 32], context_nonce: ContextNonce, plan: Vec<EstablishmentKind>) -> Self`
   with `context_nonce` read from the inception body (`InceptionBody.context_nonce`, `body.rs` L507) rather than
   from the dropped root, to prove the nonce is recoverable from public facts.
4. Derive the live state: `derive(subject, &store) -> AuthorityView`; take `Live { state, frontier }`.
5. `rebuilt.establish_from_state(&state, subject, frontier, chosen_prev) -> Result<SignedAuthorityEvent, ConstructError>`
   where `chosen_prev` is the `event_id` of the last chosen body (the B grant). Assert `Ok`, body is `Rotate`.
6. Ingest; re-derive; assert `state.generation == 1`, `state.authority == rebuilt.authority_set(1).members()`,
   `state.next_commitment == rebuilt.commitment(2)`, `devices` contains A and B with their original `granted_at`,
   `inception.body.subject() == subject` unchanged.
7. Old key: `authorize_event(&edge0, subject, frontier+1, rotation_id, device, caps, None)` ingests (admissible)
   but the frontier does not move — pins "the old edge key can no longer author".
8. New edge: `approve_enrollment(&rebuilt.authority_signing_key(1), …)` enrols C at `frontier+1`; a relying
   party via `DeviceAuthorityBundleV1::new(facts, act).canonical_bytes()` + `verify_bundle_bytes(&bytes, subject, E)`
   returns evidence with `generation == 1` for C, and still accepts B at the same `E`.
9. Negative sibling (cheap, high value): a backup with the *wrong plan* (`vec![Rotate; 5]`) rebuilt against the
   same state returns `Err(ConstructError::CommitmentMismatch { generation: 1 })` — pins "the plan is part of
   the identity and must be in the backup" on the *establishment* path (today pinned only on the genesis path,
   `gen_subject_context.rs` L310-324).
10. `for order in permutations(&all)` as in `device_lifecycle_acceptance.rs` L213-.

Classification to record: LIB-TESTED fixture; **not** N7; backup is a test-local `[u8;32]` + `Vec`, not a
format. The test should say so in its header exactly as `device_lifecycle_acceptance.rs` L1-21 does.

### 6.2 Missing protocol/state transitions, with owners

- Protected backup/export of `(secret, plan[, salt])`, its format, encryption, and the onboarding step that
  produces it — **N7 #2603** (IS §10 L612-615; HIA §12.2 L1741-1745; HIA §18 Slice C L2143).
- Guardian path (b): group principal armed at a generation, FROST-class one-message signing, shares, anti-equivocation
  state — **N7 #2603** (`needs-design`); requires a construction hook that does not exist (`construct.rs` L173-176).
- Carriage of the rotation fact and of enrollment requests/approvals — **N1-D #2800, N3 #2598, N4 #2599**.
- Per-context subject index and the client rule against a second genesis — **GEN #2602** (§5.1, §5.3).
- R9.6 contest window at relying parties — policy only, no owning issue found.
- Historical authorization evidence after supersession (O-N9) — open, no owning issue cited in IS §13.
- The IS §2.5 shortfall itself (root compromise) — recorded, not solvable in N1; **N7 #2603**.

### 6.3 Flag for the docs (not for this session to fix)

HIA §12.2 L1724-1726 ("commits a fresh `next` digest the victim does not hold") describes a non-deterministic
rotation; `construct.rs` L266 derives `next_commitment` from the root, so under the live code root compromise is
symmetric shared authority until `TERMINAL`, not a one-sided lock-out. Same verdict (unrecoverable), different
mechanism; worth a one-line amendment when N7 is designed.
