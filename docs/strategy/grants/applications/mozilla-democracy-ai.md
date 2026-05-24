---
Status: draft
Funder: Mozilla Foundation Incubator — Democracy x AI Cohort 2026
Amount target: $50,000
Deadline: 2026 cohort applications (verify current window)
Last Reviewed: 2026-05-19
---

# Application Draft — Mozilla Democracy x AI Cohort 2026

**Apply at:** [mozillafoundation.org/.../democracy-ai-cohort](https://www.mozillafoundation.org/en/what-we-do/grantmaking/incubator/democracy-ai-cohort/)
**Status:** Draft — verify 2026 cohort application window before submission

## Critical framing decision

Mozilla's theme is AI x democracy. ICN is NOT an AI project. **Two ways to play this:**

### Reframe A — "AI-resistant governance substrate"
Position ICN as the institutional substrate that democratic organizations need *because of* AI's disruption to information ecosystems. AI makes deepfakes, manipulated content, and synthetic actors trivial. Cooperative organizations need cryptographically-verifiable governance receipts so that decisions made by their members can be distinguished from AI-generated or manipulated artifacts. ICN provides that verifiability.

**Strength:** Honest. ICN's value is enhanced (not undermined) by AI's emergence.
**Weakness:** Mozilla's theme is "AI in service of democracy," not "infrastructure that protects against AI." Risk of rejection for being adjacent to theme.

### Reframe B — "Community-led governance models" priority area
The Democracy x AI program explicitly names "**community-led governance models**" as a priority area. ICN is exactly that — a substrate for community-led governance. The AI framing in this case is: as AI reshapes how communities make decisions (recommendation systems, content moderation, automated drafting), the underlying *governance infrastructure* the community uses determines whether those AI tools serve the community or capture it. ICN's constraint engine architecture means apps (including AI-augmented apps) translate into receipts the community can verify.

**Strength:** Directly hits a named priority area.
**Weakness:** Slight stretch from the application as a whole.

**Recommended:** Reframe B. The "community-led governance models" priority area is the cleanest fit.

## Fit check

| Mozilla priority area | ICN match |
|---|---|
| Information ecosystem resilience | △ Indirect — ICN's receipts give cooperatives a verifiable internal record |
| Algorithmic accountability | △ Indirect — ICN is the substrate apps including AI-augmented ones run on |
| Transparency and agency | ✓ Direct — every decision produces a verifiable receipt members can audit |
| **Community-led governance models** | ✓ **Direct** — this is what ICN is |

## Application sections (Mozilla incubator structure)

### 1. Project name
InterCooperative Network — Community-led governance infrastructure for the AI era

### 2. One-sentence summary
ICN is a peer-to-peer coordination substrate that lets cooperatives and community organizations make decisions in ways AI tools can serve but not capture — every governance action produces a cryptographic receipt the community can verify, regardless of which AI tools were involved.

### 3. Problem
Democratic organizations make decisions every day. As AI reshapes the tools they use to deliberate, vote, summarize meetings, draft policy, and communicate, the question of *who actually decided what* becomes harder. A meeting transcript could be AI-summarized. A vote tally could be aggregated by software. A facilitator's notes could be drafted by an LLM. The members of the cooperative deserve to know what was *actually* decided by them — and to have that record survive the rapid churn of AI tools.

### 4. Solution
ICN's constraint engine: apps (including AI-augmented ones) translate domain semantics into generic constraints; the kernel enforces those constraints and emits receipts. The receipt is signed, content-addressed, and can be verified by any member independently. AI tools become *participants* in the process, not *gatekeepers* — and the community retains the ability to audit what was decided regardless of what tools were used.

### 5. Why us
- 451K lines of Rust, 5,933 tests passing
- K3s cluster live since Dec 2025, federated demo flows working
- NYCN (NY Cooperative Network) preparing first pilot deployment
- Constraint engine architecture is shipped, not theoretical
- Member shell + steward cockpit specs landed May 14–15 with explicit accessibility-first design
- Lead developer is cooperative organizer (NY Cooperative Summit co-organizer); not a parachuting techie

### 6. What the $50K funds
A 9-month integration sprint:
- **AI-tool boundary spec** — declare a contract for how AI-augmented apps in a cooperative's tool catalog can compose with ICN's governance substrate without compromising the receipt chain
- **Reference AI-augmented governance app** — demonstration app that uses an LLM to summarize a deliberation, but where the summary itself produces a `MeetingMinutesReceipt` the community can verify and where the underlying transcript receipts remain intact and queryable
- **Member shell AI-transparency surface** — UI patterns showing members which decisions touched AI tools, what the tools did, and how to verify
- **Documentation + community workshop** materials so other cooperatives can adopt the pattern

### 7. Cohort participation
- 12-week program engagement
- Cross-pollination with other Democracy x AI cohort members
- Mozilla Foundation amplification + visibility
- Final demo at end of cohort

### 8. Budget
- Lead developer time: $30,000
- Mobile / member-shell UX contract work: $12,000
- Workshop materials + travel: $3,000
- Fiscal sponsor fees: $5,000

**Total: $50,000**

## Open questions before submission
- [ ] Verify Democracy x AI 2026 cohort application window (early 2026 opened, closing date?)
- [ ] Confirm Reframe B framing is honest (not stretching ICN into something it isn't)
- [ ] Identify the specific AI-augmented governance app demo (something concrete + presentable)
- [ ] Confirm SSDI-compatible payment structure
- [ ] Pre-cohort interview prep — Mozilla cohort programs usually involve interviews

## Submission checklist
- [ ] Application written (likely 2-3 pages + supplementary materials)
- [ ] Demo video or live demo URL
- [ ] Repo linked
- [ ] Submitted before cohort close
- [ ] Logged here with submission ID
