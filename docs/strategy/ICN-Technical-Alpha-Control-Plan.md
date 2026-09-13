---
Status: descriptive
Canonical: no
Last Reviewed: 2026-09-13
---

# ICN Organizational Technical Alpha — Control Plan

> **What this document is.** One navigable reconstruction of the entire
> Organizational Technical Alpha program: the bounded claim, the proof chain, the
> dependency graph, the containment ledger, and the owner boundaries. It exists so
> that a maintainer who has never seen a planning conversation can reconstruct the
> program from the repository alone.
>
> **What this document is not.** It is **not** a truth owner. It owns no domain
> fact. Every status below is a *snapshot* taken at one revision, and
> `ops/state/truth/sources.json` routes each underlying fact to its real owner.
> Where this document and an owner disagree, **the owner wins and this document is
> the stale layer.**
>
> **Live state is owned by `live_issue_state` / `live_pr_state` / `live_ci_state`
> (github-api), not by this file.** Re-resolve with `gh` before acting on any
> status here. Snapshot revision:
> **`82030804dc26003bf7e1d6e289166989108bcd63`**, taken 2026-09-13.

---

## 1. The bounded claim

The Technical Alpha is **not** "make ICN production ready." It is one bounded,
independently witnessed proof at one exact reviewed revision and profile.

The strongest intended eventual claim, in full:

> On one exact reviewed ICN Alpha profile, a fresh context-scoped human Subject
> used a separately keyed, explicitly delegated device Principal to act as a
> recognized member of an institution; that standing authorized participation in a
> frozen governance process over one exact bounded resource action; the resulting
> decision was cryptographically committed to a canonical decision hash; existing
> ICN economic and execution machinery carried that same hash through the bounded
> resource action and retained provenance; an independent Node B verified the
> retained evidence offline while Node A was stopped; and the verifier/recovery
> node subsequently survived the separately specified encrypted destroy/restore
> proof.

Anything stronger is **not** part of A1 unless added through reviewed owner work.

The program's own exit criteria are owned by **icn#2689 section 10** — seventeen
checkboxes, **all unchecked** at this snapshot. They are not duplicated here.

### Standing non-claims

A1 does **not** claim any of:

- production readiness, availability, or operational maturity;
- federation, multi-institution operation, or cross-institution identity;
- that a contained defect is fixed;
- hot or live backup;
- generalized N3/N4/N5 protocol completeness;
- that any downstream repository lock is human-signed;
- that an HTTP fetch from Node A constitutes independent verification;
- native cryptographic binding between a governance decision and a ledger entry
  (see 6.4 — this specifically does not exist today).

---

## 2. Proof chain, with every arrow resolved

Each arrow resolves to exactly one of: an **existing reviewed owner primitive**, a
**bounded implementation slice**, an **explicit containment**, or an **explicit
non-claim**. There is no "then somehow this works" edge.

