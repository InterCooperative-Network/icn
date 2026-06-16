# ICN July Demo Candidate 0.1 — Operator & Reviewer Handoff Script

> **DEV/DEMO. Local. Single-actor. Fictional data.** This is the operator and
> reviewer companion for **July Demo Candidate 0.1**. It does not run the demo
> for you and it does not narrate it to an audience — the docs below do that.
> Its job is to help you **pin** what the candidate is, **run** it without
> overclaiming, **recover** when it stumbles, capture **evidence** safely, and
> **hand it to a reviewer** who can tell exactly what is real, what is fixture,
> and what is absent.
>
> Not production. Not a pilot. Not NYCN activation. Not federation. Not
> multi-person. The image is unsigned, mutable, and not partner-distributable.

---

## Which doc do I want?

| You want to… | Read |
|---|---|
| Pin the candidate, run/recover/hand-off honestly (this doc) | **JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md** |
| Run it and narrate it click-by-click to a person | [`JULY_DEMO_HANDS_ON.md`](JULY_DEMO_HANDS_ON.md) |
| A one-page checklist to keep open during the live run | [`JULY_DEMO_OPERATOR_CHECKLIST.md`](JULY_DEMO_OPERATOR_CHECKLIST.md) |
| The technical "boot the VM" quickstart | [`../../deploy/appliance/DEMO_QUICKSTART.md`](../../deploy/appliance/DEMO_QUICKSTART.md) |
| The canonical claim boundary (proof levels) | [`../reference/project-index/proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md) (Row 11) |
| Current project truth | [`../STATE.md`](../STATE.md), [`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) |

This doc is intentionally **not** a re-narration of the click path or the
presenter scripts. Where it would duplicate the hands-on guide, it links instead.

---

## 1. Truth label

**Candidate:** July Demo Candidate 0.1 — the DEV/DEMO appliance image profile
(`ICN_APPLIANCE_DEMO_PROFILE=1`) plus its member-shell loop. For per-PR / SHA
truth, defer to [`../STATE.md`](../STATE.md) and the capability matrix Row 11 —
do not hard-code a commit here; it will rot.

