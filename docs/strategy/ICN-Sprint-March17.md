# Sprint Plan: Close the Demo Gap

**Dates:** March 17 – March 30, 2026 (2 weeks)
**Team:** Matt (solo) — chronic pain, ~4-5 productive hours/day realistic
**Sprint Goal:** Know exactly what works, close the receipt chain, submit grant applications, and have an honest demo pitch ready.

---

## Calendar Constraints

| Date | Event | Impact |
|------|-------|--------|
| **Thu Mar 19, 2pm** | NY Co-op Summit planning call (Joe Marraffino) | Must have a credible verbal pitch for ICN workshop. No slides needed — just know what you can demo. |
| **Sat Mar 21** | Blog Post 1 deadline ("What ICN Is") | Scheduled but deprioritized — the docs we just wrote ARE this content. Adapt ICN-Pitch.md into a blog post, don't write from scratch. |
| **Tue Mar 24, 11:15am + 1pm** | Nassentia appointments | ~2 hours blocked |
| **Wed Mar 25, 10am-2pm** | Grant prep: Outta Excuses + Verizon Digital Ready | Hard deadline March 31. Both applications need to happen. |
| **Thu Mar 26, 6-8pm** | RCC Meeting (Zoom) | Attendance optional but valuable for pilot recruitment context. |
| **Sat Mar 28** | Blog Post 2 deadline ("Season 14") | Stretch — only if Week 1 goes clean. |

---

## Capacity

| Day | Available Hours | Notes |
|-----|----------------|-------|
| **Tue Mar 17** (today) | 3-4h remaining | Session already in progress. Start audit. |
| **Wed Mar 18** | 4-5h | Full work day. Audit continues. |
| **Thu Mar 19** | 3h (2pm call) | Morning: finish audit + write pitch notes. 2pm: summit call. |
| **Fri Mar 20** | 4-5h | Demo script + start receipt chain work |
| **Sat Mar 21** | 3-4h | Blog post adaptation (if audit went well) or receipt chain |
| **Sun Mar 22** | 2-3h | Buffer / overflow |
| **Mon Mar 23** | 4-5h | Receipt chain: AllocationProposal |
| **Tue Mar 24** | 2-3h | Nassentia blocks midday. Receipt chain continues. |
| **Wed Mar 25** | 1-2h ICN | Grant applications eat most of the day (Outta Excuses + Verizon). |
| **Thu Mar 26** | 3-4h | Receipt chain wiring. RCC evening optional. |
| **Fri Mar 27** | 4-5h | icnctl audit verify + compliance sprint start |
| **Sat-Mon Mar 28-30** | 3-4h/day | Compliance sprint + vertical slice test |
| **Total** | **~40-48h** | At 70% efficiency: **~28-34 usable hours** |

---

## Sprint Backlog

### Week 1: Audit + Pitch + Start Building (Mar 17–22)