| # | Link | Resolves to | State at snapshot |
|---|---|---|---|
| 1 | substrate to reproducible node/profile | appliance image + manifest; `icnctl appliance verify-manifest` | EXISTS; ADR-0086 merged (PR #2458) but `status: proposed`, `implementation_status: partially implemented` — **adoption is a separate human decision** |
| 2 | to two-node communication | isolated QEMU topology in the two-node plan | EXISTS (plan, `Canonical: no`) |
| 3 | to institution package/domain | `InstitutionBootstrapManifest` (`icn-governance/src/bootstrap.rs:14`); `icnctl institution runtime-root` | EXISTS but **disclaims canonical institution genesis** (6.6) |
| 4 | to fresh current-semantic human Subject | `SubjectContextGenesisV1` | **SLICE — icn#2695** (design-reviewed, unimplemented) |
| 5 | to separately keyed delegated device Principal | `DeviceGrant` (`icn-identity/src/authority_log/derive.rs:25`) | PRIMITIVE EXISTS; unreferenced outside `icn-identity` |
| 6 | to explicit institutional recognition | `SubjectRecognitionV1` | **SLICE — no owner issue** |
| 7 | to frozen governance process | `VotingProcessSnapshotV1` | **SLICE — no owner issue** |
| 8 | to exact bounded resource-action commitment | `SemanticProposalCommitmentV1` | **SLICE — no owner issue** |
| 9 | to signed Subject ballot(s) | `MemberVoteActionV1` + immutable ballot slot | **SLICE — no owner issue** |
| 10 | to deterministic tally | `SubjectVoteSetHashV1` + tally | **SLICE — no owner issue** |
| 11 | to `GovernanceDecisionReceiptV4` | V4 | **DOES NOT EXIST.** V1 emitted on every close path, V3 conditionally and additionally; the open question is the V1-only typed read surface (6.3) |
| 12 | to canonical `decision_hash` | `compute_decision_hash_bytes` (`icn-governance/src/proof.rs:300`) | **EXISTS and exposes canonical bytes** |
| 13 | to existing `AllocationReceipt` | `icn-kernel-api/src/receipts.rs:121` | EXISTS; canonical bytes **private** (6.2); unsigned in practice |
| 14 | to existing `SettlementIntent` | `icn-kernel-api/src/economics.rs:80` | EXISTS; canonical bytes **private**; **no signature field** |
| 15 | to execution | `ExecutionRecord` (`execution.rs:78`) | EXISTS; **mutable operational state**, not a canonical receipt |
| 16 | to ledger/provenance evidence | `JournalEntry` + `ProvenanceRef` | EXISTS; **provenance is not in the hash and not signed** (6.4) |
| 17 | to self-contained offline evidence | offline bundle contract | **SLICE — icn#2465** (spec only, no implementation) |
| 18 | to Node A stopped | two-node plan Gate 4 step 5 | **Gate 4 BLOCKED** |
| 19 | to Node B independently verifies | reviewed offline verifier | **SLICE — icn#2465** |
| 20 | to encrypted destroy/restore continuity | recovery bundle | **SLICE — icn#2466**; Gate 6 BLOCKED |
| 21 | to same evidence still independently verifiable | re-run of 19 post-restore | depends on 19 + 20 |

**Eleven of twenty-one links are unbuilt.** Links 11, 13, 14 and 16 carry concrete
owner problems documented in section 6, not merely missing code.

---

## 3. Dependency DAG

```text
            SAFETY / CORRECTNESS FRONTIER
                       |
                       v
            EXACT ALPHA DEPLOYMENT PROFILE
                       |
        +--------------+--------------+
        v              v              v
    icn#2694       icn#2465       icn#2466
    semantic       offline        recovery
    convergence    evidence       proof
        |              |              |
        |              v              |
        |     (needs 6.2 owner        |
        |      canonical bytes)       |
        |              |              |
        +--------------+--------------+
                       v
              TWO-NODE A1 WITNESS
                       v
             FREEZE EXACT SHA/PROFILE
                       v
                 NYCN LOCK BUMP
                       v
            HUMAN REHEARSAL / A11Y
                       v
             BOUNDED PUBLIC CLAIM
```

This diagram is **reconciled, not immutable**. Two edges are sharper than the
ASCII suggests:

- **#2465 depends on #2694** for its *subject matter* (there is no decision
  receipt to export until the ladder produces one), but its **substrate work
  (slices A and B in section 5.6) does not** — that can start immediately.
- **#2466 is independent of #2694 entirely.** It is a sovereignty proof about node
  state, not about semantics.

---

## 4. icn#2694 — semantic convergence ladder

The missing proof is **generation convergence**, not invention of another economic
system. The target path, in the issue's own vocabulary:

```text
context-bound N1 Subject genesis -> N1 device authorization
  -> institutional Subject recognition -> context-scoped session
  -> immutable voting-process snapshot -> preview
  -> device-signed member vote action -> immutable Subject ballot
  -> Subject tally -> GovernanceDecisionReceiptV4 -> canonical decision_hash
  -> existing AllocationReceipt -> existing SettlementIntent
  -> existing ExecutionRecord -> existing journal/provenance
  -> offline evidence bundle
```

#2694 states that each numbered item becomes a bounded child issue **only when its
direct owner contracts have been reverified and implementation is authorized**.
That gate has fired exactly once.

