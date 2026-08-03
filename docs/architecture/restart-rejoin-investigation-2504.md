# Restart/rejoin investigation (#2504) — causal record

**Status:** living — investigation record, updated as the chain resolves
**Truth class:** descriptive
**Canonical:** no — issue threads are authoritative; this is the navigable summary
**Last reviewed:** 2026-08-03
**Source basis:** live four-node K3s rehearsal federation, `72dfed8e` → `045fa9e9`
**Related invariant:** [network-identity-self-exclusion.md](network-identity-self-exclusion.md)

## Original symptom

Restarting a node in a healthy federation left it unable to rejoin. The restarted node reported
`icn_gossip_network_size_estimate = 1`, never incremented `icn_gossip_digests_received_total`, and
the whole `bloom_*` / `pull_*` / `bytes_pulled` metric family was absent — while untouched nodes sat
at 4 and kept gossiping normally. Rollback did not recover it, because rollback is itself a restart.

## Causal decomposition

Four independent defects, each masked by the one before it:

```
peer restart
  → stale QUIC connection cache        #2505  fixed  adb9d07d
  → ephemeral outbound signing seq     #2510  fixed  72dfed8e (PR #2511)
  → local identity admitted as remote  #2506  fixed  045fa9e9 (PR #2513)
  → receiver restart replay floor      #2514  OPEN — current blocker
```

**The masking is the important part.** Each fix made the next defect reachable for the first time,
so a newly appearing blocker is evidence the previous fix worked, not that it was wrong.

- #2505 kept traffic from flowing at all, so nobody could observe #2510's sequence regression.
- #2510 caused peers to reject 100 % of a restarted node's traffic, which hid #2506's inbound cost.
- #2506's self-pollution meant a node barely received from its peers, so it never persisted a peer
  high-water mark — which is exactly what #2514 needs in order to bite.

### #2505 — connection registry treated map occupancy as liveness

`SessionManager.connections` never reaped dead entries and refused to replace them, so every
surviving peer held a dead `quinn::Connection` for a restarted peer's DID until its own process
restarted. The fix treats a connection whose `close_reason().is_some()` as absent. Both inbound
paths were unified behind `install_incoming_connection` after the first commit was found to fix a
helper that production did not call.

### #2510 — two sequence spaces, only the wrong one durable

The encryption nonce sequence was persisted; the **signing** sequence — the one the peer's
`ReplayGuard` actually checks — was `AtomicU64::new(0)`. A restarted sender therefore replayed from
zero and accrued severity-1.0 violations for the crime of restarting. Fixed with a durable
block-reserved counter whose watermark is flushed before any sequence in the block is emitted.

### #2506 — local identity admitted as a remote peer

A node published its own `ConnectionCandidate`, gossip echoed it back over `network:candidates`, and
the node cached it as a dial target and dialed both of its own advertised endpoints. The resulting
self-connection carried real signed traffic into the node's own replay guard and misbehaviour
detector; after a restart its resumed signing sequence sat below its own persisted replay floor, so
it scored its own messages as replays and banned its own DID.

Invariant and ownership: [network-identity-self-exclusion.md](network-identity-self-exclusion.md).

### #2514 — receiver's restart safety gap (open)

`ReplayGuard::load_and_apply_safety_gap` sets `floor_seq = stored_max_seq + 1000` for every peer on
the **receiver's** restart. A sender that did not restart has no reason to jump forward by 1000, so
the restarted receiver rejects up to a thousand legitimate messages and escalates each to a ban.

The `floor_seq` mechanism itself is sound and must be preserved — the bloom filter is transient
after restart, so a floor at the true high-water is the only thing rejecting pre-restart replays.
It is the `+1000` that has no corresponding threat.

## Disproved hypotheses

Recorded so they are not re-litigated. Each was plausible and is now ruled out by evidence.

| hypothesis | verdict | what settled it |
|---|---|---|
| #2490 rate-limit tier work caused it | **disproved** | the banning node ran the old image with old limits; peer banning predates the change |
| `burst=2` caused it | **disproved / never testable at that stage** | the canary never ran on a healthy node; confounded by #2504 itself |
| it was simply lost transport connectivity | **disproved** | Hello/Ping/Pong healthy at 1 ms RTT throughout; failures are entirely above transport |
| ban state alone explained it | **disproved** | clearing a ban restored packet flow without restoring gossip participation |
| bans are in-memory and clear on restart | **disproved** | bans are persisted and reloaded; `Loaded misbehavior state: … 3 banned` |
| mDNS was the source of the self-connections | **disproved** | `icn_network_peers_discovered = 0` on all four nodes; no node ever logged `Discovered peer:`. The source is the `network:candidates` gossip echo |
| #2506 regressed after `045fa9e9` | **disproved** | self-dial / self-connection / own-DID replay / self-ban all zero on alpha and beta |
| #2510 regressed after `045fa9e9` | **disproved** | both nodes resumed on durable monotonic counters (`resumed_at=9001`, `10001`) |
| ~63 minutes is the #2514 recovery time | **NOT ESTABLISHED** | single confounded observation; recovery is direction- and state-dependent — see below |

