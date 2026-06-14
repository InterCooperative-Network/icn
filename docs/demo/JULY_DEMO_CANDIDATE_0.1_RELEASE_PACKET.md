# ICN July Demo Candidate 0.1 — Hand-off Packet

> **"Release packet" = a candidate hand-off package for review, NOT a shipped or
> tagged software release.** No release dry-run has been run (see §3 and §9). The
> filename keeps the `RELEASE_PACKET` label only because that is how the demo index
> routes to it.
>
> **DEV/DEMO. Local. Single-node. Single-actor. Fictional data.** Read this first,
> then the linked run-path and evidence docs. You should be able to understand,
> operate, review, and share July Demo Candidate 0.1 from this packet alone —
> without anyone walking you through the whole project.

## Truth boundary (read this before anything else)

July Demo Candidate 0.1 is a **DEV/DEMO local proof artifact** for a single-node,
single-actor cooperative participation loop. It is **not** production, **not** federation,
**not** a formal NYCN pilot, **not** real member data, **not** real funds/resource
allocation, and **not** a partner-distributable signed appliance.

The core proof claim, stated narrowly:

> **standing → action card → discharge/completion → receipt → evidence/audit verification**

That is the whole claim. It is **proof of path, not deployment readiness.**

---

## 1. What is this?

A reproducible local demonstration that a cooperative member's participation loop can be
**rendered honestly and proven locally**: a member sees their *standing*, sees an *action card*
they can act on, *completes/discharges* it, gets a *completion receipt*, and can run a local
*evidence/audit* verification over the receipt chain. It ships as:

- a **member-shell v0 reference client** (`web/member-shell/`) — the human surface;
- a **served-OpenAPI member surface** (standing / action-cards / completion-receipt on `/v1`);
- a **single-VM DEV/DEMO appliance image** that runs the member loop end-to-end;
- a **one-command launcher** with a no-paste session flow;
- operator/reviewer runbooks and this packet.

## 2. What does it prove?

- The participation spine renders and verifies locally: **standing → action card →
  discharge/completion → completion receipt → local evidence/audit verification**.
- The member surface is implementable against real endpoint shapes (documented in the served
  OpenAPI), with a member-facing vocabulary and an honesty banner naming the maturity tier.
- A local receipt-chain audit reaches **13/13** on the sealed appliance image (in-VM,
  dev-attested).
- The member surface clears the **automated** accessibility floor (axe-core WCAG A+AA: 0
  violations) and the rendered-structural floor (keyboard reachable with visible focus,
  landmarks, no-credential-in-URL) — see the accessibility evidence below.

## 3. What does it NOT prove?

- **Not production.** No production security posture, hardening, or operational guarantees.
- **Not a pilot.** No formal NYCN pilot; NYCN has approved/adopted/activated nothing here.
- **Not federation.** Single node; no live federation, no cross-organization coordination.
- **Not multi-person / multi-organization governance.** The loop is **single-actor**.
- **Not real data.** Fixture/local/dev only; fictional identities (`did:icn:example-*`).
- **Not signed/partner-distributable.** The appliance is a DEV/DEMO profile — not signed, not
  immutable, not a partner artifact. The 13/13 chain uses a dev-gated self-trust seed and
  **dev-attested, not production-signed** receipts.
- **Not private-data handling.** No scoped-vault / private-disclosure runtime.
- **Not a release.** No release dry-run / artifact packaging has been performed.

## 4. Who is it for?

- **Operators / presenters** who want to run the demo locally and show the participation loop.
- **Reviewers** (technical or cooperative-movement) who want to inspect the claim boundary and
  the evidence without taking anything on faith.
- **Contributors** orienting to what is real now vs. what is aspirational.

It is **not** for partners as a deployable product, and **not** for any audience as a
production or pilot claim.

## 5. How do I run it?

Pick the path that matches what you have. **Distinguish two phases:** *building/staging* the
appliance image is **not** offline — the quickstart starts with a `git clone`, and
`build-image.sh --real` runs `virt-customize --update --install`, which fetches Debian packages
over the network (`deploy/appliance/build-image.sh`). *Operating* an already-built/staged image
(boot, launcher, member loop, verify) is **local**. None of the paths requires a partner.