| # | Slice | Owner issue | Upstream contract owner | State |
|---|---|---|---|---|
| 1 | GEN-A context binding + `SubjectContextGenesisV1` | **icn#2695** | #2602 GEN | OPEN, `status:needs-design`, design-reviewed, **unimplemented** |
| 2 | N1-D restart-durable `AuthorityFactStore` | **none** | #2607 | named by #2695 as the next child |
| 3 | GEN-B `SubjectRecognitionV1` | **none** | #2602 GEN | unowned |
| 4 | `HumanActorModel` / legacy-rail firewall | **none** | — | unowned; appears in **no other issue** |
| 5 | N4-alpha relying-party prefix evaluation | **none** | #2599 N4 | unowned |
| 6 | G1-A commitment + frozen `VotingProcessSnapshotV1` | **none** | #2600 G1 | unowned |
| 7 | context-aware session rail | **none** | — | unowned; no artifact name |
| 8 | N5-A `MemberVoteActionV1` + ballot slot | **none** | #2605 N5 | unowned |
| 9 | Subject vote-set hash + deterministic tally | **none** | — | unowned |
| 10 | `GovernanceDecisionReceiptV4` | **none** | — | unowned; see 6.3 |
| 11 | V4 `decision_hash` through the economic chain | **none** | #2625 identifier domain | unowned |
| 12 | offline evidence convergence | partially **#2465** / **#2466** | — | composition step itself unowned |

**None of the twelve artifact names exists in `icn/crates` or `icn/apps` at this
revision.** No PR references #2694 or #2695.

---

## 5. icn#2465 — offline evidence architecture

### 5.1 Architecture

```text
bundle transport / exact bytes
            |
            v
existing owner artifact verification
            |
            v
cross-artifact binding verification
            |
            v
externally pinned A1 policy
            |
            v
        composed verdict
```

**Do not invent a parallel evidence ontology.** The repository already owns the
verdict vocabulary: `VerificationStatus` at
`icn/crates/icn-governance/src/verify.rs:46` — `Pass` / `Fail` / `Unresolved` /
`NotApplicable`, fail-closed, with fold severity
`Fail > Unresolved > Pass > NotApplicable`. Normative spec:
`docs/spec/receipt-chain-verification.md` (`status: draft`). Reuse it; do not mint
a second verdict set.

Keep distinct: **integrity**, **authenticity**, **authorization**, **legitimacy**.
`verify.rs` is explicit that a Pass means integrity — and authenticity when keyed
— and **never** authorization or legitimacy.

### 5.2 Two hard criteria (verbatim, do not weaken)

> - [ ] The bundle exports no DIDs or credentials beyond the producer identity the
>       contract explicitly requires.

> - [ ] Verification never falls back to contacting the producer — a network call
>       in the verify path is a defect, not a convenience.

Neither admits tiering. Missing required evidence yields `Unresolved` or failure —
**never** a network lookup. No gateway URL, no producer callback, no remote DID
resolver, no HTTP client anywhere in the verify path.

If the chosen A1 economic action cannot satisfy the privacy criterion, that is a
concrete owner/integration problem to surface — **not** a licence to redact a
security-critical owner-canonical field and still call it independently verifiable.

### 5.3 Evidence strength — additive planning vocabulary

This taxonomy **does not exist in the repository**. It is proposed here, not
established.

| Tier | Meaning | Expected members |
|---|---|---|
| `OwnerCanonical` | owner computes and owns the canonical identity | `GovernanceDecisionReceiptV4`, `AllocationReceipt`, `SettlementIntent` |
| `OwnerAuthenticated` | owner-canonical **and** signed by a named key | none today — see 6.2 |
| `ProducerAttestedSnapshot` | producer asserts it exported this view | `ExecutionEvidenceSnapshotV1`, `JournalProvenanceWitnessV1` |

It must not be used to soften 5.2.

### 5.4 Two digest meanings — never collapse

| Meaning | Algorithm today | Subject |
|---|---|---|
| owner canonical identity | blake3 over bincode (`receipts.rs:100`); blake3 over a domain-tagged stream (governance) | domain-semantic identity |
| exact exported payload | SHA-256 | *these exact bytes were retained/exported* |

These are different algorithms over different subjects. **Conflating them is a
defect**, because canonical hashes intentionally exclude fields — see 6.2.

### 5.5 Producer attestation

A producer signature means **"this producer exported this exact
manifest/artifact-byte set."** It does **not** mean "everything in this bundle is
legitimate." Policy and trust are pinned **externally**; the bundle must not choose
how it is judged:

```text
verify_offline_bundle(retained_bundle, externally_pinned_policy, externally_pinned_trust)
```

### 5.6 Decomposition

**#2465 defines no bounded slices.** A search of 400 issues found zero hits for
`TechnicalAlphaA1Policy`, `RecoveryBundle`, `JournalProvenanceWitness` or
`ExecutionEvidenceSnapshot` — in issues *or* in the checkout.

