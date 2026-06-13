# ICN July Demo — Hands-On Guide

> A complete, click-by-click way through the ICN July Demo Candidate. Written so
> you can run it yourself, understand what you are looking at, and show it to
> another person without improvising.

This is a **DEV/DEMO** walkthrough. Read [§2 What this is not](#2-what-this-demo-is-not)
and [§16 What this proves](#16-what-this-proves) before you show it to anyone, so
you describe it honestly.

---

## Table of contents

1. [What this demo is](#1-what-this-demo-is)
2. [What this demo is not](#2-what-this-demo-is-not)
3. [What you need before starting](#3-what-you-need-before-starting)
4. [Launch it locally (laptop / dev box)](#4-launch-it-locally-laptop--dev-box)
5. [Launch it through a remote / Proxmox VM](#5-launch-it-through-a-remote--proxmox-vm)
6. [The exact click path](#6-the-exact-click-path)
7. [What to say at each stage](#7-what-to-say-at-each-stage)
8. [How to reset the demo](#8-how-to-reset-the-demo)
9. [How to verify receipts](#9-how-to-verify-receipts)
10. [How to show the OpenAPI surface](#10-how-to-show-the-openapi-surface)
11. [How to explain the evidence / audit trail](#11-how-to-explain-the-evidence--audit-trail)
12. [Common failure modes and fixes](#12-common-failure-modes-and-fixes)
13. [Screenshots](#13-screenshots)
14. [The 5-minute demo script](#14-the-5-minute-demo-script)
15. [The 15-minute demo script](#15-the-15-minute-demo-script)
16. [What this proves](#16-what-this-proves)
17. [What comes next](#17-what-comes-next)

---

## 1. What this demo is

ICN (the Intercooperative Network) is infrastructure for cooperatives to
coordinate **without** a central company in the middle. This demo is the first
hands-on slice of that: a single ICN node, running as a self-contained
appliance, driving one complete member loop end to end.

The loop is small on purpose. One member, in one context, is shown the things
they are allowed to act on, completes one of them, and the node produces a
tamper-evident **receipt** that the action happened. You can then ask the node
to prove that its record of events is internally consistent.

In plain language, the demo shows a node that can answer four questions and
prove its answers:

- **Who can act here?** (member standing)
- **What can they do?** (action cards)
- **Did they do it?** (discharge + completion receipt)
- **Can you prove it happened and wasn't edited later?** (receipt evidence +
  the audit chain)

That is the seed of member-controlled infrastructure: a box a cooperative could
eventually run themselves, instead of renting a seat in someone else's system.

---

## 2. What this demo is NOT

Be precise about this. The honesty boundary is the most important slide.

| Layer | Status | What it means |
|-------|--------|---------------|
| **Live-local** | ✅ real, running here | Node boot, `icnd`, the gateway, member standing, action cards, discharge, completion receipt, receipt↔evidence binding, seed/reset/reseed, the OpenAPI surface, and the 13/13 receipt-chain proof. These genuinely run on the node in front of you. |
| **Fixture-backed** | 🟡 real software, demo data | The NYCN institution package, the demo member/org identities, and the member-shell panes. The *mechanism* is real; the *people and org* are a scripted fixture, not a live membership roster. |
| **Design-only / not yet proven** | ⛔ do not claim | Federation, multi-organization behavior, production deployment, real pilot adoption, real member data, the mobile passport / keyring, QR node-claim, commons resource allocation, any wallet / token / payment behavior, and a signed release image. None of this is demonstrated here. |

If someone asks "is this live / in production / handling real money or real
members?" the answer is **no**. It is a development demonstration of the core
loop. Say so plainly. The credibility of the project rests on not overclaiming.

This image is **unsigned**, **not immutable**, and **not a partner-distributable
artifact**. It is a local dev image.

---

## 3. What you need before starting

- The demo appliance image (a `.qcow2` file) — see the project's appliance
  build docs, or reuse a verified candidate image.
- A way to run it: either **QEMU/KVM locally** ([§4](#4-launch-it-locally-laptop--dev-box))
  or a **hypervisor** (Proxmox/KVM/cloud) for the remote path ([§5](#5-launch-it-through-a-remote--proxmox-vm)).
- A modern browser (Chrome/Chromium/Firefox).
- For the remote path only: SSH access to the running node and the demo SSH key.
- The launcher script lives in the repo at
  `deploy/appliance/scripts/open-proxmox-demo.sh`.

> **Terminology.** The *appliance image* is a reproducible VM template. Booting
> it gives you a *running node instance*. The *hypervisor host* is whatever runs
> the VM (your laptop's QEMU, a Proxmox box, a cloud instance). The demo is a
> node instance, never a physical box and never production.

The node's gateway, member-shell, and demo-session endpoint all bind
**`127.0.0.1` inside the VM**. They are never exposed on your LAN. You reach them
through port-forwards you control (QEMU hostfwd locally, or an SSH tunnel
remotely).

---

## 4. Launch it locally (laptop / dev box)

This path needs no SSH and no launcher script — you boot the image under QEMU
with three port-forwards and open the browser. The shell host port **must** be
`18090` (the node's gateway and session CORS allow-lists pin that origin).

```bash
# Boot the demo image with the three loopback forwards and leave it running.
# 18080 -> gateway(8080)   18090 -> member-shell(8090)   18091 -> session(8091)
qemu-system-x86_64 \
  -machine accel=kvm -cpu host -m 2048 -smp 2 \
  -drive file=/path/to/icn-appliance-<version>-amd64.qcow2,if=virtio \
  -drive file=/path/to/seed.iso,if=virtio,media=cdrom \
  -netdev user,id=n0,\
hostfwd=tcp:127.0.0.1:18080-:8080,\
hostfwd=tcp:127.0.0.1:18090-:8090,\
hostfwd=tcp:127.0.0.1:18091-:8091 \
  -device virtio-net-pci,netdev=n0 \
  -nographic
```

Wait until the node finishes first boot (the gateway answers), then open:

```
http://localhost:18090/member-shell/?mode=live&demo=launcher&gw=18080&session=18091
```

Then follow [§6 The exact click path](#6-the-exact-click-path). Nothing to type,
nothing to paste.

> The repo's `deploy/appliance/smoke/smoke-local.sh --real --demo` boots this
> same image the same way and drives the loop from the command line — it is the
> automated proof that the local-boot path works.

---

## 5. Launch it through a remote / Proxmox VM

Use this when the node runs on a hypervisor you reach over SSH. This is the
**one-command launcher** path. The launcher opens the three tunnels and your
browser; you never touch the terminal again after it opens.

Set the connection via environment variables (no infra values are baked into the
script):

```bash
# Direct route — your workstation can SSH the node and holds the demo key:
ICN_DEMO_VM_IP=<your-node-ip> \
ICN_DEMO_SSH_KEY=/path/to/demo_key \
  bash deploy/appliance/scripts/open-proxmox-demo.sh

# Jump route — the key lives on a dev host that can reach the node:
ICN_DEMO_VM_IP=<your-node-ip> \
ICN_DEMO_JUMP=user@dev-host \
ICN_DEMO_REMOTE_KEY=/path/to/demo_key-on-jump-host \
  bash deploy/appliance/scripts/open-proxmox-demo.sh
```

What it does, in order:

1. Opens SSH tunnels: `localhost:18080→gateway`, `localhost:18090→member-shell`,
   `localhost:18091→demo-session`.
2. Waits for the member-shell to actually answer through the tunnel.
3. Opens your browser to the shell in launcher mode.
4. Prints the URL and the gateway, then runs until you press **Ctrl-C**.

Overridable env (all optional): `ICN_DEMO_SSH_USER` (default `debian`),
`ICN_DEMO_GW_PORT` (18080), `ICN_DEMO_SESSION_PORT` (18091),
`ICN_DEMO_NO_BROWSER=1` (set up tunnels, print the URL, don't open a browser).
The **shell port is fixed at 18090** by design and is not overridable — it is the
page origin the gateway CORS allow-list pins.

---

## 6. The exact click path

The browser opens to the member-shell in **launcher mode** (`?demo=launcher`).

1. You see an honesty banner at the top and a **"Start local demo"** button. The
   gateway field is pre-filled (`http://localhost:18080`). The manual connect
   form is hidden.
2. Click **Start local demo** — one click. The page asks the loopback session
   endpoint for a fresh DEV/DEMO session, holds the short-lived credential **in
   page memory only** (never written to disk, never in the URL), and loads the
   loop. No gateway typed, no JWT pasted.
3. **Standing** renders — the member's standing in the demo context (two
   domains).
4. **Action card(s)** appear — the things this member can act on.
5. Click **Mark complete** on a card → a confirm step appears declaring the
   action and its reversibility.
6. Click **Confirm — mark complete**.
7. **Receipt** renders, including the record hash.
8. Expand the receipt to see the **evidence** detail (the record the network
   produced).
9. The action card transitions to **Confirmed** in place.

> **Known nuance — say it out loud if asked.** After discharge, the backend
> `/me/action-cards` correctly returns **zero open cards** (the obligation is
> complete). The card you see does **not** vanish from the screen; it flips to a
> **Confirmed** state in place so the audience can see the result. The
> on-screen "Confirmed" card and the empty open-cards list are consistent, not a
> bug.

---

## 7. What to say at each stage

Plain language, no jargon fog. A real person is next to you asking "okay, what am
I actually looking at?"

- **Standing:** "The node knows who is allowed to act in this context, and what
  their standing is. Nobody had to ask a central server — the node holds this."
- **Action card:** "These are the prompts this member can act on — the work or
  the governance step in front of them. The node decides what to show based on
  who they are."
- **Discharge:** "The member completes the action. Notice it tells them what
  they're about to do and whether it can be undone before they confirm."
- **Receipt:** "The node produces a receipt — evidence the action happened, with
  a hash. This isn't a screenshot or a log line you could quietly edit; it's
  bound to the record."
- **Evidence / audit:** "And we can ask the node to prove its whole chain of
  records is internally consistent — that nothing was inserted or rewritten
  after the fact."
- **Appliance:** "The whole thing is one node a cooperative could eventually run
  themselves. This is the seed of member-controlled infrastructure — not a
  finished federation, but the first working piece."

---

## 8. How to reset the demo

Run inside the node (SSH in, or use the hypervisor console):

```bash
sudo icn-demo-reset          # destroys the demo's seeded state
sudo icn-demo-seed --json    # reseeds a fresh member + open action card
```

After reseed you should see `"standing_note": "bootstrap-standing: ok"` in the
JSON. Reload the member-shell and the loop is fresh again.

> `icn-demo-reset` destroys the demo's seeded state on that node only. It does
> not touch any other system.

---

## 9. How to verify receipts

Inside the node:

```bash
# Verify a single item's record is consistent:
sudo icn-demo-verify <item-id>

# Verify the full governed receipt chain (the cryptographic proof):
sudo icn-demo-verify --chain
```

`--chain` is the strong proof: it audit-verifies the full receipt chain and
reports **13/13**. A per-item `verify` is a consistency check, not a BLAKE3
re-derivation — `--chain` is the cryptographic path. Say it that way; don't
inflate the per-item check.

---

## 10. How to show the OpenAPI surface

The gateway serves its member API surface as OpenAPI. Through the tunnel (or
QEMU forward) on the gateway port:

```bash
curl -s http://localhost:18080/api-docs/openapi.json | head -c 400; echo
```

This is the machine-readable contract for the member surface that landed in
#2027. It is what an app, an SDK, or a partner integration would build against.

---

## 11. How to explain the evidence / audit trail

The point of the receipt is not the receipt — it's that the record can be
**proven consistent later**.

- Every meaningful action produces a record the node stores.
- The receipt binds to that record (the record hash you saw in the UI).
- `icn-demo-verify --chain` walks the governed receipt chain and confirms every
  link is consistent — **13/13**. If someone had inserted, deleted, or rewritten
  a record after the fact, the chain check would not pass.

Why a cooperative cares: it means the history of decisions and actions isn't
"trust the admin." It's verifiable by anyone who can run the check. That's the
difference between a platform that *tells* you what happened and infrastructure
that can *prove* it.

---

## 12. Common failure modes and fixes

| Symptom | Likely cause | Fix |
|---|---|---|
| Launcher exits "host port already in use" | 18080/18090/18091 busy on your workstation | Free the port, or override `ICN_DEMO_GW_PORT` / `ICN_DEMO_SESSION_PORT` (shell stays 18090). |
| Launcher "SSH tunnel exited before it came up" | Can't SSH the node / wrong key / wrong route | Confirm you can `ssh <user>@<node-ip>` with your key, or set `ICN_DEMO_JUMP` to route through a host that can. |
| Tunnel up but member-shell never answers | Node still booting, or demo profile not running | Wait for first boot; check `systemctl status icnd icn-demo-session` in the VM. |
| "Start local demo" does nothing / standing never loads | Session endpoint gated off, or dev gates not set | Confirm the image is a **demo-profile** image; check `journalctl -u icn-demo-session`. |
| Authenticated calls fail CORS | Shell served on a non-18090 origin | Use shell port **18090** (the gateway/session CORS allow-lists pin it). Don't change the shell port. |
| Cards don't refresh after discharge | Expected — see [§6 nuance](#6-the-exact-click-path) | The card flips to **Confirmed** in place; `/me/action-cards` is correctly empty. |

---

## 13. Screenshots

A real-browser pass captures the six key states. To regenerate them, run the
launcher (or local boot), then drive the click path with a browser-automation
script and screenshot each step. The canonical six are:

1. `01-launcher-open` — launcher mode, "Start local demo" visible, gateway
   pre-filled
2. `02-standing-auto` — standing rendered after one click
3. `03-action-card` — open action card
4. `04-confirm` — the confirm-before-discharge step
5. `05-receipt` — receipt with record hash
6. `06-confirmed` — card in Confirmed state

Keep screenshots free of any credential (scrub JWTs). For a live demo, the live
browser beats screenshots — use these as a fallback if the node isn't reachable.

---

## 14. The 5-minute demo script

> Goal: show the loop and the proof. No setup talk.

1. **(0:00)** "This is one ICN node, running as a self-contained appliance.
   Watch it run one full member loop." — launcher already open.
2. **(0:30)** Click **Start local demo**. "One click. No login to type, no token
   to paste." — standing renders.
3. **(1:00)** "The node knows who can act here and what their standing is." Point
   at the two domains.
4. **(1:30)** "Here's what this member can act on." Open the action card.
5. **(2:30)** Click **Mark complete** → confirm. "It tells them what they're
   doing and whether it's reversible, then they confirm." → receipt.
6. **(3:30)** "The node produced a receipt — evidence, with a hash, bound to the
   record." Expand evidence.
7. **(4:00)** Switch to a terminal: `sudo icn-demo-verify --chain` → **13/13**.
   "And it can prove its whole record chain is consistent. Not 'trust me' —
   verifiable."
8. **(4:30)** "That's the seed of infrastructure a cooperative runs themselves.
   It's a demo of the core loop — not production, not a federation yet."

---

## 15. The 15-minute demo script

> Goal: the loop, the proof, the honesty boundary, and the OpenAPI surface.

1. **(0:00–2:00) Frame it.** What ICN is ([§1](#1-what-this-demo-is)) and the
   honesty boundary ([§2](#2-what-this-demo-is-not)). "I'll show you what's real,
   and I'll be precise about what isn't."
2. **(2:00–3:00) Launch.** Run the launcher (or have it open). Explain the node
   binds loopback and you reach it through a tunnel you control.
3. **(3:00–6:00) The loop.** Click through standing → card → discharge → receipt
   → confirmed, narrating with [§7](#7-what-to-say-at-each-stage).
4. **(6:00–8:00) Evidence.** Expand the receipt; explain the record hash and what
   binding means ([§11](#11-how-to-explain-the-evidence--audit-trail)).
5. **(8:00–10:00) Proof.** Terminal: `sudo icn-demo-verify --chain` → 13/13.
   Explain why a cooperative cares about verifiable history.
6. **(10:00–11:30) Reset.** `sudo icn-demo-reset && sudo icn-demo-seed --json`,
   reload the shell — "fresh loop, repeatable."
7. **(11:30–13:00) The contract.** Show `…/api-docs/openapi.json` — "this is the
   surface an app or partner builds against."
8. **(13:00–15:00) Boundaries + next.** Walk [§2](#2-what-this-demo-is-not) and
   [§17](#17-what-comes-next). "This is the first working piece. Here's the road
   to a cooperative actually running one."

---

## 16. What this proves

- A single ICN node boots as a self-contained appliance and runs the **full
  member loop** from a clean start: standing → action card → discharge →
  completion receipt → receipt/evidence binding.
- The loop is reachable from an ordinary browser with **no credential typing or
  pasting** — the credential is fetched by one click and held in page memory
  only.
- The node can **prove its record chain is consistent (13/13)** — verifiable, not
  asserted.
- The whole thing is **reproducible**: seed, reset, reseed, and a documented
  build + smoke path.
- The member API surface is published as **OpenAPI** for integrators.

It does **not** prove federation, multi-org behavior, production readiness, real
membership, funds, mobile passport/keyring, or a signed release. ([§2](#2-what-this-demo-is-not).)

---

## 17. What comes next

In rough order of what turns this seed into something a cooperative can lean on:

1. **A baked, signed image** — fold the launcher + session sidecar into a
   from-`main` candidate (currently branch-based), then move toward a signed,
   reproducible release artifact.
2. **A second member and a two-party action** — the loop today is one member;
   the next proof is two members interacting in one context.
3. **A real org-claim ceremony** — let a cooperative claim and configure their
   own node instance (the QR node-claim is design-only today).
4. **Multi-org / federation** — two nodes coordinating; the actual
   "intercooperative" step.
5. **A real pilot** — a cooperative (NYCN is the intended first partner) running
   a node against real, consented data, with the mobile passport for member
   identity.

None of these are claimed as done. They're the map from "the core loop works" to
"members control the infrastructure."