| # | Item | Est. | Day | Dependencies | Priority |
|---|------|------|-----|--------------|----------|
| **1** | **SSH into icn-dev, run all 4 demo flows against live K3s** | 3h | Tue-Wed | K3s running | P0 |
| | Start `icnd --gateway-enable` on a K3s pod. Run `demo-governance.py` end-to-end. Then run patronage, federation, reporting scripts. Record: PROVEN / FRAGILE / BROKEN for each. | | | | |
| **2** | **Runtime-test the cryptographic proof endpoint** | 1h | Wed | #1 running | P0 |
| | `GET /v1/gov/proposals/{id}/proof` — does it return a valid, verifiable receipt after a governance vote? This is the punchline of every document we wrote. | | | | |
| **3** | **Write classification + demo script** | 2h | Thu AM | #1, #2 | P0 |
| | One-page doc: what each flow does, what status it has (PROVEN/FRAGILE/BROKEN/MISSING). One demo script for the strongest proven path. This is your prep for the 2pm call. | | | | |
| **4** | **Summit call: pitch ICN workshop** | 1h | Thu 2pm | #3 | P0 |
| | Volunteer for tech/infrastructure track. Pitch: "Live demo of provable cooperative governance — cryptographic receipts for democratic decisions." Don't promise federation demo unless #1 proved it works. | | | | |
| **5** | **Adapt ICN-Pitch.md → blog post** | 2-3h | Fri-Sat | #3 (know what's real) | P1 |
| | The pitch doc is 90% of Blog Post 1. Adapt for Hacker News tone. Add the "daemon, not a DAO" hook. Strip internal status details. Publish to icn-website blog. | | | | |
| **6** | **Start AllocationProposal (#1311)** | 3-4h | Fri-Sun | None | P0 |
| | New governance proposal type. When accepted, produces AllocationReceipt. This is link 5 of the 6-link receipt chain. The gap analysis says this is the most important missing piece. | | | | |

### Week 2: Receipt Chain + Compliance + Grants (Mar 23–30)

| # | Item | Est. | Day | Dependencies | Priority |
|---|------|------|-----|--------------|----------|
| **7** | **Finish AllocationProposal + wire to ExecutionReceiptGate** | 3-4h | Mon-Tue | #6 | P0 |
| | AllocationReceipt → ExecutionReceiptGate linkage. When this works, the 6-link chain is complete in code. | | | | |
| **8** | **Grant applications: Outta Excuses + Verizon Digital Ready** | 4h | Wed | Business plan v2 exists | P0 |
| | Outta Excuses: LLC + $15 fee + application. Verizon: complete 1 free course first, then apply. Both deadline March 31. Do not skip these — they're the lowest-friction cash available. | | | | |
| **9** | **Build `icnctl audit verify`** | 3-4h | Thu-Fri | #7 | P0 |
| | CLI command: takes receipt ID, walks Decision→Allocation→Execution chain, verifies all signatures, returns pass/fail. This is the demo punchline and the vertical slice success criterion. | | | | |
| **10** | **Compliance: terminology rename (#1303)** | 3-4h | Fri-Sat | None | P0 |
| | payment→settlement, currency→unit, balance→position. Grep-and-replace across 38 crates + test updates. Nothing behavioral changes. Blocks STF application. | | | | |
| **11** | **Compliance: CI linter (#1312)** | 1-2h | Sat-Sun | #10 | P0 |
| | GitHub Actions check that fails if banned terms appear in API-exposed paths. Makes terminology discipline self-enforcing. | | | | |
| **12** | **Compliance: UX language guide (#1304)** | 1h | Sun-Mon | #10 | P1 |
| | Seven Invariants checklist in CONTRIBUTING.md. Quick write — the content exists in the forward plan. | | | | |

### Stretch (only if ahead of schedule)

| # | Item | Est. | Dependencies | Priority |
|---|------|------|--------------|----------|
| **S1** | Vertical slice integration test (D1) | 3h | #9 | P1 |
| **S2** | Blog Post 2 ("Season 14") | 2-3h | None | P2 |
| **S3** | Update demo scripts to real icnctl calls (D2) | 2-3h | #9 | P1 |
| **S4** | RCC meeting attendance | 2h evening | None | P2 |

---

## Critical Path

```
TODAY                    THU 2PM              FRI-SUN           MON-TUE
  │                        │                    │                  │
  ▼                        ▼                    ▼                  ▼
#1 Audit 4 flows ──→ #3 Classification ──→ #6 Allocation ──→ #7 Wire to
#2 Test proof       #4 Summit call          Proposal           ExecutionGate
                                               │
                    ║                          ▼              THU-FRI
                    ║  #5 Blog post      #8 Grants (Wed) ──→ #9 icnctl
                    ║     (parallel)                          audit verify
                    ║                                            │
                    ║                                            ▼
                    ║                    #10 Terminology ──→ #11 CI linter
                    ║                       rename              │
                    ║                      (parallel)           ▼
                    ╚══════════════════════════════════════► Sprint done
```

**The chain that matters:** Audit → Classification → AllocationProposal → Execution wiring → `icnctl audit verify`. Everything else is parallel or stretch.

---

## Risks

| Risk | Impact | Prob. | Mitigation |
|------|--------|-------|------------|
| **Audit reveals `icnd` won't start on K3s** | Can't verify anything. Sprint stalls day 1. | Low | Pods are confirmed Running (5d16h). If gateway fails, test locally on Zentith WSL2. |
| **Proof endpoint returns empty/invalid** | Governance flow downgraded from PROVEN to FRAGILE | Medium | The handler verifies before serving (vertical slice assessment §4). If it fails, debug is scoped — it's one endpoint. |
| **3 unverified flows are all broken** | "Four demo flows" is dead. Blog post needs rewriting. | Medium | Pitch governance only at the summit call. One proven flow is enough for the workshop proposal. |
| **AllocationProposal is harder than scoped** | Receipt chain incomplete by sprint end | Medium | #1311 is well-defined. ExecutionReceiptGate is already merged. The risk is integration glue, not design. If stuck, ship the 4-link chain and mark Allocation→Execution as "v1.1". |
| **Grant applications eat Wednesday** | Less time for receipt chain | High | Non-negotiable — March 31 deadlines. Budget for it. The grant apps are 4h, not 8h. Business plan already written. |
| **Pain day kills a full day** | Lose 4-5h from the budget | High | Buffer days (Sat/Sun) exist. Sprint is planned at 70% capacity. If a day is lost, cut S1-S4 stretch items. |

---

## Definition of Done

Sprint is complete when:

- [ ] All 4 demo flows classified honestly (PROVEN / FRAGILE / BROKEN)
- [ ] Summit call happened, ICN workshop pitched
- [ ] AllocationProposal merged or PR open
- [ ] `icnctl audit verify` exists and walks at least the Decision→Allocation chain
- [ ] Terminology rename (#1303) merged
- [ ] CI compliance linter (#1312) merged
- [ ] Outta Excuses grant application submitted
- [ ] Verizon Digital Ready course completed + grant application submitted
- [ ] ICN-Pitch.md adapted and published as Blog Post 1

---

## Today's Next Actions (March 17)

Right now, in order:

1. **SSH into icn-dev** (10.8.30.45). Check `icnd` is running on K3s. If not, start it with `--gateway-enable`.
2. **Run `demo-governance.py`** against the live gateway. Record output.
3. **Hit the proof endpoint** manually: `curl https://<gateway>/v1/gov/proposals/{id}/proof`. See what comes back.
4. **Run the other 3 demo scripts** (patronage, federation, reporting). Record what passes and what breaks.
5. **Write the classification** in a file. This is the ground truth everything else builds on.

That's tonight. Tomorrow continues the audit and preps for Thursday's call.
