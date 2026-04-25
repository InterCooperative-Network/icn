---
Status: normative (architecture doctrine)
Authority: architecture (authoritative on what democratic-cybernetic primitives ICN core may model and what stays in institution packages)
Canonical: yes
Last Reviewed: 2026-04-25
Companions: `INSTITUTION_PACKAGE_BOUNDARY.md`, `KERNEL_APP_SEPARATION.md`, `IDENTITY_MEMBERSHIP_ARCHITECTURE.md`
---

# Democratic Cybernetic Primitives

> ICN is **not** a cybernetic command room. It is a substrate for democratic reflexes.

This document defines the foundation for the next generation of generic primitives ICN core will eventually model — institutional signals, escalation, action cards, temporary authority, governed indicators, obligation/allocation/settlement, and the support-institution layer — without implementing any of them in this PR.

The point is to fix doctrine before the runtime so that, when each primitive lands, it lands shaped like a nervous system reflex (member-originated, scoped, expiring, receipted, contestable) and not shaped like a dashboard for someone above the membership.

The lessons encoded here come from four traditions:

| Tradition | Lesson kept | Anti-pattern rejected |
|---|---|---|
| **Cybersyn / Stafford Beer** | Build a nervous system; recursive autonomy; institutional pain signals. | Priest-room dashboard socialism — central command pretending to be cybernetic. |
| **OGAS** | Information bottlenecks are political power; transparency threatens bureaucracy; anti-bypass architecture (scope + source + receipt + review). | Modeling the whole economy first; surveillance pretending to be planning. |
| **Participatory budgeting** | Democracy gets real when members touch resource decisions; visible tradeoffs; receipts. | Black-box allocation; "delegated to the staff" as the answer. |
| **Mondragón** | Federations need support institutions, not just votes — education, onboarding, mutual aid, shared services. | Hardcoding any one federation's school/fund/service as a core primitive. |

## Core stance

We are not building dashboards.
We are building **reflexes**:

- **Standing** tells members where they are.
- **Action cards** tell them what needs them.
- **Signals** tell the institution where it hurts.
- **Member-originated signals** let reality travel **upward from below**.
- **Temporary authority** makes crisis power visible, scoped, expiring, and reviewable.
- **Indicators** make metrics governable and challengeable.
- **Receipts** prove legitimacy.
- **CCL** expresses institutional rules.
- **Rust host runtime** executes and stores generic lifecycle transitions.
- **Institution packages** define local meaning without poisoning the substrate.

If a future PR adds a primitive that doesn't earn its keep across at least two unrelated institutions, it belongs in an institution package, not in core.

---

## 1. Recursive autonomy

