---
Status: normative
Canonical: yes
Last Reviewed: 2026-09-06
---

# Identity Semantics — the N2 semantic contract

What each identity-bearing thing in ICN *is*, which identifier domain names it,
which substitutions are permitted, and what a legacy `did:icn:<key>` may and may
not become. This document is the settled output of #2597.

> **Truth status.** This document is **canonical for the narrow
> `identity_semantics` domain only** — the seven semantic contracts, identifier
> domains, substitution rules, context-scope semantics, bridge invariants, and the
> type-level requirements implementation must satisfy. It is **not** a claim that
> any of it is implemented, deployed, migrated, or wired into production. See §1.

---

## 1. Scope and truth status

### 1.1 What this document owns

`ops/state/truth/sources.json` assigns the `identity_semantics` domain to this
file. Within that domain this file wins. The domain is deliberately narrow:

| Owned here | Not owned here |
|---|---|
| The seven semantic contracts and what each means | Identity requirements, derivation and threat model — `HUMAN_IDENTITY_ARCHITECTURE.md` |
| Which identifier domain names each identity-bearing class | Session-authority attenuation lifecycle — `AUTHORITY_SPINE.md` |
| Allowed and forbidden substitutions | Historical evidence map — `PRINCIPAL_MODEL.md` §1–§3 |
| Context-scope and correlation constraints | Every downstream protocol in §13 |
| Legacy-DID bridge invariants | The bridge *evidence object* (N2-E2) |
| Type-level invariants implementation must enforce | Any Rust implementation |

`HUMAN_IDENTITY_ARCHITECTURE.md` (HIA) remains the broader architecture note and
is registered `living`. It is deliberately **not** canonical: two canonical
sources for one set of facts is the ambiguity this document exists to remove.
Where HIA and this document address the same fact, this document controls.

### 1.2 Four different truth levels — do not collapse them

| Layer | Status |
|---|---|
| **This semantic contract** | **Canonical.** Independently reviewed twice; zero blocking findings. |
| **N1 authority-log primitive** (`icn/crates/icn-identity/src/authority_log/`) | **Implemented, tested, merged** as `7c28876d`. **A library primitive only.** |
| **N2 migration / integration** | **Absent.** Nothing in this document is built. |
| **Production integration** | **Absent.** `authority_log` has **zero references outside the `icn-identity` crate**. |
| **Downstream protocols** (§13) | **Not implemented, and not implemented by this document.** |

No deployment, federation, pilot, migration or production-readiness claim follows
from this document or from N1's merge.

**N2 is a boundary slice, not a type-introduction slice.** The N1 vocabulary
already exists as Rust types — `PrincipalKey`, `SubjectId`, `ContinuityRoot`,
`ContextNonce`, `DeviceGrant` — and every one of them sits **inside `icn-identity`
and reaches no other crate in the workspace**: `authority_log` has zero references
outside that crate. (Stated as a boundary fact rather than a crate count, which
would drift.)

The two remaining contracts are **not** in that position, and this document does not
pretend otherwise: **Institution** is carried by `EntityId` in a *different* crate
(`icn-entity`, and a verbatim duplicate under `apps/membership`), and **Node** has
**no durable identity domain at all** — `icn-kernel-api` declares
`pub type NodeId = String`, which that crate *does* use in its own
coordination/lease API, but which has no known consumer outside it and carries no
durable node-identity semantics (§3). So the work N2 names is moving meaning across
boundaries that already exist, and supplying the one domain that does not.

---

## 2. The semantic contracts

**Seven normative contracts, of which six are identity-bearing classes and one is a
protocol-data contract.** #2597 enumerates six classes. `ContinuityRoot` and
`ContextNonce` get separate contracts because one is secret and the other is public
protocol data (§10) — but only `ContinuityRoot` sits on the identity side.

To be unambiguous about what "seven" counts:

| | Contracts |
|---|---|
| **Identity-bearing** — names an actor, a human, a collective, an instance, or the secret material behind one | Principal · Human Subject · Institution · Node · ContinuityRoot · Device Principal |
| **Protocol data — names nothing** | **ContextNonce** |

Splitting the previously conflated continuity material in two is a **correction to a
boundary, not the addition of a seventh identity class.** A `ContextNonce` identifies
nothing whatsoever, and treating it as an identifier of anything is a category
error (§4).

### 2.1 Principal

**A cryptographic actor identified by a validated public / verifying key.**

Control of the corresponding secret signing material enables production of
signatures for that Principal. **Secret material is not part of the public
identity object.** "A Principal is a keypair" is rejected: it places secrets
inside a public identity and excludes a threshold group verifying key, which is
**one** Principal even though no single local keypair exists and no participant
can sign alone.

- **Domain** — `did:icn:<multibase-verifying-key>`.
- **Genesis** — anyone, offline, no registrar.
- **Authority** — a valid signature over a domain-separated canonical preimage,
  verifiable against the public key. Nothing else, ever.
- **Lifetime** — the key's. A rotated key is a *different* Principal; that is the
  point. Rotation is not a property of this class — it is an event in some
  subject's log that changes which Principals are authorized.
- **Publicity / correlation** — public to whoever verifies a signature, and
  **fully correlatable across contexts by construction**. This is precisely why a
  Principal must never be a durable human identifier.
- **Signing** — the only class that directly produces and verifies cryptographic
  authorship. Every other class acts by having a Principal sign an act
  authorized for it.
