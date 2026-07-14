# Workstation browser test matrix — LAN Rehearsal Node

**Status: descriptive · non-production.** The canonical step list for a
workstation walkthrough of the LAN rehearsal deployment. Fill one dated copy
per witnessed run (keep filled copies with the run's evidence, not in this
file). Statuses: **pass** / **fail (issue #)** / **follow-up (issue #)** /
**n/a (reason)**.

| # | Area | Step | Expected | Status |
|---|---|---|---|---|
| 1 | Connectivity | Resolve the rehearsal hostname on the workstation | resolves to the VM address via internal DNS | |
| 2 | Connectivity | Open `https://<host>/` (no port) | landing page renders; TLS padlock (internal CA), no warning | |
| 3 | Connectivity | Refresh + direct navigation to `/member-shell/…` | both load; no mixed content; no unexplained console errors | |
| 4 | Landing | Status strip | services available; workspace state + build commit shown | |
| 5 | Organizer | Start a new rehearsal (one click) | organizer session starts; no credential visible anywhere | |
| 6 | Organizer | Review the pending item | item listed and reviewable | |
| 7 | Organizer | Edit content | edit persists in the workspace | |
| 8 | Organizer | Assign the member-readable holder | assignment recorded | |
| 9 | Organizer | Preview | exact plan + preview digest shown; confirmable | |
| 10 | Organizer | Confirm with a stale digest (edit after preview) | refused, fail-closed (409); helpful message | |
| 11 | Organizer | Confirm the current preview | exactly one action item created; process receipts visible | |
| 12 | Organizer | Attempt member-only completion | refused (403) | |
| 13 | Roles | Transition to member | fresh least-privilege session (never a token upgrade) | |
| 14 | Member | Attempt organizer review/confirm | refused (403) | |
| 15 | Member | View assigned action card | card visible with the organizer's edited content | |
| 16 | Member | Complete the item | completion receipt shown; card reaches completed state | |
| 17 | Member | Retry completion | idempotent — no double effect | |
| 18 | Evidence | Evidence summary in organizer surface | value-withheld export; no DID/JWT/secret visible | |
| 19 | Evidence | Operator verify (`icn-demo-verify --rehearsal`) | receipt ladder + leak-absence pass | |
| 20 | Evidence | Tamper a copied packet, re-verify | verification fails closed | |
| 21 | Persistence | Restart services (`icn-rehearsal-ctl restart`) | reload works; durable receipts remain | |
| 22 | Persistence | Reboot the VM | full stack recovers unattended; LAN URL reachable | |
| 23 | Reset | Start a new rehearsal after completion | prior run retired; receipts remain; clean second run | |
| 24 | Reset | Second full walkthrough (steps 5–17) | passes with no leakage from the prior run | |
| 25 | A11y (automated floor) | Keyboard-only pass of organizer + member flows | all interactive elements reachable and operable; visible focus | |
| 26 | A11y (automated floor) | 200% zoom + narrow width | content reflows; no loss of function | |
| 27 | A11y (automated floor) | Forced-colors / high-contrast mode | all states distinguishable; no color-only meaning | |
| 28 | A11y (human gate) | NVDA screen-reader smoke (real human pass) | **cannot be closed by automation** — see icn#2041 | |

Rows 1–27 can be executed and recorded by a tester at the workstation.
Row 28 belongs to the human accessibility gate (icn#2041) and must not be
marked passed on the strength of automated checks.