ICN's entity / structure / activity / program / federation graph is the substrate's expression of **recursive autonomy** (Beer's Viable System Model, expressed as data, not as a graphic).

| Layer | Authority comes from | What it owns |
|---|---|---|
| **Individual** | DID identity. | Their own attestations, signatures, signals. |
| **Cooperative** | Charter + governance domain + treasury account. | Local operations and economic life. |
| **Community** | Charter + governance domain. | Civic decisions; member standing. |
| **Federation** | Charter + governance domain + `member_entities`. | Inter-cooperative coordination, **only what must be coordinated above**. |
| **Structure** (committee/working group) | Delegated authority via `RoleAssignment.authority_scope` from a parent entity. | Bounded operational mandate. |
| **Activity / Program** | Time-bounded mandate from a parent entity. | A specific endeavor; not sovereign. |

### Doctrinal rules for recursive autonomy

1. **Local action is the default.** A primitive that requires federation-level approval to do *anything* is a coordination smell.
2. **Escalation must be explicit.** A federation never automatically pre-empts a cooperative's autonomy — it must traverse a defined escalation path with receipts.
3. **Federation coordinates only what cannot be coordinated locally.** Treasury policy, charter conflicts, inter-cooperative settlement, federation-level legitimacy challenges. Not "everything."
4. **Recursion all the way down.** A federation of federations works the same way. No special "top" layer.

### Why ICN is a nervous system, not a command center

A command center is a small group looking at a large screen and deciding for the rest. A nervous system is a body in which each part has its own reflexes, and signals travel up and down so that pain in one finger reaches the brain *and* the body's response respects the finger's autonomy.

ICN's job is to provide the **wiring** for that nervous system:

- the standing, role, and authority surface that lets each part know who it is;
- the signal/escalation surface that lets pain travel;
- the receipt/decision surface that proves the response was legitimate;
- the contestation surface that lets a part challenge a metric or decision aimed at it.

**Not in scope for ICN core**, ever: a "control panel for the federation president." A federation that needs one has misunderstood what federation is.

---

## 2. Institutional signals (future generic primitive)

> Cybersyn's actual contribution wasn't the room — it was the idea that **institutional pain should be visible and routable**.

An `InstitutionalSignal` is a generic, future record that says: *"something at this scope crossed a threshold or was raised by a member, and it needs attention."* It is not yet implemented.

### Sketch of the future shape (illustrative — not a contract)

```text
InstitutionalSignal
├── id
├── domain_id            (governance domain)
├── source               (rule | member | external_observer | scheduled)
├── kind                 (opaque string — "blocked_work", "accessibility_concern",
│                         "obligation_overdue", "metric_challenge", ...)
├── severity             (info | watch | concern | escalation | emergency)
├── opened_at, opened_by_did
├── closed_at, closed_by_decision_id  (decisions/receipts that resolved it)
├── escalation_state     (open | acknowledged | escalated | resolved | rejected)
├── related_refs         (proposals, structures, activities, role assignments)
└── receipts[]           (every state transition leaves one)
```

### Sketch of `SignalRule` and `EscalationPolicy`

A `SignalRule` is a generic CCL-or-data trigger: *"this state of affairs must raise a signal of this kind at this severity."* The rule lives in the institution package; the runtime evaluates it.

An `EscalationPolicy` is a generic CCL-or-data routing table: *"a signal of this kind at this severity, opened in this scope, escalates to this body within this window unless acknowledged."*

### What belongs where

| Layer | Owns |
|---|---|
| **ICN core** | `InstitutionalSignal` record, store, lifecycle state machine, escalation execution, receipts, member-originated signal API. |
| **CCL** | Rule expressions for *when* a signal is raised and *how* it routes. |
| **Institution package** | Vocabulary of signal kinds (`accessibility_concern`, `sponsor_overdue`, etc.), severity-to-kind mapping, escalation paths. |

### How signals become action cards

A signal at the appropriate severity, scoped to a body the caller belongs to, surfaces in that caller's `/me/action-cards` (future) as something they're being asked to attend to. The signal record itself is the unit of institutional truth; the action card is the member-facing rendering.

### Anti-patterns this primitive must refuse

- **Signal-as-broadcast.** A signal must have a defined target body, not just be screamed into the federation.
- **Signal-as-ticket.** Signals are not tickets. They're institutional pain. Closing one without a receipt is a violation.
- **Severity inflation.** Severity ladder must be defined; arbitrary "P0/P1" inflation defeats prioritization.

---

## 3. Member-originated signals

> The OGAS lesson, harder than people remember: information bottlenecks are political power. If only the top can sense, the top owns reality.

ICN's signal subsystem is **not** purely top-down sensing. It must include `MemberSignal` — a signal opened by a member acting in their own DID, addressed to a body they belong to.

### Examples (vocabulary owned by the institution package, not by ICN core)

- A member raises an **accessibility concern** about a venue or process.
- A worker raises a **blocked-work** signal because an upstream decision is overdue.
- A volunteer raises an **overload** signal because their committee can't absorb more.
- A member raises a **legitimacy challenge** against a decision they were excluded from.
- A member raises a **metric challenge** against an indicator they believe misrepresents reality (see §6).

### Doctrine

- **No surveillance-only sensing.** If the only signals the substrate supports come from rules-on-state, the substrate is a panopticon.
- **Member signals are first-class.** Same record shape, same escalation, same receipts as rule-originated signals.
- **Anonymous-but-authenticated** must be a future option. A member should be able to raise a signal whose contents are visible and whose authorship is provable to a designated body without being publicly attributed. Out of scope for v1; design space reserved here.
- **The institution must answer.** A member-originated signal that escalates without acknowledgement crosses a defined window and creates an obligation on the receiving body. The receipt of that obligation is permanent.

### What belongs where

| Layer | Owns |
|---|---|
| **ICN core** | `POST /v1/gov/signals` for member-originated signals, the open/acknowledge/escalate/resolve state machine, receipts, audit. |
| **Institution package** | The **kinds** members may raise, the body each kind addresses by default, the SLAs for acknowledgement. |
| **UI / mobile** | The accessible composition surface for raising one. |

---

## 4. Action cards

> Action cards are not a new ontology. They are a **rendering of institutional need that the caller can act on**.

`/v1/gov/me/standing` (#1604) tells you *who you are*. `/v1/gov/me/action-cards` (future, #1608) tells you *what needs you*.

An action card is not a stored entity. It is a derived view computed from:

- the caller's `StandingResponse`;
- proposals in their domains where their vote is open;
- action items assigned to them or to a body they hold a role in;
- signals (§2/§3) targeted at a body they're a member of;
- temporary authority grants (§5) about to expire or pending review.

### Why the card layer exists

- **Reduces the cognitive cost of participation.** A member should not have to traverse five endpoints to find the next thing.
- **Makes the institution legible from below.** Rendering need as cards puts the substrate in service of the member, not the other way around.
- **Mobile-shaped.** Cards are paginatable, dismissable, and addressable by a single tap. The desktop dashboard is a happy accident, not the design target.

### Doctrinal rules

1. **A card is a view, not a record.** It points at a primitive. Closing a card means closing the underlying primitive (vote, action item, signal); the card disappears as a derived consequence.
2. **No card without provenance.** Every card carries the source primitive's id and the route to its detail and receipts.
3. **Accessibility is a substrate concern.** Card text, ordering, and interaction must be accessibility-first. ICN core defines the data shape; the institution package may shape the *strings* but not the *structure*.
4. **No card invents authority.** A card cannot grant authority the standing doesn't already imply.

### Relationship to other primitives

- **`/me/standing`** is the source of truth for *who you are.*
- **Action cards** are the source of truth for *what's currently asking for you.*
- **Receipts** are the source of truth for *what you and others have already done.*
- **Signals** are the source of truth for *where the institution hurts.*

---

## 5. Temporary / emergency authority

> The hardest historical lesson: emergency powers are not the problem. **Permanent emergency powers** are the problem.

A future `TemporaryAuthorityGrant` primitive will let a body extend a member's authority for a bounded purpose, with a defined expiry and a mandatory post-review.

### Required properties

| Property | Why |
|---|---|
| **Scoped** | Carries an explicit `authority_scope: Vec<String>` like a normal `RoleAssignment` (#1629). The scope says exactly what the grant covers. |
| **Expiring** | Carries `expires_at` that the runtime enforces. After expiry the grant is silently inert; revival requires a fresh grant. |
| **Receipted** | Issuance and exercise are logged with full provenance. |
| **Post-reviewable** | Closes into a review record that the granting body must acknowledge. Failure to review opens a `legitimacy_challenge` signal automatically. |
| **Visible** | Always surfaces in `/me/standing` and `/me/action-cards` for both grantee and granting body. |

### Doctrinal rules

1. **No silent permanence.** A grant that lapses without review leaves a record. Renewals are explicit, not automatic.
2. **No invisible scope.** A grant that names "trust the bearer" is rejected at construction.
3. **Scope ≠ sovereignty.** A grant cannot escalate a structure into an entity, cannot create new entities, cannot move treasury beyond the granted scope.
4. **Crisis is not a license.** The runtime treats `severity = emergency` signals the same way it treats other signals: through the same scoped, expiring, reviewed grant flow.

### Relationship to today's authority surface

- `RoleAssignment.authority_scope` (now plumbed end-to-end via #1630) is the **standing-grade** authority surface.
- `TemporaryAuthorityGrant` will be the **moment-grade** authority surface.
- Both will roll into `StandingResponse.authority_scopes`, with provenance distinguishing standing vs grant.

---

## 6. Governed indicators / metric humility

> Metrics are not reality. Treating them as reality is how dashboards become priesthoods.

A future `Indicator` primitive will let an institution publish a measurement *with* the structure required to govern it.

### Required fields

```text
Indicator
├── id
├── name                   (display)
├── definition             (precise — what is being counted, how, with what window)
├── owner                  (a structure or role — the body that maintains it)
├── caveats[]              (known reasons the number can mislead)
├── review_interval        (when the definition itself must be re-examined)
├── challenge_path         (link to the signal kind a member raises to dispute it)
└── current_value, samples[]
```

### Doctrinal rules

1. **Owner-required.** An indicator with no body responsible for it does not get published.
2. **Definition first.** The raw number is meaningless without the definition; the runtime refuses to surface a value without one.
3. **Caveats are first-class.** An indicator without disclosed caveats is a propaganda surface.
4. **A challengeable indicator.** Every indicator must declare the signal kind a member uses to dispute it (linking back to §3). Unchallenged indicators rot into priestly facts.
5. **Re-examination is scheduled, not optional.** The runtime will eventually open a `metric_review_due` signal when a `review_interval` lapses.

### What belongs where

| Layer | Owns |
|---|---|
| **ICN core** | `Indicator` record, `current_value`/`samples` storage, review-due signal emission. |
| **CCL** | Definitions, computation rules, threshold-to-signal bindings. |
| **Institution package** | The vocabulary of indicators, the bodies that own them, the locale-specific caveats. |

### Anti-patterns

- **Dashboard priesthood**: a screen of green numbers no member can dispute.
- **Counter-as-truth**: a metric whose definition is "what the counter happens to count today."
- **Owner-less metric**: a number nobody is responsible for.

---

## 7. Resource governance (future obligation / allocation / settlement primitive track)

> Participatory budgeting works when members **see the tradeoff and authorize it**, not when they ratify a finished plan.

A future generic primitive set will let an institution model the lifecycle of resource flows in a regulatory-safe vocabulary:

| Primitive | What it is |
|---|---|
| **Obligation** | A commitment one party makes to another (e.g. a sponsor commitment, a mutual-aid pledge, a federation contribution). Lifecycle: open → fulfilled / partially-fulfilled / lapsed / cancelled. Receipts at every transition. |
| **Allocation** | An authorized intent to direct units (of any settlement asset) toward a purpose. Lifecycle: proposed → authorized → in-flight → settled / reversed. Authorization comes from a governance receipt. |
| **Settlement** | The terminal record that an allocation actually moved units, with full provenance. Settlement is **not** a payment — it is a recognition that two ledger positions changed in a coordinated way. |
| **Position** | A snapshot of an account's units at a point in time, scoped to a settlement asset. |
| **Unit** | An opaque count, qualified by a settlement asset. |
| **Settlement asset** | The thing units are denominated in (commons credit, mutual-credit unit, hour, etc.). Not a currency. |

### Vocabulary discipline (regulatory-safe)

| **Use** | **Avoid** |
|---|---|
| settlement | payment |
| unit | currency |
| position | balance |
| settlement asset | currency code |
| obligation | invoice / bill |
| allocation | wallet transfer |

This is not a stylistic preference. It's a regulatory boundary. Hosting a "wallet" with a "currency balance" that processes "payments" pulls ICN deployments into a category the substrate must not be in. The vocabulary above keeps the lifecycle expressible without performing the regulated activity.

### Why these are not in core today

The runtime would have to know what an "asset" is and how a "position" is computed. NYCN's sponsor/budget lifecycle is currently expressed in package data because the generic primitives don't yet exist. **The graduation rule applies**: only when a second institution's resource lifecycle has the same shape with different content does the primitive earn a place in core.

### Doctrinal rules (anticipating the design)

1. **No flow without authorization.** An allocation without a governance receipt is rejected at construction.
2. **No allocation without obligation or program ref.** Resources don't move "for general purposes."
3. **No settlement without a matching allocation.** No after-the-fact reconciliation as the source of truth.
4. **Receipts are immutable** and form the primary audit chain.
5. **Member-originated obligation challenges** (§3) are first-class — *"this obligation should not have been authorized"* must be raisable.

---

## 8. Mondragón support-institution layer

> Mondragón's actual lesson isn't its named institutions. It's that **a federation that doesn't invest in support institutions can't reproduce itself**.

A federation with only governance and only a treasury, but no support layer, runs out of capacity. Members burn out, new cooperatives never spin up, technical knowledge stays trapped in the few who have it, mutual aid happens informally and breaks under stress.

The substrate must make it **possible** for a federation to declare its support layer — without baking any specific federation's school, fund, or service into core.

### What ICN core may eventually know

| Generic primitive | What it carries |
|---|---|
| **Support program** (a `Program` with a particular semantic role) | Something the federation runs *for* its members and member entities, not as direct production: education, onboarding, technical support, legal/admin support, peer mentorship. |
| **Service relationship** | A bilateral declaration: *"this entity provides this service to that entity, under these terms."* Generic — institution package supplies the service vocabulary. |
| **Mutual-support obligation** (instance of §7's `Obligation`) | A declared, receipted commitment from one entity to support another (e.g. a federation member contributes hours to a peer co-op's onboarding). |
| **Resource pool** (instance of §7's `Allocation` + `Position` over a settlement asset) | A pool of units a federation governs collectively for member support. Not in core until the resource-governance track lands. |
| **Federation service catalog / package template** | A package-supplied catalog of support programs and service offerings. Lives in the institution package; ICN core just stores and indexes the structures. |

### What ICN core will never know

- A primitive named `MondragonSchool`, `Caja`, `Eroski`, or any institution's named entity.
- A primitive named `OrganizingCooperative`, `SupportFederation`, `SponsorTier`.
- Any service catalog entry whose semantics are specific to one federation.

### How a federation declares its support layer (today, with current primitives)

NYCN-style packages can already express most of the support layer using existing primitives:

- **Support program** = `BootstrapProgramRecord { kind: Cycle | Custom("onboarding") | ... }` parented to the support entity.
- **Shared service** = `BootstrapStructureRecord { kind: Office | WorkingGroup }` with a clear mandate.
- **Education / onboarding cohort** = `BootstrapActivityRecord` linked to a support program.
- **Inter-cooperative solidarity** = `BootstrapEntityRecord { entity_type: federation, operating_purpose: "<institution-local>" }` plus future obligation primitives.

The graduation rule for moving any of this into core: a second federation, unrelated to the first, must need the same shape with different content.

---

## 9. Anti-patterns (hard rules)

The following are **forbidden** in ICN core code review. Each one is a way the substrate could be corrupted into the thing it must not become.

| Anti-pattern | Why it's forbidden |
|---|---|
| **Dashboard priesthood** | A screen no member can dispute. Indicators (§6) without challenge paths are this. |
| **Surveillance from above without member-originated signals** | The substrate becomes asymmetric: the "top" senses; the membership is sensed. §3 prevents this. |
| **Institution-specific nouns in core** | `OrganizingCooperative`, `SponsorTier`, `MondragonSchool`. Already pinned by `INSTITUTION_PACKAGE_BOUNDARY.md` §C2. |
| **Emergency powers without expiry/review** | Section 5 is non-negotiable: scoped, expiring, receipted, post-reviewed. |
| **Metrics without challenge paths** | Indicators (§6) require a challenge signal kind. |
| **Resource flows without receipts** | Allocations must have authorizing receipts; settlements must reference allocations (§7). |
| **"Support institution" as a bespoke NYCN/Mondragón hardcode** | Use generic Program + Structure + Activity + future Obligation. |
| **Signals-as-tickets** | Closing a signal without a receipt or decision reference erases institutional truth. |
| **Action card as a new ontology** | Cards are a derived view of existing primitives (§4). |
| **Federation-as-supreme-authority** | Recursive autonomy (§1) means federation coordinates only what cannot be coordinated locally. |

---

## 10. Implementation backlog (foundation track)

This document is doctrine. Each section above implies one or more future runtime tracks. None of them land in this PR.

| Track | Scope | Depends on |
|---|---|---|
| **`/v1/gov/me/action-cards`** | Standing-derived action queue (§4). | `/me/standing` (✅ landed), action items, proposals (already exist). |
| **InstitutionalSignal + SignalRule + EscalationPolicy** | Generic signal record, lifecycle, store, escalation execution (§2). | None — can begin design RFC now. |
| **MemberSignal API** | `POST /v1/gov/signals` self-originated, with package-supplied kinds (§3). | InstitutionalSignal lifecycle. |
| **TemporaryAuthorityGrant** | Scoped, expiring, receipted, post-reviewed authority extension (§5). | `RoleAssignment.authority_scope` (✅ landed via #1630). |
| **Indicator primitive** | Definition + owner + caveats + review_interval + challenge_path (§6). | Signal primitive (for the challenge path). |
| **Obligation / Allocation / Settlement** | RFC + design before code (§7). | None — RFC first. |
| **#1623 standalone persistence** (proposals/votes/delegations) | Existing tracked gap. Smoke-testing the above primitives requires decisions to survive restart. | None. |
| **Package-declared support institution scaffolding** | A documented pattern (not a new core primitive) for how an institution package declares its support layer (§8). | None — doc-only. |
| **Federation service catalog** | Package-first; consider core primitive only after a second federation needs the same shape. | Long-term; gate on graduation rule. |

### Companion follow-up issues

The following umbrella issues track the RFCs and design work each section above implies. They were opened together with this doc.

- **#1608** — `feat(gateway): implement GET /me/action-cards — standing-derived participation queue` (§4).
- **#1623** — persist standalone proposals/votes/delegations across gateway restart (cross-cutting; required before any of these primitives is meaningfully smoke-testable).
- **#1631** — `docs(rfc): institutional signals, escalation policies, and member-originated signals` (§2 + §3).
- **#1632** — `feat(governance): design temporary authority grants with expiry and post-review` (§5).
- **#1633** — `docs(architecture): governed indicators and metric challenge paths` (§6).
- **#1634** — `docs(rfc): obligation/allocation/settlement primitives` (§7), explicitly using regulatory-safe vocabulary.
- **#1635** — `docs(architecture): support-institution package pattern for federations` (§8).

The graduation rule from the boundary doc applies to every one of these: only land the runtime in ICN core when a second institution needs the same shape with different content.

---

## Cross-references

- [`INSTITUTION_PACKAGE_BOUNDARY.md`](./INSTITUTION_PACKAGE_BOUNDARY.md) — what belongs in ICN vs what belongs in an institution package.
- [`KERNEL_APP_SEPARATION.md`](./KERNEL_APP_SEPARATION.md) — the meaning firewall between kernel and app.
- [`IDENTITY_MEMBERSHIP_ARCHITECTURE.md`](./IDENTITY_MEMBERSHIP_ARCHITECTURE.md) — DID, membership, standing primitives.
