# Flow 1 — Presenter Notes: Harbor Homes Governance
**Audience**: Cooperative members, organizers, community leaders (non-technical)
**Duration**: ~12 minutes with pauses
**Key message**: A cooperative just ran a fully democratic, verifiable vote — using software that belongs to them.

---

## Beat: Step 0 — Connection
**Say**: "We're connecting to Harbor Homes' node. They run this software themselves — there's no ICN cloud service they're paying for."
**Point to**: The connection output
**If asked**: Nothing complex here — just setup.

---

## Beat: Authenticating as Harbor Homes board
**Say**: "This is the board authenticating. Their cryptographic identity is like a key only they hold — no username and password that can be phished."
**Point to**: The DID in the output
**If asked "What is a DID?"**: "DID stands for Decentralized Identifier. Think of it as a membership credential that's mathematically tied to their keys — only Harbor Homes can use theirs. No central directory, no account to hack."

---

## Beat: Step 1 — Inspection report received
**Say**: "This is a real cooperative making a real decision. The roof inspector flagged major repairs needed. The board has to respond — and the whole membership gets to weigh in. That's governance."
**Point to**: The narration header
**If asked**: "Yes, this is exactly the workflow a housing cooperative runs. We didn't invent the process — we just recorded it on the network."

---

## Beat: Step 2 — Governance domain
**Say**: "Every cooperative has its own governance domain on the network. Harbor Homes controls theirs. Nobody else can create proposals or cast votes in it."
**Point to**: The domain identifier
**If asked "What's a domain?"**: "It's the cooperative's own space on the network — their territory. Only their keys can govern within it."

---

## Beat: Step 3 — Proposal raised
**Say**: "Delphine is the board president. She's raising this to the full membership — not deciding it herself. Anyone with access can see this proposal immediately."
**Point to**: The proposal text
**If asked**: "Yes, in a traditional coop this would be a notice posted or mailed. Here it's instantly visible to every credentialed member."

---

## Beat: Step 4 — Proposal opened for voting
**Say**: "The proposal is now open. Every member gets a vote. The voting window is enforced by the network — not by trusting an administrator to close it fairly."
**Point to**: The open status
**If asked "Who enforces the voting period?"**: "The software does. There's no human administrator who could close it early or extend it to favor an outcome."

---

## Beat: Step 5 — Members vote
**Say**: "Three votes: for, for, for. See the long identifier next to each vote? That's a member's permanent cooperative identity. This vote is public and permanent — you can always check who voted and how."
**Point to**: The vote list with DIDs
**If asked "Is voting anonymous?"**: "In this implementation votes are public — members can see each other's votes. Anonymous voting is technically possible and on the roadmap. Different coops can configure different rules."

---

## Beat: Step 6 — Vote tally
**Say**: "100% in favor. Any member — or any outside observer the coop chooses to give access — can verify this tally independently. They don't have to trust our count."
**Point to**: The tally result
**If asked**: "Right — the tally is computed from the votes on the network. There's no separate 'results spreadsheet' that could be edited."

---

## Beat: Step 7 — Proposal closed
**Say**: "The proposal is closed. The outcome is accepted. This record cannot be altered. If you want to undo this decision, you'd need a new proposal — a new democratic act."
**Point to**: The accepted status
**If asked**: "No, you can't delete it. That's the point — the history is permanent."

---

## Beat: Step 8 — Governance proof
**Say**: "Here's what Harbor Homes can now prove to anyone: this decision happened, this many members voted, this was the outcome, on this date. The roof contractor, a bank, a funder — whoever they want to share this with can verify it without calling Harbor Homes to confirm."
**Point to**: The verifiable result
**If asked "Who do they share it with?"**: "They control access. They choose who gets read access to their governance records. It's not public to the internet — it's selectively shareable."

---

## Beat: Step 9 — Provenance
**Say**: "Any member can query the record right now. They don't have to ask the board. That's what transparency means in a cooperative — not 'we'll send you the minutes eventually.'"
**Point to**: The query result

---

## Beat: Step 10 — Governance to execution
**Say**: "This is the governance-to-execution link. The cooperative decided to repair the roof. The network recorded that authorization. When the repair contract is signed and executed, it can reference this decision as its authority."
**Point to**: The authorization record
**If asked**: "Think of it like the board resolution that a bank requires before a coop can take a loan. Except instead of a PDF, it's a cryptographic record anyone can verify."

---

## Common Questions

**"Is this a blockchain?"**
"It uses similar ideas — a distributed, tamper-evident record that no single party controls. But it's purpose-built for cooperatives, not for trading tokens or running financial speculation. There's no coin."

**"Who runs ICN?"**
"The cooperatives run it. ICN is software they install and operate. There's no ICN Inc. sitting in the middle taking fees or controlling the network."

**"What happens if the software company disappears?"**
"The cooperatives keep running. They have the software. This is exactly the opposite of depending on a platform — the network belongs to its members."

**"How is this different from a Google Form for voting?"**
"With a Google Form, Google could edit the results. The form could be taken down. The data is owned by Google. With ICN, the record is held across all the participating nodes — no single party can tamper with or delete it."
