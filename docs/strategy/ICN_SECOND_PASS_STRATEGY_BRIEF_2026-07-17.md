---
Status: descriptive (strategy analysis snapshot; NOT canonical, NOT a roadmap)
Canonical: no
Current-state source: docs/STATE.md + docs/PHASE_PROGRESS.md
Last Reviewed: 2026-07-17
Intended reader: ICN maintainers + prospective contributors + partner-facing leads
---

# ICN Second-Pass Strategy Brief (2026-07-17)

> **What this is.** A dated analysis snapshot from a second-pass, multi-lane audit
> (baseline `origin/main b44a1821`). It records a capability-maturity reading, a
> dependency/minimum-cut analysis, a horizon map, and a re-ranked wedge list.
> **What it is not.** It is not canonical state (that is
> [`../STATE.md`](../STATE.md) + [`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md)),
> not a roadmap replacement (that is
> [`ICN-Roadmap-Live.md`](ICN-Roadmap-Live.md)), and it lands no code and makes no
> readiness claim. Where it names a decision, it recommends opening an ADR — it
> does not decide.

## 1. The one sentence that reorganizes the strategy

**ICN's maturity is being measured on the axis that is already winning (software), while the axis that is actually stuck (institutional) has no instrument.** What is publicly established (and consistent with [`../STATE.md`](../STATE.md)): NYCN is an *intended* first partner, **not** a committed pilot; and no committed institution, second maintainer, second operator, or independent professional sign-off is yet in evidence. This brief deliberately publishes **no** private-repository audit detail, authorship statistics, partner-internal fixtures, or repository-governance metadata — only public-safe role descriptions. We also do not claim any specific idle duration, nor that no off-repo organizing relationships exist. A capability-maturity matrix can go green while all of those institutional facts stay empty. The corrective is to add a **non-software institutional-progress axis** (seated ratifying body · committed partner · second maintainer · accountant sign-off · one real obligation run) and to **gate every horizon transition on it**, not only on code.

**The load-bearing rule for that axis: it advances only on a non-AI, human-authorized act** — a real person accepts maintainership, a committee signs a recurring commitment, a key-holder completes a custody ceremony, an accountant accepts a model. A document, issue, test, implementation, or AI-produced strategy (**including this brief**) may *define* a gate but never *satisfies* one. This brief moves the axis by exactly zero.

### 1a. The instrument is not "freeze engineering" — it is the Summit recruitment pathway

A stuck institutional axis is **not** a reason to pause technical work, and this brief must not be read that way. The two axes are **parallel and mutually reinforcing**. The intended mechanism for *filling* the human roles is a recruitment-and-discovery pathway centered on the New York Cooperative Summit and NYCN: continue making ICN technically real, coherent, and contributor-friendly; use the Summit to name the shared coordination problem in cooperators' own language; demonstrate one narrow working workflow (tracked obligation → action card → completion → durable evidence → cleared action) rather than asking anyone to accept the whole architecture; and invite people in at several depths (learn more · give feedback · bounded contribution · share a use case · explore a later organizational conversation · decline). Real interest converts into bounded workstreams *after* the Summit, and deeper stewardship/pilot relationships emerge from informed participation — they are not preconditions demanded before anyone has seen the work.

So the correct posture is: **keep engineering** (the tranches below), while (a) documenting which future capabilities require professional/institutional acceptance, (b) never claiming those roles are filled, and (c) keeping technical work aligned with a credible Summit demonstration and a friendly contributor on-ramp. "Attendance is not consent; interest is not authorization; a workshop participant is not a pilot partner" — but the Summit is the honest funnel through which the institutional axis is *meant* to advance over time.

## 2. The defensible architectural center

ICN is best understood as **two separable things wearing one name**:

1. A **constraint-enforcement substrate + institutional-evidence protocol** — the
   irreducible, genuinely strong kernel: the **Meaning Firewall** (CI-enforced;
   `icn-kernel-api` imports zero domain crates — verified), plus receipted,
   fail-closed, hash-tagged governance events that "record a fact and grant zero
   authority," plus capability/mandate authority. This is the defensible center
   and ICN's real contribution. Its crown-jewel property — a receipted,
   verifiable governance event — is a **format**, not a runtime.
2. A set of **engines, packages, surfaces, and provider infrastructure** that
   translate adopted meaning into bounded effects and render them to humans.
   Mostly early-maturity, and repeatedly exhibiting the project's signature
   failure mode below.

**Strategic implication:** because the center is a *format*, a **format-first
architecture** (a thin verifiable receipt/evidence format + a standalone verifier,
layered over the boring tools co-ops already run) should be **costed** against the
current *substrate-owns-everything* path before another horizon is committed. The
format-first path can dissolve the **infrastructure** walls — self-hosting/operator
burden, always-on dependence, and a share of the bus-factor risk. It does **not**
dissolve the **legitimacy** walls: trusted enrollment, authority legitimacy,
ratification, governance *of the format itself*, and the integration layer someone
must maintain to emit receipts from those tools all remain. It moves the
infrastructure problem, not the enrollment/authority problem. This brief does not
decide it; it records that the comparison is owed (see §7, ADR-C).

## 3. The signature failure mode: built-but-sidelined (composition-root verified)

The strongest mechanism exists but the live/member path uses the weaker one.
Confirmed with installation-site evidence **at the declared baseline `b44a1821`**
— this table is a baseline snapshot, not a claim about present `main`. One row has
since changed: the **receipt-chain verification** row was addressed by PR #2431
(merged), which replaced the string-equality `icnctl audit verify` with mechanical
recomputation — see the implementation-status note in §9 for the post-#2431 state.

| Capability | Strong mechanism that exists | What the live path used at `b44a1821` |
|---|---|---|
| Anti-domination credit limits | progressive (POPLevel) + dynamic limit managers (compiled, tested) | only the flat static limit is installed (`icn/apps/ledger-app/src/init.rs`) |
| Capability revocation | RPC `TokenRevocationList` / `is_revoked` | gateway `verify_token` = decode only, no `jti` |
| Receipt-chain verification | `proof.rs` `compute_record_hash` / `verify_binding` | `icnctl audit verify` = string-equality; store trust-on-write |
| Ledger / federation inbound sync | `handle_sync_message` (full apply) | publish-only; inbound called only in `examples/` |
| On-device member custody | React Native SDK local Ed25519 signing | member-shell pastes a bearer token |
| Effect invariant gating | `EffectManifest` + `InvariantGate` | `translate_payload_to_effects` (ungated) |

**Auditing discipline this implies:** trace every capability to its composition
root (`icn/bins/icnd/src/main.rs`, `icn/crates/icn-core/src/supervisor/**`, `icn/apps/*/src/init.rs`)
and confirm it is *installed*, not merely defined. "The type exists" is not
"it runs."

## 4. Minimum cut sets (the small groups that block the most)

Five institutional-value outcomes, each blocked by a small cut set:

- **A member can legitimately participate** — blocked by {trusted issuance /
  enrollment · on-device-signing-or-honest-session-custody incl. gateway
  revocation · enrollment desk as human labor}. *This is the pilot's precondition.*
- **An institution can adopt and execute a rule** — blocked by {**evaluator
  selection** (no symbol in code — the keystone; until adopted policy binds to an
  evaluator, everything a domain adopts is inert) · mandate on the effect-dispatch
  path · receipt verification}.
- **Two institutions cooperate without a platform sovereign** — blocked by
  {per-node domain identity · inbound sync with conflict-*recording* not blind
  last-writer-wins · agreement under domain authority · governed exit}.
- **A resource is governed as Commons** — blocked by {GovernedServiceBinding
  schema · a witnessed cross-node compute loop · non-inflationary settlement}.
- **An institution survives failure** — blocked by {operator succession (bus
  factor 1) · portable signed export · recovery that rebinds authority · one
  recorded restore drill}.

The capabilities appearing in the most cut sets are **trusted
issuance/enrollment**, **receipt verification**, and **evaluator selection**. The
non-code cuts — **operator succession** and **enrollment-desk labor** — gate the
pilot regardless of code.

## 5. Horizon map (institutional gate in **bold**)

- **H0 — Truth + consolidation (now; mostly doable solo):** ship the receipt-chain
  verifier + adoption receipt; install anti-domination limits; apply the clock-sync
  offset; resolve the license contradiction; extend signing to appliance images;
  write a custody-split/succession doc; add the institutional-progress axis.
  **Gate: a written acceptance authority + a second key-holder exist.**
- **H1 — Credible single-institution operation:** gateway token revocation;
  member-shell on-device/QR-session custody; mandate on the effect path; the
  evaluator-selection seam; an enrollment ceremony; a minimal member challenge
  path. The pilot itself is the **money-free** RANK-1 committee records loop
  (decision → action-item → completion receipt), so its success is measured on
  **both** axes: a real committee **commits to and completes N recurring cycles**,
  someone other than the founder can operate/recover it, the added labor and
  duplicate-entry burden are measured, members can understand and recover the
  evidence, and the institution records a continue/modify/stop decision — not
  merely "prefers receipts to a spreadsheet." **Gate: organizer decision recorded ·
  #2041 AT pass done · second maintainer · a seated ratifying body.** (No
  accountant sign-off here — this pilot moves no money; the accountant is an H3
  gate. See §8.)
- **H2 — Two-domain federation proof:** the M0→M5 program (per-node identity →
  countersigned agreement over real sockets → partition/rejoin → conflict-recording
  → fail-closed unsigned settlement → governed exit); resolve the kernel
  super-domain ordering first. **Gate: two real domains consent.**
- **H3 — Cooperative operating capability:** accountant-legible economics; plural
  primitives (obligation/allocation/settlement); mobile/offline v0; provider role
  delegated off one homelab. **Gate: a support institution exists + a bookkeeper
  signs off.**
- **H4 — Resource Commons; H5 — Protocol ecosystem** (independent implementations;
  published wire specs + conformance suite; stewardship in a body, not a founder).

## 6. Re-ranked wedges (is the receipt verifier still first?)

**Reassessment:** the receipt verifier is *not* the single first priority, and the
binding constraint is not a software wedge at all — it is institutional. Ordering:

- **Tier 0 (highest leverage, non-code, solo):** promote + refresh a top-level
  `GOVERNANCE.md` with an ADR/RFC acceptance authority and a succession plan;
  de-personalization (sign images, custody-split doc, CoC contact, one recorded
  restore drill); resolve the license; add the institutional-progress axis.
- **Tier 1 (small software, co-equal):** the **receipt-chain verifier** —
  **merged to `main`** (`icn-governance::verify` wired into `icnctl audit verify`,
  PR #2431) — backing the "verifiable/export" thesis and rung one of a protocol
  conformance suite. The companion **domain-policy adoption receipt is deferred**
  (issue #2434, pending a crash-atomic cross-store transition), so it is not yet
  part of this tier's delivered surface — **paired with, and not a substitute for, trusted enrollment + gateway
  token revocation**, which remain the actual *member-participation* keystone
  (`DenyUntilWired`/offstage-steward-paste through the pilot) and are the honest
  custody prerequisite (the mobile falsifiable slice fails without revocation).
  Enrollment/revocation touch auth/write paths → a **separate, human-authorized**
  tranche, co-equal in priority to what was built here. Do not read the shipped
  verifier as "member participation is now honest" — it makes *audit* honest, not
  *enrollment*.
- **Tier 2 (cheap correctness):** install anti-domination limits; apply the clock
  offset; rename the settlement-vocabulary fossils.
- **Tier 3 (design-first, later):** evaluator-selection seam; inbound
  sync + conflict recording; portable signed export.

## 7. Recommended ADRs (decisions this brief surfaces but does not make)

- **ADR-A — License resolution.** Root `LICENSE` is AGPL-3.0; the Cargo workspace
  declares `MIT OR Apache-2.0`; no crate declares AGPL; a project-governance doc
  asserts "MIT/Apache regardless." A co-op cannot tell what governs their data.
  *Decide deliberately (network-copyleft runtime + permissive libraries is the
  co-op-aligned default) — needs counsel.*
- **ADR-B — Kernel scope ordering / federation-as-super-domain.** `ScopeLevel`
  derives `Ord` as `Local < Cell < Org < Federation < Commons`, encoding
  federation *above* org — a super-domain, which contradicts the doctrine that a
  federation is an agreement among sovereign domains. *Resolve before building the
  agreement graph (H2).*
- **ADR-C — Format-first vs substrate-owns-everything.** Cost the thin
  verifiable-format path (§2) against the current runtime-owns-everything path on
  operator burden, bus factor, and the leave/repair/export tests. *Owed before
  committing H2.*
- **ADR-D — Self-hosting vs anti-tenancy.** The only path a working-class co-op can
  walk today is hosted, yet the constitution forbids tenancy. *Decide: drive the
  operator floor to near-zero, or adopt cooperative hosting with contractual
  non-tenancy (AGPL alone does not guarantee exit).*
- **ADR-E — Who ratifies machine-authored governance records.** Machine-prepared,
  not-yet-ratified records accumulate with no seated body; the only human input authorizes
  the prompt, not the content.

## 8. Human-validation map (software cannot close these)

Organizer presentation and pilot decision; the #2041 assistive-technology pass
(owner needed); a co-op lawyer (license, legal entity, tenancy, legal-process
exposure of a single node); a working accountant (subsidiary-ledger legibility);
an accessibility specialist and translators (a real `es` catalog is a
precondition, not a nice-to-have); a facilitator (enrollment desk, dispute
intake); a **second maintainer** and a **second key-holder** (bus factor 1 is the
largest non-technical risk); and a field-of-membership definition per pilot
institution.

### 8a. The deepest risk: named roles have no acquisition mechanism

The strategy *names* gates and roles but cannot *staff* them, and naming is not
staffing. By its own separation-of-duties rule ("the person who proves a gate is
not the person who accepts it"), even the deliberately-least-impressive RANK-1
pilot needs **3–4 distinct trusted humans** (steward-operator · named backup
key-holder · privacy/data steward · a second maintainer for security/kernel
review) **plus** a lawyer and — only when money enters — an accountant. None exist
today, and human acquisition is the slowest, most failure-prone variable; a
schedule cannot manufacture it. The strategy must carry an explicit
acquisition table (bounded responsibility · required independence · time ·
compensation assumption · recruitment channel · what counts as filled · what stays
blocked if unfilled):

| Role | Bounded responsibility | Independence | ~Time | Funding assumption | Channel | Counts as filled when | Blocked if unfilled |
|---|---|---|---|---|---|---|---|
| Second maintainer/reviewer | Required review on auth/kernel/economic PRs | Not the author | ~2–4h/wk | **unfunded today** (largest risk) | co-op tech networks, security research, existing contributors | a real person merges with `required_reviews≥1` on those paths | no independent review; bus factor 1 persists |
| Second operator / key-holder | Hold a break-glass key share; run one recorded restore drill | Separate person + location | ~1–2h/mo | volunteer/partner | trusted co-op member, hosting partner | a custody ceremony + one recorded restore drill occur | device/person loss = permanent lockout |
| Pilot institutional sponsor | Consent to + govern the pilot | The institution itself | committee time | none (in-kind) | NYCN committee | a signed recurring-cycle commitment | no committed partner; pilot is founder-run theatre |
| Enrollment steward | Staff the in-person credential desk | Institution role | event-bound | in-kind | pilot institution | a human desk enrolls N members | enrollment is offstage founder-paste |
| Privacy/data steward | Own the never-commit list + HALT | Not the operator | ~1h/wk | in-kind | pilot institution | named person holds the sign-off gate | no separation of duties on data |
| Accessibility owner | Perform/own the #2041 human AT pass | Independent of dev | project | **needs funding** | disability-justice orgs, a11y contractors | a signed human AT pass exists | AT users excluded, not served |
| Co-op lawyer | License · entity · tenancy · seizure exposure | External | advisory | contract/pro-bono | co-op law networks | a written opinion on ADR-A/D | legal posture undecided |
| Co-op accountant | Subsidiary-ledger legibility (H3 only) | External | advisory | contract | NSAC-style practitioners | accepts/rejects a defined export model | economics not legible |
| Ratifying body | Approve machine-authored governance records | A body, never one person | ongoing | in-kind | pilot + maintainers | a seated body records an approval | machine-prepared records accumulate unreviewed |

### 8b. Why a real co-op might decline (adoption hypotheses to test, not dismiss)

1. **Additive work / double entry.** The RANK-1 pilot deliberately excludes live
   Drive/Sheets sync, so the committee keeps its spreadsheet *and* records in ICN —
   the receipt loop is additive, for a "better record" they did not ask for.
2. **It avoids the urgent needs.** The operational needs a cooperative most cares
   about for a real event — settlement of obligations, registration/payment
   handling, and accessibility provision — are blocked (#1634) or external; the
   pilot touches none of that forcing function.
3. **AI-in-governance optics.** An AI-prepared, not-yet-human-ratified governance
   substrate may repel a labor/left co-op audience regardless of the
   ratification mechanism.
4. **Switching/training cost and the burden of operating a second system.**

These are hypotheses to instrument in the pilot, not objections to wave away.

### 8c. Accessibility must not be "proven by exclusion"

The RANK-1 pilot reaches "honest" status partly by running on labels and
**excluding** AT users until #2041 and non-English members until a real `es`
catalog ships. That is an acceptable *scoping* of a first pilot, but it is **not**
an accessibility success — the first "win" is bought partly by exclusion. The
member-included pilot requires: a named accessibility owner, identified
participant access/language needs, defined stop conditions, and no universal
accessibility claim until human validation exists.

## 9. Non-claims

This brief is analysis, not state. It claims no institutional adoption, partner
commitment, security/legal/accounting/accessibility acceptance, federation, or
production readiness, and it does not itself advance the institutional axis.

**Implementation status of the receipt-evidence integrity tranche (updated
2026-07-18):** the tranche was **split**. The mechanical receipt-chain verifier
(`icn-governance::verify`, consumed by a rewritten `icnctl audit verify`) is
**implemented, tested, and merged to `main`** via PR #2431 (squash `6b016ff6`),
per its design contract
([`../spec/receipt-chain-verification.md`](../spec/receipt-chain-verification.md)).
It is a **read-only** re-verifier — it adds no write path — and proves
**integrity/authenticity, not authorization or institutional legitimacy**; it is
not integrated into a running daemon beyond tests. The `DomainPolicyAdoptedReceipt`
**adoption-receipt transition is deferred** (issue #2434): its cross-store commit
needs a crash-atomic write-ahead-log protocol (domain-state and receipt stores
are separate `sled::Db` instances) before durable policy state and committed
evidence can be guaranteed to agree; the full implementation + tests are
preserved on branch `wip/domain-policy-adoption-receipts`. So the "receipt-chain
verification" row of the built-but-sidelined table above describes the baseline
`b44a1821` state (audit verify was a string compare); post-#2431 `main` now has
the mechanical recomputation. Every other capability-maturity level in this brief
is a snapshot at
`b44a1821` and must be re-verified before reuse.
