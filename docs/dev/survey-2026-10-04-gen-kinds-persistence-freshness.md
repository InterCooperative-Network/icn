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
> Title of record: Survey: GEN context kinds for personal/household contexts (#2602), N1-D/N3 persistence and delivery (#2800, #2598), and the class-2 admission position (G1-A).

> **Convergence note (2026-10-04).** Mentions below of `DeviceAuthorityBundleV1`,
> `icn.n4.device-authority-bundle`, `MAX_BUNDLE_FACTS`, `verify_bundle` / `store_from_bundle`,
> `device_authority_bundle.rs` and its tests describe N4-B as it stood when this record was
> written. N4-B has since been converged on `EvidenceBundle` (`icn.n4.evidence-bundle`,
> `evidence_bundle.rs`; `N4B_PORTABLE_EVIDENCE_BUNDLE.md` §11). Read those references as
> historical; the text is preserved as written.

# Survey: GEN context kinds, persistence/delivery, evaluation position

Read-only survey of the ICN monorepo on icn-dev, 2026-10-03/04.
No files modified, no cargo run. Everything below is cited to file:line or doc section.

## Provenance

| What | Where | State |
|---|---|---|
| Survey checkout | `~/icn-dev/worktrees/icn/katie-home-identity-integration`, branch `task/katie-home-identity-integration` | HEAD `b098396b5`, 46 commits ahead of `main` (`5add7a48d`, which is an ancestor) |
| N1-D (PR #2800) | `~/icn-dev/worktrees/icn/task-2694-n1d` @ `b871dafdc` | PR OPEN; `fact_store.rs` is **not** on `main` and **not** on the katie branch |
| Uncommitted work on the katie branch | `icn/crates/icn-identity/src/device_channel_binding.rs` (untracked), `tests/device_channel_binding.rs` (untracked), `src/lib.rs` (modified, adds `pub mod device_channel_binding;` L28) | another session's WIP (Home profile §6.5 TLS binding, routed #2599); **not surveyed** |
| `gh` on the VM | works for issues #2602, #2598, #2694, #2599 and PR #2800 | live text used |

Paths below are relative to the repo root; Rust under `icn/crates/`.

---

## A. Context kinds (GEN, #2602)

### A.0 Sources read

- `docs/architecture/GEN_SUBJECT_CONTEXT_GENESIS.md` (427 lines, all of it; status `normative`, canonical `no`)
- `icn/crates/icn-identity/src/subject_context.rs` (724 lines, all of it)
- `icn/crates/icn-identity/tests/gen_subject_context.rs` (test names + the shape/relabel tests)
- `icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py` (kind constant L71, use at L90, L123)
- `docs/architecture/HOME_RUNTIME_IDENTITY_PROFILE.md` (264 lines, all; status `proposed`)
- `docs/adr/ADR-0083-institutional-domain-and-domain-policy-runtime-root.md` L1-140 (status `proposed`, `implementation_status: not-started`)
- `docs/architecture/IDENTITY_SEMANTICS.md` (IS; canonical) §1.1, §2.2, §2.6, §2.7, §5, §8, §10, §13
- `docs/architecture/HUMAN_IDENTITY_ARCHITECTURE.md` (HIA) §11.1 L1605-1625, R1 L195-210, §12.2
- issue #2602 (full), #2599 invariants, handoff `docs/dev/handoff-2026-10-03-katie-home-identity-integration.md` (grep)

### A.1 What a new kind tag changes mechanically

The kind tag enters exactly two GEN preimages and, through one of them, a third reference:

| Preimage | Code | Contains `u8(kind.tag())`? |
|---|---|---|
| `context_preimage` → `context_nonce` | `subject_context.rs:323-336` (`w.u8(self.kind.tag())` at L327) | yes |
| `subject_context_ref_preimage` | `subject_context.rs:339-348` (L343) | yes |
| `initial_device_binding_ref_preimage` | `subject_context.rs:352-362` | not directly; it commits `subject_context_ref` (L359), which does |

The verifier reconstructs the descriptor from `bundle.context_kind` by struct literal (`subject_context.rs:592-596`, module-private field access), so verification already works for any variant the enum has. The kind-mismatch check at `subject_context.rs:581-583` exists today and is **unreachable with one variant**; the comment at L577-580 says it was "kept rather than deferred so that adding a v2 context kind cannot silently let a bundle of one kind satisfy a claim about another". The code was written to be extended here.

**N1 is unaffected.** The only thing N1 receives is the 32-byte `ContextNonce` (`authority_log/body.rs:105`, written at L641, read at L700). IS §2.6: the nonce is "not an identifier of anything"; IS §10: "public protocol data", N1 does not interpret it. GEN doc §1 L24-27: "No N1 wire format, canonical body, signature preimage or digest rule is altered or extended." A second kind changes which 32 bytes go in; the inception body's shape, admission (`admissible_bytes`), `derive`, `derive_prefix` and `MAX_POSITION` never see a kind.

**N4-A is unaffected.** `verify_device_act(signed, subject, store, position)` (`device_authority.rs:384-406`) and `evaluate_device_authority` (L302-348) take no context input at all. N4-A doc §10 L398-405 lists "any new context kind for personal or household use (GEN, #2602)" as out of scope precisely because nothing in it depends on the kind; Home profile §4 L132: "N4-A evaluates identically whatever the kind." Likewise N4-B (bundle carries N1 facts only; §8 L182-184 excludes "any new context kind") and N4-C (request names a `SubjectId` only; §7 L143).

**Two mechanical consequences that are not free:**

1. **`context_id` is a `String`** (`subject_context.rs:283`) and the preimage grammar is `LP(context_id_utf8)` (GEN §6.1 L165). The Home profile proposes that for `PersonalContextV1` and `HouseholdContextV1` the context id is "an opaque 32-byte descriptor digest" (§4 L136-137, L141-142). Raw 32 bytes are not valid UTF-8 in general. Either (a) encode the digest as lowercase hex (64 ASCII bytes) and validate the form in the kind's constructor, leaving the preimage grammar and the `String` field untouched; or (b) change the field to bytes with per-kind validation, which touches the v1 vector and the Python reference's `lp(context_id.encode())` assumption. (a) is the zero-grammar-change option. This is a decision #2602 has to state in the doc; it is not in the Home profile.
2. **Vectors and the reference.** GEN §8 L346-357: the pinned literals "were produced by an independent reference implementation written from this document" and the reference is self-checking. Adding a kind means a new §8 vector row per kind and a new constant in `gen_subject_context_reference.py` (it currently hardcodes `KIND_GOVERNANCE_DOMAIN_V1 = 0x01` at L71). The discipline is doc → reference → Rust literal, in that order.

**Versioning.** GEN §5 L104-106: "The kind tag is inside every preimage, so a later GEN *version* may define further kinds without disturbing this one." The §4 table labels the constants "frozen for v1". Mechanically no version bump is needed: `GEN_CONTEXT_VERSION = 1` stays in the preimage, `0x01` bundles are byte-identical, and a `0x02` bundle is a different preimage. The verifier's `version == 1` check (L574-576) would have to grow to accept a `2` if the doc chose to bump. Recommend: keep v1, state in §5 that v1 defines three kinds, and record that decision explicitly; the doc's wording currently leaves it ambiguous.

### A.2 Contract change needing an ADR, or an anticipated extension?

Evidence:

- **No ADR exists for GEN-A or N4-A.** `grep -li "GEN-A|subject_context|SubjectContextGenesis|N4-A|device_authority" docs/adr/*.md` returns nothing. Both landed as normative architecture docs plus an issue (#2695, #2599) and a PR with an independent reference. That is the established form for identity-layer contracts.
- **The canonical document already delegates kinds to GEN.** IS §2.2 L125-132: "a context may later be bilateral, informal, community-level, federation-level, or another relationship GEN (#2602) defines. What is normative here is that a human Subject is scoped to one context and is never global — not what kinds of context may exist." IS §13 L699: #2602 owns "the genesis/context-establishment protocol". So adding kinds changes **no canonical contract**; IS does not need editing.
- **The GEN doc anticipates it** (§5 L104-106, quoted above) and the verifier was built for it (`subject_context.rs:577-583`).
- **ADR-0083 is not in the path.** It admits `Individual` as an owning entity class for `InstitutionalDomain` (L88-92). Home profile §4 L130 rejects that route for personal/household use (makes a person an owning entity class, extends the person-as-Entity debt IS §8 L538-544, puts a `governance:write` bearer in the genesis path against HIA R1.1 [HARD] L201). ADR-0083 lives in the governance app layer (L81-83); GEN kinds live in the kernel crate. They do not collide so long as nothing claims a Personal context *is* an `InstitutionalDomain`. #2806 tracks ADR-0083's status drift separately.
- **IS §13 L710-711** forbids "any generalization of N1's machinery beyond human subjects". A Household kind does not do that: a household never gets a Subject; each human incepts their own Subject *into* the household context (Home §4 L148-150). That is the line to hold in the doc text.

Verdict: **an extension the GEN doc's own text anticipates, under #2602's ownership, in the form the project already uses (doc revision + vectors + code + issue routing). No ADR by precedent.** What *is* a design decision rather than mechanics: the byte form and meaning of `context_id` per kind (A.1 item 1), whether v1 stays v1, and the Household agreement ceremony (A.3). Those must be written into GEN §5 before the reference is extended.

Caveat: #2602 is labelled `status:needs-design` and its operating rules say "brief before coding". The Home profile is `Status: proposed` and says of its own shape "for #2602 to decide (not decided here)" (L134). The handoff (L114) and the Home profile (§7 row 4) both already committed to "new GEN kinds, not a `GovernanceDomainV1` string". The *direction* is settled; the *bytes* are not written down yet.

### A.3 Household membership (GEN-B) and whether the kind can exist first

**What GEN-B is today.** `SubjectRecognitionV1` is defined only in #2694 §3 as an *institutional* fact: `domain_id + SubjectId + standing_class + recognition_basis_kind + recognition_basis_hash → recognition_hash`. Grep across `docs/architecture/*.md`: it appears only in GEN §10 (L417, scope exclusion) and Home §4 L143. Nothing in code. The Home profile proposes household membership as "mutual recognition between members' Subjects (the GEN-B recognition shape, bilateral), never an `AuthorityGrant`, never a steward" (§4 L142-144). That bilateral variant (Subject recognizes Subject, no `domain_id`) is **undesigned**; the institutional variant is **unbuilt** (#2694 rung 3).

**Can a Household context exist before recognition exists? Yes.** GEN §2 L36-42: a verified genesis "does not establish ... membership or standing"; "each of those is a separate, later fact". A `HouseholdContextV1` context is a `(kind, context_id)`; each human incepts one Subject into it with their own fresh salt (§5.3 one Subject per context is a client invariant). Two Subjects in one context publish unlinkable nonces (§5.2 argument, kind-independent). Without recognition there is simply no fact that says *S_Katie* and *S_Matt* are co-members, which is exactly the state of two coop members before GEN-B today. The relying parties that matter for the Home slice (the Pi, the hosted workstation) evaluate **device** acts with N4-A over each Subject's own log; co-membership is not an input to that (N4-A §2 L66: recognition "by any institution (GEN-B / N5)" is explicitly not proven and not needed).

**What is missing for Household specifically:** how two clients agree on one `context_id`. Home §4 L141-142: "the genesis digest of a household descriptor the founding members' clients agree on". No descriptor format and no two-party agreement step exist. For GEN-A's purposes the `context_id` is opaque; the agreement is a ceremony (carrier: the same kind of QR/link/local-network path N4-C step 4 L40 leaves unbuilt). Owner: #2602 (GEN item 4 "recognition" / a child issue), not GEN-A.

### A.4 Classification of the three fixture contexts

| Context | Class | Kind | Which ICN object owns it | Notes |
|---|---|---|---|---|
| **Private** (one human) | personal | `PersonalContextV1` | **none.** No ICN object exists for it; the context is a `(kind, context_id)` the person's client minted and keeps (Home §4 L136-139). The only "owner" is the person's client-held `ContinuityRoot` (IS §10; HIA R1.2) | never published beyond the person's own devices; no registrar, founder or charter |
| **Household** (two humans, no governance object) | interpersonal | `HouseholdContextV1` | **none.** No charter, steward, treasury or `GovernanceDomain` row (Home §4 L141-146; §5 row 3 L165: a household mints no `AuthorityGrant`) | membership = future bilateral recognition (A.3); each human has their own Subject in it; shared decisions are "recognition and app-layer policy, not grants" (L165) |
| **Pet Care** (one-person business today) | personal **today**, organizational **only after institutional genesis** | `PersonalContextV1` today, as a *second* personal context with its own salt and id, distinct from Private | **none today.** Home §4 L138-139: "*Private* and *a person's own small project* are both this kind; if a project later takes on members it becomes a household or an institution through genesis, not by relabelling" | if it becomes a cooperative: GEN item 2 institutional genesis (#2602) produces a governed domain; Matt incepts a **new** Subject in that `GovernanceDomainV1` context; the Pet Care personal Subject is not promoted; the only link is the `ContinuityRoot` (IS §5 L336-338). No bridge is defined and none is needed now, because no institutional fact exists yet. Any acts recorded in the personal context do not become institutional facts |

Why Pet Care must not be a `GovernanceDomainV1` from day one: Home §4 L131 ("correct bytes, wrong meaning; nothing could later distinguish a household from a domain"); IS §8 L523-524 ("one human — a person is not a governed collective"); and the Individual-owned `InstitutionalDomain` route is rejected at L130.

### A.5 One device, three contexts, three Principals, nothing links them

Confirmed from four independent places:

- HIA §11.1 L1612-1614: "Device keys must be per-context too. If one device key appears in two contexts, the identifier separation is defeated. Per-context device keys are a deterministic derivation from the device's own secret."
- Home profile §5 row 1 L163: "The Pi, with three contexts, holds three device Principals derived from one local device secret. The derivation is client-side and must not take `SubjectId` as input." §6.1 L182.
- N4-C §5 L94: "a device joining three contexts presents three requests with three keys"; §6 L129-132: "Nothing links them except the person's own `ContinuityRoot`, which never appears in any request, fact or bundle."
- GEN: the three preimages (A.1 table) contain only `(kind, context_id, salt, inception_event_id)`; no cross-context field exists. §9.2 L398-400: holding one bundle "does not help recognize or link a Subject in another context."

So the device holds three `PrincipalKey`s, sends three `EnrollmentRequestV1`s naming three different `SubjectId`s, and receives three `Authorize` facts in three logs. GEN, N1, N4-A/B/C contain no object that relates them.

**What is missing here:** the per-context device key derivation helper is DOC-ONLY (Home row b′ L57: "no derivation helper exists in code"); N4-C §2 row 2 L38: "derivation DOC-ONLY, client-side"; N4-C §4 L84-86 admits the request shape "does not prevent a misbehaving device" from reusing one key. Rule it must obey: HIA L1616-1618 (no `SubjectId` input). No issue owns it by name; it belongs with the device lifecycle (#2599).

### A.6 Recommendation for A

**Implement the tags now, on the branch, in one PR that revises the GEN doc first** — with one explicit decision written down before code. The architecture is settled enough for the *tags*: three committed documents (Home profile §4/§7, the handoff L114, N4-A/N4-B/N4-C scope sections) already say "a new GEN kind, never a `GovernanceDomainV1` string", IS delegates kinds to GEN, and the verifier was built with the mismatch check waiting. What is *not* settled is representable without deciding it: make `context_id` for both new kinds an **opaque 32-byte value carried as 64 lowercase hex ASCII bytes** (preimage grammar unchanged, byte-exact comparison unchanged), validated at the constructor, and push "what those 32 bytes digest" to the client. That keeps GEN-A ignorant of descriptor contents, exactly as it is ignorant of what a `GovernanceDomainId` string means.

Smallest sound piece (est. ~150 lines Rust + doc rows + two Python constants):

1. GEN doc §5: add `0x02 = PersonalContextV1`, `0x03 = HouseholdContextV1`; the `context_id` form for each (hex32); the sentence that a household never has a Subject of its own; the statement that v1 stays v1; §8: one vector row per new kind, produced by the reference first.
2. `gen_subject_context_reference.py`: `KIND_PERSONAL_V1 = 0x02`, `KIND_HOUSEHOLD_V1 = 0x03`, two more derivations.
3. `subject_context.rs`: two enum variants and tags (L180-192); `SubjectContextDescriptor::personal_v1(context_id_hex: &str, salt)` and `::household_v1(...)` (beside L293-305) refusing anything but 64 lowercase hex chars; no change to `incept_*` or `verify_*`.
4. `tests/gen_subject_context.rs`: (a) the kind-mismatch refusal is now reachable — a `GovernanceDomainV1` bundle claimed as `PersonalContextV1` → `ContextKindMismatch`; (b) same `context_id` bytes under two kinds → different nonce, different Subject, different `subject_context_ref` (cross-kind separation); (c) a Household genesis for two Subjects with two salts → unlinkable nonces (reuse of the existing §5.2 test shape); (d) constructor rejects a non-hex / wrong-length id for the new kinds; (e) pinned literals for the two new vectors.

If Matt wants the "brief before coding" rule honoured to the letter, the same content minus steps 3-4 is the #2602 comment, and the code follows on a yes. Either way the Home deployment is not blocked (Home §4 L152-155: the device proof is context-agnostic and runs on a fixture `GovernanceDomainV1`; no production Private/Household context is to be created until the kind exists).

### A.7 Missing pieces and owners (A)

| Missing | Owner |
|---|---|
| `PersonalContextV1` / `HouseholdContextV1` tags, constructors, vectors, reference, doc §5/§8 | #2602 (GEN-A extension) |
| `context_id` byte form for non-domain kinds (hex32 recommended) and the v1-stays-v1 decision | #2602 (doc decision) |
| Household descriptor agreement (two clients, one `context_id`) ceremony | #2602 (GEN item 4 / child); carrier is the same unbuilt N4-C step 4 transport |
| Bilateral `SubjectRecognitionV1` (household co-membership) | #2602 GEN-B; institutional shape in #2694 §3 (rung 3), bilateral variant undesigned |
| Per-context device key derivation helper (no `SubjectId` input) | HIA §11.1; route #2599 |
| Client-side subject index (refuse a second genesis per context, GEN §5.3; retain salts for recovery §5.1) | no issue names it; N7 #2603 for the retained material, otherwise the client app |
| Pet Care → cooperative: institutional genesis | #2602 GEN item 2; nothing needed now |

---

## B. Persistence and delivery (N1-D #2800, N3 #2598)

### B.0 Sources read

- PR #2800 description (full, via `gh`), file list, and the code in worktree `task-2694-n1d`: `icn/crates/icn-identity/src/authority_log/fact_store.rs` (243 lines, all), `icn/crates/icn-core/src/authority_facts.rs` L1-70, `authority_log/mod.rs` exports L213, L233-234
- `icn/crates/icn-identity/src/authority_log/store.rs` L21-160; `derive.rs` L425-520; `mod.rs` L1-60, L260
- `icn/crates/icn-identity/src/device_authority_bundle.rs` (header, record, `decode`, `store_from_bundle`, `verify_bundle`, `verify_bundle_bytes` L291-327)
- `docs/architecture/N4B_DEVICE_AUTHORITY_BUNDLE.md` (all), `N4C_DEVICE_ENROLLMENT_REQUEST.md` (all), `N4A_DEVICE_AUTHORITY_EVALUATION.md` §2, §5, §7, §9, §10
- HIA §9.2.1 L1006-1110 (the "filter commutes with union" text is HIA L1075-1077, not IS; IS has no §9.2.1 — IS §9 is "Node semantics")
- `tests/device_lifecycle_acceptance.rs` L1-45, L160-260
- issue #2598 (full)

### B.1 Record layout, and whether N4-B's equals it

N1-D (`fact_store.rs:26-29`, `PersistedFact::key` L118-123, `::value` L125-128):

```text
key   = event_id (32) || signature (64)       -- this IS the N1 `Witness` (store.rs:39)
value = canonical_bytes(body)
```

N4-B (`N4B §2 L54-57`; `device_authority_bundle.rs:51-54`, emitted at L209-211):

```text
fact := b32(event_id) || b64(witness_signature) || LP(canonical_body)
```

**They are the same record; the only difference is stream framing.** `N4B_fact == N1D_key || LP(N1D_value)`. The 96-byte prefix is byte-identical; N4-B adds `u32be(len)` because it is a byte stream, whereas a kv store delimits rows itself. N4-B §2 L74-76 and N4-A §9.2 L165-167 call this "one framing"; precisely, it is one record with two framings, and the mapping `PersistedFact::from_key_value(&fact[..96], body)` / `key() || LP(value())` is a bijection.

The validation gates are equivalent as well: N1-D `into_admitted` (`fact_store.rs:155-161`) runs `admissible_bytes` then checks `body.event_id() == key.event_id` (`KeyDisagreesWithValue`); N4-B `decode` checks `event_id == SHA-256(body)` (L243) and canonical re-encode (`BundleError` L113), then `store_from_bundle` (L291-304) runs `AuthorityStore::ingest` → `admissible`. Neither has a privileged path; both fail closed (N1-D `rehydrate` L185-192 on the first bad record; N4-B on the first inadmissible fact, naming its index).

Ordering: N4-B requires strictly ascending `(event_id, witness)` (L59). N1-D's `InMemoryAuthorityFactStore` is a `BTreeMap<Vec<u8>, _>` keyed by `event_id || signature` (L199-201), so `load_facts()` already yields N4-B's canonical order; a sled prefix scan is lexicographic by key too. The trait does not promise order (L173-177) and does not need to (`AuthorityBody: Ord` over canonical bytes), but the coincidence means a fact-set container can be emitted straight from `load_facts()` without a sort.

Status: PR #2800 is OPEN; `fact_store.rs` is absent from `main` and from the katie branch (`ls` fails). `icnd::build_services` wiring is explicitly deferred to #2777 ("data-root ownership/exclusion boundary"; PR body "Follow-up"). The Home profile row (f) L63 and §6.3 L204 both say PROPOSED and "once merged".

### B.2 "Delivery to a newly enrolled device" in this model

What the device needs, from N4-C §6 L118: "its secret, its per-context Principals, and the facts it has received." Why it needs the facts and not only its own `Authorize`:

- To **act**, it must bind an `evaluation_position` the relying party will hold (`device_authority.rs:394-399` requires `act.E == relying party's E`; see C). It cannot know a sound E without the prefix.
- To **check its own grant is live** (`derive` needs the inception and every position up to its grant: `derive_indexed` L476-520 walks from position 1).
- In the Home topology the hosted workstation / Pi is itself a **relying party** for the other devices' acts (N4-C §6 L115-118: "the Home runtime ... stores public N1 facts in the N1-D layout ... builds an N4-B bundle and runs the N4-A verifier"), so it needs every fact for every Subject it serves (Home §6.3 L204).

Who sends: the **authority edge** (the phone) authors the `Authorize` (N4-C §2 step 8 L44) and is the only party that has it first. Step 9 L45: "the fact becomes durable at the edge (N1-D store, #2800) and reaches the device and relying parties — transport — N1-D PROPOSED; delivery not built." N4-C §7 L140-141 and `device_enrollment.rs:37-38` exclude "delivery of the resulting fact back to the device or to relying parties (N3/N4 transport)". Home §6.2 L194: "MISSING — delivery/transport".

Push/pull primitive: **none.** N3a (#2598) owns "dissemination of canonical N1 bodies and retained witnesses; ... gap detection and missing-fact requests" and is `needs-design` with nothing in code. The only existing cross-process carrier is the N4-B bundle read from a **file** by `icnctl device-authority verify` (N4B §4 L107). The legacy `sync.rs` (`IdentityUpdateMessage`, `IDENTITY_UPDATES_TOPIC`, `lib.rs:98`) is DID-document gossip for the superseded path (Home row (j) EXPERIMENTAL, HIA F8) and must not be repurposed for N1 facts.

**The bundle is the carrier, but it cannot carry facts alone.** `DeviceAuthorityBundleV1::decode` requires an act (`device_authority_bundle.rs:255-256`: `DeviceActV1::decode(r.lp("act")?)`, then `act_signature`), and `new(facts, act)` takes one (L178-188). Delivering facts to a device that has no grant yet would mean fabricating an act, which is a signed statement by a device and so a semantic lie. Hence a facts-only container is genuinely the missing object, not a convenience.

### B.3 How revocations propagate

Mechanically: `Revoke` is one more N1 body. `AuthorityStore::join` (`store.rs:116-127`) is componentwise set union, "associative, commutative, idempotent and monotone" (L113-115; HIA §9.2.1 L1053-1056). Admission is state-independent so filtering commutes with union (HIA L1075-1077). `derive` removes the grant when it folds the `Revoke` (N4-A §5.1 L125: "N1's `Revoke` removes the grant from the derived state; there is no `revoked_at`"). The lifecycle test pins: after `revoke_event(..., 3, ..., a)` (L167) every permutation of the four facts gives `A` refused / `B` accepted at `E = 3` (L214-232).

Therefore: **any transport that carries facts converges**, because ingesting carried facts is `join(local, carried)`. Whether the carrier is a fact-set, an N4-B bundle, kv rows, or a future N3 message makes no difference to the result; the contract already says so (N4B §6 L158: "carry a bundle over any transport").

What is missing is exactly what the task suspected:

1. **Transport** — no process-to-process primitive exists for N1 facts (B.2).
2. **Trigger** — nothing decides *when* and *to whom* the edge sends a new fact. Home §6.4 L211-216: the edge authors `Revoke`; "effectiveness is per relying party, exactly as far as its retained facts go. There is no global instant revocation" (#2599 invariant 5). L219-220: a Home runtime that loses contact with the edge "cannot revoke anything; it can only stop relaying".
3. **Gap detection / request** — N3a (#2598 "gap detection and missing-fact requests").

A relying party that never receives the `Revoke` keeps evaluating correctly at positions it holds; it is only wrong about *now*, which the contract already admits is unprovable (N3 invariant 6, no negative-universe proof). The class-2 hole (a compromised device keeps binding an old E, HIA §9.3 L1346-1352) is a C question.

### B.4 How a relying party learns which prefix / frontier it holds

- `derive(subject, store) → AuthorityView::Live { state, frontier }` (`derive.rs:74-82`, returned at L492-495): `frontier` = "the first position with no authorized candidate" (comment L77). The relying party holds a clean prefix through `frontier − 1`.
- `derive_prefix(subject, store, E)` (L458-465) → `Live { frontier = E + 1 }` iff clean through `E`; `Live { frontier ≤ E }` is a gap; `Halted { disputed_at }` is a fork (N4-A §5.2 table L137-146).
- GEN gives the same number for a genesis: `VerifiedSubjectContext.frontier` (`subject_context.rs:497`), always `2`.
- Gaps stop the frontier at the missing position (`derive.rs:737-755` test "frontier stops at the missing position"); unauthorized spam never advances it (`mod.rs:18-20`).

What does **not** exist: a wire shape to *tell another party* your frontier ("digest/range/frontier summary shape" is an N3 required wire question in #2598), and any observation receipt ("observer O observed σ through K", N3b). Two parties can only compare frontiers by exchanging the facts themselves, which with the join is harmless but is not a summary.

### B.5 Fail-closed on a missing revocation: already proven

- `evaluate_device_authority` (`device_authority.rs:312-324`): `frontier <= position → PrefixIncomplete { frontier, required }`.
- `tests/device_lifecycle_acceptance.rs:233-249`: `stale = &all[..3]`; `A`'s act at `E = 3` → `PrefixIncomplete { frontier: 3, required: 3 }`; at `E = 2` → `Ok` (L251-257; history is not rewritten).
- N4-B §3 L96-99 "Withholding shrinks, never extends", pinned by a test; N4-A §8.1 L115-116.

What remains is choosing `E` — and the honest corollary the lifecycle test also pins: a stale relying party *does* accept a revoked device's act at an `E` it legitimately holds (L251-257). That is correct as history and is the class-2 limit as a current decision. Section C.

### B.6 Recommendation for B

**A `FactDelivery` service is not needed and would be the wrong layer** (it would be a transport with a trigger, which is N3). **What is needed is a facts-only container**, because the only carrier that exists demands an act (B.2). Smallest sound piece:

`authority_log::AuthorityFactSetV1` (kernel crate, `std` only, placed under `authority_log/` so the no-clock test covers it):

```text
fact_set_v1 :=
      LP("icn.n1.authority-fact-set")       -- distinct from every N1/GEN/N4 separator; pin it
   || u16be(1)
   || u32be(fact_count)                      -- ≤ MAX_BUNDLE_FACTS, bounded before allocation
   || fact_count × (b32(event_id) || b64(witness) || LP(canonical_body))   -- the N1-D/N4-B record, unchanged
```

API: `new(facts)` (sort + dedup by `(event_id, witness)`), `canonical_bytes()`, strict `decode()` (same rules as N4-B §2 table), `into_store() -> Result<AuthorityStore, _>` (fail-closed ingest, naming the index, like `store_from_bundle`), and `for_subject(store, subject)` (export side; today there is no `AuthorityStore` method that yields `SignedAuthorityEvent`s for a subject — `bodies_for` L139 and `witnesses_for` L148 exist separately, so this is a ten-line join of the two). When #2800 merges, `from_persisted(&[PersistedFact])` / `to_persisted()` are one-liners because the record is the same bytes.

Do **not** change `DeviceAuthorityBundleV1`'s bytes (its domain and vectors are pinned); note in its doc that it is "fact set + act" by construction.

Tests: fact-set round trip reproduces `Bodies` and `Witnesses` exactly; `verify_device_act` over `into_store()` equals `verify_bundle` over the same facts; a set missing the `Revoke` yields `PrefixIncomplete` at the revoke position; `into_store(a ∪ b) == join(into_store(a), into_store(b))` (the N3 join property through the container); arrival-order independence; every N4-B malformed case (reordered, duplicated, wrong `event_id`, non-canonical body, impossible count) refused.

This is the "transport container for facts ... (next slice)" the Home profile §0 L36 names. Owner: #2694 (container as the shared missing boundary; N4-A §9.2 item 2) for the object; **N4-C step 9 / #2599 for the device-delivery use; N3 #2598 for replica-to-replica**. The trigger ("edge pushes after approval and after revoke to every party it knows") stays with #2599/#2598 and is the next slice after the container.

### B.7 Missing pieces and owners (B)

| Missing | Owner |
|---|---|
| Facts-only container (`AuthorityFactSetV1`) and a per-subject export from `AuthorityStore` | #2694 container boundary; consumed by #2599 (device delivery) and #2598 (replication) |
| N1-D merge + `icnd` wiring | #2800 (open) → #2777 (data-root exclusion) |
| Transport for request/fact bytes (QR, link, local network) | #2599 (N4-C step 4, step 9) |
| Trigger / fan-out: edge pushes new `Authorize`/`Revoke` to device and relying parties | #2599 (ceremony), #2598 (N3a dissemination) |
| Gap detection, missing-fact request, frontier/digest summary | #2598 (N3a) |
| Scoped observation evidence ("observed through K") | #2598 (N3b) |

---

## C. Evaluation position / freshness (G1-A, #2694 §6)

### C.0 Sources read

- issue #2694 §6 (`VotingProcessSnapshotV1`), §7 (N4-alpha prefix evaluation), §8 (`MemberVoteActionV1`), and the three comments (rung 5 landed; "the act binds the position but the relying party must supply an equal one — the signer never selects the window")
- N4-A doc §2 L53-72, §5 L107-146, §7 L192-260 (esp. §7.1 L217-231, §7.2 L233-239), §9.3-9.4 L366-396
- HIA §9.3 L1314-1408, §17 rows O-N1 L2093, O-N6 L2098; N3 #2598 invariants 5-7 and vocabulary
- `device_authority.rs` L1-60, L295-410; `derive.rs` L437-465
- `grep -rn "position oracle|O-N1|O-N6|G1-A" docs/architecture docs/adr`: **"position oracle" occurs nowhere.** The nearest concepts are N3's "no hidden time oracle" (invariant 7) and "metaphysical finality oracle" (#2598 purpose), and N4-B §1 L38 "it is a relying party, not an oracle." G1-A appears only in N4-A §9.4 L393 and N4-B §7 L175, both as "which position a relying party should pin (G1-A, #2694 §6)".

### C.1 What the contract says about choosing E

N4-A §7.1 L219-231 is the rule, verbatim in substance:

> `E` is **the last position the relying party relies on**: the position of the last authority fact it has itself observed, retained and admitted for this Subject. It is a position, never a timestamp, and it is never read from the act.

| Situation | `E` |
|---|---|
| a governance process that pinned a prefix when it froze its electorate (#2694 §6) | the pinned position, same for every ballot |
| a verifier whose `derive` reports `Live { frontier }` | `frontier − 1` |
| a verifier that has not retained the position the act names | **refuse**; "do not fetch, guess or fall back to the frontier" |

§7.1 L228-230: `frontier − 1` on a store that has not seen a revoke is "**prospective only**: it evaluates the authority the verifier knows about. That is the honest semantics of class-2 acts." §7.2 L233-239: class 1 (deferred-decision) — "the decision pins `E`"; class 2 (immediately settled) — "the acceptor evaluates at its own retained `frontier − 1` once, at admission; **no validity bound exists** and R5.1 remains unmet for this class."

#2694 §7: "`E` is chosen by the relying governance process, not by the signer ... No new branch selector. No wall clock. No ambient network lookup." "#2694 §6" is `VotingProcessSnapshotV1`, the frozen process fact that would carry the pinned position for class 1; it is **rung 6 (G1-A), unbuilt**, and it depends on O-N6 (deterministic proposal closure; HIA L2098: "a governance problem, not an identity one").

HIA §9.3 L1339-1368 is the reasoning: a window checked against the signer's own position bounds nothing ("a compromised device simply keeps referencing `N`"); it binds only against "an evaluation position the signer does not control", which exists for class 1 (the decision) and for class 2 only as "the acceptor's own current frontier — receiver-local, therefore divergent (R4.2/R4.3)". L1363-1368: for class 2 "the only real controls are scope limits and a relying party's own freshness bar, which is the acceptor's risk decision under Rivest Proposition 1 and does not converge between acceptors."

### C.2 Is "evaluate at frontier−1 of the facts you hold" sanctioned?

**Yes for class 2, with the stated caveat; no for class 1.** For class 1 a relying party that used its own `frontier − 1` per ballot would reintroduce the arrival-order divergence HIA L1399-1401 rejects; the process must pin one `E` (G1-A).

The load-bearing detail from code: the relying party does not pick `E` *per act* in any free sense. `verify_device_act` (`device_authority.rs:394-399`) refuses unless `act.evaluation_position == position`. So the operational question is never "at what E shall I judge this device" but "**is the E this act binds the E I would have pinned?**" For class 2 at admission that is: accept iff `act.E == frontier(my store) − 1`.

- `act.E < frontier − 1`: the act is **stale** (or a compromised device replaying an old position, HIA L1346-1352). Refusing it is what closes that hole from the acceptor's side. It is still a correct *historical* statement, which is why the library lets a caller evaluate there (lifecycle test L251-257), but it is not a current admission.
- `act.E ≥ frontier`: the relying party is **behind**; it must obtain facts, not guess (§7.1 row 3; Home §6.6 L245-246 MUST NOT "fall back to the frontier when the act's `E` is not retained").

A consequence worth stating for the Home slice: every time the edge appends a fact (any `Authorize`/`Revoke`), the frontier advances, and every device that has not yet received that fact will bind a now-stale `E` and be refused by an up-to-date relying party until it catches up. HIA L1378: "a subject who advances too aggressively can strand an offline device" (O-N1). This makes **fact delivery to devices (B.6) load-bearing for liveness**, not merely for relying parties.

### C.3 When the relying party cannot establish a current frontier

It **never can**, by design, and the contract does not pretend otherwise (N3 invariant 6 "no negative-universe proof"; N4-A §2 L62-64: a verified act does not prove "that any other replica would reach `E`"). It has a frontier of what it holds:

| `derive` says | Meaning | Current-admission answer |
|---|---|---|
| `Unknown` | no inception retained | refuse (`SubjectUnknown`) |
| `Halted { disputed_at }` | fork in the retained prefix | refuse for any `E ≥ disputed_at`; the current authority is disputed, so no current admission (historical evaluation below the fork remains valid, N4-A §8.1 L117) |
| `Live { frontier: 1 }` | inception only | no delegation exists yet; `E = 0` is `PositionOutOfRange` (L310-311) |
| `Live { frontier ≥ 2 }` | clean prefix through `frontier − 1` | admit at `frontier − 1`; the claim is scoped to "under the facts I hold" (N3b vocabulary: *Observed*, *Valid-at-frontier*, *Policy-actionable*, never *globally irreversible*) |

A gap looks like a short frontier; nothing distinguishes "I am behind" from "nothing more has happened" except receiving facts. That is N3a's problem (#2598), not the verifier's.

### C.4 Wall-clock dependence

Forbidden at every layer, and pinned: `authority_log_no_clock.rs::module_reads_no_wall_clock` (PR #2800 body cites it as the constraint the fact store sits under); `derive.rs:456-457` "The bound is a **position**, never a timestamp"; N4-A §2 L71 "no registrar, directory, clock, network lookup or randomness"; N4-A §8.1 L129-130 "the module reads no clock"; N3 invariant 7 "no hidden time oracle"; IS §2.2 "never a clock"; HIA §9.3 L1341-1344 "No clock is read anywhere, which is the property worth keeping (R4.1)". N4-C L50-51: "N1 has no clock, so the request carries a nonce, not a deadline" and expiry is enforced at the edge's UI, outside the protocol. A hosting operator may use time to decide to *stop relaying* (Home §6.4 L219-220) — that is a hosting decision that "confers no authority" and is not an evaluation input.

### C.5 Smallest executable piece for C

In `device_authority.rs` (no doc contract change; it executes N4-A §7.1 row 2):

```rust
/// The class-2 admission position: the last position this relying party holds a clean
/// prefix through. Never read from an act; never a timestamp; never fetched.
pub fn admission_position(subject: SubjectId, store: &AuthorityStore)
    -> Result<u64, AdmissionPositionRefusal>;
// Unknown            -> SubjectUnknown
// Halted{disputed_at}-> AuthorityHalted{disputed_at}
// Live{frontier<2}   -> NoDelegationYet
// Live{frontier}     -> Ok(frontier - 1)

/// `verify_device_act` at `admission_position`. An act binding any other position is refused
/// by the existing `EvaluationPositionMismatch { act, relying }`; `act < relying` is a stale act,
/// `act > relying` means this relying party is behind and must obtain facts.
pub fn verify_device_act_at_admission(signed: &SignedDeviceAct, subject: SubjectId, store: &AuthorityStore)
    -> Result<DeviceAuthorityEvidence, DeviceActVerifyError>;
```

This does **not** choose `E` from the act (Home §6.6 / N4-A §9.3 MUST NOT): it computes `E` from the store and compares. Document it as class 2 only, with the §7.1 L228-230 "prospective only" caveat copied in, and that class 1 uses the process pin (G1-A) instead.

Tests (fixture from `tests/device_authority.rs`: `S`; `A@1`, `B@2`, `revoke A@3`):

1. Full relying party (frontier 4): `admission_position == 3`; `A`'s act bound to 3 → `DeviceNotAuthorized`; `B`'s → evidence `{granted_at: 2, generation: 0}`.
2. **Revoked device replays an old position**: `A` signs a new act binding `E = 2` (where it was authorized). Full relying party → `EvaluationPositionMismatch { act: 2, relying: 3 }`. This is the requested test: the act binds `E`, the relying party cannot judge the device at an `E` other than the act's, and it refuses an `E` older than its own admission position.
3. **Stale relying party** (holds through 2, frontier 3): `admission_position == 2`; `A`'s act bound to 3 → `EvaluationPositionMismatch { act: 3, relying: 2 }` (behind; never accepted, never fetched); `A`'s act bound to 2 → **accepted**, pinned deliberately as the class-2 limit (it does not hold the revoke; HIA §9.3 R5.1 not met for class 2). Honesty here matters more than a green test that hides it.
4. `Halted` → refused; `Unknown` → refused; inception-only → `NoDelegationYet`.
5. Verdicts identical over all permutations of the facts; module still reads no clock/randomness (extend the existing source-inspection pin).

### C.6 Missing pieces and owners (C)

| Missing | Owner |
|---|---|
| `admission_position` / `verify_device_act_at_admission` (class-2 rule made executable) | #2599 (the acceptor's freshness bar) |
| `VotingProcessSnapshotV1` pinning `E` for class 1 (G1-A) | #2694 rung 6; depends on #2600 (G1) |
| O-N6 deterministic proposal closure | #2600 (HIA L2098: governance, not identity) |
| O-N1 validity span `k` and re-authorization cadence | HIA §17 open; no issue number; empirical UX input (L2093) |
| N3b scoped observation evidence; frontier summary wire shape | #2598 |
| Fact delivery to devices so they can bind a current `E` (liveness) | B.6 container + #2599 trigger |

---

## Cross-cutting notes

- **One framing, three uses.** The record `event_id ‖ witness ‖ body` is what the daemon persists (N1-D), what a client carries with an act (N4-B), and what a facts-only set should carry (B.6). Keep it one definition.
- **Fixture-only, still.** Every test identity is fixture material (N4-A/B/C, Home §0). None of A, B, C creates a real Private/Household/Pet Care context; A.6 only makes it *possible* to.
- **ADR-0083 drift (#2806)** is untouched by anything here; nothing in A claims a personal context is an `InstitutionalDomain`.
- **Untracked WIP** on the katie branch (`device_channel_binding.rs`) should be committed or set aside before any of the above lands on that branch, or the diff will mix two slices.
