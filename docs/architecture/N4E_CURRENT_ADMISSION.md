---
Status: normative
Canonical: no
Last Reviewed: 2026-10-04
---

# N4-E — current admission: the relying party's position, from its own retained facts

**Companion to:** `N4A_DEVICE_AUTHORITY_EVALUATION.md` (the historical verifier, §7.1 and §7.2),
`N4B_PORTABLE_EVIDENCE_BUNDLE.md` (carrying facts), `N4D_DEVICE_CHANNEL_BINDING.md` §7 (the
class-2 act that needs this), `HUMAN_IDENTITY_ARCHITECTURE.md` §9.3 (act classes),
`HOME_RUNTIME_IDENTITY_PROFILE.md` row (n) · **Issues:** #2599 (N4), #2694 §6 (G1-A, the class-1
rule this is *not*) · **Code:** `icn/crates/icn-identity/src/device_admission.rs`,
`tests/device_admission.rs`

> **Truth status.** Normative for the admission rule and the refusal vocabulary; **LIB-TESTED**
> (8 tests, observed on icn-dev at `21a6cf541`; the module is unchanged since it landed at
> `18b536ea8`). No runtime, route, transport profile or `icnctl` verb calls it; it is not
> CLI-reachable and not PRODUCTION. It adds **no kernel primitive** and changes nothing in N1 or
> N4-A: the commit that introduced it (`18b536ea8`) touched only `device_admission.rs`, its test
> file and `lib.rs`; `device_authority.rs` and its 25 tests are byte-for-byte what they were. It
> computes one position from a store and then calls `verify_device_act` unchanged. This document
> describes the implementation as it exists; it introduces no semantics the code does not already
> have. Fixture identities only.

---

## 1. Two questions that must not be confused

N4-A answers a **historical** question for a position the relying party supplies:

> *Was device `K` covered by Subject `S` with capability `C` at position `E`?*

That must stay answerable for any `E` the relying party retains, because a decision pinned at
`E` (class 1) must evaluate every act at that `E` forever, and because a later revocation must
not rewrite what was true before it (N4 invariant 4). N4-A deliberately leaves the *choice* of
`E` to the relying party (N4-A §7.1).

For an act that is settled **at admission** — a connection binding, a request to open a session,
anything with an immediate effect (class 2, HIA §9.3) — that choice is not free. If the device
could name any `E` at which it was once authorized, a device revoked at 3 would keep signing acts
bound to 2 forever, and every relying party would honestly confirm that history. N4-E answers
the second question:

> *Is this act valid against the position this relying party itself currently holds a clean
> authority prefix through?*

The two questions have the same verifier underneath and different inputs: N4-A takes `E` as a
parameter; N4-E computes `E` from the relying party's own store and refuses an act that binds any
other position. N4-E is therefore a *caller* of N4-A, never a replacement for it: historical
verification at an explicitly chosen `E` remains available and unchanged for every act, including
one N4-E has refused.

## 2. The rule

For Subject `S` and the relying party's retained store `σ`:

```text
admission_position(S, σ) :=
    match derive(S, σ) {
        Unknown                     → refuse SubjectUnknown
        Halted { disputed_at, .. }  → refuse AuthorityHalted { disputed_at }
        Live { frontier, .. }       → if frontier < 2 then refuse NoDelegationYet
                                      else frontier − 1
    }

verify_device_act_at_admission(act, S, σ) :=
    let A = admission_position(S, σ)?
    if act.evaluation_position < A  → refuse StaleAct { act, admission: A }
    if act.evaluation_position > A  → refuse RelyingPartyBehind { act, admission: A }
    verify_device_act(act, S, σ, A)          -- N4-A, unchanged
```

