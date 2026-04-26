# ops/coordination/

Coordination artifacts that span RFCs, ADRs, issues, and doc surfaces. These files are **planning indices**, not authoritative decisions.

## The pipeline

> **RFCs explore. ADRs decide. Issues build. Tests prove. The website claims only what the proof supports.**

```text
   rfc_candidates.yaml      unresolved design spaces, no decision yet
              │
              │  promote
              ▼
   docs/rfcs/RFC-NNNN-*.md  design exploration: options, tradeoffs, alternatives
              │
              │  accept (project converges)
              ▼
   adr_candidates.yaml      likely future decisions; rows updated to point at the accepted RFC
              │
              │  draft ADR
              ▼
   docs/adr/ADR-NNNN-*.md   architectural decision receipt
              │
              │  open issue(s)
              ▼
   GitHub issue             implementation commitment
              │
              │  implement
              ▼
   tests / proof            evidence that implementation matches the decision
              │
              │  proof passes
              ▼
   ADR.implementation_status   not_started → partial → implemented → verified
              │
              │  evidence is public
              ▼
   website/src/             public truth boundary; only claims what evidence supports
```

The pipeline is one-way in spirit. Each arrow filters out designs that should not advance.

## Files

- **`rfc_candidates.yaml`** — Index of unresolved design spaces that need an RFC before an ADR or implementation. Each row describes a design space with enough detail that a future contributor or agent can draft the RFC without re-doing scope discovery.
- **`adr_candidates.yaml`** — Index of likely future decisions ICN intends to record, including rows that have been drafted (Tranche 1) and rows still queued (Tranches 2–4). Some ADR candidates should be preceded by an RFC; others (back-fills of existing implementation reality) can proceed directly. Decisions live in [`docs/adr/`](../../docs/adr/).

## Conventions

### `rfc_candidates.yaml`

- Every row has an `id` (zero-padded, four digits) matching the eventual RFC number, and a `status` of `candidate` until the RFC is drafted.
- Rows describe the design space: why it matters, related ADR candidates, related issues, design questions to address, and proposed outputs.
- Rows flip from `candidate` → drafted (an RFC file appears under `docs/rfcs/`) → resolved (RFC accepted, rejected, superseded, or withdrawn).

### `adr_candidates.yaml`

- Every row has an `id` matching the eventual ADR number, a `registry_state` (`candidate | drafted | merged`), and a `tranche` (`1 | 2 | 3 | 4`).
- `proposed_status` is the status the ADR will carry once written; it is `proposed` for forward-state direction and `accepted` for back-fills of existing implementation reality.
- Drafted rows reference the file path of the ADR (`adr_path`).
- Some candidates should be preceded by an RFC (those whose design space is unsettled). Examples:
  - ADR-0023 (CCL Institutional Process Language) → RFC-0004
  - ADR-0027 (Action Card Contract) → RFC-0005
  - ADR-0028 (Accessibility Baseline) → RFC-0003
  - ADR-0029 (Conflict Resolution Object Model) → RFC-0002
  - ADR-0044/0045/0046/0047/0048 (Economics tranche) → RFC-0001
- Other candidates (e.g., ADR-0021 CCL determinism, ADR-0030 compute manifest) back-fill existing reality and do not need an RFC first.

### Lifecycle distinctions

| Layer | Statuses |
|---|---|
| RFC | `draft` / `active` / `accepted` / `rejected` / `superseded` / `withdrawn` |
| ADR | `proposed` / `accepted` / `amended` / `superseded` / `deprecated` |
| Implementation | `not_started` / `partial` / `implemented` / `verified` |

**Hard rule.** Accepted RFC does NOT mean implemented. Accepted ADR does NOT mean implemented. Implementation status is a separate field on the ADR and changes only with code/test evidence.

## Why this exists

ADR-0034 records the decision to maintain `adr_candidates.yaml` as architectural memory. RFC-0000 records the corresponding decision for `rfc_candidates.yaml`. Without these registries, the long arc of design (CCL as institutional process language, accessibility baseline, conflict resolution object model, federation depth, etc.) tends to fade between sessions and gets re-discovered ad hoc.

The two registries serve different purposes:

- `rfc_candidates.yaml` answers "what design conversations are still pending?"
- `adr_candidates.yaml` answers "what decisions does the project intend to record?"

A single concern often appears in both — first as an RFC candidate (design space unsettled), then as an ADR candidate (after the RFC is accepted), then as an ADR (decision recorded), then as code (issue closed), then on the website (public claim). The two files keep the long arc legible at every step.

## Not authoritative

- These are coordination artifacts. They do not bind project decisions.
- [RFC-0000](../../docs/rfcs/RFC-0000-rfc-process.md) governs RFC lifecycle.
- [ADR-0018](../../docs/adr/ADR-0018-adr-lifecycle-and-canonical-decision-index.md) governs ADR lifecycle.
- Issue numbers in registry rows are pointers, not promises. Issues may be closed without the ADR landing, and ADRs may land without an issue.

## See also

- [docs/rfcs/](../../docs/rfcs/) — RFC documents (RFC-0000 is the seed)
- [docs/adr/](../../docs/adr/) — ADR decision receipts
- [docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md](../../docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md) — long-arc architectural roadmap