| Path | Doc |
|---|---|
| Appliance build + one-command launcher + manual fallback | [deploy/appliance/DEMO_QUICKSTART.md](../../deploy/appliance/DEMO_QUICKSTART.md) |
| Detailed presenter/operator click-by-click + failure-mode table | [JULY_DEMO_HANDS_ON.md](JULY_DEMO_HANDS_ON.md) |
| Keep-open live-demo card (preflight, launch, expected states, panic fixes) | [JULY_DEMO_OPERATOR_CHECKLIST.md](JULY_DEMO_OPERATOR_CHECKLIST.md) |
| Operator + reviewer handoff (claim boundary by proof level, evidence hygiene, known gaps) | [JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md](JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md) |

The recommended human path is the launcher (no credential paste, no gateway address to type);
the manual `seed`/`verify` fallback is documented in the quickstart and operator script.

## 6. How do I review the evidence?

A reviewer should be able to confirm each link of the chain:

- **Standing visible** — the member's memberships and roles render.
- **Action card visible** — at least one card with authority basis, scope, consequence, and an
  authorization check in plain language.
- **Completion/discharge accepted** — the card moves `Open → Sent — waiting for receipt →
  Confirmed` (live path), behind a pre-confirm summary stating what changes and the receipt
  expected.
- **Receipt rendered** — a plain-language summary first; record class, ids, actor DID, and raw
  `record_hash` behind "Show evidence detail".
- **Receipt fields visible** — `item_id`, `domain_id`, `actor_did`, `transition`, `completed_at`,
  `record_hash`.
- **Local per-item verification** — the operator script's evidence checklist (§10) shows the
  per-item consistency check.
- **13/13 chain verification** — from the sealed appliance run (in-VM `icn-demo-verify --chain`,
  PASS exit 0). Evidence: the sealed-image evidence map in the operator's artifact store
  (artifact class `demo-image-20260612`).
- **Accessibility / rendered-browser evidence** — see
  [JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md](JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md)
  (12-category gate outcome, axe 0 violations, keyboard/focus, with the human screen-reader/zoom
  pass recorded as owed).

## 7. What screenshots / artifacts are safe to share?

- Redacted screenshots (member shell with fixture data, banner visible).
- This packet and the other claim-boundary docs (all are public-repo docs).
- Fixture screenshots **without** any credential field populated.
- Evidence **summaries** (counts, PASS lines, gate outcomes) without local secrets.

## 8. What must NOT be shared?

Never share, in any screenshot, transcript, issue, PR, or artifact:

- access credentials / session tokens / JWTs;
- private IPs or hostnames; local machine paths;
- SSH keys; keystore passphrases;
- unredacted terminal transcripts that contain any of the above.

(The demo paths are designed to avoid these: the launcher pastes no credential, the member-shell
credential field is `type="password"` and never persisted, and the demo banner states nothing is
signed.)

## 9. Known gaps

- **Human accessibility is partial.** Automated axe + rendered-structural floor passed; the
  **screen-reader / assistive-technology** pass and **human low-vision / browser-zoom** pass are
  **not** done — recorded as owed in the accessibility evidence doc.
- **Release dry-run not run.** No release packaging/dry-run has been performed.
- No production security posture.
- No formal NYCN pilot; no NYCN activation.
- No federation; no multi-person / multi-organization flow.
- No real member or funds/resource-allocation data.
- No scoped private-disclosure (scoped-vault) runtime.
- No signed / immutable / partner-distributable artifact.
- Broad `governance:write` is **not** retired (migration landed with accepted-also fallback;
  retirement is a separate future decision).

## 10. What are the next responsible lanes?

Only the responsible next steps — not a roadmap:

- **Human accessibility fixes** for anything the accessibility pass surfaces (screen-reader / AT
  pass first).
- **Rendered-browser walkthrough follow-ups** (live-mode rendered pass once a local gateway is
  available; the fixture-mode pass is done).
- **Release dry-run / artifact packaging** — only when actually performed, and named as such.
- **Two-member action flow** — only **after** this release packet is stable (out of scope now).
- **Private-disclosure / scoped-vault runtime** — a separate future lane.
- **Governance scope retirement** (broad `governance:write`) — a separate future lane.

---

## Provenance

- Canonical current-state truth: [docs/STATE.md](../STATE.md), [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md).
- Proof-level vocabulary: [docs/reference/project-index/proof-level-taxonomy-capability-matrix.md](../reference/project-index/proof-level-taxonomy-capability-matrix.md).
- Demo index: [docs/demo/README.md](README.md).
- Sealed-image + rendered-walkthrough evidence (screenshots, JSON, logs) live in the operator's
  local artifact store (classes `demo-image-20260612`, `july-demo-accessibility-<date>`), not in
  this repo, per the evidence-log convention. This packet is the shareable summary.
