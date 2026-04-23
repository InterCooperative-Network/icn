# NYCN Is Not an App

> NYCN is where ICN stops performing institutionhood and starts containing institutions.

## The Frame

NYCN is not an app built on top of ICN.

NYCN is a federation living inside ICN.

Its organizing, planning, meetings, budget, directory, communication, records, and governance all happen through ICN's native institutional features. Not through a custom side application. Not through a parallel stack. Through ICN itself.

That means **ny-coop-net disappears** as a separate product. There is no second system. No bespoke organizer app. The interface organizers use is ICN's interface: pilot UI, web portal, console, mobile shell, whatever ICN's standard institutional surface becomes.

And that has a hard implication:

If ICN's interface is not good enough for real organizers doing real work, then ICN is not good enough yet.

Fixing that is not "supporting NYCN."
Fixing that **is ICN development**.

---

## The Development Principle

NYCN's needs are not special requests.
They are ICN's feature requirements.

Meeting management is not an NYCN feature.
It is an ICN platform feature.

Budget visibility is not an NYCN feature.
It is an ICN platform feature.

Committee views, event structures, document collaboration, sponsor tracking, role-scoped communication, institutional records, and organizer workflows are not customizations for one federation. They are the baseline requirements of any serious cooperative, community, or federation operating inside the system.

So every sprint, every primitive, every workflow gets tested against one question:

**Can NYCN use this to do real organizing work?**

If yes, ship it.
If no, it is not done.

---

## Identity: Who's Who and Acting As What

Every organizer gets a DID. Every participating organization gets one. The practical value is **scope switching** — Matt acting as himself versus Matt acting as the marketing committee versus Matt acting as an NYCN organizer.

This matters because volunteer organizing is full of ambiguity about who has authority to commit to what on behalf of whom. Right now that's handled informally ("Joe said it was fine"). On ICN, when someone approves a $2,000 sponsorship commitment, the system knows they did it as the finance committee chair with budget authority, not as a random volunteer.

First circle: organizers and participating organizations. Attendees optionally later.

---

## Entity Structure: What Exists

The NYCN federation is the top-level network entity. Under it, the organizer co-op is the body that actually runs things. Under the co-op, you have working groups: steering, content, logistics, marketing, finance. Summit 2026 is a recurring event instance attached to the co-op. Member organizations (the 217+ co-ops, credit unions, nonprofits in the directory) relate to the federation as members or affiliates.

```
NYCN Federation (top-level network entity)
├── Organizer Co-op (the body that runs things)
│   ├── Steering Committee
│   ├── Content Committee
│   ├── Logistics Committee
│   ├── Marketing Committee
│   ├── Finance Committee
│   └── Summit 2026 (recurring event instance)
└── Member Organizations (217+ co-ops, credit unions, nonprofits)
    └── Each relates to federation as member or affiliate
```

The practical effect is that every committee, every event, every organizational relationship is a real addressable thing in the system — not a row in a spreadsheet that someone might delete.

---

## Governance: What Gets Decided and How

The NYCN charter (written in CCL) defines: how decisions get made, what thresholds apply, what authority committees have, and what requires full steering committee approval.

### Concrete Decision Flows

**Venue selection.** Logistics committee researches options, narrows to two or three, and submits a proposal to the steering committee. The proposal includes the options, cost comparison, accessibility analysis, and a recommendation. Steering committee members vote. The charter says venue decisions require simple majority with quorum of half the steering committee. When the vote passes, the decision is recorded with full provenance — who proposed, who voted, what the options were, when it was decided. That receipt is permanent. Next year's logistics committee can look at it and understand exactly why Albany was chosen over Buffalo.

**Budget approval.** Finance committee drafts the budget as a document. It gets submitted as a governance proposal. The charter says budget approval requires two-thirds of the steering committee. When approved, the budget becomes a set of treasury constraints enforced by the charter engine. The marketing committee gets a $500 allocation. The logistics committee gets $3,000. Those limits are real — if marketing tries to commit $600 to a print run, the system knows it exceeds their authority and requires an additional approval or budget amendment proposal.

**Speaker/program approval.** Content committee proposes sessions. Depending on the charter, the committee might have autonomous authority to approve individual sessions (no full-body vote needed), but the overall program structure — keynote selection, track organization, session count — might require steering committee sign-off. The charter defines that boundary. This is the governance versus management line made explicit.

**Committee formation.** If someone proposes a new working group (say, a volunteer coordination committee), that's a governance proposal. It defines the scope, authority, and membership of the new group. When approved, the committee becomes a real entity in ICN with its own scope and permissions.