This is N4-A §7.1 row 2 ("a verifier holding a store whose `derive` reports `Live { frontier }`:
`frontier − 1`") and §7.2 class 2 ("the acceptor evaluates at its own retained `frontier − 1`
once, at admission"), executed rather than left to the caller. Properties that hold by
construction, each pinned by a test (§5):

- **The position comes from the store, never from the act.** The act must bind exactly the
  admission position. Below it is `StaleAct`: the device is behind, or is replaying a position it
  was still authorized at. Above it is `RelyingPartyBehind`: *this relying party* is behind and
  must obtain facts; it never guesses, fetches or falls back.
- **No clock, no network, no registrar, no randomness** enter the decision. The source guard
  test forbids every such symbol in the module.
- **Unknown, halted and inception-only Subjects admit nothing.** No inception retained:
  `SubjectUnknown`. A fork in the retained prefix: `AuthorityHalted` — current authority is
  disputed, so there is no current admission, while history *below* the fork stays answerable
  through N4-A at an explicit `E`. Inception only (`frontier < 2`): `NoDelegationYet` — position
  0 is the inception and is never an evaluation position.
- **N4-A runs unchanged** once the position is fixed. Authorship under the device key and
  authorization over the retained facts remain its two separate checks; every N4-A refusal
  surfaces as `Act(…)` verbatim.
- **Arrival order is irrelevant.** The admission position is a function of the fact set.

## 3. Refusals

| Refusal | Meaning | Who should act |
|---|---|---|
| `SubjectUnknown` | no inception body for `S` is retained | the relying party needs facts |
| `AuthorityHalted { disputed_at }` | the retained prefix forks at `disputed_at`; current authority is disputed | nobody is admitted now; N1 fork handling (O-N7) |
| `NoDelegationYet` | only the inception is retained; no grant exists yet | the relying party needs facts |
| `StaleAct { act, admission }` | the act binds a position below the admission position | the device must re-sign at the current position |
| `RelyingPartyBehind { act, admission }` | the act binds a position above the admission position | the relying party must obtain facts (N3), never guess |
| `Act(DeviceActVerifyError)` | evaluated at the admission position and N4-A refused | as N4-A §7: unauthorized, revoked, wrong capability, bad signature, … |

Every variant is a refusal. There is no partial admission and no "admit at the nearest position
I do hold".

## 4. What this deliberately does not claim

**Retained current position is local evidence, not universal finality.** A relying party whose
facts stop *before* a revocation holds a clean prefix through its old tip and admits there. It is
not wrong about its facts; it is behind. Test 4 pins this rather than hiding it: obtaining facts
is the only cure, and the module offers no other. This is exactly HIA §9.3's statement that R5.1
(a bounded validity window) is **not met for class 2**; N4-E narrows the exposure to "the
relying party's own tip" instead of "any position the device likes", and claims nothing more.

**Class-1 acts do not use this.** A deferred decision pins its own `E` (G1-A, #2694 §6) and
evaluates every ballot at that one position; evaluating each at a receiver-local tip would
reintroduce arrival-order divergence. N4-E is for acts settled at admission.

**No freshness, liveness or revocation-propagation guarantee** is made: no clock, no lease, no
"latest globally". Whether facts reach a relying party in time is N3's question (#2598).

**No session is created.** Admission is one decision about one act; what a runtime does after
admitting is its own policy and never an N1 fact.

## 5. Behaviour pinned by the tests

`tests/device_admission.rs`, fixture: `S` incepted; `A` authorized at 1; `B` authorized at 2;
`A` revoked at 3.

| # | Test | Pins |
|---|---|---|
| 1 | an up-to-date relying party admits at its clean tip | store holds 0..3 → admission 3; `B` bound to 3 → evidence `{ granted_at 2, generation 0 }`; `A` bound to 3 → `Act(Refused(DeviceNotAuthorized))` |
| 2 | a revoked device replaying an older position is refused for current admission | `A` bound to 2 verifies historically at 2 (N4-A) and is `StaleAct { act: 2, admission: 3 }` for admission |
| 3 | an act bound beyond the relying party's facts means it is behind | store holds 0..2 → admission 2; `B` bound to 3 → `RelyingPartyBehind { act: 3, admission: 2 }` |
| 4 | a relying party that has not received the revocation admits at its old tip and says so | store holds 0..2; `A` bound to 2 → admitted at 2 (the honest class-2 limit) |
| 5 | two simultaneously valid devices bind the same current position | before the revoke, `A` and `B` both admitted at 2 |
| 6 | unknown, halted and inception-only Subjects have no admission position | empty store → `SubjectUnknown`; inception only → `NoDelegationYet`; two authorized grants at 1 → `AuthorityHalted { disputed_at: 1 }`, and an act at 1 is refused the same way |
| 7 | admission is a function of the facts, not their arrival order | all 4! ingest orders give admission 3, admit `B`, refuse `A` |
| 8 | admission reads no wall clock and no environment | the module source contains none of `SystemTime`, `Instant::now`, `std::env`, `std::fs`, `std::net`, `rand::`, `OsRng`, `thread_rng` |

## 6. Composition with the other N4 pieces

- **With N4-B.** A relying party that holds its own store computes `admission_position` from it
  and evaluates there. A *stateless* consumer of an `EvidenceBundle` has no store of its own: its
  position is the one the act binds and the bundler assembled for, so it is relying on the
  bundler's selection (N4-B §3.3 and §7) and is not making a current admission. The `icnctl`
  verb is such a consumer: it takes `--position` from the operator and does **not** compute an
  admission position (N4-B §9).
- **With N4-D.** A channel binding is a class-2 `Present` act, and N4-D (`verify_channel_binding`)
  takes `position` from its caller exactly as N4-A does; it does **not** call N4-E. A runtime that
  wants current admission for a live connection computes `admission_position` from its own store
  and passes it as N4-D's `position`. A binding bound to any other position is then refused inside
  N4-D step 3 by N4-A itself (`EvaluationPositionMismatch`), before the channel is compared; the
  `StaleAct` / `RelyingPartyBehind` vocabulary belongs only to `verify_device_act_at_admission`.
  Verifying the act and checking the observed channel remain two separate steps (N4-D §4, §7).
- **With N4-C.** Enrollment produces the N1 grant; admission reads it. Nothing here approves,
  enrolls or revokes.

## 7. Scope boundaries

Not defined here: how facts reach the relying party (N3, #2598); any validity window or lease
for class-2 acts (R5.1, open by HIA's own statement); the class-1 pinned position (G1-A); session
state after admission; a runtime, route or verb that calls this (none exists); fork resolution
(N1 O-N7). `device_admission.rs` has no serde impl, no wire encoding and no persistent state.
