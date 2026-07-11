# ICN July Demo Image — Quickstart

One VM. One browser. The core ICN loop, honestly labeled:

**standing → action card → discharge → receipt → evidence/audit**

## What this demonstrates

- A real ICN node (`icnd`) booting on a disposable VM and serving its
  JWT-authenticated HTTP gateway.
- Member standing, pending action cards, action discharge, and a
  cryptographic completion receipt — rendered in the member-shell
  reference client, served by the same VM.
- Evidence you can re-check: `icn-demo-verify <item-id>` re-fetches the
  receipt and consistency-checks it (32-byte `record_hash` shape + field
  binding — it does not re-derive the BLAKE3 hash), and (deeper,
  cryptographic) `icn-demo-verify --chain` runs a 13/13 governed
  receipt-chain proof entirely inside the VM via `icnctl audit verify`.

## What this does NOT demonstrate

No production deployment. No pilot adoption. No federation or
multi-organization anything. No real member data — the institution is a
fictional fixture (NYCN package, in-tree). The image is unsigned and
mutable. Dev gates are enabled and labeled. If someone tells you this VM
is production infrastructure, they are wrong on purpose.

## System requirements

- Linux host with QEMU (`qemu-system-x86`, `qemu-utils`), `ssh`, `curl`,
  `jq`, `cloud-image-utils` (only if you need to build a cloud-init seed).
- ~4 GB free disk, 2 GB RAM for the VM (`-m 2048` recommended).
- For building the image: Rust toolchain + `libguestfs-tools`, a staged
  Debian cloud base image (glibc ≥ your build host's — see
  "Host / image compatibility" in [README.md](README.md)).

## Get or build the image

> **Network: build-time vs operate-time.** *Building or staging* this image is
> **not** offline — the steps below start with a `git clone` and
> `build-image.sh --real` runs `virt-customize --update --install`, which fetches
> Debian packages and dependencies over the network. *Operating* an
> already-staged local image (boot, launcher, member loop, verify, reset) is
> **local / offline-ish** and needs no partner. This remains a DEV/DEMO appliance
> profile — not production, and not partner-distributable infrastructure.

There is no prebuilt download. Build from a public checkout:

```bash
git clone https://github.com/InterCooperative-Network/icn && cd icn
export ICN_APPLIANCE_BASE_IMAGE=/path/to/debian-13-genericcloud-amd64.qcow2
export ICN_APPLIANCE_OUTPUT_DIR=$HOME/icn-appliance-build
export ICN_APPLIANCE_VERSION=0.0.2-demo
export ICN_APPLIANCE_DEMO_PROFILE=1
bash deploy/appliance/build-image.sh --real
```

Output: `$ICN_APPLIANCE_OUTPUT_DIR/icn-appliance-0.0.2-demo-amd64.qcow2`
plus a manifest JSON with the image SHA256 and `demo_profile: true`.

## Run it