**Sponsorship commitments above a threshold.** Say the charter sets $500 as the line. Below that, the finance committee can accept sponsors autonomously. Above that, it requires a steering committee vote. When a sponsor commits, it becomes a ledger entry and a governance record.

---

## Ledger: Money and Obligations

The summit budget lives in ICN's mutual credit ledger as a treasury. Every financial event is a double-entry journal record.

**Registration income.** When someone registers and pays, that's a credit to the treasury. The entry links to the registrant's identity (if they have one) or is logged as an external receipt.

**Sponsorship income.** When a sponsor's payment comes in, it's a ledger entry linked to the governance record that approved the sponsorship. The provenance chain is: sponsor commits → governance approval → payment received → ledger entry. An auditor can trace it end to end.

**Grant income.** CDF Ed Fund grant, Verizon Digital Ready — each is a ledger entry with its own provenance.

**Expenses.** When the logistics committee books a venue deposit, that's a debit from the treasury. The entry references the budget authorization and the committee's spending authority. If the finance committee set a $3,000 logistics allocation and $2,800 has already been spent, the system knows there's $200 left before the committee needs to request more.

**Financial reconciliation.** At end of cycle, the ledger gives you a complete financial picture: all income, all expenses, all committee allocations, all surplus or deficit. This is not a manually assembled spreadsheet. It's the actual institutional record.

The real power here is transparency. Every organizer can see the budget position at any time. No more "I think we're on track financially but I'd have to check with Frank."

---

## Trust: Earned Authority

New volunteers start with lower trust. They can participate, take on tasks, join committees. But they can't unilaterally commit the organization to financial obligations or make governance proposals until they've built trust through participation. Experienced organizers start with higher trust based on their history.

This isn't arbitrary hierarchy. It's the same thing that already happens informally ("we wouldn't let a brand new volunteer approve a $2,000 sponsorship"), just made explicit and consistent. The charter can define trust thresholds for different actions: anyone can propose a session topic, but only organizers above a certain trust level can propose budget amendments.

---

## Institutional Memory: The Killer Feature

The current situation: 13 years of organizing knowledge scattered across a Google Drive with inconsistent ownership, naming, and structure. Every year, new organizers reconstruct context from scratch.

On ICN, every meeting record, every decision, every budget, every evaluation, every committee report is a content-addressed, provenance-tracked institutional artifact. It doesn't disappear when someone's Google account changes. It doesn't get filed in the wrong folder. The 2028 planning committee can trace the complete institutional history: here's how 2026 was organized, here's what was decided, here's what the evaluations said, here's what the budget looked like, here's who was on each committee and what they did.

Durable institutional memory. The thing NYCN has never had and desperately needs.

---

## Communication: Committee Channels

ICN's gossip layer gives each committee its own communication channel. Steering committee discussions stay in the steering scope. Marketing coordination stays in marketing scope. Cross-committee announcements go to the broader organizing space. Public announcements go to the community-facing layer.

This replaces email threads, group texts, and "did everyone see what was posted in the Facebook group?" The communication is scoped to the institutional structure.

---

## Federation: The Network Beyond the Summit

The 217 organizations in the directory become real federation members (or affiliates, depending on their engagement level). Each participating co-op has its own entity. The federation surface lets them: discover each other, coordinate on shared concerns, participate in network governance, contribute to shared resources.

The summit then becomes the annual convening of that living network — not a one-off event that a small team scrambles to produce each year.

---

## Where ICN Becomes Real

### Compute

Budget calculations enforcing charter constraints. Evaluation analysis. Automated reports derived from governance and ledger state. Session-speaker matching. Routine institutional computation executed through ICN's compute primitive.

That is how compute stops being theoretical. It starts doing normal organizational work under real institutional constraints.

### Commons

The cooperative directory becomes a shared federation resource. Organizing guides, marketing templates, onboarding materials, speaker resources, archival records, and source indexes become commons artifacts.

Accessible across the federation. Maintained collectively. Persisting across volunteer turnover and leadership transitions.

That is how commons stops being rhetoric and becomes durable institutional memory.

### Federation

Cooperation Buffalo, GreenStar, ICA Group, and every other participant stop being rows in a sponsor sheet. They become federation members with their own identities, entity records, permissions, relationships, and history.

A sponsorship becomes an inter-entity transaction with governance provenance. A co-op sending a speaker becomes a cross-entity contribution tracked in the trust and contribution systems. A working relationship becomes an institutional relationship expressed in the substrate.

### Gossip

Committee communication becomes entity-scoped ICN gossip. The marketing committee channel exists because the marketing committee is a real institutional entity. When the steering committee makes an announcement, it propagates through the federation's gossip topology. Communication is not an external bolt-on. It is what the protocol does when institutions actually inhabit it.