| Slice | Scope | Owner |
|---|---|---|
| **A** owner canonical export surfaces | expose owner-controlled canonical bytes for `AllocationReceipt` / `SettlementIntent`; make the existing hash consume those same bytes | **needs an issue** |
| **B** deterministic generic offline-bundle substrate | byte-stable bundle, digest manifest, no network | **needs an issue** |
| **C** bounded A1 execution/journal evidence adapters | `ExecutionEvidenceSnapshotV1`, `JournalProvenanceWitnessV1` | **needs an issue** |
| **D** `TechnicalAlphaA1PolicyV1` + composed proof | externally pinned policy, tamper-negative test | **needs an issue** |

Slice A must **not** change the semantic hashes of those types merely to support
export. The direction is: expose the bytes the owner already commits to, then make
the existing hash consume them.

---

## 6. Owner boundaries, seams, and live contradictions

This section records what the code actually does. It is the part most likely to
invalidate planning vocabulary.

### 6.1 No registered owner for economics

`ops/state/truth/sources.json` has **26 domains**, and only two touch this proof
chain: `identity_semantics` (`docs/architecture/IDENTITY_SEMANTICS.md`) and
`adr_decisions` (`docs/adr/`). There is **no registered truth domain for receipts,
provenance, ledger, settlement, treasury or economics.** For those, the ADRs plus
the code are the only authority.

### 6.2 Canonical bytes are private for both economic receipts

`AllocationReceiptCanonical` (`receipts.rs:148`) and `SettlementIntentCanonical`
(`economics.rs:122`) are **module-private and not re-exported**; there is no
`canonical_bytes()` anywhere in the crate. An external verifier can only recompute
those hashes by owning a bincode-compatible copy of the struct. The governance path
is the opposite: `compute_decision_hash_bytes` (`proof.rs:300`) is `pub`.

Two live traps for any bundle contract:

- `AllocationReceipt::canonical_hash` **sorts** intent hashes (order-independent).
- `SettlementIntent`'s hash **excludes `memo`**, and its ids and signature are
  `#[serde(skip)]`.

So the exported payload is strictly **wider** than the canonical preimage:
**mutating `memo` leaves `canonical_hash` unchanged.** A digest manifest covering
only canonical hashes would therefore *pass* #2465's "flip one byte, must fail"
criterion on a genuinely tampered bundle. This is the sharpest single reason slice
A exists.

Also: `AllocationReceipt::with_signature` has **no production caller** (unsigned in
practice), and `SettlementIntent` has **no signature field at all**. Hence
`OwnerAuthenticated` in 5.3 currently has no members.

### 6.3 Governance receipt versions

V1 (`proof.rs:226`), V2 (`:541`, defined but never emitted), V3 (`:820`).
**V4 does not exist** — zero repo-wide hits for `ReceiptV4`.

An earlier revision of this document described the seam as "V3 emitted, V1
persisted." Review corrected that, and re-verification against `main` agrees.
The accurate shape is:

- **V1 is emitted on every close path** (`apps/governance/src/actor.rs:2613`,
  `:2432`, `:2891`; `manager.rs:4409`; `lib.rs:380`, `:430`).
- **V3 is emitted conditionally and additionally**, not as a replacement:
  `actor.rs:2561` gates on a present `capability_scope` *and* a configured
  receipt store, so timer/scheduler auto-close and forced-accept emit no V3.
- **V3 is persisted**, but opaquely — `receipt_backend.rs:509` writes it through
  `put_opaque`, so it never crosses the gateway's typed boundary.
- **The real seam is the read surface.** The gateway store's typed API is V1 only
  (`receipt_store.rs:386` `put_governance`, `:426` `get_governance`), and
  `icnctl audit verify` recomputes the **V1** decision hash
  (`icnctl/src/main.rs:12752`) through a *local* `verify_receipt_chain`
  (`:12702`) rather than through `icn-governance::verify`.

So a V4 does not have to reconcile a persistence mismatch; it has to decide what
the **typed chain/audit read surface** returns, and whether the conditional V3
emission becomes unconditional first.

A stale comment at `proof.rs:817` still reads "No handler emits a v3 receipt yet
— this is schema preparation only." That is now false, and is recorded here as a
follow-up rather than fixed, because this is a control-plane document and that is
a Rust change.

