# Flow 4 — Presenter Notes: External Reporting
**Audience**: Cooperative members, organizers, community leaders (non-technical)
**Duration**: ~15 minutes with pauses
**Key message**: A foundation officer can verify everything cooperatives claimed to do — without asking them to self-report.

---

## Beat: Step 0 — Connecting to four nodes
**Say**: "This time we're connecting to all four cooperative nodes. We're going to look at everything through the eyes of an outside observer."
**Point to**: The connection output

---

## Beat: Authenticating all participants
**Say**: "Each participant authenticates. The outside observer — Amara, the program officer — gets read-only access."
**Point to**: The auth output

---

## Beat: Step 1 — The reporting scenario
**Say**: "Amara is a program officer at a foundation that has been funding cooperative development. Her board wants evidence: did these cooperatives actually govern democratically? Did they distribute surplus fairly? Instead of asking for a self-reported PDF, she's going to verify it directly."
**Point to**: The narration
**If asked**: "This is the accountability layer. Not 'trust us,' but 'check for yourself.'"

---

## Beat: Step 2 — Harbor Homes governance evidence
**Say**: "Here's the Harbor Homes roof repair vote. Amara can see the vote records — who voted, what they voted, the outcome. Not a summary from the board — the actual record."
**Point to**: The governance data
**If asked "Could Harbor Homes hide a bad vote?"**: "No. The record is on the network. They gave Amara read access — she's seeing what's actually there, not a curated report."

---

## Beat: Step 3 — BrightWorks patronage evidence
**Say**: "Here's the BrightWorks patronage allocation. The formula, the vote to ratify it, the ledger settlement. Amara can verify: yes, the workers voted on this; yes, the surplus was distributed as they stated."
**Point to**: The allocation data

---

## Beat: Step 4 — Full allocation trail
**Say**: "Every transaction in order. The foundation can trace the full chain from surplus to member wallets. This is not a summary — it's the raw record."
**Point to**: The history
**If asked**: "Yes, a forensic accountant could follow this. It's designed to be auditable."

---

## Beat: Step 5 — Receipt chain
**Say**: "The receipt chain ties every ledger entry back to the governance decision that authorized it. This is the provenance trail."
**Point to**: The receipts

---

## Beat: Step 6 — River City coordination evidence
**Say**: "The federation agreement is also in the record. River City and BrightWorks didn't just claim they were cooperating — the agreement, the terms, the exchange history is all verifiable."
**Point to**: The federation data

---

## Beat: Step 7 — Finger Lakes overview
**Say**: "Finger Lakes' facilitation of the federation is also on record. Amara can see who vouched for what."
**Point to**: The facilitator records

---

## Beat: Step 8 — Authorization boundary
**Say**: "This is the security point: Amara has read access only. She can see everything she needs to verify. She cannot submit a vote, create a transaction, or modify anything. The cooperatives granted her a scoped credential — read, not write."
**Point to**: The access boundary output
**If asked "What if she wanted write access?"**: "The cooperative would have to explicitly grant it. Access is controlled by the cooperative, not by ICN."

---

## Beat: Step 9 — Governance dashboard
**Say**: "One view across all four cooperatives. Active proposals, recent decisions, ledger activity. This is what a network of cooperatives looks like from the outside — a living ecosystem, not a stack of annual reports."
**Point to**: The dashboard
**If asked**: "Foundation program officers spend a lot of time on due diligence. This compresses weeks of back-and-forth into a single verified view."

---

## Beat: Step 10 — The grant report
**Say**: "This is the output: a verifiable report. Not a PDF that Harbor Homes typed — a cryptographic record of actual events. Amara's foundation can grant with confidence because they can verify the claims."
**Point to**: The report output
**If asked "Can she export this?"**: "Yes — the query results can be exported. The foundation can archive its own copy of the verification."

---

## Beat: Step 11 — ExecutionReceiptGate (coming soon)
**Say**: "The last piece: when a cooperative takes an action — signs a contract, makes a payment — the execution receipt will close the loop between 'we voted to do this' and 'we actually did it.' That feature is in development now."
**Point to**: The gap callout
**If asked "When will it be ready?"**: "It's the next milestone. Today you're seeing the governance and ledger layer — the execution receipt gate connects them to real-world actions."

---

## Common Questions

**"What stops ICN from reading this data?"**
"ICN is software, not a service. The cooperatives run their own nodes. There's no ICN server that has access to their data. The data lives on their hardware."

**"Can the cooperative revoke Amara's access?"**
"Yes, at any time. Access is granted via credentials that the cooperative controls and can revoke."

**"What about privacy? Should funders see this much?"**
"That's a governance decision each cooperative makes. They choose what to share and with whom. Some cooperatives might share only aggregated data. This demo shows full access because Amara was granted it — that's a choice, not a requirement."

**"Is this GDPR compliant?"**
"Good question — the answer depends on implementation details and jurisdiction. GDPR compliance is on the roadmap. For now, cooperatives should treat member data the same way they would with any record-keeping system."
