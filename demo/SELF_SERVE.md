# See ICN Run — Self-Serve Quickstart

You don't need to know how ICN works to run this. You need a laptop,
and depending on the path you pick: nothing else, Docker, or a Rust
toolchain. **Nothing here requires anyone's cluster, homelab, or
private infrastructure.**

Every path below is labeled with its honesty tier:

- **live-local** — real ICN runtime (`icnd` + gateway) running on your
  machine, producing real signed records you can audit.
- **fixture-backed** — presentation surfaces rendering committed,
  CI-drift-guarded fixture data. No backend. By design.
- **design-only** — specified, not runnable. We say so rather than fake it.

Things no path demonstrates, because they don't exist yet: production
deployments, a live multi-organization federation, member-facing apps
in real use, private-data handling. There is no pilot. See
[`docs/strategy/ICN_HARD_QUESTIONS.md`](../docs/strategy/ICN_HARD_QUESTIONS.md)
for the unvarnished status.

---

## What You're Looking At

ICN is coordination infrastructure for cooperatives: it turns
decisions and work into **legible obligations** and **verifiable
receipts** on infrastructure the group controls. The loop every path
below demonstrates, in part or in full:

> work → obligation → action card → discharge → receipt → audit

A receipt is an evidence record — it proves *this actor* discharged
*this obligation* at *this time*. It is not currency, not a token, and
not a payment. Nothing here is a financial product.

---

## Path 0 — Two minutes, zero build (fixture-backed)

See the member-facing surfaces render: standing, action cards,
plain-language receipts.

```bash
git clone https://github.com/InterCooperative-Network/icn
cd icn/web/pilot-ui
python3 -m http.server 8000
# open http://localhost:8000/?mode=demo
```

In demo mode the standing and action-cards sections read committed
fixture JSON (`web/pilot-ui/fixtures/icn-organizer-demo/`) instead of
a gateway. The fixtures use fictional identities
(`did:icn:example-*-not-live`) and are CI-checked against the real API
schemas, so what you see is shaped exactly like live data — but it is
**fixture-backed**: no node is running and nothing is signed.

## Path 1 — The proof, one command (live-local)

The strongest single artifact ICN has: a governed proposal → vote →
close → allocation driven over a real, JWT-secured local gateway,
ending in a **13/13 receipt-chain audit** (`icnctl audit verify`).

```bash
git clone https://github.com/InterCooperative-Network/icn
cd icn
bash scripts/local_receipt_chain_13of13_rehearsal.sh
```

Requirements: Rust toolchain (rustup), ~10 GB disk. The first run
compiles the workspace — **20–60+ minutes depending on your machine**.
The run ends with `RESULT: PASS` and writes a schema-valid evidence
packet to `demo/output/receipt-chain-13of13/`.

This is dev-gated and local: the script seeds standing/trust through
explicitly dev-only switches that do not exist in production
configuration. What it proves is the receipt chain and audit path, on
hardware you control. Last verified from a clean checkout: 2026-06-11.

## Path 2 — The story, one command (live-local)

The cooperative-work narrative end to end: a piece of organizing work
becomes a tracked obligation, shows up as an action card, gets
discharged, and produces a hash-bound completion receipt.

```bash
# from the repository root (the directory you cloned, e.g. `cd icn`)
cargo build --release -p icnd -p icnctl --manifest-path icn/Cargo.toml   # skip if Path 1 already built these
export ICN_PASSPHRASE=demo-anything   # protects a local throwaway keystore
bash demo/nycn-dogfood/run.sh --fresh --record
```

The build line is a no-op if Path 1 already compiled the workspace —
the script itself only checks for the binaries, it does not build them.
`--record` renders an HTML transcript under
`demo/nycn-dogfood/runs/<timestamp>/` you can replay for an audience.

Two honest footnotes:
- **If you already run ICN on this machine** (an `~/.icn/identity.age`
  exists), the script binds to that operator identity and will fail
  with a mismatched passphrase. Run hermetically:
  `HOME=$(mktemp -d) ICN_PASSPHRASE=demo-anything bash demo/nycn-dogfood/run.sh --fresh --record`
- The local node stays running after the script exits (the next
  `--fresh` run reclaims it). That's deliberate — the node belongs to
  the cooperative — but don't be surprised by the leftover process.

Last verified from a clean checkout: 2026-06-11.

## Path 3 — Three nodes on your laptop (live-local, Docker)

Boot a three-node devnet and run the governance demonstration suite
against two independent nodes.

```bash
# from the repository root (the directory you cloned, e.g. `cd icn`)
docker compose -f deploy/devnet/docker-compose.yml build   # one-time; long
bash demo/run-all.sh
```

Requirements: Docker + compose, python3 with the `cryptography`
package. The image build compiles the workspace inside Docker —
budget an hour on a small machine. Ends with a per-node pass/fail
summary box.

Three independent nodes on one machine is **not** a federation of
independent organizations — it demonstrates that nodes are
self-contained, not that production federation exists (that remains
design-only).

## Presenter path — scripted flows on owned infrastructure

`demo/scripts/flow-1-governance.sh` through `flow-5-compute.sh` (plus
`reseed-federation-demo.sh`, `rehearsal-probe.sh`, `present.sh`) are
presenter-driven flows written for a multi-node deployment the
presenter operates — they assume a seeded cluster and a reachable
gateway, and some narrate known deployment gaps (403/404 with a
yellow note) rather than hiding them. If you are not the presenter,
Paths 0–3 are your versions of the same claims, minus the cluster.

### Reading the output (all paths)

- **White**: narrator — what's happening, who's doing it
- **Green ✓**: succeeded, verified
- **Yellow ·/⚠**: presenter context, or an expected, narrated gap
- **Red ✗**: actually broken — a red line in a fresh checkout is a bug,
  and a bug report is a welcome contribution

---

## After the demo

- What each claim is allowed to assert, with evidence links:
  [`docs/strategy/ICN_INTRODUCTION_EVIDENCE_MAP.md`](../docs/strategy/ICN_INTRODUCTION_EVIDENCE_MAP.md)
- The hard questions, answered without marketing:
  [`docs/strategy/ICN_HARD_QUESTIONS.md`](../docs/strategy/ICN_HARD_QUESTIONS.md)
- Current truth, per-PR: [`docs/STATE.md`](../docs/STATE.md)

If something above is overclaimed, file an issue saying exactly that.
That's not politeness — the evidence-gated introduction discipline is
the product working as intended.