### 6.4 The ledger provenance limitation — be exact

`compute_entry_hash` (`icn-ledger/src/hash.rs:9`) is serde_json then SHA-256 over
`HashableEntry` (`hash.rs:37-45`), which carries `timestamp, author, contract_ref,
accounts, parents, nonce`. **`ProvenanceRef` is absent.** `sign_entry`
(`entry.rs:28`) signs `entry.id`.

Therefore: **the author signature does not bind the governance `decision_hash`.
Provenance can be altered without invalidating either the hash or the signature.**
No production code cross-verifies that a `ProvenanceRef::Governance.decision_hash`
corresponds to a real receipt; the gateway only echoes it.

**Do not rewrite ledger identity casually, and do not claim native cryptographic
strength that does not exist.** For A1 the bounded evidence architecture may use a
**producer-attested witness** connecting `entry_hash`, `decision_hash` and
`provenance_kind`, while explicitly reporting that proof strength. A future
owner-defined governance-to-ledger binding receipt may supersede it.

### 6.5 Three incompatible canonical-hash schemes

| Layer | Scheme |
|---|---|
| kernel-api receipts | bincode then blake3 |
| governance proof | domain-tagged byte stream then blake3 |
| ledger journal | serde_json then SHA-256 |

No single re-verification primitive spans them. A bundle verifier must dispatch per
artifact class rather than assume one hash function.

### 6.6 Institution genesis is not canonical yet

`icnctl institution runtime-root` (shipped in #2744/#2749) creates two keystores, a
cooperative record, a treasury registration, trust edges and a receipt — but its
**own doc comments disclaim canonical institution genesis**: no `EntityId`, no
signed founding act, no institution DID, and `genesis_authority_did` is today the
*same principal* as `node_did`. It is not idempotent and not restartable. ADR-0083's
"runtime root" is a different, `not-started` concept.

### 6.7 The fail-closed verifier is wired to nothing

`icn-governance/src/verify.rs` is a complete four-valued fail-closed verifier with
**zero callers repo-wide**. A1 should consume it rather than write a third
verification path.

### 6.8 Identity primitives exist but are unreferenced

N1's authority log (`icn-identity/src/authority_log/`) is described by its owner as
"a library primitive only" with **zero references outside `icn-identity`**.
`AuthorityFactStore`, `SubjectContextGenesis`, `SubjectRecognition`,
`HumanActorModel`, `VotingProcessSnapshot`, `MemberVoteAction` and
`SubjectVoteSetHash` **do not exist in code**. Session authority exists under
different names (`SessionAuthority`, `AuthorityProfile`, `AuthorityCapabilities` in
`icn-gateway/src/session_authority.rs`). `ScopeLevel` is **network reach, not
authority** — do not conflate. `NodeId` is `pub type NodeId = String`; Node has no
durable domain.

### 6.9 Unresolved owner contradictions

1. **Two competing decompositions.** #2694 defines a 12-step semantic ladder.
   #2689's 2026-09-02 checkpoint defines an overlapping **P0-P10 proof ladder** with
   six lanes, adds N3-A two-node authority reconciliation (which #2694 explicitly
   non-goals) and a mobile edge. **Neither supersedes the other in writing.**
   #2689's *body* still describes the older four-lane model. **A maintainer decision
   is required.**
2. **`ops/state/truth/program.json` does not exist on `main`.** A checkpoint claimed
   the ladder was "durably encoded" in a registered `program_structure` domain. It
   is not. Verified at this snapshot: the file is absent from `main`, `sources.json`
   registers no `program_structure` domain, and **PR #2690 (OPEN)** carries both
   `ops/state/truth/program.json` and a `sources.json` change among its fifteen
   files. Note #2690's *headline* contract is the agent registry, so the program
   surface rides along inside a PR about something else — which is part of why it
   has not landed. Until it does, the only durable record of the ladder is the
   #2694 issue body and this document. **This document deliberately does not create
   a competing machine-readable program surface**; when #2690 lands, this file
   should link to it rather than duplicate it.
3. **V3 emitted / V1 persisted** (6.3) — confirmed at the type level, not traced to
   runtime wiring. Verify before relying on it.
4. **The two-node plan is `Canonical: no`, last reviewed 2026-07-27** — predating
   #2689. It references ADR-0086, which **does** exist on `main` (PR #2458
   merged 2026-07-28) but carries `status: proposed` and
   `implementation_status: partially implemented`, so the profile is proposed,
   not adopted.

