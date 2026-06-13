# ICN July Demo — Operator Checklist

> Keep this open during the demo. Full context:
> [JULY_DEMO_HANDS_ON.md](JULY_DEMO_HANDS_ON.md). Candidate pin, evidence
> capture, and reviewer handoff:
> [JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md](JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md).
> This is a **DEV/DEMO**. Not production, not a federation, not real members/funds.

---

## ☐ Preflight (before anyone is watching)

- ☐ Node instance is booted and the gateway answers (first boot finished).
- ☐ Host ports `18080` / `18090` / `18091` are free on your workstation.
- ☐ You can reach the node (local QEMU forwards, or `ssh <user>@<node-ip>` works).
- ☐ Browser open and ready.
- ☐ Run one full loop privately once, then reset — so the live run is clean.
- ☐ You can say the honesty boundary in one breath: *"core loop, real; demo data,
  fixture; federation/production/funds, not yet."*

---

## ☐ Launch command

**Local (QEMU on this box):** boot the image with forwards (see hands-on §4), then open the URL below.

**Remote / Proxmox (one command):**
```bash
ICN_DEMO_VM_IP=<your-node-ip> ICN_DEMO_SSH_KEY=/path/to/demo_key \
  bash deploy/appliance/scripts/open-proxmox-demo.sh
```
Jump route: add `ICN_DEMO_JUMP=user@dev-host ICN_DEMO_REMOTE_KEY=/path/to/key`,
drop `ICN_DEMO_SSH_KEY`.

---

## ☐ Browser URL

```
http://localhost:18090/member-shell/?mode=live&demo=launcher&gw=18080&session=18091
```
(The launcher opens this for you. Shell port is fixed at **18090** — don't change it.)

---

## ☐ Expected visible states (in order)

1. ☐ Launcher view: **"Start local demo"** button, gateway pre-filled, manual form hidden
2. ☐ One click → **standing** renders (two domains)
3. ☐ **Action card** appears
4. ☐ **Mark complete** → confirm step (declares action + reversibility)
5. ☐ **Confirm — mark complete** → **receipt** with record hash
6. ☐ Expand → **evidence** detail
7. ☐ Card shows **Confirmed** in place

> Expected nuance: after discharge `/me/action-cards` is **empty**; the on-screen
> card flips to **Confirmed** rather than disappearing. Consistent, not a bug.

---

## ☐ Reset command (between runs) — inside the node

```bash
sudo icn-demo-reset                 # clears state; reseed or relaunch after
```
Reset proves nothing and does not reseed. Then **launcher (recommended):**
reload the launcher URL, then click **Start local demo** — the button hides
after the first session, so a reset alone won't restore it on an open tab
(seeds one card, no paste). **Manual/debug:** `sudo icn-demo-seed --json`
(prints a local DEV credential — don't paste it into public artifacts); look for
`"standing_note": "bootstrap-standing: ok"`. Reload the shell.

---

## ☐ Verification commands — inside the node

```bash
sudo icn-demo-verify <item-id>     # single item consistency
sudo icn-demo-verify --chain       # full chain → expect 13/13 (the real proof)
```
Show the OpenAPI surface (gateway port):
```bash
curl -s http://localhost:18080/api-docs/openapi.json | head -c 400; echo
```

---

## ☐ Panic fixes (live)

| Problem | Do this |
|---|---|
| Port in use | Override `ICN_DEMO_GW_PORT` / `ICN_DEMO_SESSION_PORT`; free 18090. |
| Tunnel exits immediately | Re-check `ssh <user>@<node-ip>` / key; or use `ICN_DEMO_JUMP`. |
| Shell never answers | Wait for boot; in VM: `systemctl status icnd icn-demo-session`. |
| Start-demo does nothing | Demo gates: in VM `journalctl -u icn-demo-session`; confirm demo-profile image. |
| Auth calls fail CORS | Shell must be on **18090**. Don't change it. |
| Loop got messy / extra cards | `sudo icn-demo-reset`, then **reload the launcher URL** and click **Start local demo** (the button hides after the first session, so reset alone won't restore it) — or `sudo icn-demo-seed --json` for the manual path. |

---

## ☐ Final cleanup

- ☐ Press **Ctrl-C** in the launcher terminal to close the tunnels.
- ☐ Optionally `sudo icn-demo-reset` in the node to leave it clean.
- ☐ If the node was disposable, shut it down / remove the VM.
- ☐ No credential was written to disk or a URL; nothing to scrub on the workstation.