### Trust

A new volunteer joins for the 2027 summit. They receive identity. They join committees. They attend meetings, complete tasks, contribute to decisions, build reputation, accumulate trust. Later, that trust supports real authority: budget access, committee leadership, proposal sponsorship, governance weight. Trust is no longer abstract scoring. It's the tracked history of real participation.

### Charter Engine

NYCN's charter becomes the constitutional document of an actual federation using the system to govern itself. Real quorum thresholds. Real committee authority. Real spending constraints. Real proposal rules. Not demo governance. A real institution governing itself through the charter engine.

---

## What Stays Outside ICN

Not everything should live on ICN. These stay app-layer or external:

- **Task-level operations**: "email this sponsor back," "design the flyer," "book the AV equipment." Management, not governance.
- **Email campaigns**: Mailchimp or equivalent.
- **Ticket sales UX**: SimpleTix or equivalent.
- **Website**: Public marketing surface.
- **Social media**: External platforms.
- **Day-of logistics**: Room assignments, AV setup, volunteer shifts, food service coordination.

**The line**: ICN holds the institutional reality (who, what authority, what was decided, what money moved, what the rules are). External tools handle operational execution.

---

## What This Means for ICN Development

### Pilot UI

Stops being a demo shell and starts being where organizers actually work.

Needs: dashboards scoped to identity and roles, navigation following entity structure (my committees, my federation, upcoming meetings, recent decisions), document viewing and editing, governance proposal submission and voting, ledger/budget visibility, communication, institutional records that remain legible over time.

If organizers cannot run NYCN through the UI, the UI is not finished.

### Gateway API

Full range of operations per organizer session: check my committees, open tomorrow's agenda, add notes, review recent decisions, submit a proposal, inspect budget state, message logistics, track pending votes, retrieve federation documents, follow provenance from governance to expenditure.

That is what a real institutional API looks like.

### Storage

Content-addressed, provenance-tracked artifacts scoped to producing entity: meeting agendas, notes, committee reports, planning documents, evaluation data, templates, images, directories, onboarding material, shared knowledge artifacts. Not just files in a bucket. Institutional memory with authorship, scope, and history.

### Entity Model

Must hold the actual NYCN topology: federation with member organizations, organizer co-op with committees as working groups, event instances, individual members across multiple scopes, nested governance authority, delegated roles, cross-entity relationships.

If the entity model can't represent that cleanly, it can't represent real institutions.

---

## Concrete Scenario: 2026 Summit on ICN

**January**: Steering committee meets. Agenda in ICN storage. Committee reports submitted. Steering votes to approve 2026 budget (two-thirds threshold). Finance committee draft becomes ratified charter constraint. Committee allocations set. Meeting notes become permanent institutional record.

**February**: Marketing committee operates within $500 allocation autonomously. Marketing plan stored in ICN, scoped to marketing working group. $150 Facebook ads spend logged against authorized budget.

**March**: Content committee proposes program structure (four tracks, keynote, 20 sessions) → steering governance proposal → approved. Individual sessions approved within content committee's delegated authority.

**April**: Logistics proposes Albany as venue → steering vote → passes → decision recorded with provenance. Venue deposit ($1,500) = ledger entry debited from logistics allocation, referencing venue approval.

**May**: Sponsor commitments. Below $500: finance committee accepts autonomously. GreenStar commits $1,000 → requires steering approval per charter → proposal, vote, approval, ledger entry.

**June–September**: Operational execution in external tools. Every financial commitment, major decision, and committee action that matters institutionally flows through ICN.

**October**: Summit happens. Registration data links attendees to event. Financial transactions flow through ledger.

**November**: Post-event. Evaluation captured and stored. Financial reconciliation from ledger. Knowledge capture. Everything becomes permanent institutional memory. 2027 planning committee inherits complete, auditable, structured record.

---

## The Demo Changes

When ICN is demoed, NYCN is the demo.

Not an invented scenario. Not abstract "cooperative governance." Not a sterile proof of concept.

A real federation. Real members. Real committees. Real meetings. Real proposals. Real budget flows. Real institutional records. Real governance. Real work.

That is not proof of concept. That is **proof of life**.

And when the next co-op, network, or federation asks what it would actually look like to use ICN, the answer is:

**Look at NYCN.**

NYCN becomes the reference institution. The first real inhabitant of the system. Everything required to make NYCN function is everything required to make ICN function for anyone.

---

*Established 2026-04-13. The first serious proof that ICN works will not be a feature checklist. It will be a real federation using it to govern itself.*