---

## 7. icn#2466 — recovery

A **separate sovereignty proof**, independent of #2694.

#2466 specifies no field list and no ordered ceremony — it specifies a drill plus
invariants. The expected artifact shape (planning vocabulary, `RecoveryBundleV1`)
is: exact profile/revision, encrypted state, encrypted effective secret material,
node identity commitment, config commitment, institutional/runtime-root
commitments, retained evidence commitments, extent/completeness manifest, bundle
root digest.

Its binding invariants, which **are** from the issue: secrets never written to an
unencrypted archive *at any point including temporary files*; an explicit custody
model; restore into a fresh overlay assumes the prior node's identity, state and
relationships and reopens its keystore without operator re-entry outside the
documented custody path; continuity asserted over identity, machine ID,
configuration and genesis hashes; missing required material **refuses** to restore
rather than half-restoring.

```text
capture known state -> encrypt -> destroy/remove Node B
  -> recreate from fresh appliance overlay -> restore -> reopen keystore
  -> compare identity/config/state commitments
  -> independently verify retained Alpha evidence again
```

**Do not claim hot backup.** Do not let recovered-prefix behaviour masquerade as
complete recovery proof.

### Completeness dependencies

| Issue | Defect | Effect on recovery claims |
|---|---|---|
| **icn#2746** | a truncated ledger reopens via sled recovery and scans short *without error*, so `verify-backup --verify-ledger` reports a passing verification for an incomplete archive | **Blocks completeness outright.** Nothing records how many rows the archive should hold. Needs a backup-time extent record cross-checked at verify. |
| **icn#2739** | the verifier writes into the tree it inspects (sled has no read-only open; the N2-A gate writes into the audited tree) | **Caps claim strength**, does not falsify it. Blocks "verified a write-protected medium" and "two verifications observed the same bytes". |

Note also: **CI invokes the weak `verify-backup` form, without `--verify-ledger`**
(`.github/workflows/ci.yml:530`), and the command prints its own non-claims at
`icnctl/src/main.rs:7133-7134`.

---

## 8. Two-node roles

```text
Node A   institution host / producer / artifact source
Node B   independent technical witness / verifier
```

Node B is **not** a second institution, not federation, not a production peer and
not a governance participant. Two-institution operation requires a later ratified
enrollment ceremony.

**For offline evidence, Node A must actually be stopped or unreachable.** A Node B
`curl` back to Node A is **transfer evidence only**. The plan's Gate 4 step 5 is
literally "Disconnect Node A"; Gate 5 re-runs verification while Node A remains
disconnected.

For recovery, Node B is the natural destroy/restore witness.

---

## 9. Containment ledger

A contained flaw is **not fixed**. Each row names the exact profile restriction
that prevents it from invalidating A1.

