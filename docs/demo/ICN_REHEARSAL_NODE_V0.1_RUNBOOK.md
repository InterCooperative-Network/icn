# ICN Rehearsal Node v0.1 Runbook

Status: dev/demo operator runbook
Related: #2386

## What this is

A single-node, fixture-backed rehearsal node path built on the existing ICN
appliance DEV/DEMO profile, with one named operator entrypoint:

```bash
bash deploy/appliance/scripts/icn-rehearsal-node.sh --help
```

"Rehearsal Node v0.1" is a name and a runbook for machinery that already
shipped (the July Demo Candidate 0.1 appliance profile), not a new node path.
The node instance is a disposable local VM; the demo data is a fictional
fixture institution; operation needs no outbound network beyond the operator's
own SSH tunnels.

## What this proves

- A local ICN node can boot from the DEV/DEMO appliance image (per-VM secrets
  generated at first boot; no secrets baked into the image).
- `icnd` serves health and gateway endpoints (`GET /v1/health`, `/v1/gov/*`).
- The member shell can render the organizer/member loop in a browser.
- Fictional fixture work can become an action card
  (`GET /v1/gov/me/action-cards`).
- Completing that action produces a completion receipt (the canonical mutation
  is `PUT /v1/gov/domains/{domain_id}/action-items/{item_id}/status` with
  `{"status":"completed"}`; the receipt is fetched from
  `GET .../completion-receipt`).
- The receipt can be re-fetched and consistency-checked
  (`sudo icn-demo-verify <item-id>`).
- The deeper 13-of-13 governed receipt-chain rehearsal remains available
  through `sudo icn-demo-verify --chain`.

A note on verification honesty: `icn-demo-verify <item-id>` and
`icn-demo-verify --chain` are **re-fetch + consistency / provenance-linkage
audits**. Neither re-derives the BLAKE3 `record_hash` or checks a signature on
the client. The hash binding is created server-side when the receipt is
emitted (`ActionItemCompletionReceipt`, domain tag
`icn:gov:action_item_completion:v1`). Do not describe the client verify steps
as cryptographic verification.

## What this does not prove

- No production readiness.
- No pilot adoption.
- No live federation.
- No multi-organization network.
- No real NYCN data.
- No private overlay.
- No live DID activation.
- No runtime bridge.
- No connectors.
- No payment / wallet / balance / currency / token framing — the loop is
  governed coordination with provenance receipts.

## Existing pieces reused

