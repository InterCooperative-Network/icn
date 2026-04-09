# ICN Concept Map

**Status:** v0 — canonical source for concept terminology.
**Parent doc:** [brief-v0.md](./brief-v0.md)

This document maps every canonical ICN system concept to:

1. **Canonical** — the stable internal term used in architecture, code, and technical writing.
2. **Public (English default)** — the plain-language label used on public-facing surfaces by default.
3. **Short description** — a one-line gloss suitable for tooltips, subtitles, and helper text.
4. **Localization notes** — guidance for organizations, federations, and translators building local terminology packs.

The rule: the canonical term is the system's semantic anchor. The public label is what shows on default English surfaces. Local institutions may override the visible label as long as the underlying canonical meaning is preserved.

---

## Closure-loop stations (ordered)

These nine concepts constitute the ICN institutional closure loop. They are rendered visually by the `ClosureLoop` component and referenced by number across pages.

### 01. Identity

- **Canonical:** `identity`
- **Public:** *Who you are*
- **Short:** Cryptographic identity held by the member — not a platform account.
- **Localization notes:** The word "identity" is loaded across cultures and legal systems. Local packs may prefer "recognized person," "named participant," or a term rooted in the local institutional tradition. The underlying meaning is *a self-held cryptographic anchor distinct from platform accounts*.
- **Color token:** `--accent-teal`
- **Loop position:** 01

### 02. Standing

- **Canonical:** `standing`
- **Public:** *Your recognized status*
- **Short:** Provable participation inside a scope. The institution can verify it directly.
- **Localization notes:** "Standing" has institutional weight in English but no universal equivalent. Local packs often prefer "recognized status," "member in good standing," "active steward," "confirmed participant," or role-specific terms. Avoid "rank" and "reputation" — both distort the meaning.
- **Color token:** `--accent-teal`
- **Loop position:** 02

### 03. Authority

- **Canonical:** `authority`
- **Public:** *What you are allowed to do*
- **Short:** Derived from scope rules and the decisions members have made under them.
- **Localization notes:** Do not translate "authority" as "power" or "permission." The meaning is *the action-space granted by the scope's rules and the decisions its members have made*. Local packs often prefer "mandate," "authorization," or scope-specific language like "charter permissions."
- **Color token:** `--accent-teal`
- **Loop position:** 03

### 04. Governance

- **Canonical:** `governance`
- **Public:** *How group decisions are made*
- **Short:** Proposals, deliberation, and decisions the institution will honor.
- **Localization notes:** "Governance" has strong institutional resonance in English but can translate as "administration" or "management" in other languages, which distorts the meaning. The underlying meaning is *the collective process by which members decide*. Local packs may prefer "decision-making," "assembly process," or culturally specific terms for collective deliberation.
- **Color token:** `--accent-amber`
- **Loop position:** 04

### 05. Policy

- **Canonical:** `policy`
- **Public:** *The rules this follows*
- **Short:** Shared rules produced by decisions, shaping what can happen next.
- **Localization notes:** "Policy" in English can read as bureaucratic. The meaning is *the binding rules an institution has adopted through its governance*. Local packs may prefer "rules," "bylaws," "charter provisions," or "operating rules."
- **Color token:** `--accent-amber`
- **Loop position:** 05

### 06. Accounting

- **Canonical:** `accounting`
- **Public:** *How resources and obligations are tracked*
- **Short:** Obligations, treasury, patronage, mutual-credit positions — governed social accounting.
- **Localization notes:** Do not translate as "finance" or "bookkeeping" on public surfaces. The meaning is *relational social accounting of obligations and resources between named parties*, not commercial finance. Local packs may prefer "resource ledger," "collective books," "mutual obligations."
- **Color token:** `--accent-blue`
- **Loop position:** 06

### 07. Execution

- **Canonical:** `execution`
- **Public:** *What actually happens*
- **Short:** Where accepted decisions translate into real operational effect.
- **Localization notes:** "Execution" can read as cold or bureaucratic. The meaning is *the step where an accepted decision becomes an operational fact*. Local packs may prefer "action," "effect," "carrying out," "implementation."
- **Color token:** `--accent-amber`
- **Loop position:** 07

### 08. Receipts and Provenance

- **Canonical:** `provenance`
- **Public:** *History and proof*
- **Short:** The chain from authority to outcome — durable, auditable institutional memory.
- **Localization notes:** "Provenance" is precise but rare outside English. The meaning is *the verifiable chain from decision to outcome that an institution can show its members*. Local packs may prefer "history and proof," "institutional memory," "audit trail," or "the record."
- **Color token:** `--accent-green`
- **Loop position:** 08

### 09. Member Experience

- **Canonical:** `member_experience`
- **Public:** *What people can do and see*
- **Short:** Where all of the above becomes something a person can see, use, and live in.
- **Localization notes:** "Member experience" sounds product-y in English. The meaning is *the surface through which an ordinary participant interacts with the institution*. Local packs may prefer "what members see," "the member-facing layer," or simply "the interface."
- **Color token:** `--accent-amber`
- **Icon slug:** `member_experience` — a rounded surface frame with a simplified person silhouette inside. Communicates "the member inhabits the context of the surface" without reading as a phone mockup or duplicating the standalone `identity` person icon.
- **Loop position:** 09 (convergence)

---