## Live evidence

Four-node K3s federation. gamma and delta were never touched and remained on `828eb596` throughout,
serving as controls.

### #2506, before and after (alpha)

| | pre-fix (6.3 h) | post-fix |
|---|---|---|
| connection events from own pod IP | 5192 of 5451 (**95.2 %**) | **0** |
| self-dials | 111 | **0** |
| own connection candidates received | 111 | 3 |
| …of which stored as dial targets | **111** | **0** |
| self-replay / quarantine / ban | 167 / 167 / 167 | **0 / 0 / 0** |

Pre-fix, **all 111** candidates alpha received in 6.3 hours were its own — it never received a
remote peer's candidate at all.

gamma is the natural control that isolates the mechanism: **5014 self-connections, zero self-replays,
zero self-bans**, because it never restarted. Self-connection is always present and harmless alone;
the restart is what weaponises it.

### #2514, as observed

Beta restarted `00:17:19Z`; alpha restarted `00:18:46Z` — **87 seconds apart**.

| | |
|---|---|
| alpha's persisted high-water for beta | 10011 |
| applied gap | 1000 |
| alpha's effective floor for beta | **11011** |
| beta's live sequence just after alpha's restart | 10017 |
| beta's observed rate | 15.0 seq/min (276 samples over 29 min) |

Beta — the side that did *not* restart in that action — began accepting alpha's traffic at
**t+5m51s**, by the sender-climb path: alpha's own restart burned a fresh sequence block, which
cleared beta's floor quickly. Alpha, receiving from a peer that did not restart and therefore did
not jump forward, got no such help.

**Alpha recovered at t+60.0 min — and not by climbing the floor.** Beta was still at sequence 10914,
97 short of the 11011 floor, when `cleanup()` aged the window out at exactly
`max_peer_age_secs = 3600`:

```
01:18:49  LAST beta rejection — sequence 10914, floor 11011
01:18:51  Replay guard cleanup: 2 -> 0 peers (2 removed)     <- start + 3600s
01:19:39  alpha digests_received > 0
```

`last_update` is written only on the **accepted**-message path (`replay_guard.rs:375`) and at window
creation (`:649`); the floor check bails before reaching it. So a window rejecting everything never
refreshes its own liveness and ages out on a fixed timer. 566 rejections did not extend it.

**Corrected model:**

```
outage = min( time for sender to emit RESTART_SAFETY_GAP messages , max_peer_age_secs )
```

crossover at `1000/60 min` = **16.7 msg/min**. Beta sent at 15.0 msg/min — just below — so cleanup
won by ~6 minutes over a ~66 min sender-climb ETA. Both mechanisms were observed in the same run,
one per direction.

**Do not carry forward the earlier "~63 minutes" figure.** It was a rate extrapolation that happened
to land near the right number for the wrong reason. Two candidate mechanisms agreeing within 10 % are
not distinguishable by watching the clock — that near-tie is the methodological lesson here.

Note the security interaction, undecided: aging the window out discards the restart floor with it, so
after `max_peer_age_secs` of no accepted traffic the protection #468 added is gone for that peer.

## Open lead — candidate freshness (unresolved, do not promote)

Post-fix alpha received 143 of beta's connection candidates and stored **none**. By elimination on
`CandidateCache::store`, a DID not already cached can only be refused on the freshness check — but
the announce cadence (150 s) is deliberately half the TTL (300 s), so candidates should arrive
fresh. Cluster clock sync is failing (`Insufficient time server responses: got 0, need 3`), and
`is_fresh` depends on wall clock.

Two live hypotheses, insufficient evidence to choose. **Not filed as a defect.** Requires causal
evidence before promotion.

## Issue and PR index

| issue | PR | merge SHA | state |
|---|---|---|---|
| #2504 restart/rejoin (umbrella) | — | — | **open**, blocked on #2514 |
| #2505 stale connection replacement | #2505 | `adb9d07d` | closed |
| #2510 durable signing sequence | #2511 | `72dfed8e` | closed |
| #2506 self-peer exclusion | #2513 | `045fa9e9` | closed |
| #2514 receiver restart replay floor | — | — | **open** |
| #2507 undeclared `network:profiles` | — | — | open |
| #2508 bans persist, no TTL | — | — | open, compounds #2514 |
| #2509 candidate re-announcement | — | — | open |
| #2512 `banned_peers` gauge only increments | — | — | open — do not use as ban-state truth |

## Method notes worth keeping

- **`icn_misbehavior_banned_peers` is not current state** (#2512). Every ban figure in this record is
  log-derived.
- **A test can pass against unfixed code for the wrong reason.** #2506's dial test was written three
  times; the first two passed because quinn cannot connect to its own endpoint, then because a dial
  with no acceptor timed out. Check that the failure message is the assertion you wrote, and that
  the runtime is plausible.
- **Never read an exit code through a pipe** — `cmd | tail; echo $?` reports the tail's status.
