# ops/coordination/

Coordination artifacts that span ADRs, issues, and doc surfaces. These files are **planning indices**, not authoritative decisions.

## Files

- `adr_candidates.yaml` — Index of every ADR ICN intends to write, including rows that have been drafted (Tranche 1) and rows still queued (Tranches 2–4). Decisions live in [docs/adr/](../../docs/adr/); this file just tracks what should exist there over time so that long-arc architecture is not accidentally designed out.

## Conventions

- Every row in `adr_candidates.yaml` has an `id` matching the eventual ADR number, a `registry_state` (`candidate | drafted | merged`), and a `tranche` (`1 | 2 | 3 | 4`).
- `proposed_status` is the status the ADR will carry once written; it is `proposed` for forward-state direction and `accepted` for back-fills of existing implementation reality.
- Drafted rows must reference the file path of the ADR (`adr_path`).
- Candidate rows describe the ADR with enough detail that a future agent can draft it without re-doing scope discovery.
- The registry is updated when a tranche lands; rows flip from `candidate` → `drafted` → `merged` over time.

## Why this exists

ADR-0034 records the decision to maintain this registry as architectural memory. Without it, the long arc of design (CCL as institutional process language, accessibility baseline, conflict resolution object model, federation depth, etc.) tends to fade between sessions and gets re-discovered ad hoc.

## Not authoritative

- This is a coordination artifact. It does not bind project decisions.
- ADR-0018 governs ADR lifecycle. Status changes happen in the ADR file itself.
- Issue numbers in registry rows are pointers, not promises. Issues may be closed without the ADR landing, and ADRs may land without an issue.