- **Maps to** — `PrincipalKey(VerifyingKey)`; `Did` via `from_public_key`.

### 2.2 Human Subject

**A human being, named within one context — institutional or otherwise.**

`SubjectId` is produced by N1's human-subject authority-log primitive and names a
**human subject only**.

> **On "context".** The examples throughout this document are institutional
> (membership in a cooperative, a community, a federation), because that is what
> ICN builds today. **The identifier construction does not impose that
> restriction**, and this document does not narrow it by prose: a context may
> later be bilateral, informal, community-level, federation-level, or another
> relationship **GEN (#2602) defines**. What is normative here is that a human
> Subject is scoped to *one* context and is never global — not what kinds of
> context may exist.

- **Domain** — `SubjectId = event_id(inception_body)`, 32 bytes, self-addressing
  over an event. **One subject per context, never global.** Nodes, institutions
  and accounts do not draw identifiers from this domain.
- **Genesis** — the human's own client/device (§6). Admission requires
  `σ == event_id(b)`.
- **Authority** — replay the authority log from inception. A Principal may act
  iff the derived view authorizes it at the referenced position. Never a
  resolver, never a registry, never a clock.
- **Lifetime** — survives key rotation, device replacement, total device loss and
  algorithm change. **Architecturally**: the identifier is fixed at inception and
  never depends on any live key, so none of those events can change it.
  **Surviving total device loss in practice additionally requires the person's
  continuity material to be recoverable**, and the protected backup / recovery /
  threshold-share protocol that would deliver that is **N7 (#2603) — not built**
  (§2.5). The property is a property of the design, not a shipped capability.
- **Publicity** — public **to that context only**.
- **Correlation** — never canonically. Linking two subjects to one human is
  voluntary and subject-initiated. No protocol party may hold the linkage.
- **Signing** — **cannot sign.** A subject has no key.
- **Maps to** — `authority_log::SubjectId`.

**Known architectural limit — historical authorization across a superseding
transition.** Signature verification survives forever: "key K signed these bytes"
stays checkable. *Authorization* proof may not. After a superseding recovery
orphans a branch, a receipt signed by a key first authorized on that branch keeps
a verifying signature but can lose its proof that K was authorized, if sufficient
historical authority evidence is unavailable. Durable-authorization guarantees
therefore hold unconditionally only for positions preceding the earliest fork.
**N2 does not solve this, and no downstream slice may inherit it implicitly.**

### 2.3 Institution / governed entity

**A governed collective, constituted by a charter and its governance rather than
by custody of a key.** Not a Human Subject; does not take a `SubjectId`. See §8.

- **Domain** — its own domain: `EntityId`, shaped `entity:icn:<kind>:<slug>`.
- **Genesis** — a founding act signed by founding Principals.
- **Authority** — a governance decision evidenced against authenticated state,
  never a signature by any single key.
- **Lifetime** — survives complete steward turnover; stewards change
  continuously, the identifier never does.
- **Publicity / correlation** — public and freely correlatable. Institutions are
  meant to be found. Their *members* are the privacy subjects.
- **Signing** — **cannot sign.** A steward Principal signs an act the
  institution's governance authorized. An institution must never appear in a
  signer position.

### 2.4 Node

**A running instance of infrastructure.** Not its operator, not the institution
it hosts, not the humans whose data it carries. Not a Human Subject; does not
take a `SubjectId`. See §9.

- **Domain** — its own durable node-identity domain, distinct from the node's
  current transport Principal. **It does not exist today.** `icn-kernel-api`
  declares `pub type NodeId = String` and **uses it within its own
  coordination/lease API** — as a lease `holder`, and as a group `leader` and
  `members` — but it has **no known consumer outside that crate** and carries **no
  durable node-identity semantics**. It is a `String` alias standing where a domain
  should be, not an unused one. Its confinement to a single crate bounds the blast
  radius of defining that domain; it does **not** by itself make renaming or
  removing the alias safe, and the coordination API that uses it is a real
  consumer to be considered.
- **Genesis** — the node operator, offline, at first boot.
- **Authority** — per connection, via the Hello three-fact certificate check.
  That authenticates a *Principal on a connection*. Relating that Principal to a
  durable node identity requires an **explicit binding object**.
- **Publicity / correlation** — public and freely correlatable; nodes must be
  dialable. A node identifier must never become a correlation handle for the
  humans it hosts.
- **Signing** — the node's current Principal signs transport envelopes **for
  itself only**. A node cannot sign for a human or an institution.

### 2.5 ContinuityRoot / secret continuity material

**Private secret material from which subject-context authority material may be
derived, plus the person's own index of their subjects.** See §10.

- **Domain** — opaque client-side secret. **No public identifier form exists or
  may be defined.**
- **Genesis** — client-side at first onboarding; phone-only onboarding must
  produce this by default.
- **Correlation** — *it is* the correlation. That is why it stays local: the
  linkage exists and no protocol party holds it.
- **Signing** — does not sign protocol acts. It derives and recovers Principals
  that do.

**Admitted shortfall, not a discharged requirement:** compromise of the
continuity secret is a takeover and is unrecoverable under the default recovery
path. Recorded, not solved. The protected backup / recovery / threshold-share
protocol belongs to N7 (#2603).

### 2.6 ContextNonce

**Public context-specific inception input.** Not secret, not continuity material,
and **not an identifier of anything**. Its job is to break the genesis fixed
point: deriving the initial device key from `SubjectId` would require
`S = H(inception(KDF(secret, S), …))`, which is uncomputable. See §10.

### 2.7 Device Principal

**A Principal holding a key on one device, authorized to act for a human subject
within stated bounds.** Not a subject, and not a Principal class of its own.

- **Domain** — an ordinary `Did`. **Device-ness is not in the identifier — it is
  the grant.**
- **Genesis** — the device generates its own key. A device never receives another
  Principal's key.
- **Authority** — an `authorize` event in the subject's log naming this
  Principal, with capabilities and a validity span. **Attenuation is mandatory**:
  issued ⊆ issuer ∩ flow ∩ requested.
- **Lifetime** — bounded by the grant. Losing a device does not touch the subject.
- **Signing** — signs acts. The act names the subject it acts for; the receiver
  checks the grant. **A device signature alone never establishes subject
  authorship.**

---

## 3. Identifier domains

Five distinct domains. A value from one is never a value from another.

| Domain | Names | Canonical / current type | Legacy or ambiguous forms |
|---|---|---|---|
| **Cryptographic principal** | Principal, Device Principal | `PrincipalKey`; `Did` from `from_public_key` | `Did` from `from_str` / `from_anchor_id`; `icn-kernel-api` `Did = String` |
| **Context subject** | Human Subject | `authority_log::SubjectId` | `DidDocument.id`; `Anchor` / `Anchor::to_did`; `member_did`; vote-key voter; `Membership.member_id: EntityId` |
| **Governed entity** | Institution | `EntityId` | `coop_id: String`; `org_did: Did`; `EntityKind::Individual` |
| **Infrastructure / node** | Node | *(none yet — a durable node domain is undefined)* | `node_did: Did`; `operator_did` set equal to `node_did`; `NodeId = String` (used inside `icn-kernel-api`'s coordination/lease API; no known consumer outside it; no durable node-identity semantics) |
| **Account / resource** | Treasury and other governed accounts | *(none selected — see §12)* | `treasury_did` (`String`-shaped at 15 of 21 fields); `AccountId`; account keying by `Did` |

**Legacy marker.** `Did(String)` is a stringly-typed newtype that today stands in
for person, node, device, institution namespace and wire sender simultaneously.
Narrowing it to *cryptographic principal* is the single largest disambiguation
this contract requires.

**Ambiguous, explicitly marked.** `Anchor` and the anchor-derived DID hatch serve
four different semantic classes from one constructor; only one is person-shaped.
`AccountId` is a migration union, not an account namespace (§12).

---

## 4. Substitution rules

> **The default is a category error.** A substitution not listed as permitted
> below is a **category error**, not merely undocumented. This table is
> **exhaustive and normative**, not illustrative. Compatibility alone never
> licenses a substitution.

| From → To | Verdict | Reasoning |
|---|---|---|
| `Did` (key-derived) → Principal | **allowed — the one permitted direct conversion** | This is what `Did` means. Requires canonicalized encoding, or two encodings of one key are two unequal Principals. |
| `SubjectId` → `PrincipalKey` | **category error** | A subject has no key. Already enforced by N1: Principal fields carry a distinct tag, so subject bytes in a Principal field are a wire-level decode error. |
| `PrincipalKey` → `SubjectId` | **category error** | A Principal may be *named as an authorized key inside* a subject's log — membership in a set, not a conversion. |
| `NodeId` ↔ `SubjectId` | **category error** (both directions) | A node is not a human subject. |
| `InstitutionId` ↔ `SubjectId` | **category error** (both directions) | A person is not a governed collective, and vice versa. |
| `Did` (key-derived) → Subject | **forbidden** | Inception admission requires `σ == event_id(b)`; a `Did` encodes a verifying key. Barring a preimage they cannot coincide. The shortcut destroys inception uniqueness. |
| Principal → Institution | **forbidden** | The most dangerous substitution available today. |
| Institution → Principal | **category error** | Institutions hold no signing key. An institution authorizes; a Principal signs. |
| `AccountId` → Principal | **category error** | An account holds no key. |
| `AccountId` → `InstitutionId` | **forbidden** | An account is *governed by* an institution. |
| `Did` → `AccountId` | **forbidden as a blanket impl** | Retain only as an explicit, named legacy read path. |
| Device Principal → Subject | **forbidden** | A reinstall would otherwise produce a different human. |
| `ContinuityRoot` → any public object | **forbidden** | Secret material. Protected local persistence and future encrypted/threshold export are permitted (§10, I4a). |
| `ContextNonce` → public inception body | **required — not a substitution** | It is already in the canonical body by construction. The obligation is freshness, not concealment. |
| `ContextNonce` → identifier of anything | **category error** | It names nothing. Treating it as a subject or context identifier turns a correlation handle into an identity. |
| legacy `Did` → new context `SubjectId` | **bridge only — never a conversion** | An allocation. The legacy key must not become the new subject's initial public authority (§7). |
| `SubjectId` → legacy Principal | **sensitive, context-local** | Inherent transition-correlation data. Context-local, access-bounded, retention-limited, never exported, erasable. Not "safe". |
| `icn-authz::SubjectId` ↔ `authority_log::SubjectId` | **category error** | Same name, incompatible contracts. A rename to perform, not a conversion to define. |
| session-token subject → Subject | **forbidden** | A session is not authorship. |
| `String` → any class | **forbidden at new sites** | Existing untyped identity fields are inventoried legacy debt. No *new* identity field may be untyped where an established semantic type applies. |

**The rule this encodes.** Exactly one permitted direct conversion, because that
one is an identity rather than a translation. Everything else relating two
classes does so through an **explicit, verifiable object that names both** — an
inception body naming a Principal, a grant naming a device and a subject, a
binding naming a node and its transport Principal, a governance record naming an
institution and an account. **Objects, not casts.**

**Mechanism reuse is not type equivalence.** Two classes may one day be served by
the same authority-log and pre-rotation machinery. Those are properties of an
append-only authenticated log and mention no humans. That generality does **not**
make their identifiers interchangeable and does **not** license reusing
`SubjectId`. Generalizing N1's machinery beyond human subjects requires an
explicit generalized abstraction, designed and reviewed as such. **N2 does not
perform that generalization, and no later slice may perform it implicitly by
widening `SubjectId`'s meaning.**

**Named ≠ signed ≠ authorized.** Authorship is not authority: a verifying
signature proves K signed these bytes, never that K was authorized — two checks,
two types, never one function. Dependency does not imply sovereignty: hosting,
relaying, storing and routing are zero-authority relationships.

---

## 5. Context scope and correlation

1. **A human Subject is context-scoped.** One `SubjectId` per institutional
   context.
2. **There is no global public Person identifier, and none may be introduced.**
   Nothing needs one: authorship is provable against context-local authenticated
   state, and sybil resistance needs *uniqueness*, not *identification*.
3. **Publishing or reusing one `SubjectId` across institutions defeats the
   model.** The identifier is public to its context only.
4. **Cross-context linkage is voluntary and subject-initiated** where supported.
   No protocol party may hold the linkage; the only linkage that exists lives in
   the client-held `ContinuityRoot`.
5. **No global or service-visible forward index** from a legacy `Did` to the
   subjects it seeded.

**What N2 owns, and what it does not.** N2 defines the semantic *constraint*
above. It does **not** define the establishment protocol. In particular:

> **Nothing in the current inception body structurally binds a subject to a
> context.** Context-scoping is presently client discipline. This is stated
> honestly rather than papered over, and **GEN (#2602) owns the
> genesis/context-establishment protocol**, including what — if anything —
> should structurally bind a new Subject to its institutional context. That is
> downstream GEN work, not an N2 semantic defect. See §13.

---

## 6. Genesis authority — semantics only

> **Genesis authority semantics ≠ genesis protocol.** This section records *who
> or what may establish each contract* — all seven, including `ContextNonce`, whose
> establishment rule is a freshness obligation rather than an identity question
> (§10). It does not specify **how**. The protocol is GEN (#2602).

| Contract | Who may establish it |
|---|---|
| Principal | Anyone, offline, no registrar. |
| Human Subject | **The human's own client/device**, under fresh context-specific material. |
| Institution | A founding act signed by founding Principals. |
| Node | The node operator, offline, at first boot. |
| ContinuityRoot | The person's own client, at first onboarding. |
| ContextNonce | The generating client, freshly per context (§10). |
| Device Principal | The device itself, generating its own key; authorized by an event in the subject's log. |

**Human-subject genesis — normative.** An institution or gateway **MAY** provide
an invitation, a context identifier, enrollment parameters, a challenge, and
policy requirements. It **MUST NOT** manufacture the person's continuity root,
manufacture the person's fresh authority key, create or sign the person's
inception on their behalf, or silently assign an institution-controlled initial
Principal.

**Two distinct facts.** Recognizing a person in a context requires:

- **FACT A — personal continuity.** The legacy Principal authorizes and binds the
  newly created subject for this transition.
- **FACT B — institutional recognition.** The institution's authorized process
  recognizes that `SubjectId` as the successor for the legacy membership,
  standing or account context.

**A signature by the old principal proves FACT A only. It never proves FACT B.**
The authority separation is normative now; the evidence object that carries FACT
B is not specified here (§7, §13).

*Live enforcement gap, recorded not solved:* #2589 — invite redemption mints a
token for a caller-supplied DID with no proof of key control. The rule above
presupposes that gap closes. N2 does not implement it.

---

## 7. The legacy DID bridge

### 7.1 The contradiction that drives the design

A migration that names the person's existing `Did` as the new subject's initial
authorized key **defeats the privacy requirement it is meant to serve**. N1's
inception body carries the signer and initial authority **in cleartext**, and
verifying `σ == event_id(b)` requires the whole body — so the initial authority
cannot be withheld. One legacy `Did` seeding two contexts puts the same key in
both institutions' hands.

### 7.2 Bridge invariants — normative

A legacy key-derived `Did` **does not become a `SubjectId`.** The migration path
must:

1. **Create a new Subject identity** — each institutional context gets a fresh
   `SubjectId`.
2. **Use fresh context-specific principal material as that context's initial
   authority.** The legacy key is *not* the new subject's initial authority.
3. **Use legacy authority only as bridge evidence.** The legacy `Did` is treated
   as a **Principal only**, and its sole role in the migration is to authorize a
   transition/bridge-evidence object. **It does not thereby become an authorized
   Principal in the new Subject's authority log.** Prior existence of the key
   confers nothing: if that key is ever to act for the new Subject, it must be
   authorized by an ordinary `authorize` event carrying its own capabilities and
   validity span, on exactly the same terms as any other device Principal
   (§2.7) — and the default is that it is **not** so authorized. Signing the
   bridge evidence is **not** an enrolment.
4. **Bound disclosure.** That evidence is context-bounded and may be disclosed
   only to a context that *already holds* that legacy `Did` — then it reveals
   nothing new, and the historical correlation leak is **inherited rather than
   widened**.
5. **Preserve history rather than rewrite it.** Historical records remain
   historically truthful. Do not rewrite old signed records merely to replace the
   old DID. Historical credentials and membership are preserved.
6. **Terminate explicitly** (§7.3).
7. **Avoid widening cross-context correlation** — no global or service-visible
   forward index; the reverse mapping is sensitive (§4).

**Non-replication is policy, not cryptography.** A recipient that has seen bridge
evidence can copy, log, forward or replicate it. Non-replication is a protocol
and policy requirement on the *receiving context*, enforceable only by storage
and replication controls a future implementation must actually identify and
build. Stating it as a data-model property would be an overclaim.

### 7.3 Migration terminus

For a successfully migrated institutional context:

- The human has **one** active new `SubjectId` in that context.
- New membership, governance, credential-issuance, authorization decisions and
  mutable account bindings use the new semantic model.
- The legacy DID is **no longer an active authorization or membership
  identifier**.
- There **MUST NOT** be an indefinite period in which both the legacy DID and the
  new `SubjectId` independently authorize new acts for the same standing.
- Context-local bridge data may remain only as long as required to interpret
  historical records or satisfy explicit audit/retention requirements, and is
  not globally indexed, not used for new authorization, access-bounded,
  retention-limited and **erasable**.

### 7.4 Deliberately not specified here

**The bridge-evidence object's contents are not defined by this document.** What
that object must contain to establish FACT B is a membership-evidence question
owned downstream (N2-E2, and see §13). The *authority separation* is normative
now, and no downstream design may violate it while remaining conformant.

### 7.5 Hard migration gate — membership and vote re-keying

Vote storage is keyed by voter identity, so dual-keying would let the same
historical person create two counted rows. **Before any live membership or vote
re-key**, four things must be designed: migration ordering · alias/transition
recognition · duplicate-act prevention · final cutover. N2 does not solve
governance storage; it states the gate.

**Duplicate-act prevention is delivered** (#2641). Governance admission and
tallying resolve a voter to the bytes its `did:icn:` identifier decodes to, via
`icn-governance`'s `VotingPrincipal`, so one cryptographic voter contributes at
most one effective vote whatever multibase spelling names it. Rows for one
principal that express conflicting acts fail closed rather than electing a
survivor, because choosing between two conflicting historical acts is this
gate's business, not the duplicate-act guard's.

The two apps/governance admission paths reach that one-vote result under
**different recast policies**, and a consumer must not assume either from the
other:

* the in-process `GovernanceManager` **refuses** a repeat vote, constructing
  `GovernanceError::AlreadyVoted` via `ensure_has_not_voted`. Ballots are
  immutable on this path;
* the actor backend **permits a vote change**, which is its pre-existing
  behaviour, and supersedes the principal's existing row rather than adding one.
  A ballot is mutable on this path until the proposal closes.

Both keep exactly one stored row and one effective vote per principal, which is
what duplicate-act prevention requires; they differ on whether the voter may
change their mind, which is a governance-policy question this gate does not
settle. Where a principal already holds several pre-#2641 rows the actor refuses
without mutating, since it overwrites by spelling-keyed row and cannot supersede
them all at once.

The other three prerequisites — migration ordering, alias/transition
recognition, final cutover — remain undesigned, and **no persisted vote or
membership key has been re-keyed**.

In-memory membership *comparison* is a separate matter from the persisted
*keys* this gate governs, and it has moved. Since I7 landed (#2686), `Did`
equality is principal equality, so a static membership list — a `Vec<Did>`
tested with `contains` — now admits **any** accepted spelling of a listed
member. A previous revision of this section stated that an alias spelling is
refused at the membership gate and called that fail-closed; that described
pre-I7 behaviour and is no longer true.

This does not narrow the gate. Admitting an alias of a genuine member is the
correct reading of principal equality, and it is why the three remaining
prerequisites still matter: recognition at the comparison boundary is not
migration of the persisted keyspace, and nothing above re-keys a stored
membership or vote row.

---

## 8. Institution semantics

**Institution legitimacy is governance-derived.** Institution identity must not
reduce to:

- **one operator** — possession of infrastructure is not authority;
- **one human** — a person is not a governed collective;
- **one ordinary signing key** — `Did` models custody, and an institution is
  constituted by governance;
- **one FROST group** merely because a FROST group can act as **one Principal** —
  a threshold group is a Principal, and `Principal → Institution` is forbidden.
  Threshold custody removes single-custodian capture; it does not manufacture
  governance.

An institution **cannot occupy a signer position**. A steward Principal signs an
act the institution's governance authorized, and that signature is evidence
*about the act*, never constitutive of the institution.

**Legacy semantic debt, dispositioned but not scheduled.** `EntityKind::Individual`
places a person in the same enum as Cooperative/Community/Federation; the other
three variants carry required governance structure and it carries none. The debt
is deeper than one enum variant: `Membership.member_id: EntityId` embeds
person-as-Entity in membership *records* across three files, so retirement
touches stored membership history and is a later compatibility program, not a
rename. **No repo-wide migration is scheduled.**

**Open, not decided here.** Whether an institution additionally carries an
authority log over its stewards (O12), and whether an institution identifier
should commit to its genesis decision (O11), remain open. **O12 does not license
borrowing the human identifier.**

---

## 9. Node semantics

- **Node ≠ Subject.**
- **Node ≠ Institution.**
- **Operator ≠ Node.** These are collapsed today: operator and node are populated
  from the same value.
- **Authentication of a host does not establish political or institutional
  authority.** The Hello certificate check binds a Principal to a *connection* —
  per connection, not per peer. It is sound and unchanged by N2.

A future stable node identifier requires an **explicit binding** to its current
transport Principal; there is no conversion between them. **That binding cannot
be written merely because a host or operator possesses the machine** —
authorization for creating or updating it is a downstream protocol question.

**What withdrawing node-as-subject costs, recorded explicitly.** Node-as-subject
was previously proposed as the principled fix for node key rotation and
node/operator conflation, and as part of the answer to "does a restored node keep
its identifier?" (O2). Withdrawing node-as-Subject **also withdraws that proposed
solution**, and **N2 does not replace it with a finished node lifecycle
protocol.** Node key rotation remains unpersisted. O2 and O15 remain downstream
and open. Saying otherwise would trade a stated gap for a hidden one.

`NetworkMessage.from` is **the current sending Principal**. This lets #2480
proceed without node-subject machinery that does not exist.

---

## 10. ContinuityRoot and ContextNonce — the split

These are **two different things** and were previously conflated.

| | `ContinuityRoot` | `ContextNonce` |
|---|---|---|
| **Nature** | private continuity / recovery material | **public protocol data** |
| **In the canonical inception body?** | **never** | **yes, by construction** |
| **Names anything?** | no | **no** |
| **Obligation** | secrecy | **freshness and non-reuse** |

**`ContextNonce` is public.** It is declared in the inception body, written into
the canonical bytes, and read back on decode. It sits inside the preimage the
event id hashes, so it **cannot be withheld** from any relying party that
verifies `σ == event_id(b)`. Publication is intentional and needs no enforcement.

**Freshness rule — normative.** A `ContextNonce` **MUST** be generated
independently with cryptographically secure randomness for each intended context,
and **MUST NOT** be reused. **Deterministic derivation from a globally stable
public value is prohibited.** Any alternative generation method requires an
explicit unlinkability argument, stated and reviewed; it is not permitted by
default.

**Why reuse is worse than a correlation leak.** No randomness enters
establishment construction, and the generation seed is a hash over the context
nonce, the generation counter and the continuity root. Therefore, **for the same
continuity secret, reusing a nonce reproduces the same derived initial authority,
the same canonical inception body, and therefore the same `SubjectId`** — two
intended distinct identities **collapse into one**. (Scoped deliberately: for
*independent* continuity secrets, reuse leaks correlation without merging
identities.)

**`ContinuityRoot` must never enter canonical or public encoding.** Protected
local persistence, and future encrypted transfer, recovery or threshold sharing,
are permitted — N7 (#2603) owns that protocol.

**Implementation debt, not unresolved semantics.** The live `ContinuityRoot`
helper bundles the secret with the public `ContextNonce`, exposes an accessor
that populates the public inception body, renders the nonce in `Debug`, is
publicly re-exported, and carries a doc comment stating the whole bundled value
is never published — stale, now that `ContextNonce` is explicitly public. The
**semantic boundary above is correct** and the secret bytes genuinely remain
private; `ContinuityRoot` is not a wire type and has no canonical encoder. **A
structural Rust split is therefore NOT a prerequisite of this contract.** The
documentation and API-boundary cleanup is bounded implementation debt, owned by
N2-F′ (§14).

---

## 11. Type-level invariants

What future Rust work must enforce. No Rust is proposed here.

| # | Invariant | Enforcement |
|---|---|---|
| **I1** | Distinct types; **no blanket `From`/`Into` between any pair** of identity classes | Absence of impls. Violated today by blanket conversions into the account namespace. |
| **I2** | **A `SubjectId` cannot satisfy an API requiring a `PrincipalKey`**, at type level and on the wire | Already structurally enforced by N1: Principal fields encode a tag alongside the key and decode rejects a wrong tag; a `SubjectId` is written as bare bytes. **A `SubjectId` in a Principal field is a genuine decode error, not a probabilistic one.** Extend this to the other crates. |
| **I3** | An institution identifier and any account identifier **cannot occupy a signer position** | Type-level, not source scans: signer APIs accept only Principal-capable types, so a non-Principal is a *compile* error. |
| **I4a** | **Secret continuity material MUST NOT be representable** in ordinary public identity protocol bodies or canonical wire encodings. Protected local persistence and future threshold-share/recovery export are permitted | Must constrain the **actual canonical encoders and public wire body types**. Enforcement by "the type has no `serde` impl" is **vacuous** — N1's canonical codec is hand-written and never consults `serde`. |
| **I4b** | **`ContextNonce` is public, and must be independently fresh per context and never reused**; deterministic derivation from a globally stable public value is prohibited | A generation-site rule plus a reuse check. Publicity needs no enforcement — it is a fact of the encoding. |
| **I5** | Device authorization requires **explicit subject delegation evidence** — never inferred from a valid signature | Verification returns a **grant**, not a bool. |
| **I6** | Wire envelopes carry sender Principal and governed subject/institution in **separate, differently-typed fields** | Envelope type shape. |
| **I7** | **`Did` equality is key equality, not string equality** | Equality/hash over decoded bytes, or encoding pinned at parse. **Gated on the stored-key inventory** (N2-A0). |
| **I8** | **No unvalidated `Did` construction exists** | Remove the unchecked and anchor-derived constructors. This closes the deserialization-path defect. **It does NOT close** the consumer-side all-zero-issuer fallback, which survives constructor removal and needs its own owner. |
| **I9** | **New identity-bearing fields MUST NOT use untyped `String`** where an established semantic type applies | Mechanism deferred to an implementation slice. **A source-name scan is withdrawn as the normative mechanism** — identity-bearing meaning cannot be robustly inferred from variable names, and this repo's source-scan guards have a record of failing open. Prefer type-system elimination of escape hatches (I12). |
| **I10** | **No global or service-visible forward index** from a legacy `Did` to the subjects it seeded; the reverse mapping is sensitive, context-local, access-bounded, retention-limited, non-exported and erasable | Structural where possible; policy where not (§7.2). |
| **I11** | **Exactly one type in the workspace may be named `SubjectId`** | Rename the capability-subject type (N2-0). |
| **I12** | Identity-shaped `String` aliases must be removed or renamed so a typed identity field carries a real guarantee | Not one alias but a **family** — the kernel API declares sixteen identity-ish aliases as `String`. N2 does **not** broaden into a repo-wide rename; N2-H addresses the identity-bearing subset. |

---

## 12. Account / treasury boundary

**A treasury is not a separate identity class.** It is not a Principal, not the
Institution, and does not become a human or institutional identity merely because
legacy types permit `Did`-shaped representations.

Three things, three classes:

| Thing | Class | Identifier |
|---|---|---|
| The cooperative governing the account | **Institution** | `EntityId` |
| The account itself | **Account / resource — not an identity class** | **undetermined** |
| Whoever signs an act concerning it | **Principal** | `Did` |

**The code already half-encodes this.** `Treasury` carries the owning entity and
the account identifier **in the same struct, seven lines apart**; the charter type
repeats the pattern independently. The **owner slot is already filled correctly**;
the account slot is occupied by a legacy key-shaped mechanism.

**No target type is prescribed.** The candidate account domains each fail for a
stated reason — the existing `AccountId` union is a migration union with blanket
conversions **into** `AccountId` from **both** `Did` and `EntityId` (there is no
blanket conversion back out of `AccountId`), and an `is_individual()` that returns
true for any `Did`;
the capability-graph resource type is kind-tagged but scoped to addressing and
name-collides with a kernel `String` alias; and account keying by `Did` **is the
defect itself**. **Do not select an account domain merely because a type exists.**
N2-C′ (§14) is an investigation whose first deliverable answers this, before any
account-identifier field is retyped.

---

## 13. Downstream ownership and routing

Every boundary N2 sets, and who owns what remains.

| Issue | Lane | What N2 establishes (the boundary) | What remains downstream (the open question) |
|---|---|---|---|
| **#2598** | **N3** — authority-fact reconciliation | The classes N3 reconciles facts *about*; authorship ≠ authority | Byzantine reliable broadcast and scoped observation evidence |
| **#2599** | **N4** — device authorization | The Device Principal contract; attenuation is mandatory (I5) | Device authorization/rotation/revocation mechanics; subsumes #2588, #2590 |
| **#2605** | **N5** — member-origin envelope | Institution ≠ Principal; account ≠ institution; the FACT A / FACT B shape; the historical-authorization limit (§2.2) it **must not inherit implicitly** | The canonical member-origin action envelope and authority proof. **Blocked** |
| **#2600** | **G1** — process admission / commit | Institutions cannot sign; institutional authority is governance-derived and evidenced against authenticated state (§8) | Deterministic process admission, input closure, and **conflict-safe institutional commit**. G1 is *not* N3 branch selection |
| **#2601** | **TIME** — temporal evidence | Subject authority is resolved by **log replay, never a clock** (§2.2) | Deadlines, leases, causal vs wall-clock semantics. TIME is *not* a sovereign ordering service |
| **#2602** | **GEN** — genesis / establishment | **Genesis *authority* semantics for all seven N2 semantic contracts** (§6): who may establish each, and that an institution never mints a person's subject | **The genesis/context-establishment *protocol*** — including the unresolved question of **what structurally binds a new Subject to its institutional context** (§5), and the institutional-recognition (FACT B) evidence object. GEN is *not* a global person registry |
| **#2603** | **N7** — recovery | The `ContinuityRoot` contract, including that protected export and threshold sharing are **permitted** (§10) | The protected backup / recovery / threshold-share protocol, and the continuity-secret-compromise shortfall (§2.5) |
| **#2604** | **O-N5/O-N8** — replication topology | Subjects are per-context; topic naming is a correlation surface (§5) | Authority-log replication topology and adversarial storage bounds |
| **#2606** | **O-N7** — finality under forks | The historical-authorization limit across superseding transitions (§2.2) — the precise fact O-N7 must reason about | **Policy-scoped finality evidence for irreversible effects under authority forks.** N2 states the limit; it does not bound it |

**Coordination, not ownership.** #2480 (sender identity — N2 keeps it shippable
now via §9), #2469 (authorship ≠ authority at the substrate), #2589 (the live
enforcement gap §6 presupposes will close), #2441 (authenticated standing, which
#2605 cannot be built before), #2448, #2591, #2613.

**Explicitly not smuggled into N2.** Byzantine reliable broadcast · device
authorization mechanics · member-origin envelopes · protected continuity
backup/recovery · replication topology · transport protocol changes · governance
storage · the node rotation lifecycle (O2/O15) · O12 · any migration execution ·
**any generalization of N1's machinery beyond human subjects**.

---

## 14. Implementation decomposition

**This is a DAG, not a sequence, and nothing here is scheduled.** Several roots
are independently startable. **A repo-wide identity rename is not the plan** and
never was — no single change retypes the untyped identity fields en masse.

```text
                 N2-G'  persist this contract  (this document)
                        gates any code that CITES the contract
                                   │
      ┌──────────┬─────────────────┼─────────────────┬──────────────┐
      │          │                 │                 │              │
   N2-0       N2-F'             N2-A0             N2-C'          N2-E1
  authz    continuity        STORED-KEY         account-        bridge
 SubjectId   boundary         INVENTORY          domain      PROHIBITIONS
  rename    (I4a/I4b)        [HARD GATE]      investigation  (specifiable
  8 files                          │                │             now)
                                   ▼                │              │
                                 N2-A  ◄────────────┘              ▼
                            Did canonicalization   (informs which  N2-E2
                                  (I7)              keys are        bridge
                                   │                account vs      EVIDENCE
                                   ▼                principal)      object
                                 N2-B                              [BLOCKED]
                           principal-only Did
                                   │
                        ┌──────────┴──────────┐
                        ▼                     ▼
                      N2-H                  N2-D
                 kernel String-Did      node / sending
                     aliases              principal
```

| Node | Risk | Scope |
|---|---|---|
| **N2-0** | low | Rename the capability-subject `SubjectId` → `CapabilitySubjectId`. **8 files**, including the public error variant — an API surface beyond the type name. Zero known external consumers. Serialization is transparent, and **the numeric hash domain tag is FROZEN**, so a Rust rename does not move it. |
| **N2-F′** | low | Implement I4a/I4b against the **real canonical codec**. Plus the §10 documentation/API cleanup: correct the stale doc comment, document the nonce accessor as returning *public* inception data, make the secret/public boundary unambiguous, ensure canonical and public encoders cannot receive secret material, preserve protected local persistence. **Requires the §10 split; does NOT require complete N7.** Whether a clearer structural split helps is decided *during* implementation and is **not a prerequisite**. |
| **N2-A0** | **HARD GATE** | **Stored-key inventory.** `Did` derives equality and hashing over its inner string, and live `Did`-keyed maps and persisted keyspaces exist, so canonicalization **can silently merge previously distinct persisted rows**. Inventory every persisted `Did`-keyed store **before I7 is attempted**. |
| **N2-A** | high | `Did` canonicalization (I7). **Blocked on N2-A0.** |
| **N2-B** | high | Principal-only `Did`: remove the unvalidated constructors. Anchor-derived DIDs are persisted and returned in API responses, so compatibility-only reads come first. Closes the deserialization defect; **does not close** the consumer-side fallback (I8). |
| **N2-C′** | investigation | Account domain. **No retyping.** Must consider all five inputs (§12) before any account field is touched. |
| **N2-D** | high | Node / sending-principal boundary. Prefer the operator/node split and documentation first, with no wire change. Coordinates with #2480. |
| **N2-E1** | low | **Bridge prohibitions — specifiable and persistable now, before any migration:** no global forward index · bounded disclosure · no unauthorized replication or export · sensitive retention and erasability. **Precedes N2-E2.** |
| **N2-E2** | **blocked** | The bridge evidence object. Requires FACT A / FACT B and migration semantics (§7.4). |
| **N2-H** | medium | The identity-bearing subset of the kernel `String` alias family (I12). |

**Separate roots — not N2 slices, no N2 dependency:** the consumer-side all-zero
issuer fallback · re-key ordering and duplicate prevention (**a hard gate before
any live membership or vote re-key**, §7.5) · the broad kernel meaning-firewall
debt, which is distinct from N2-H — same crate is not the same defect.

**No fundamental semantic reopening during implementation.** The classes,
substitution matrix, bridge invariants and DAG direction are settled. An
implementation slice that finds itself needing to change one of them has found a
contract defect and must say so explicitly, not widen a type quietly.

---

## 15. Provenance

Produced by #2597 across three design passes and **two independent
fresh-context adversarial reviews**. The first returned *revision required*
(three blocking defects, six required revisions); all nine were re-derived
against live `origin/main` and repaired. The second returned **N2 CONTRACT
CLOSED** — zero blocking findings, zero reopened semantic classes, no
substitution-rule, bridge-invariant or migration-terminus defect, and an acyclic
implementation DAG — with two nonblocking findings, both discharged by this
document: the criterion-9 truthfulness correction, and the downstream routing
table in §13.

Attacks the contract closed: a global human identifier · human ≡ public key ·
host/node owning a subject · a cooperative master key, including
FROST-group-as-institution · device enrollment as subject creation · continuity
exposure · old-DID reuse · authentication implying institutional authority ·
cross-institution correlation · bypassing N3/N4/N5 · claiming migration or
deployment exists. **The substitution matrix's default-is-category-error rule
(§4) is what makes unlisted pairs safe.**

Everything in §1.2 that is marked absent remains absent.