| Piece | Role |
|---|---|
| `deploy/appliance/build-image.sh` | Builds the QCOW2 appliance image; `ICN_APPLIANCE_DEMO_PROFILE=1` adds the DEMO profile. Run separately — the wrapper never builds. |
| `deploy/appliance/smoke/smoke-local.sh --real --demo` | Boots a disposable QEMU overlay and drives the full loop headlessly over the same forwarded ports a browser would use. |
| `deploy/appliance/scripts/icn-demo-seed.sh` | In-VM: mints a short-lived DEV/DEMO session JWT via **trusted local issuance** (signs with this VM's own per-instance gateway secret — see the auth-boundary note below), bootstraps the fixture institution, creates one action item. Not idempotent — use reset for a clean slate. |
| `deploy/appliance/scripts/icn-demo-verify.sh` | In-VM: per-item receipt consistency check; `--chain` runs the bundled 13-of-13 receipt-chain rehearsal. |
| `deploy/appliance/scripts/icn-demo-reset.sh` | In-VM: marker-gated demo-state reset (does not reseed). |
| `deploy/appliance/scripts/icn-demo-session.py` | In-VM loopback (`127.0.0.1:8091`) session endpoint behind a double dev-gate and an Origin allow-list; powers the shell's no-paste "Start local demo" button. |
| `deploy/appliance/scripts/open-proxmox-demo.sh` | Workstation launcher: SSH-tunnels gateway/shell/session ports and opens the member shell — no JWT paste, no gateway typing. |
| `web/member-shell/` | The browser surface the appliance serves (`:8090` in-VM): standing, action cards, the single completion mutation, receipt rendering, permanent honesty banner, i18n seam, automated accessibility harness. |
| `demo/nycn-dogfood/run.sh` | The workstation-native sibling of the same loop (deliberately on gateway `:8085`); useful for development without a VM. |
| `scripts/local_receipt_chain_13of13_rehearsal.sh` | The strongest single proof artifact (governed proposal → vote → close → allocation, 13/13 audit, repo-safe evidence packet); bundled in-VM as `icn-demo-verify --chain`. |

## Auth boundary — trusted local issuance vs self-asserted (#2075)

The demo profile binds the gateway to `0.0.0.0:8080` so a browser on the host
reaches it through QEMU host-forwarding. On any non-loopback bind the public
self-asserted `/auth/verify` path is **fail-closed** (issue #2075): a DID that
proves only key ownership can never mint a JWT carrying an arbitrary `coop_id`.

The seed therefore mints its session JWTs by **trusted local issuance**:
`icnctl auth token --local-mint` signs a JWT in-process with the node's own
per-instance gateway secret (`ICN_GATEWAY_JWT_SECRET`, generated at first boot,
stored mode 0600 owned `icn:icn` — readable by the icn service account, not
root-exclusive; any process running as `icn` can exercise gateway signing
authority). That is the gateway issuing a JWT for itself to its local operator —
categorically distinct from an untrusted remote self-asserting a coop over the
network. It makes no network call, adds no endpoint, and leaves `/auth/verify`
fail-closed. `institution bootstrap apply --local-mint` uses the same path. The
secret never leaves the VM and is never baked into the image; a JWT minted with
one VM's secret does not verify on any other node.

The seed mints two least-privilege credentials: an internal **setup** token that
provisions the demo (never printed), and the **browser** token it hands to the
member shell — scoped to `governance:read` plus `governance:meeting:write`, the
narrowest gateway scope that authorizes the action-item completion the shell
performs. As a member of the seeded fictional domain that scope also permits
creating action items / meetings there (a completion-only scope decomposition is
a tracked follow-up), but the browser token cannot reach cooperative-admin,
entity, treasury, or the broad / other governance write classes.

## Fast path A — smoke an already-built demo image

```bash
ICN_APPLIANCE_IMAGE=/path/to/icn-appliance-demo.qcow2 \
ICN_APPLIANCE_SSH_KEY=/path/to/smoke_ed25519 \
bash deploy/appliance/scripts/icn-rehearsal-node.sh smoke-image
```

This boots a disposable local QEMU overlay (the image is never modified) and
drives the demo loop headlessly: health → member shell → seed → standing →
action card → complete → receipt.

Cloud-init seed: one of two preconditions must hold, or the run fails before
the VM boots (the wrapper preflights this):

- `ICN_APPLIANCE_CLOUD_INIT_SEED` points at a pre-built seed ISO, **or**
- `deploy/appliance/smoke/cloud-init/user-data.example.yaml` has been edited
  to carry the smoke-only **public** key matching `ICN_APPLIANCE_SSH_KEY`
  (smoke-local then builds a seed via `cloud-localds`). smoke-local refuses
  the shipped `INVALIDREPLACEME` placeholder and does **not** derive the
  public key from `ICN_APPLIANCE_SSH_KEY`.

The image must have been built with `ICN_APPLIANCE_DEMO_PROFILE=1`; building
it is a separate step documented in `deploy/appliance/DEMO_QUICKSTART.md`.

## Fast path B — open an already-running node instance

Direct route:

```bash
ICN_DEMO_VM_IP=192.0.2.50 \
ICN_DEMO_SSH_KEY=~/.ssh/icn_demo_ed25519 \
bash deploy/appliance/scripts/icn-rehearsal-node.sh open-running-node
```

Jump-host route (key lives on the jump host):

```bash
ICN_DEMO_VM_IP=192.0.2.50 \
ICN_DEMO_JUMP=user@jump.example.internal \
ICN_DEMO_REMOTE_KEY=/home/user/.ssh/icn_demo_ed25519 \
bash deploy/appliance/scripts/icn-rehearsal-node.sh open-running-node
```

This tunnels the gateway (`18080`), member shell (`18090` — fixed: it is the
browser Origin the gateway CORS and the session endpoint pin), and the
loopback demo-session port (`18091`), then opens the shell with
`?demo=launcher`. The "Start local demo" button mints a short-lived DEV/DEMO
session via the in-VM loopback endpoint — the credential lives in page memory
only, never in a URL, never pasted.

## Evidence path

- `sudo icn-demo-verify <item-id>` — re-fetches the completion receipt and
  checks its field binding (item, domain, transition, 32-byte `record_hash`
  present). A consistency check, not a client-side hash re-derivation.
- `sudo icn-demo-verify --chain` — runs the deeper 13-of-13 governed
  receipt-chain rehearsal (`icnctl audit verify`) against an ephemeral in-VM
  gateway and emits a repo-safe evidence packet conforming to
  `urn:icn:contract:rehearsal-evidence-export:v1`.
- Evidence lives under `/var/lib/icn-demo/` in the demo VM
  (`receipt-chain-13of13/` for the chain packet).
- `bash deploy/appliance/scripts/icn-rehearsal-node.sh verify-running-node`
  prints these steps (and ready-to-copy ssh one-liners when the route env vars
  are set); it never executes anything remotely.

## Network posture (which route guarantees what)

The two fast paths make **different** isolation promises. Do not collapse
them.

**Fast path A (`smoke-image`) — enforced and proven.** The demo smoke boots
the disposable VM with QEMU user-net `restrict=on` by default. Per the QEMU
manual, the guest "will not be able to contact the host and no guest IP
packets will be routed over the host to the outside," while explicitly set
`hostfwd` rules (SSH, gateway, member shell) are unaffected. Two proofs back
this:

- *Static (runs everywhere, including offline CI):*
  `deploy/appliance/smoke/net-restrict-check.sh` asserts the constructed
  `-netdev` string — demo default restricted, override honored, base smoke
  unchanged — and is wired into `deploy/appliance/check.sh`.
- *Runtime (runs on every real `--demo` smoke):* the smoke starts a canary
  listener on host loopback, verifies it is reachable **from the host**, then
  proves it is **unreachable from the guest** via the `10.0.2.2` slirp host
  alias. No public internet host is involved, so an offline runner cannot
  false-pass; a guest that reaches the listener fails the smoke loudly
  (FAIL-OPEN). Setting `ICN_APPLIANCE_ALLOW_OUTBOUND=1` permits outbound,
  skips the canary, and says so in the output and the PASS footer.

What was tested **statically** in the change that introduced this posture: the
netdev construction matrix (4 cases) and all script negative paths. **Since
witnessed through a running VM:** the restricted boot and in-guest
outbound-isolation canary have been exercised on real KVM boots (2026-07-10, and
again on freshly built demo-profile images 2026-07-11) — the guest could not
reach a per-run host listener while the loopback demo loop completed. "Blocked
by default" is therefore a witnessed runtime result on the `smoke-image` route,
not only a documented-QEMU-semantics + static-construction claim. If the
restricted boot misbehaves, rerun with `ICN_APPLIANCE_ALLOW_OUTBOUND=1` and file
the finding.

**Fast path B (`open-running-node`) — operator-provided, not enforced.** The
launcher opens SSH tunnels to an already-running VM; it cannot and does not
seal that VM's network. The demo profile binds its services beyond loopback
inside the VM, so an already-running node on a bridged network is reachable
from that network regardless of anything this wrapper does. The wrapper
states this on every run and adds a bounded, warn-only exposure preflight: if
the node's gateway port answers this workstation directly, it warns; if it
does not, that is explicitly **not** proof of isolation. A real sealed-node
firewall profile does not exist yet; until it does, the no-outbound-by-default
claim is scoped to the disposable local QEMU route only.

## Relationship to other ICN run paths

| Path | What it is | Where it fits |
|---|---|---|
| `demo/SELF_SERVE.md` Path 0 | Fixture-only static browser demo (pilot-ui `?mode=demo`), zero build | First look, two minutes, nothing live |
| `demo/SELF_SERVE.md` Path 1 | `scripts/local_receipt_chain_13of13_rehearsal.sh` — the strongest single proof (live-local 13/13 + evidence packet) | Proof, not presentation |
| `demo/SELF_SERVE.md` Path 2 | `demo/nycn-dogfood/run.sh` — same loop, workstation-native, gateway `:8085` | Developer story runner, no VM |
| `deploy/devnet/` | Three-node Docker devnet | Multi-node gossip/convergence; explicitly not federation |
| **Appliance DEV/DEMO profile (this runbook)** | Disposable VM + member shell + launcher + in-VM verify | **The Rehearsal Node v0.1 route** |

Rehearsal Node v0.1 is specifically the appliance/member-shell route because
it is the only path where a facilitator can walk the loop in a browser with no
terminal after startup, on a node instance that is disposable-by-construction,
with per-VM secrets, honesty labels on the surface, and the deeper chain proof
one command away inside the same VM.

## Known gaps

- The demo-profile image must be built or staged separately
  (`build-image.sh --real` with `ICN_APPLIANCE_DEMO_PROFILE=1`); the wrapper
  does not build or download images.
- The wrapper does not create the Debian base image or a cloud-init seed
  itself; it only preflights the seed precondition described in Fast path A
  (pre-built ISO, or an edited `user-data.example.yaml` from which smoke-local
  builds one).
- The shell is the member-shell v0 reference client, not a production app and
  not the #1726 organizer rehearsal shell; the human assistive-technology pass
  (#2041) is still owed, and only automated accessibility evidence exists.
- The no-outbound guarantee is scoped to the `smoke-image` QEMU route (see
  "Network posture"): enforced by default, statically tested, and canary-proven
  on real KVM boots (witnessed 2026-07-10 and 2026-07-11). The
  `open-running-node` route stays operator-provided. #1727's
  shell-level demo-mode criteria (fixture loader as default, live mode as a
  labeled opt-in with a mutation warning) remain open.
- Action cards derive from three of five source paths; `signal_rule` and
  `obligation_lifecycle` are reserved and not emitted.
- v0.2 (a two-node local proof) is a separate follow-up issue, not this path.