## Scope concepts

Concepts describing where and on behalf of whom action is taken.

### Scope

- **Canonical:** `scope`
- **Public:** *Where you are acting*
- **Short:** A distinct institutional entity the system recognizes — a member, a cooperative, a community, a federation, or a commons.
- **Localization notes:** "Scope" is a technical term in English but can be translated as "context" or "institutional frame." Some cultures may not distinguish between "self" and "group" scopes the same way; helper text matters.

### Cooperative

- **Canonical:** `cooperative`
- **Public:** *A cooperative*
- **Short:** A scoped institution owned and governed by its members.
- **Localization notes:** The word "cooperative" has strong legal-structural meaning in some jurisdictions and cultural meaning in others. Local packs should use the local legal term when relevant (e.g., *cooperativa*, *Genossenschaft*, *协同社*).

### Community

- **Canonical:** `community`
- **Public:** *A community*
- **Short:** A scoped institution of people organized around shared rules and purpose, not necessarily an economic coop.
- **Localization notes:** "Community" is broad in English; local packs should narrow it to the institutional form being described (neighborhood association, civic body, mutual aid network, etc.).

### Federation

- **Canonical:** `federation`
- **Public:** *How groups work together*
- **Short:** A coordinating scope that connects distinct cooperatives and communities without dissolving them.
- **Localization notes:** "Federation" has different meanings in different legal traditions. The ICN meaning is *a voluntary coordinating scope with its own governance that does not subsume its member institutions*. Local packs should be careful not to translate it as "union," "alliance," or "merger" unless those words carry the same meaning locally.

### Commons

- **Canonical:** `commons`
- **Public:** *Shared resources*
- **Short:** A scoped domain of shared resources governed by the institutions that participate in it.
- **Localization notes:** "Commons" is a specific English-language tradition (Hardin, Ostrom). Local packs should use equivalent local traditions where they exist — *ejido*, *agora*, *公共资源*, *allmenning*. Avoid translations that sound like "public goods" in a government-provision sense.

---

## Economic concepts

### Agreement

- **Canonical:** `agreement`
- **Public:** *A formal relationship*
- **Short:** A recorded understanding between scopes or members, with standing inside the system.
- **Localization notes:** Translate carefully — "agreement" should not collapse into "contract" in the commercial sense unless that is what is meant.

### Allocation

- **Canonical:** `allocation`
- **Public:** *What has been assigned*
- **Short:** A decision to direct resources, obligations, or authority to a specific party.
- **Localization notes:** "Allocation" is technical. Local packs may prefer "assignment," "grant," or "distribution."

### Obligation

- **Canonical:** `obligation`
- **Public:** *What is owed or required*
- **Short:** A structured claim one party has on another within the system's rules.
- **Localization notes:** Avoid "debt" as a default translation — it loads commercial connotations. Local packs may prefer "duty," "commitment," "owing."

### Settlement

- **Canonical:** `settlement`
- **Public:** *How accounts are resolved*
- **Short:** The process of discharging obligations between parties.
- **Localization notes:** "Settlement" is a precise accounting term in English and a loaded colonial term in other contexts. Local packs should strongly consider alternatives — "resolution," "clearing," "balancing."

### Attestation

- **Canonical:** `attestation`
- **Public:** *A verified claim*
- **Short:** A signed statement by one party that another party can rely on.
- **Localization notes:** "Attestation" is formal. Local packs may prefer "witnessed statement," "verified claim," "confirmation."

---

## Membership concepts

### Role

- **Canonical:** `role`
- **Public:** *Your function in this context*
- **Short:** A named bundle of authority and responsibility within a scope.
- **Localization notes:** Local packs typically have strong culturally specific role names (steward, elder, facilitator, delegate). Use them.

### Membership

- **Canonical:** `membership`
- **Public:** *Your place in the group*
- **Short:** Recognized participation in a scope.
- **Localization notes:** "Membership" is institutionally loaded. Local packs may prefer "belonging," "participation," "standing" (careful — distinct from the canonical `standing`).

### Resource

- **Canonical:** `resource`
- **Public:** *Something shared, used, or governed*
- **Short:** A tangible or intangible asset the institution tracks and allocates.
- **Localization notes:** Broad term; local packs should specify the resource type when possible (compute, storage, capital, labor-time, etc.).

---

## How this map is used

1. **Public pages default to the Public label.** Helper text draws from the Short description.
2. **Canonical terms appear when precision matters** — technical docs, developer surfaces, the architecture page. Always paired with the Public label on first use.
3. **Future localization packs override the Public label** per scope or region. The canonical term is the stable anchor; the visible label is the render.
4. **Color tokens are reinforcement, not meaning.** The label is the source of truth. Color is a hint.

---

## v1 expansion targets

- Icon slug for each concept (once the symbol family exists)
- Locale file path and translation status
- Example local labels for at least three real organizational traditions (worker co-op, agricultural federation, mutual aid network)
- Semantic relationship graph between concepts (e.g., `standing` depends on `identity`, `authority` derives from `governance` operating on `policy`)
- Machine-readable version as `website/src/data/concepts.ts` for runtime consumption

---

## Related artifacts

- [brief-v0.md](./brief-v0.md) — the full design language brief
- [accessibility.md](./accessibility.md) — accessibility rules that apply to every concept's visual treatment