- **Class:** DEV/DEMO, local, **single-actor**, fictional-data.
- **Posture:** dev gates enabled and labeled; image unsigned and mutable.
- **Terminology (use precisely):** the **appliance image** is a reproducible VM
  image/template; a **running node instance** is a VM booted from it; the
  **hypervisor host** (your laptop's QEMU, a Proxmox box, a cloud instance) runs
  one or more node instances. Org/domain assignment and any future QR
  node-claim ceremony apply to a **running node instance** — never to the
  generic image, never to a physical appliance.

## 2. Demo goal

Demonstrate the single-actor cooperative-participation spine:

> **standing → action card → discharge → receipt → evidence/audit**

on one local node instance, with every claim inside the proof boundary.

## 3. What this proves

Each item is a live surface on the running node instance unless tagged otherwise.
Proof levels are from the capability matrix (Row 11 = **L5**, stranger-runnable
local proof, single-actor).

- A **running node instance boots** from the appliance image and serves its
  JWT-authenticated gateway.
- The **member shell loads** (served by the same node instance).
- **Standing is visible** — `GET /v1/gov/me/standing` (the member's domain
  standing in the fictional institution).
- An **action card is visible** — `GET /v1/gov/me/action-cards`
  (`action_item / complete`, fictional: "Confirm Summit 2026 venue booking").
- The action can be **discharged** (live mutation under the hood).
- A **completion receipt is visible** — retrievable at
  `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`,
  carrying the 32-byte BLAKE3 `record_hash` that binds it.
- **Evidence/audit can be checked** — `sudo icn-demo-verify <item-id>` runs a
  per-item consistency check (record-hash shape + field binding; **not** a
  BLAKE3 re-derivation).
- The **13/13 governed receipt-chain proof can run locally/dev-gated** —
  `sudo icn-demo-verify --chain` (wraps `icnctl audit verify`) on a fresh
  ephemeral in-VM node. This is the cryptographic path; the per-item check is not.

Honest wording: *"proof of path, not proof of deployment readiness."*

> Action-card runtime is real for **three of five** source paths
> (`proposal/vote`, `action_item/complete`, `meeting/attend`). `signal_rule` and
> `obligation_lifecycle` remain RFC-gated (#1646 → #1631/#1634). Show three of
> five, not five of five.

## 4. What this does NOT prove

State these plainly; the project's credibility rests on not overclaiming.

- **Not production.** No production posture is demonstrated.
- **Not a pilot.** No formal pilot authorization.
- **Not NYCN activation.** NYCN is the *intended* first partner — no commitment,
  no live integration.
- **Not live federation.** Single node only; no multi-org coordination.
- **Not multi-person.** Single-actor only; two-member interaction is a future lane.
- **Not partner / private-data handling.** The institution is a fictional
  fixture; rehearsal privacy is by **exclusion**, not enforcement.
- **Not a signed/immutable image.** The image is unsigned and mutable.
- **Not a partner-distributable appliance.** Local dev image only.
- **Not broad `governance:write` retirement** (#1868, future).
- **Not runtime scoped-vault / private disclosure enforcement** (#1792,
  design-direction only).

## 5. Operator preflight checklist

Run privately once before anyone is watching.

- ☐ **Clean checkout (if building).** Building the image? Build from a clean
  checkout of **public `main`** (`git status` clean, on `main`). Running a
  prebuilt candidate image? Skip the build and record the image SHA256 from its
  manifest instead.
- ☐ **Demo-profile image present.** The `icn-demo-session` endpoint ships only
  with `ICN_APPLIANCE_DEMO_PROFILE=1`. Build/run prerequisites:
  [`../../deploy/appliance/DEMO_QUICKSTART.md`](../../deploy/appliance/DEMO_QUICKSTART.md).
- ☐ **No secrets enter the repo or repo logs.** Do not paste JWTs, keystore
  passphrases, real node IPs, or key paths into any tracked file or committed
  log. The in-VM demo scripts read the keystore passphrase themselves — run
  them with `sudo`; never export your own.
- ☐ **Local ports/tunnels free.** Host ports `18080` (gateway), `18090`
  (member-shell, **fixed**), `18091` (demo-session) are free on your workstation.
- ☐ **Browser path ready.** A modern browser; the launcher opens the shell URL
  for you.
- ☐ **Reset/reseed path known.** Manual JWT flow: `sudo icn-demo-reset && sudo
  icn-demo-seed --json` inside the node instance (look for `"standing_note":
  "bootstrap-standing: ok"`). Launcher flow: `sudo icn-demo-reset` only —
  clicking **Start local demo** re-seeds, so don't also seed by hand or you get
  a duplicate card. Or whole-disk overlay reset.
- ☐ **Verification commands known.** `sudo icn-demo-verify <item-id>` and
  `sudo icn-demo-verify --chain` (expect **13/13**).
- ☐ **You can say the boundary in one breath:** *"core loop, real; demo data,
  fixture; federation/production/funds, not yet."*

## 6. Recommended live demo path

When a running node instance already exists, prefer the one-command launcher —
no JWT copy/paste, no gateway typing. Placeholders only below; use your own
values.

```bash
# Direct route — this workstation can SSH the node instance and holds the demo key:
ICN_DEMO_VM_IP=192.0.2.50 ICN_DEMO_SSH_KEY=/path/to/demo_key \
  bash deploy/appliance/scripts/open-proxmox-demo.sh

# Jump route — the key lives on a dev host that can reach the node instance:
ICN_DEMO_VM_IP=192.0.2.50 ICN_DEMO_JUMP=user@dev-host \
  ICN_DEMO_REMOTE_KEY=/path/to/demo_key-on-jump-host \
  bash deploy/appliance/scripts/open-proxmox-demo.sh
```

`192.0.2.50` is a documentation placeholder (TEST-NET-3) — substitute your node
instance's real IP, which **does not** belong in this repo. The launcher opens
the three tunnels (gateway→18080, shell→18090, session→18091) and your browser
to the shell in launcher mode; click **Start local demo** once. Full walkthrough
and the direct-SSH vs jump-host detail: [`JULY_DEMO_HANDS_ON.md`](JULY_DEMO_HANDS_ON.md)
§5, at the same abstraction level as the quickstart. The shell port `18090` is
fixed (it is the page Origin the gateway CORS allow-list pins).

## 7. Manual fallback path

If the launcher is not usable, drive it manually per
[`../../deploy/appliance/DEMO_QUICKSTART.md`](../../deploy/appliance/DEMO_QUICKSTART.md).
The five steps, summarized:

1. **Inspect standing** — open the shell, point it at the forwarded gateway.
2. **Inspect the action card** — one open `action_item/complete` card.
3. **Discharge the action** — complete it in the shell.
4. **View the receipt** — the completion receipt with its `record_hash`.
5. **Verify evidence/audit** — `sudo icn-demo-verify <item-id>`, then
   `sudo icn-demo-verify --chain` → **13/13**.

Do not duplicate the quickstart's port/CORS detail here — read it there.

## 8. Facilitator script (plain language)

Organizer-friendly framing for each step. No crypto/blockchain/fintech words.
This is a crib; the full 5-minute and 15-minute scripts are in
[`JULY_DEMO_HANDS_ON.md`](JULY_DEMO_HANDS_ON.md) §14–§15, with longer
per-stage narration in §7.

- **Standing** — "The node knows who is allowed to act here and what their
  standing is. Nobody had to ask a central company."
- **Action card** — "These are the things this member can act on right now. The
  node decides what to show based on who they are."
- **Discharge** — "The member completes the action. It tells them what they're
  doing and whether it can be undone, then they confirm."
- **Receipt** — "The node produces a receipt — evidence the action happened,
  bound to the record. Not a screenshot you could quietly edit."
- **Evidence/audit** — "And we can ask the node to prove its whole chain of
  records is internally consistent — nothing inserted or rewritten after the
  fact. That's **13/13**: verifiable, not 'trust me.'"
- **Close** — "This is one node a cooperative could eventually run themselves —
  the seed of member-controlled infrastructure. **Proof of path, not deployment
  readiness.**"

Keep ICN vocabulary: standing, action card, discharge, receipt, evidence,
provenance, settlement, unit, obligation. Avoid payment, wallet, balance, token,
crypto, blockchain.

## 9. Recovery / troubleshooting

The two canonical troubleshooting tables already cover the common failures —
use them rather than a third copy:

- [`JULY_DEMO_HANDS_ON.md`](JULY_DEMO_HANDS_ON.md) §12 (failure modes + fixes)
- [`../../deploy/appliance/DEMO_QUICKSTART.md`](../../deploy/appliance/DEMO_QUICKSTART.md)
  (Troubleshooting) and the panic table in
  [`JULY_DEMO_OPERATOR_CHECKLIST.md`](JULY_DEMO_OPERATOR_CHECKLIST.md).

Quick orientation for the cases the brief calls out:

| Symptom | First move |
|---|---|
| Browser "Failed to fetch" in live mode | CORS / wrong gateway port — shell must be on **18090**; gateway is the forwarded **18080**. |
| Wrong gateway port | Use the forwarded `18080`, not the shell's `:8080` default. |
| Stale state / extra action cards after re-runs | **Launcher flow:** `sudo icn-demo-reset` only, then reload and click **Start local demo** — the Start click re-seeds (one card). Do **not** also run `icn-demo-seed`, or the live demo shows a duplicate card (the session endpoint already seeds on each Start). **Manual JWT flow:** `sudo icn-demo-reset && sudo icn-demo-seed --json`, then paste the new JWT. Or whole-disk overlay reset. |
| Need a clean reseed | `sudo icn-demo-seed --json` → expect `"standing_note": "bootstrap-standing: ok"`. |
| Firstboot not complete | `icn-demo-seed` fails "$ENV_FILE missing"; `journalctl -u icn-appliance-firstboot`. The icnd unit is gated on firstboot by design. |
| 13/13 proof slow | Normal — it builds nothing but runs a full governed lifecycle; a few minutes at 2 GB RAM. |
| Host port already in use | Free the port or override `ICN_DEMO_GW_PORT` / `ICN_DEMO_SESSION_PORT` (shell stays 18090). |

> A missing-`ss` edge case in the gossip-port preflight is tracked as **#1956**
> (minor follow-up). It is noted here, **not** fixed in this doc.

## 10. Evidence checklist

What to capture locally, what must never be committed, what is safe to summarize
in a PR body.

**Capture locally (operator notebook / `~/artifacts`, outside the repo):**
- PASS/FAIL of each step and the **13/13** chain result (`exit 0`).
- The image SHA256 from the build manifest and whether `demo_profile: true`.
- `check.sh` pass count (e.g. `20/20`) and smoke `--real --demo` result.
- Host/toolchain identity (OS, kernel, QEMU version) for reproducibility.
- The schema-validated evidence packet `icn-demo-verify --chain` emits to
  `/var/lib/icn-demo/` (stays in the VM / your artifacts; **gitignored** by
  construction).

**Must NOT be committed (anywhere in the repo or repo logs):**
- Any JWT / session token, keystore passphrase, or private key.
- Real node-instance IPs, hostnames, usernames, or key paths (use TEST-NET-3
  placeholders like `192.0.2.50`).
- Raw audit transcripts or the raw evidence packet (repo-safe by exclusion).

**Safe to summarize in a PR body:**
- "Booted Candidate 0.1; ran the full spine; 13/13 chain PASS (exit 0)."
- Check counts and the image SHA256 (a hash, not a secret).
- Which surfaces were live vs fixture-backed, by reference to Row 11.

## 11. Known gaps / next lanes

Open and explicitly **not built**. Issue-numbered so a reviewer can follow each.

| Gap / lane | Issue | Status |
|---|---|---|
| Integrated NYCN organizer rehearsal surface (operable participation surface) | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) | open milestone |
| Private disclosure boundary / scoped-vault runtime enforcement | [#1792](https://github.com/InterCooperative-Network/icn/issues/1792) | design-direction only |
| Broad `governance:write` fallback retirement | [#1868](https://github.com/InterCooperative-Network/icn/issues/1868) | future |
| CI disk-space flake (gates the 13/13 CI run) | [#1955](https://github.com/InterCooperative-Network/icn/issues/1955) | operational / CI |
| nycn-dogfood gossip-port preflight (missing `ss`) | [#1956](https://github.com/InterCooperative-Network/icn/issues/1956) | minor follow-up |

Future lanes with no issue gate yet, **not built**: multi-person / two-member
action flow, QR node-claim ceremony, commons resource allocation, signed /
immutable image, partner-distributable image, live federation.

## 12. Reviewer checklist

Before approving any PR that touches this candidate's surface or its docs:

- ☐ **Real vs fixture vs absent is legible.** A reviewer can tell which surfaces
  are live (standing, action card, discharge, receipt, 13/13), which are
  fixture-backed (the institution, `?mode=demo` panes), and which are absent
  (federation, multi-person, production) — cross-check Row 11.
- ☐ **Fixture-backed is labeled as such.** No fixture is presented as live data.
- ☐ **Absent is labeled as absent.** No design-only lane is implied as built.
- ☐ **All commands are placeholder-safe.** No real IPs, hostnames, usernames,
  key paths, JWTs, or passphrases. TEST-NET-3 (`192.0.2.x`) placeholders only.
- ☐ **No secrets / private infrastructure values** anywhere in the diff.
- ☐ **Aligns with canonical truth.** Nothing here contradicts
  [`../STATE.md`](../STATE.md) or [`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md)
  (Phase 2 — Pilot Launch, in progress; single-actor). A docs/runbook
  convenience change is **not** a phase-state change.
- ☐ **Vocabulary discipline holds** — identity/standing/obligation/allocation/
  settlement/unit/receipt/provenance/evidence; no payment/wallet/token/crypto/
  blockchain for ICN-native flows.

---

## Cross-links

- Run + narrate: [`JULY_DEMO_HANDS_ON.md`](JULY_DEMO_HANDS_ON.md)
- Live one-pager: [`JULY_DEMO_OPERATOR_CHECKLIST.md`](JULY_DEMO_OPERATOR_CHECKLIST.md)
- Boot the VM: [`../../deploy/appliance/DEMO_QUICKSTART.md`](../../deploy/appliance/DEMO_QUICKSTART.md)
- Claim boundary (proof levels, Row 11): [`../reference/project-index/proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md)
- Runtime surfaces: [`../reference/project-index/runtime-surface-map.md`](../reference/project-index/runtime-surface-map.md)
- What is real now: [`../reference/project-index/current-truth-map.md`](../reference/project-index/current-truth-map.md)
- Project truth: [`../STATE.md`](../STATE.md), [`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md)