| Issue | Disposition | Containment / profile restriction |
|---|---|---|
| **#2750** trust bloom scores persisted edges 0.0 | **MUST FIX** | None available. Rejects entries authored by the institution treasury principal at the ledger author-trust gate — directly on the A1 economic chain. |
| **#2779** snapshot-delete path traversal | **FIXED / LANDED** (PR #2781, squash `08f5bc2cc`) | — |
| **#2748** keystore files at umask | **FIXED / LANDED** (PR #2782, squash `82030804d`) | — |
| **#2777** exclusion domain (#2758/#2759) | **IN PROGRESS** (draft PR) | — |
| **#2746** truncated ledger certified complete | **MUST FIX for #2466** | Cannot be contained if recovery completeness is claimed. |
| **#2739** verifier writes the audited tree | **CONTAIN** | Claim recovery verification over a mutable copy; do not claim read-only-medium verification. |
| **#2778** backup reads a data root with no exclusion | **CONTAIN or FIX** | Take A1 backups only with the node stopped. |
| **#2755** native `icnd` ignores runtime-root config | **CONTAIN** | A1 profile must not use the native service path. |
| **#2747** `init-coop` emits an `icn.toml` `icnd` cannot load | **CONTAIN** | A1 uses `institution runtime-root`, not `init-coop`. |
| **#2757** no trusted bootstrap authority for first gateway token | **CONTAIN** | Single-institution A1; trusted-local mint only; no generalized remote bootstrap. |
| **#2772** node-authority migration across DID change | **EXCLUDE** | A1 does not rotate the node DID. |
| **#2627** N2-A `Did` equality/hashing | **PARTIALLY LANDED** | Owns the legacy `compute_vote_hash` DID-spelling defect; must be resolved before Subject ballots are trusted. |
| TIME / deadline semantics (#2601) | **EXCLUDE** | Explicitly out of Alpha scope. |
| vote amendments | **EXCLUDE** | Frozen process snapshot only. |
| cross-institution identity federation | **EXCLUDE** | Node B is a witness, not an institution. |
| generalized N3/N4/N5 | **EXCLUDE** | Only the N4-alpha prefix slice is in scope. |
| mobile / QR workflow | **EXCLUDE** | Not on the A1 chain. |
| generic installer claims | **EXCLUDE** | One pinned appliance profile only. |
| live / hot backup | **EXCLUDE** | Cold ceremony only. |
| historical DID-member migration | **EXCLUDE** | Fresh context-scoped Subjects only. |
| Other `.mode(0o600)` creation sites (`data_dir_lock.rs`, `institution_runtime_root.rs`) reachable at mode `000` under an extreme umask | **FOLLOW-UP — not a blocker** | Surfaced by the #2782 review and deliberately left outside #2748. `.mode()` is a request and `mode & !umask` can clear owner bits, so an extreme umask could make a lock anchor or runtime-root file unusable. It cannot invalidate A1 **provided the pinned appliance profile fixes the service umask** — confirm that at profile freeze. No owner issue yet; it needs one only if the profile does not pin the umask. |

---

## 10. Status snapshot

Program-level **evidence maturity**. This axis is distinct from the per-PR delivery
lifecycle owned by `ops/state/truth/delivery.json`
(`DRAFTING -> REVIEWING -> FIXING -> VERIFYING -> FROZEN -> MERGING -> DONE`),
which this document does not duplicate.

| Term | Meaning |
|---|---|
| IDENTIFIED | a defect or need is described, nothing built |
| IMPLEMENTED / UNLANDED | code exists on a branch; not on `main` |
| LANDED / UNVERIFIED | merged to `main`; not exercised on the Alpha profile |
| VERIFIED | exercised and witnessed on the pinned profile |
| BLOCKED | cannot proceed until a named dependency resolves |
| CONTAINED | not fixed; excluded by a named profile restriction |
| DEFERRED | deliberately out of A1 |

### How merge readiness is actually gated here

Recorded because the first pass guessed wrong and the #2781/#2782 merges settled
it. Live branch protection on `main`:

| Gate | Value |
|---|---|
| `required_approving_review_count` | **0** — no second-identity approval is required |
| `required_conversation_resolution` | **true** — unresolved review threads block the merge |
| `strict` | true — the branch must be level with `main`, so every merge puts every other PR `BEHIND` |
| `enforce_admins` | true — no admin bypass |
| required contexts | 11 named checks. `Security Audit` and `Compare Against Base` are **not** among them. |

So a `mergeStateStatus` of `BLOCKED` on this repository usually means *unresolved
threads*, not red CI and not a missing approval; `UNSTABLE` means only
non-required checks are red. Read the protection API before concluding a PR is
waiting on a human.

### Safety / correctness frontier

| Item | State | Evidence at snapshot |
|---|---|---|
| **PR #2781** (#2779 traversal) | **LANDED** | merged as `08f5bc2cc`; #2779 CLOSED/COMPLETED. |
| **PR #2782** (#2748 keystore mode) | **LANDED** | merged as `82030804d`; #2748 CLOSED/COMPLETED. Also normalises a newly created keystore, since `.mode()` is a *request* and a hostile umask can clear owner bits. |
| **PR #2777** (#2758/#2759 exclusion) | IMPLEMENTED / UNLANDED | draft; `mergeStateStatus: BLOCKED`. |
| **#2750** trust bloom filter | IDENTIFIED | no PR. **Next correctness blocker.** |
| **#2746 / #2739** verify-backup | IDENTIFIED | no PR. |
| N2-A principal-state lane | LANDED / UNVERIFIED | #2700-#2716 merged; not exercised on an Alpha profile. |
| Institutional genesis (#2744) | LANDED / UNVERIFIED | merged as `fc9e7b8b6` (#2749); disclaims canonical genesis (6.6). |

### Alpha lanes

| Lane | State | Blocking fact |
|---|---|---|
| Deployment profile | BLOCKED | ADR-0086 exists and is merged but `status: proposed` / partially implemented — **adoption not decided**; two-node plan is `Canonical: no`. |
| **#2694** semantic convergence | IDENTIFIED | 1 of 12 slices owned (#2695); none implemented; no artifact exists in code. |
| **#2465** offline evidence | IDENTIFIED | spec only; 0 of 4 slices owned; blocked on 6.2 for any tamper-negative claim. |
| **#2466** recovery | IDENTIFIED | spec only; completeness blocked by #2746. |
| Two-node witness | BLOCKED | Gate 4 and Gate 6 both blocked. |
| NYCN lock bump | BLOCKED | requires a frozen SHA and a human signature. |
| Public claim | BLOCKED | requires all seventeen #2689 section 10 criteria. |

---

## 11. Critical path and parallel work

### Critical path

```text
#2750  (now the ONLY uncontained defect on the economic chain)
   v
Alpha profile freeze  (needs ADR-0086 ADOPTED, not merely merged)
   v
#2694 slice 1 (#2695 GEN-A) -> slice 2 (N1-D) -> ... -> slice 10 (V4)
   v
V4 decision_hash through AllocationReceipt / SettlementIntent
   v
#2465 slice D  (composed tamper-negative proof)
   v
Gate 4  (Node A stopped)
   v
#2466  -> Gate 6
   v
freeze SHA -> NYCN lock -> rehearsal/a11y -> bounded public claim
```

### Safely parallel right now

- **#2465 slice A** (owner canonical export surfaces) — depends only on
  `icn-kernel-api`, not on the ladder. It is also a *precondition* for any honest
  tamper-negative claim, per 6.2.
- **#2465 slice B** (bundle substrate) — generic and deterministic.
- **#2466 completeness prerequisite** (#2746 extent record) — independent of #2694
  entirely.
- **#2627** DID-spelling defect in legacy `compute_vote_hash` — must precede
  trusting Subject ballots.
- **Resolving the 6.9 decomposition contradiction** — a maintainer decision, no
  code.

### Blocked, not worth starting

Slices 3-12 of #2694 (each gated on its predecessor and on owner-contract
reverification); #2465 slice C (needs the execution/journal shape the ladder
produces); Gates 4, 5 and 6; anything downstream of the profile freeze.