Prepare a one-off SSH key + cloud-init seed (see "Real local build + boot
smoke" in [README.md](README.md)), then either:

**Scripted (recommended first run):**

```bash
ICN_APPLIANCE_IMAGE=$HOME/icn-appliance-build/icn-appliance-0.0.2-demo-amd64.qcow2 \
ICN_APPLIANCE_SSH_KEY=/path/to/smoke_ed25519 \
ICN_APPLIANCE_CLOUD_INIT_SEED=/path/to/seed.iso \
ICN_APPLIANCE_VM_MEMORY=2048 \
bash deploy/appliance/smoke/smoke-local.sh --real --demo
```

That boots the VM on a disposable overlay, seeds the demo, drives the whole
loop headlessly, and prints PASS/FAIL. It is the same path your browser
will take. The demo smoke also blocks guest-initiated outbound networking by
default (QEMU user-net `restrict=on`; the loopback port forwards are
unaffected) and proves it with an in-guest canary probe — set
`ICN_APPLIANCE_ALLOW_OUTBOUND=1` to permit outbound. This applies to the QEMU
smoke only; an already-running Proxmox/cloud node's network isolation is
operator-provided (see the runbook's "Network posture" section).

**Manual (for the browser demo):** boot QEMU yourself with the demo ports
forwarded. `restrict=on` matches the scripted smoke's default posture —
guest-initiated outbound is blocked while the loopback port forwards keep
working. Drop `,restrict=on` only if you deliberately want guest outbound;
note the manual path never runs the smoke's isolation canary either way, so
the scripted smoke remains the proven route:

```bash
qemu-img create -f qcow2 -b "$IMAGE" -F qcow2 overlay.qcow2
qemu-system-x86_64 -machine accel=kvm:tcg -m 2048 -smp 2 -display none \
  -drive if=virtio,format=qcow2,file=overlay.qcow2 \
  -drive if=virtio,format=raw,file=seed.iso,readonly=on \
  -netdev user,id=net0,hostfwd=tcp:127.0.0.1:2222-:22,hostfwd=tcp:127.0.0.1:18080-:8080,hostfwd=tcp:127.0.0.1:18090-:8090,restrict=on \
  -device virtio-net-pci,netdev=net0 -nographic
```

Then seed it:

```bash
ssh -p 2222 debian@127.0.0.1 sudo icn-demo-seed
```

The seed prints the member-shell URLs, the open action item, and a dev
session JWT (local VM only).

## Rehearsal Node v0.1 wrapper

The appliance DEV/DEMO paths on this page have one named operator entrypoint:
`deploy/appliance/scripts/icn-rehearsal-node.sh` (`smoke-image`,
`open-running-node`, `verify-running-node`, `--dry-run`, `--help`). It only
delegates to the commands documented here — same env vars, same safety
boundaries, same non-claims. Runbook:
[docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md](../../docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md).

## One-command launcher (recommended for a live human demo)

If a node instance is already running (e.g. a Proxmox VM at `192.0.2.50` — use your node's real IP),
open the whole demo with one command from your workstation — no JWT
copy/paste, no gateway typing:

```bash
# direct (this machine can SSH the node and has the demo key):
ICN_DEMO_VM_IP=192.0.2.50 ICN_DEMO_SSH_KEY=/path/to/smoke_ed25519 \
  bash deploy/appliance/scripts/open-proxmox-demo.sh

# or jump through a dev host that holds the key and can reach the node
# (ICN_DEMO_REMOTE_KEY is REQUIRED with ICN_DEMO_JUMP — it is the key path
# ON the jump host):
ICN_DEMO_VM_IP=192.0.2.50 ICN_DEMO_JUMP=user@dev-host \
  ICN_DEMO_REMOTE_KEY=/path/to/smoke_ed25519 \
  bash deploy/appliance/scripts/open-proxmox-demo.sh
```

It opens the SSH tunnels (gateway→18080, shell→18090, demo-session→18091),
then opens your browser to `…/member-shell/?mode=live&demo=launcher`. In the
page, the gateway is pre-filled and a **Start local demo** button appears —
select it once and your standing + a sample action card load automatically.
Nothing is copied or pasted; the credential lives only in the page's memory.
Press Ctrl-C in the terminal to close the tunnel when you're done.

This needs a demo-profile image (the `icn-demo-session` endpoint ships with
`ICN_APPLIANCE_DEMO_PROFILE=1`). Everything below is the manual fallback.

## First URL to open (manual fallback)

> http://localhost:18090/member-shell/

(Use `?mode=demo` for the self-labeled fixture mode that needs no JWT. The
manual `?mode=live` paste flow below is the advanced/debug path.)

## The demo script (5 steps)

> These five steps are the **manual fallback**. With the one-command launcher
> above, standing and the action card load on a single **Start local demo**
> click — no gateway typing and no JWT paste — so step 1's manual connect is
> unnecessary. Drive the shell by hand with the steps below only when the
> launcher is not usable.

1. **Inspect standing** — open the shell in live mode, set the **Gateway
   address** field to `http://localhost:18080` (the forwarded gateway —
   the shell's default of `:8080` points at an unforwarded host port in
   this setup), and paste the JWT from `icn-demo-seed`. The standing pane
   shows the operator's domain membership in the fictional NYCN
   institution.
2. **Inspect the action card** — one open card: "Confirm Summit 2026 venue
   booking" (`action_item / complete`, fictional).
3. **Discharge the action** — complete it in the shell (live-mode
   mutation: `PUT .../status {"status":"completed"}` under the hood).
4. **View the receipt** — the shell fetches the completion receipt: item,
   domain, actor DID, transition, timestamp, and the 32-byte BLAKE3
   `record_hash` that binds them.
5. **Verify evidence/audit** — in the VM:
   `sudo icn-demo-verify <item-id>` re-fetches the receipt and
   consistency-checks it (shape + field binding; not a BLAKE3
   re-derivation); `sudo icn-demo-verify --chain` runs the full 13/13
   governed receipt-chain rehearsal (`icnctl audit verify`) on a fresh
   ephemeral node and emits a schema-validated evidence packet to
   `/var/lib/icn-demo/`.

## Reset

`sudo icn-demo-reset` clears this node's demo state — it proves nothing and does
**not** reseed; nothing shows until you reseed or relaunch.

- Cheapest: power off and delete `overlay.qcow2`, recreate, reboot —
  whole-disk reset, nothing persists.
- In-place: `ssh -p 2222 debian@127.0.0.1 sudo icn-demo-reset` destroys
  node state and re-runs firstboot.
- Reseed — **launcher (recommended live path):** reload the launcher (rerun the
  one-command launcher, or refresh the launcher URL) and click **Start local
  demo** — the Start button only shows on a fresh page load, so a reset alone
  won't restore it on an already-open tab. It seeds one fresh card with no JWT
  to paste. **Manual / debug fallback:** `sudo icn-demo-seed` reseeds directly
  but prints a local DEV credential — keep that credential out of docs,
  screenshots, terminal transcripts, and PRs.

## Troubleshooting

| Symptom | Likely cause / fix |
|---|---|
| Shell says "Technical detail: Failed to fetch" in live mode | CORS or wrong gateway port. The demo drop-in allows origins `localhost:8090/18090` and `127.0.0.1:8090/18090`. Open the shell via one of those exactly; the gateway must be the hostfwd'd 18080 (or in-VM 8080). |
| Port already in use on the host | Another process holds 2222/18080/18090. Change the hostfwd host-side numbers. Changing the **shell** port is a manual-QEMU-only path: the gateway/session CORS allow-lists must then contain that exact origin (edit `/etc/systemd/system/icnd.service.d/20-demo-profile.conf` in the VM, `systemctl daemon-reload && systemctl restart icnd`). The one-command launcher does **not** support a shell-port override — it pins 18090, so free that port instead. |
| `icn-demo-seed` fails: "$ENV_FILE missing" | Firstboot has not completed. `journalctl -u icn-appliance-firstboot`. The icnd unit is gated on firstboot success by design. |
| Passphrase/identity errors from icnctl | The VM's keystore passphrase lives in `/etc/icn/icnd.env` (mode 600, owned `icn:icn`). The demo scripts read it themselves — run them with `sudo`, don't export your own. |
| QEMU: KVM permission denied | Your user lacks /dev/kvm access; the launch line falls back to TCG (slow but works). `usermod -aG kvm $USER` for speed. |
| Stale state after re-running the demo | Each `icn-demo-seed` adds a new open card. Old JWTs die with `icn-demo-reset` (new per-instance secret). When in doubt: overlay reset. |
| 13/13 rehearsal slow | It builds nothing (uses installed binaries) but runs a full governed lifecycle; a few minutes at 2 GB RAM is normal. |
| "Start local demo" returns **403** | Read the JSON error — two distinct causes. `origin not allowed` = the page is not on an allowed shell origin (`http://localhost:18090` or `http://127.0.0.1:18090` — exactly those two loopback spellings on the fixed shell port; don't change it). `demo session disabled (not a DEV/DEMO posture)` = dev gates off; confirm a demo-profile image and check `journalctl -u icn-demo-session`. |
| Session worked but gateway calls then fail | The session endpoint (18091) and the gateway (18080) are separate. Confirm `gateway→18080` is tunnelled and the page's gateway field reads `http://localhost:18080`; a non-18090 page origin also fails the gateway's own CORS. |
| `icnd` won't start / sled lock held | A previous `icnd` did not exit cleanly and still holds the sled DB lock. In the VM: `systemctl restart icnd` (clears the stale process + lock). If it persists: `sudo icn-demo-reset` (destructive) or overlay reset. |
| Live node unreachable — abandon the live demo | Fall back without a node: open `…/member-shell/?mode=demo` (self-labeled fixtures, no JWT, no node), or show recorded screenshots. Say plainly it is the fixture/recorded fallback, not a live run. |

## Honesty labels

| Tier | Surfaces |
|---|---|
| **live-local** | node boot, `/v1/health`, standing, action cards, discharge, completion receipt, receipt binding check, 13/13 chain rehearsal — on this VM's own node |
| **fixture-backed** | member-shell `?mode=demo` panes (self-labeled), NYCN institution package contents |
| **design-only / absent** | production posture, signed/immutable image, federation, multi-org, pilots, attendance-receipt retrieval endpoint (known gap, `docs/dev/openapi-member-surface-gaps.md`) |

## See also

- [`docs/demo/JULY_DEMO_HANDS_ON.md`](../../docs/demo/JULY_DEMO_HANDS_ON.md) — full click-by-click guide, presenter "what to say" script, and the complete failure-mode table.
- [`docs/demo/JULY_DEMO_OPERATOR_CHECKLIST.md`](../../docs/demo/JULY_DEMO_OPERATOR_CHECKLIST.md) — the one-page checklist to keep open during a live run.
- [`docs/demo/JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md`](../../docs/demo/JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md) — operator + reviewer handoff: candidate pin, claim boundary by proof level, evidence-capture checklist, and reviewer checklist.