---

## 12. Navigation

| Question | Authority |
|---|---|
| What is being worked on right now? | `gh issue list` / `gh pr list` — domain `live_issue_state` |
| Program control surface and exit criteria | **icn#2689** |
| Semantic convergence ladder | **icn#2694**; GEN-A child **icn#2695** |
| Offline evidence contract | **icn#2465** |
| Encrypted recovery contract | **icn#2466** |
| Two-node acceptance gates | `docs/demo/TWO_NODE_APPLIANCE_PROOF_V0.2_PLAN.md` (`Canonical: no`) |
| Identity semantics | `docs/architecture/IDENTITY_SEMANTICS.md` (**normative**) |
| Receipt/provenance envelope | `docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md` |
| Verification vocabulary | `icn/crates/icn-governance/src/verify.rs`; `docs/spec/receipt-chain-verification.md` |
| Fact ownership | `ops/state/truth/sources.json` |
| PR delivery lifecycle | `ops/state/truth/delivery.json` |
| Merge policy | `ops/state/truth/policy.json` |
| Contract document conventions | `docs/contracts/` (`.md` + `.schema.json` + `.example.json`) |

---

## 13. Maintainer decisions outstanding

1. **Reconcile the two decompositions** (6.9.1): #2694's 12-step ladder versus
   #2689's P0-P10 proof ladder. Pick one, or state how they compose. Until then the
   program has two disagreeing structures.
2. **Land or close PR #2690** (6.9.2), which carries the machine-readable
   `program_structure` domain this document deliberately does not duplicate.
3. **Adopt or reject ADR-0086**. It is merged but `status: proposed`; adoption
   is the human decision the profile gate actually waits on. Also whether the two-node
   plan should be promoted from `Canonical: no`.
4. **Decide what the typed chain/audit read surface returns** (6.3) before a V4
   is specified, and whether conditional V3 emission becomes unconditional first.
5. **Accept or reject the 5.3 evidence-strength taxonomy**, which is proposed here
   and exists nowhere in the repository.
6. **Decide whether economics needs a registered truth domain** (6.1).
