---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-07-28
---

# ICN State (living doc)

<!-- [sync edit] 2026-07-28 (merge-train addendum to the 2026-07-27 block below; branch
     `docs/truth-sync-20260727`; docs/ops-state-only). The 2026-07-27 block was written
     while its three sibling PRs were still open and says so explicitly. Those three have
     now MERGED, in this order, ahead of this truth-sync:

       #2458 → squash `9ca12148`  proposed deployment-profile record (ADR-0086)
       #2435 → squash `1af341cb`  evaluator identity correction
       #2463 → squash `a0b970ac`  two-node appliance proof v0.2 PLAN

     WHAT THAT DOES AND DOES NOT CHANGE:
     - ADR-0086 now EXISTS in `docs/adr/` and is linked from `docs/INDEX.md`. It remains
       `status: proposed`, `implementation_status: partially implemented`. **Merging the
       record did not adopt the decision.** Adoption is a separate human act, and the
       `status:` field is the owner of that fact — not this file, not the PR history.
     - The evaluator package identity on `main` is now `icn-portable-evaluator`, version
       0.0.4, owned by `deploy/appliance/evaluator/package-spec.env`. Published tags and
       asset filenames at or below 0.0.3 are RETAINED unchanged so existing `.sha256`
       verification keeps working; their payload provenance was always genuine and only
       the display identity was foreign. Nothing was retroactively rewritten.
     - The two-node appliance proof v0.2 PLAN now exists at
       `docs/demo/TWO_NODE_APPLIANCE_PROOF_V0.2_PLAN.md`. **No two-node proof has been
       executed.** Gate 4 remains BLOCKED pending a reviewed offline receipt-bundle
       exporter/verifier; Gate 3 (institutional enrollment) remains OPTIONAL, and omitting
       it restricts Node B to the title "technical witness"; federation is explicitly not
       exercised and may not be inferred from any weaker layer. The plan does not depend on
       `COMMUNITY_TOPIC`.
     - `docs/registry.toml` `total_entries` was recounted mechanically at each step of the
       train rather than incremented: the reconciled corpus is 360 explicit rows and the
       declared value is 360. (Side-picking a branch during conflict resolution would have
       silently dropped the ADR-0086 and two-node rows and reverted the counter.)

     UNCHANGED BY THE TRAIN: A1 still only MEASURES kernel/app separation and completes
     nothing; B0's claim is still bounded to zero direct dependency and zero direct source
     reference with the `icn-core → icn-gateway → icn-community` transitive path intact;
     B1 remains a design NO-GO with nothing implemented; B2 has not begun; only the
     AUTOMATIC PRIVATE DEPLOYMENT FROM PUBLIC CI was retired, and Kubernetes/K3s/Helm remain
     optional operator material; the appliance witness still proves only bounded persistence
     (identity, machine ID, config/genesis hashes, one completion receipt, restart, real
     reboot) and NOT rehearsal-workspace durability, signed distribution, independent
     restoration, production readiness, adoption, or federation; independent appliance
     restoration remains BLOCKED because `icnctl backup` omits `/etc/icn/icnd.env`; the
     dormant `community:updates` ownership problem (#2457) was NOT patched; and every human
     and institutional gate is untouched (#1703/#1746, nycn#41/#52, #2041; the NYCN pin on
     ICN `8c0fe926` did not move).
     Refs #2458 #2435 #2463 #2457. No close keywords. -->

<!-- [sync edit] 2026-07-27 (truth-root catch-up for the 2026-07-21 → 2026-07-26 window:
     architecture tranches A1 + B0, the public/private deployment boundary, the appliance
     payload repair, and the B1 design NO-GO; branch `docs/truth-sync-20260727`;
     docs/ops-state-only — no code, schema, route, or auth-decision change lands with it).
     Append-only/newest-first; the 2026-07-17 evaluator block below remains accurate as of
     when it was written. THIS BLOCK REPAIRS A TRUTH-SYNC LAPSE: this file's previous
     `Last Reviewed` was 2026-07-17 and `docs/PHASE_PROGRESS.md` was last updated
     2026-07-13, while `main` advanced through nine merges to
     `425f513f24d7f45130273770f346e8b5bdddbf9f`. The rendered `## Current status` section
     is re-snapshotted to 2026-07-27 and the 2026-07-17 snapshot is relabeled historical
     (content preserved verbatim, per the 2026-07-13b convention).

     WINDOW COVERED (all four SHAs verified as ancestors of `origin/main` `425f513f`):

     (1) A1 — MEANING-FIREWALL TRUTH RECONCILIATION, MERGED `4bdae326` (PR #2452,
     2026-07-22). One authoritative crate taxonomy (`scripts/firewall-taxonomy.toml`)
     now drives every firewall mechanism, replacing 17 hand-copied crate-list variants
     that disagreed with each other (`icn-ledger` was classified kernel in 5 mechanisms
     and forbidden-domain in 6). The required "Meaning Firewall Check" was fail-INCAPABLE
     since 2026-03-24 — its script unconditionally `exit 0`'d behind branch protection —
     and is now fail-closed, including on taxonomy load failure. All 48 workspace members
     are classified; 16 boundary-debt edges from `icn-core` are pinned as typed
     `[[exception]]` entries with tracking and machine-checkable `expiry = "edge-absent"`;
     stale-exception detection prevents a lingering transitive or dev-only path from
     masking a removed direct edge. WHAT A1 DOES **NOT** DO: it removes no dependency
     edge, changes no runtime behavior, and does NOT complete kernel/app separation — it
     measures that separation truthfully for the first time. A1 is the honest baseline the
     B-tranches subtract from, not a claim that the boundary is clean.

     (2) B0 — COMMUNITY EDGE INVERSION, MERGED `c1ea355e` (PR #2454, 2026-07-25). The
     first worked example of the B-tranche pattern. The daemon composition root (`icnd`)
     now constructs `CommunityActor`/`CommunityStore` and hands `icn-core` a
     `CommunityFactory` closure; `icn-core` transports an opaque handle it never inspects
     and `icn-gateway` downcasts it back. Community construction, LWW gossip-merge logic,
     and domain meaning left the kernel crate. EXACT EDGE CLAIM, re-verified live at
     `425f513f`: `icn-core/Cargo.toml` declares **zero direct `icn-community`
     dependency**, and `icn-core/src/` contains **zero direct `icn_community::` source
     references** (the only textual hits are `#[cfg(test)]` ratchet pins in
     `meaning_firewall.rs` recording the count as 0). THIS IS NOT GRAPH ISOLATION: a
     transitive path `icn-core -> icn-gateway -> icn-community` REMAINS, because
     `icn-core` still depends directly on `icn-gateway` and `icn-gateway` depends on
     `icn-community`. Do not describe B0 as removing community from the kernel's
     dependency graph; it removed the direct edge and the ownership.

     (3) PUBLIC/PRIVATE DEPLOYMENT BOUNDARY, MERGED `75d15750` (PR #2455, 2026-07-26).
     Public `main` no longer runs the automatic private-homelab build/deploy/cleanup
     workflow: a public merge invokes no private registry, SSH, K3s rollout, self-hosted
     runner, kubectl, or scheduled homelab cleanup. The replacement
     (`.github/workflows/oci-image-build.yml`) is a GitHub-hosted generic OCI build with
     `push: false` — build validation only, no publish and no deploy. The CI/deploy
     routing map no longer names the deleted workflow or asserts private-cluster
     liveness. SCOPE OF THE RETIREMENT: only the *automatic private deployment from
     public CI* was retired. Kubernetes, K3s, and the Helm chart REMAIN available as
     optional operator deployment material; they are not "removed from ICN". The legacy
     `deploy/k8s/` tree remains in Git as explicitly non-generic material pending a
     separate archive/genericize/move tranche.

     (4) APPLIANCE DEMO-PAYLOAD MODE REPAIR, MERGED `425f513f` (PR #2456, 2026-07-26) —
     current `origin/main`. Defect: a fresh assembled image booted and returned health
     200 but `icn-member-shell` failed 200/CHDIR because the guest payload inherited
     restrictive host-checkout modes. The builder now normalizes the explicitly
     non-secret DEV/DEMO payload to root ownership and `u=rwX,go=rX`, installs demo
     helpers root-owned 0755, and asserts those modes fail-closed at build time. No Rust
     runtime, API, protocol, storage-schema, or authority behavior changed. PR #2456 is
     MERGED, not awaiting auto-merge.

     APPLIANCE WITNESS AND ITS EXACT PROVENANCE. The assembled single-node appliance was
     witnessed on 2026-07-26 at integrated build head
     `67a6566e2335be108ca69bb5d60e0cfb761e63b5`. That commit is NOT an ancestor of `main`
     (it was the PR head that squash-merged as `425f513f`), but its tree is
     BYTE-IDENTICAL to current `main`: both resolve to tree
     `d3604c4c3896ff14417336a9a2d352c696d1fe32`. The witness therefore covers exactly the
     content of `425f513f`. Artifact: `icn-appliance-0.0.2-demo-67a6566e-20260726-amd64.qcow2`,
     image sha256 `1ef6085b…`, base sha256 `f8573792…`, manifest sha256 `80d4541d…`,
     typed manifest independently re-verified against image, base, `icnd`, and `icnctl`.
     Witnessed: clean boot and firstboot; `icnd` under systemd returning health; organizer
     and member rehearsal flows; least-privilege role negatives; wrong-digest confirm
     rejected; completion receipt created and re-fetched; outbound isolation; service
     restart; full VM reboot; `deploy/appliance/check.sh` 40 passed / 0 failed.
     DURABILITY BOUNDARY — STATE IT EXACTLY: across restart and full reboot the node
     identity, machine ID, configuration and genesis hashes were UNCHANGED while the
     kernel boot ID CHANGED (proving a real reboot), and the completion receipt remained
     re-fetchable. The rehearsal WORKSPACE view and aggregate rehearsal state are
     process-local and intentionally EPHEMERAL — they reset on daemon restart and are
     reconstructed by reseeding. Durable identity plus a durable completion receipt is
     what was earned; general workspace durability was NOT, and must not be reported as
     such.

     (5) B1 — LEDGER EDGE: DESIGN NO-GO, NOTHING IMPLEMENTED. B1 proposed removing
     `icn-core -> icn-ledger`. It reached the mandatory architecture gate and was
     REJECTED at design review on 2026-07-25; no implementation branch, PR, or commit
     exists, and none should be described as underway. The review found multiple
     incompatible ledger implementations, multiple composition roots, unclear
     authoritative ownership, insufficiently typed recovery operations, and inadequate
     authorization, custody, provenance, idempotency, and durable workflow receipts.
     BINDING CONSTRAINT: B1 must NOT be implemented by hiding ledger ownership behind
     `Any`, untyped opaque objects, callbacks, closures, meaning-erasing generic traits,
     ambient recovery authority, or incomplete receipt/provenance semantics — that is,
     the B0 opaque-handle pattern does NOT transfer to the ledger edge. B1 may resume
     only after there is one authoritative application composition root, one
     authoritative ledger implementation, typed recovery commands, explicit authority,
     and durable workflow evidence. The mandatory prerequisite tranche is
     composition-root consolidation. B2 HAS NOT BEGUN and is not product-critical.

     QUEUE STATE AS OF THIS BLOCK'S AUTHORSHIP — READ THE DATE. The two
     paragraphs that follow describe the sibling PR queue as it stood when this
     block was written, with `main` at `425f513f` and NONE of #2458 / #2435 /
     #2463 merged. They are a point-in-time record, not standing claims: this
     block may itself merge after one or more of those PRs. For the live value
     of anything they describe, the owners are `docs/adr/` for ADR-0086's
     existence and `status:` field, and
     `deploy/appliance/evaluator/package-spec.env` for the evaluator package
     identity. Where a later `[sync edit]` block above records one of these as
     landed, that newer block wins.

     DEPLOYMENT-PROFILE DECISION STATE (open, human-gated at authorship time).
     PR #2458 proposes ADR-0086 with `status: proposed` and
     `implementation_status: partially implemented`: (a) the Debian appliance VM as the
     canonical sovereign-node artifact, (b) Docker Compose as the disposable development
     network, (c) Kubernetes/K3s as OPTIONAL hosted operator infrastructure, (d) direct
     native Linux as an advanced installation form. IT IS PROPOSED, NOT ADOPTED — writing
     or merging the ADR does not adopt it, and only the appliance profile currently has a
     retained build-and-boot witness. Backup/restoration, artifact signing,
     reproducibility, two-node institutional operation, and generic Kubernetes
     reconciliation remain incomplete. A specific gap worth naming: `icnctl backup` omits
     `/etc/icn/icnd.env`, so an appliance restored from a backup alone cannot reopen its
     keystore — treat independent restoration as BLOCKED until an encrypted recovery
     bundle exists. Do not present Compose as sovereign-node proof.

     EVALUATOR NAME AND PROVENANCE STATE (open at authorship time). At `425f513f`
     the foreign package identity was still shipping:
     `deploy/appliance/evaluator/package-spec.env` carried
     `PKG_STEM="icn-common-sense-vertical-slice"`. PR #2435 renames the line to
     `icn-portable-evaluator` (0.0.4) and retitles three release pages with a correction
     banner. THE DURABLE FACTS, TRUE BEFORE AND AFTER #2435 MERGES: the name was
     never an ICN-ratified identity; the affected release PAYLOADS are genuine
     (manifest `git_commit` values are real commits of this repository); and
     tags/assets at or below 0.0.3 are deliberately RETAINED for checksum
     continuity and must NEVER be renamed or deleted. For the live value, read
     `package-spec.env` — do not infer it from this block.

     NEXT EXECUTABLE GAPS, SCOPED BUT NOT STARTED. (i) COMMUNITY GOSSIP TOPIC: the
     production runtime still does not create `community:updates` before subscribing —
     `icn-community/src/actor.rs` publishes to it, `icnd/src/community_wiring.rs`
     subscribes without a create, the gossip layer rejects publish to an undeclared topic
     under the reject policy, and publish failure is logged AFTER local mutation, so peers
     can diverge. This is a PRE-EXISTING defect that B0 neither introduced nor fixed. It
     is filed as #2457, which explicitly withholds authorization for an opportunistic
     patch until topic ownership, access rules, startup ordering, and failure semantics
     are decided. It does not block the single-node appliance witness. (ii) TWO-NODE
     APPLIANCE PROOF: NOT EXECUTED. No two-node run has occurred; connectivity between
     two development nodes would not constitute live federation, and the proof must
     separate transport connectivity, peer identity, enrollment/authority, state
     synchronization, receipts, and federation.

     OPEN HUMAN AND INSTITUTIONAL GATES, UNCHANGED BY THIS WINDOW: the real organizer
     presentation → pilot formalization → first operator rehearsal (#1703/#1746;
     partner-side nycn#41/#52) and the member-shell human assistive-technology pass
     (#2041). NYCN's partner repo remains pinned to ICN `8c0fe926` and this window does
     NOT move that pin. Also open: production trusted issuance (#2080), recurring
     assembled-image smoke (#2398), provider-boundary slice 3 (#2393), RPC credential
     lifetime (#2445), SDIS capability-vs-trust authority (#2447), unauthenticated anchor
     key rotation (#2448).

     NON-CLAIMS: no production readiness, no formal pilot, no organizer acceptance, no
     human-accessibility sign-off, no live federation, no two-node proof, no signed or
     immutable appliance (`non_production=true, signed=false`), no independent appliance
     restoration, no adopted deployment ADR, and no claim that kernel/app separation is
     complete. Receipts record institutional facts and grant zero authority.
     Refs #2452 #2454 #2455 #2456 #2457 #2458 #2435 #2398 #2041. No close keywords. -->

<!-- [sync edit] 2026-07-19 (evaluator package IDENTITY CORRECTION; branch
     `fix/evaluator-identity-correction`; docs + deploy-lane naming only — no
     code/schema/route/auth-decision change lands with it).
     WHAT THIS CORRECTS: the portable evaluator package has until now carried the
     name "Common Sense (bootable) vertical slice". That name arrived with an
     externally assembled distribution (the ad-hoc 0.0.2 packages) and was never
     an ICN-ratified identity: it was never proposed, defined, or ratified
     anywhere in this repository (the project owner attests it names a separate
     unrelated project; its external origin is otherwise undetermined; repo-wide
     search AS OF THE 2026-07-19 AUDIT, i.e. against `main` BEFORE this PR: the
     only occurrences were the package
     lane's own titles/stem, this file, and one incidental English use of the
     phrase in a 2026-05-15 handoff doc. That inventory is a point-in-time audit
     finding, not a standing invariant — this PR itself necessarily adds further
     occurrences, in this sync block and in the lane's own
     correction/supersession notes, which is expected: the retained historical
     names are what preserve checksum continuity for released assets at or below
     0.0.3). The 2026-07-17 lane work verified the
     PAYLOAD provenance rigorously (manifest git_commit = real main commits;
     0.0.3 runtime-witnessed on exact published bytes) but never audited the NAME
     provenance — the foreign identity was carried through into the repo-owned
     spec, the release titles, and this file.
     WHAT CHANGES: `deploy/appliance/evaluator/` PKG_STEM is now
     `icn-portable-evaluator` (next lane artifact = 0.0.4); template/package
     titles renamed; a naming-and-provenance section added to the lane README.
     WHAT DOES NOT CHANGE: published release tags and asset filenames for ≤0.0.3
     are retained (checksum continuity — renaming assets would break published
     `.sha256` verification); their release pages are retitled with a correction
     note. The 0.0.3 KVM runtime witness remains valid — it witnessed ICN bytes;
     only the display name was wrong. Older sync blocks below record the name as
     it stood then and are historical record, not current naming.
     NON-CLAIMS unchanged: unsigned, non-production; NOT a pilot, NOT organizer
     acceptance, NOT accessibility completion, NOT federation. Human gates
     unchanged: #2041, #1703/#1746, nycn#41/#52. Refs #2428. No close keywords. -->

<!-- [sync edit] 2026-07-17 (repository-owned portable evaluator lane MERGED + canonical
     release; branch `docs/evaluator-lane-state-sync`; docs/ops-state-only — no
     code/schema/route/auth-decision change lands with it). Append-only/newest-first;
     the 2026-07-15 LAN-witness block below remains accurate.
     WHAT MERGED: PR #2428 (`f34f9f29`) adds `deploy/appliance/evaluator/` — a
     declared-input, fail-closed, repository-owned lane that GENERATES the external
     "Common Sense bootable vertical slice" evaluator package (was previously assembled
     ad hoc). It takes a `build-image.sh` demo-profile image + typed manifest and emits
     the distributable ZIP: sanitizes the manifest paths to basenames (no build-host
     `/home` leak), deterministic archive, `SHA256SUMS`, static validator + 11-case
     defect suite (privacy/bind/checksum/manifest/injection), and a KVM-free CI lane.
     PORTABLE vs LAN: this packages the DEMO profile (QEMU user-net, localhost-only host
     forwards, disposable overlay, one-command setup for a reviewer's own machine) — a
     DIFFERENT threat model + audience from the LAN Rehearsal Node (operator-controlled,
     nginx single TLS origin, internal DNS/CA; `deploy/appliance/lan/`). The profiles are
     deliberately not collapsed.
     CANONICAL RELEASE: `common-sense-vertical-slice-0.0.3-amd64` — built from a
     current-main demo image (git_commit `f34f9f29`, image sha256 `097b2b2a…`, archive
     sha256 `5df053e5…`, `non_production=true, signed=false, demo_profile=true`),
     GENERATED THROUGH THE MERGED LANE. Static-validated AND runtime-witnessed on a real
     KVM boot of the exact published bytes: loopback-only forwards (LAN address refused),
     one-click role sessions (no credential in any URL), organizer review/assign/preview/
     digest-bound confirm (wrong digest → 409; correct → one action item + ADR-0026
     ladder), member completion + idempotent retry, source qcow2 unchanged, clean
     teardown, repeat run; the downloaded release bytes match the witnessed bytes. The
     four ad-hoc 0.0.2 pre-releases are reconciled: two recipient-name-contaminated demo
     releases DELETED, the two 0.0.2 vertical-slice releases marked SUPERSEDED.
     WHAT STATIC CI PROVES vs RUNTIME: the CI lane is fast static validation only
     (layout/syntax/ShellCheck/checksum/manifest/privacy/bind); it is NOT assembled-image
     runtime proof — that is the separate KVM witness recorded here. NON-CLAIMS: an
     unsigned, non-production evaluator artifact; NOT production, NOT a pilot, NOT
     organizer acceptance, NOT accessibility-completion, NOT live federation, NOT general
     cooperative storage/compute hosting. Human gates unchanged and still owed: #2041,
     #1703/#1746, nycn#41/#52. #2422 CLOSED (fulfilled by merged #2426). Follow-up #2429
     (bounded blob publish/fetch). Refs #2428 #2426 #2429 #2421 #2398. No close keywords. -->

<!-- [sync edit] 2026-07-15 (LAN rehearsal MERGED-MAIN witness + hardening;
     branch `docs/state-mainmerge-witness`; docs/ops-state-only).
     Records that the LAN Rehearsal Node work MERGED to main and was
     re-witnessed from merged-main provenance (superseding the 2026-07-14
     branch-image note below).

     MERGE TIMELINE (explicit, to avoid provenance ambiguity):
       - main was c4364f93.
       - PR #2424 (LAN single-origin appliance + landing + operator ctl +
         testing docs) merged first -> main = e74a8915.
       - The appliance image and the workstation witness were built/run against
         main-AT-e74a8915 (i.e. after #2424, the appliance-bearing PR). The
         image's typed manifest records git_commit e74a8915.
       - PR #2425 (public website truth pass) merged AFTER -> main = 0030e730.
         #2425 touches only website/ and does not change the appliance image,
         so the deployment provenance is e74a8915 while current main is 0030e730.

     SECURITY FIX CAUGHT PRE-MERGE (#2424 review): the LAN profile had left the
     dev gateway (ICN_ENABLE_ADMIN_ENDPOINTS=true) bound 0.0.0.0:8080 — directly
     LAN-reachable and bypassing the TLS single-origin proxy. The merged profile
     rebinds gateway + member-shell + session endpoint all to loopback so nginx
     is the ONLY LAN HTTP surface (verified: :8080/:8090 refuse LAN connections,
     nginx serves).

     DEPLOY + WITNESS: the e74a8915 image (non_production=true, signed=false) was
     deployed to the dedicated hypervisor VM via the documented reversible disk
     swap (old branch image retained as rollback). The full organizer->member
     loop was driven from a real Windows workstation browser against the
     merged-main deployment: one-click role-scoped sessions (no terminal, no
     paste, no credential in any URL), review->edit->assign->digest-bound
     preview/confirm, WRONG-digest confirm -> 409 (fail-closed, server-verified),
     exactly one action item per confirm, fresh least-privilege member session,
     completion, idempotent retry; in-VM `icn-demo-verify --rehearsal` PASS incl.
     the negative capability matrix and the value-withheld (no-DID/no-credential)
     evidence check; service restart + full VM reboot recovered unattended with
     node IDENTITY durable (stable DID, firstboot not re-run) and sled stores
     intact — the rehearsal WORKSPACE VIEW is intentionally ephemeral (rebuilt
     per process; a clean reseed follows).

     WEBSITE: #2425's changes are LIVE on intercooperative.network (Deploy
     Website workflow success on 0030e730; verified: no 'Live on K3s' /
     'Institution-in-a-box' / 'every step leaves a durable receipt'; dev-demo
     framing + Rehearsal-Node wedge + capability-horizon link present; no private
     LAN address/hostname leaked).

     NO CLAIM CHANGE: still a LAN development rehearsal on operator-controlled
     infrastructure — not production, not a pilot, not federation, not organizer
     acceptance, and NOT the #2041 human AT pass. One blocked action remains
     (needs firewall admin, not agent-executable): an internal DNS domain-override
     forwarding the internal zone to its authoritative resolver so the canonical
     hostname resolves from the workstation; the IP origin works meanwhile.
     Human gates unchanged: #2041, #1703/#1746, nycn#41/#52. #2422 (evidence
     packet validator privacy leak scan) implemented in PR #2426 (open, awaiting
     Matt). Refs #2398 #2415 #2421. No close keywords. -->
<!-- [sync edit] 2026-07-14 (LAN rehearsal deployment; branch `feat/lan-rehearsal-deployment`). WHAT THIS EDIT DOES: adds one bullet to the current-status snapshot recording the first LAN workstation witness of the Rehearsal Node. Facts: a demo+LAN-profile appliance image was built from THIS BRANCH at commit 916629d7 (the typed manifest is the provenance record; branch = main c4364f93 + the LAN single-origin feature itself) and deployed as a dedicated hypervisor VM on the operator's LAN behind one TLS origin (internal CA). The complete organizer→member loop was then driven TWICE from a real Windows workstation browser (Chrome) with no terminal, no credential paste, and no credential in any URL: one-click role-scoped sessions → review/edit/assign → digest-bound preview/confirm (post-preview edit invalidates the stale preview, fail-closed) → exactly one action item per confirm → fresh least-privilege member session → completion → durable receipt; in-VM steward verify (`icn-demo-verify --rehearsal`) PASSED against the browser-created state including the negative capability matrix; service restart and full VM reboot recovered unattended with durable receipts intact; run 2 executed cleanly after reboot + browser-initiated fresh reset. NO claim change: this is a LAN development rehearsal on operator-controlled infrastructure — not production, not a pilot, not federation, not organizer acceptance, and NOT the #2041 human AT pass (automated a11y floor only; NVDA/zoom/forced-colors remain human-owed). Human gates unchanged: #2041, #1703/#1746, nycn#41/#52. New follow-up: #2422 (evidence packet validator leak-scan gap found by tamper testing). Refs #2398 #2415 #2421. No close keywords. -->
<!-- [sync edit] 2026-07-13b (orientation truth refresh; branch `docs/orientation-truth-refresh`; docs/ops-state-only — no code/schema/route/auth-decision change lands with it). Append-only/newest-first; the 2026-07-13 witness block immediately below remains accurate and is the current truth root. WHAT THIS EDIT DOES: (1) replaces this file's stale rendered `## Current status (2026-05-15 snapshot)` section with a fresh 2026-07-13 snapshot and relabels the old section as historical (content preserved verbatim); (2) closes the frozen `ops/state/sprint/current.json` (Sprint 26, stale since 2026-03; the full frozen record — spine, shipped, task list — is archived in-tree at `ops/state/sprint/sprint-26-closed.json` per the existing sprint-N-closed convention, and `current.json` carries no active work items; `what-matters-now.sh` now reports it closed); (3) reduces `docs/TODO.md` to a thin dated pointer at the tracker + canonical state docs; (4) rewrites `docs/reference/project-index/current-truth-map.md` and `show-readiness-map.md` from the pre-Rehearsal-Node wedge (drive-ingest ladder, image `91a63eec`, "K3s running since 2025-12-03" as-written guidance) to the witnessed rehearsal reality, removing deployment-age claims per `docs/status.toml` NEEDS-OPS-RE-CONFIRMATION; (5) annotates `docs/PHASE_HISTORY.md`'s January-2026 "Current Status" block as historical. NO new capability, NO route/enforcement change, NO phase-status change, NO production/pilot/organizer-ready/live-federation/human-accessibility claim. Human gates unchanged and still owed: #2041, #1703/#1746, nycn#41/#52. Refs #2398 #2393 #2080. No close keywords. -->

<!-- [sync edit] 2026-07-13 (rehearsal organizer→member tranche + first assembled-image witness of the organizer loop; branch `docs/truth-sync-rehearsal-witness`; docs-only truth-sync — no code/schema/route/auth-decision change lands with it). Append-only/newest-first; the #2400 block immediately below remains accurate as of when it was written. **This block catches the truth-root up from #2402/#2403: five merges (#2404–#2408), the NYCN-side adoption, and the 2026-07-13 fresh assembled-image KVM witness are recorded here.**
     WHAT LANDED SINCE THE #2400 BLOCK (grouped; evidence = merged squashes on `main`):
       - **#2404** (`323880bf`) + **#2405** (`48cef862`) — public provider-address scrub slices 1+2 (355→0 occurrences across 60 non-runtime files; SDIS test fixture + `.github/agents/**`) with the `scan_public_docs_boundary` guard extended accordingly. **#2393 stays open** (operational categories deferred, tracked there).
       - **#2406** (`b307f22c`) — rehearsal organizer review→confirm surface (runtime): build-mode-gated (`ICN_GOVERNANCE_BUILD_MODE=rehearsal`, exact-match; routes absent → 404 in every other mode), domain-scoped rehearsal workspace, three narrow scopes (`governance:rehearsal:setup` / `pending-publish:review` / `pending-publish:confirm`), BLAKE3 `preview_digest` binding a confirm to the exact previewed plan (wrong/stale digest → 409, fail-closed), confirm executes the REAL ADR-0026 ladder (gate→activation→plan→create_action_item→applied) creating ONE real action item, value-withheld bindings (labels in, DIDs never echoed), membership fail-closed (422).
       - **#2407** (`7a5e9e18`) — member-shell `?surface=organizer` (live-only) guided review→confirm browser flow; axe-clean automated a11y with the 12-category gate filed; human-AT categories → **#2041** (open).
       - **#2408** (`8c0fe926`) — appliance wiring: demo-session daemon test→rehearsal (additive; **#2075 untouched**), `icn-demo-seed --session organizer|member` (idempotent workspace ensure, least-privilege role JWTs, NO pre-seeded item — the organizer's confirm creates it), closed role intent on the loopback demo-session endpoint, `icn-demo-verify --rehearsal` steward verifier, and the no-paste launcher (`?mode=live&surface=organizer&demo=launcher`; member transition = a FRESH least-privilege session, never a token upgrade).
       - Downstream (private NYCN package, recorded only at boundary level here): the package now carries the adopted `8c0fe926` witness state and an independently steward-operable facilitator path. Private-repo PR numbers, private dry-run IDs, package mechanics, and human-review details stay in the private repo; this public record makes no organizer-acceptance, pilot, production, or human-signoff claim.
     FRESH ASSEMBLED-IMAGE KVM WITNESS (2026-07-13; evidence retained under `~/artifacts/icn/appliance-witness-8c0fe926-20260712/`, NOT committed — repo-safe summary only, also posted to #2386/#1746/#2398): a demo-profile qcow2 built from clean, unedited `main` `8c0fe926` (image sha256 `f2aa7d24d062…`, staged-base sha verified, in-build fail-closed manifest OK) completed, from a clean restrict=on boot: firstboot → health → the legacy member loop → the least-privilege 403 negative → **the full rehearsal organizer→member loop ON the assembled image** (role sessions via `icn-demo-seed --session`; member→review 403; organizer→bindings 403; approve → digest-bound preview → WRONG-digest 409 → confirm 201 with ladder hashes → organizer→completion 403 → member card → completion receipt binding check → in-VM `icn-demo-verify --rehearsal` exit 0) → outbound canary held. A browser-observed pass (Playwright, session-local tooling) additionally drove the REAL `?mode=live&surface=organizer&demo=launcher` path (one-click session, no credential paste, confirm in the browser, receipt ladder + evidence panel rendered, no DID/JWT visible on the organizer surface), validated the evidence export (`urn:icn:contract:rehearsal-workflow-evidence:v1`, `dids_exported=false`, `credentials_exported=false`), and a restart check (completion receipt survives reboot — durable; the rehearsal workspace is an in-memory scaffold BY DESIGN and is restored by re-seeding). Evidence redaction-audited clean (0 JWT shapes, 0 DIDs, loopback-only). The committed reproducible walkthrough driver is PR **#2409**, merged as `7437e412ec37136f7f3dbba684aa50597a988f90`; it extends `smoke-local.sh --demo` with the rehearsal loop + negatives. The witness remains pinned to the clean-main `8c0fe926` image and does not imply later commits were included in that image.
     ALSO IN THIS PR (doc corrections found by a 2026-07-13 truth audit): workspace-member count corrected 44→**48** (38 crates + 7 apps + 3 bins, verified against `icn/Cargo.toml`) in `docs/status.toml` and `docs/ARCHITECTURE.md`; `docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md` updated for the #2406–#2408 reality (organizer launcher, `--session` seeds, `--rehearsal`/`--pending-publish` verify modes); `docs/demo/README.md` headline reframed to the two-role rehearsal loop; `docs/demo/rehearsal-node-appliance-loop.md` verification status updated from "KVM witness NOT performed" to the 2026-07-13 witness; historical-snapshot labels added where undated present-tense claims remained (`docs/status/MOBILE_APP_DEMO.md`, `docs/SYNC_LOG.md`).
     `Refs #2404 #2405 #2406 #2407 #2408 #2409 #2398 #2399 #2393 #2386 #2041 #1703 #1746 #2080 #2075`. No close keywords. **No production / pilot / organizer-ready / live-federation / current-deployment-health / human-accessibility / Phase-2-completion claim.** The witness is automated evidence at one commit and closes NO human gate; an internal or partner dry run is not an organizer rehearsal; receipts record process facts and grant zero authority; the appliance stays `non_production=true, signed=false`; trusted-local issuance is appliance-local operator bootstrap, NOT production trusted issuance (**#2080 open**; **#2075 unchanged**). Human gates unchanged and still owed: **#2041** (assistive technology), **#1703/#1746** + nycn#41/#52 (organizer presentation / decision / first operator rehearsal). Phase 2 status ⏳ unchanged. -->

<!-- [sync edit] 2026-07-11 (#2400 completion-only action-item scope — the follow-up named in the #2397 block below is now landed; branch `docs/truth-sync-2400-completion-scope`; docs-only truth-sync — no code/schema/route/auth-decision change lands with it). Append-only/newest-first; the #2397 block immediately below remains accurate as of when it was written. **This block records the completion-only action-item capability now on `main` (PR #2402, squash `9060b3ed`; #2400 CLOSED).**
     WHAT LANDED: a new canonical scope `governance:action-item:complete` (`icn_rpc::auth::scopes::GOVERNANCE_ACTION_ITEM_COMPLETE`), a finer sub-capability decomposed from `governance:meeting:write` — added to the gateway `ALLOWED_SCOPES` allowlist (locked by the drift test `test_allowed_scopes_contains_action_item_complete`) and re-exported via `icn_rpc::auth::scopes`. `update_action_item_status` (`apps/governance/src/http/handlers.rs`) parses the requested transition first (already server-deserialized, so trustworthy) then gates by value: the `completed` transition accepts `[governance:action-item:complete, governance:meeting:write, governance:write]` (narrowest-first; the matched scope is recorded as `capability_scope_presented` evidence), every other transition keeps `[governance:meeting:write, governance:write]` unchanged; an unknown transition fails closed (400). Ownership turns on the caller's ACTUAL authority — the completion-only scope requires the caller be the item's **assignee**, while the broad `meeting:write`/`write` scopes retain creator-or-assignee (`owner_ok = is_assignee || (is_creator && holds_broad_scope)`); a same-status PUT is an idempotent no-op (no metadata mutation, no second receipt). The appliance demo seed's **browser** JWT moved `governance:read,governance:meeting:write` → **`governance:read,governance:action-item:complete`** (the internal setup JWT keeps `meeting:write` and is never emitted); the static auth guard asserts the new set. A 19-test `icn-governance-actor` integration suite + an `icn-http-kit` scope-boundary test + the gateway drift test lock the behavior. Three Codex P2s and one Copilot finding were absorbed during review (assignee requirement; broad-scope-aware ownership + truthful evidence; same-status idempotence).
     APPLIANCE WITNESS (real KVM boot; evidence retained under `~/artifacts/icn/appliance-witness-2400-completion-20260711/`, NOT committed — repo-safe summary only): a fresh demo-profile image built from `9060b3ed` (`image_sha256 a4befbcd…`, in-build manifest fail-closed re-verified) under an enforced `restrict=on` no-outbound posture. The completion-only browser JWT completed the member loop (standing → Action Card → discharge `PUT {"status":"completed"}` → completion receipt with record_hash binding → card cleared) AND was denied — live, 403 each — creating an action item, creating a meeting, driving a non-completion (`in_progress`) transition, and reading an entity; a repeat completion returned an idempotent 200 no-op; the outbound-isolation canary held; teardown clean. The evidence set was redaction-audited clean (no tokens, secrets, DIDs, or non-loopback IPs).
     `Refs #2400 #2397 #2386 #2075 #2080 #2398 #2399 #2393 #2041 #1703 #1746`. No close keywords. **No production / pilot / organizer-ready / live-federation / current-deployment-health / human-accessibility / Phase-2-completion claim.** A completion receipt records a member act and grants zero authority; the appliance is `non_production=true, signed=false`; trusted-local issuance is appliance-local, not production enrollment (**#2080 open**). **#2075 unchanged** (pre-closed 2026-06-17). **#2398/#2399/#2393 remain open and separate.** Human gates unchanged and still owed: **#2041** (assistive-technology), **#1703/#1746** (organizer presentation / first operator rehearsal). Phase 2 status ⏳ unchanged. -->

<!-- [sync edit] 2026-07-11 (Rehearsal Node v0.1 tranche completion + post-#2396 auth/secret hardening; branch `docs/truth-sync-rehearsal-hardening`; docs-only truth-sync — no code/schema/route/auth-decision change lands with it). Append-only/newest-first; the #1728 pending-publish read-model block immediately below remains accurate as of when it was written. **This block catches the truth-root up: the newest block below stopped at `GET /v1/gov/me/pending-publish-summary` (#2389); seven subsequent merges plus two live-KVM appliance witnesses are recorded here.**
     WHAT LANDED SINCE THE #2389 READ-MODEL BLOCK (grouped; evidence = merged squashes on `main`):
       - **#2391** member-shell read-only pending-publish preview panel binding the #2389 endpoint (WCAG2.2AA, default view-only; merged `a4418077`).
       - **#2392** public infrastructure-map scrub + a hard IPv4/IPv6 boundary guard in `scripts/check-truth-spine.py` (concrete provider host addresses removed from public files → role refs; merged `7a26e5da`). The broader scrub is tracked at **#2393** (open, separate — not combined with this work).
       - **#2394** rehearsal-node pending-publish evidence export + steward verification: a committed `urn:icn:contract:rehearsal-evidence-export:v1` packet, the operator generator `scripts/rehearsal_pending_publish_evidence.py`, `icn-demo-verify --pending-publish` steward mode, and a CI drift guard (merged `389418c6`). Load-bearing honest mapping: `approved_for_publish→deferred` (a preview is not authorization), receipts `not-attempted`, `mutation.executed=false`, no DID in the packet. #1729 stays CLOSED.
       - **#2396** trusted-local appliance demo-seed issuance: `icnctl auth token --local-mint` / `institution bootstrap apply --local-mint` sign the demo session JWT **in-process** with the node's OWN first-boot, instance-local `ICN_GATEWAY_JWT_SECRET` via `AuthManager::issue_token` — the gateway issuing a JWT for itself to its local operator. This repairs a demo-seed regression: the self-asserted `/auth/verify` mint is (correctly) fail-closed by **#2075** on the demo's required routable `0.0.0.0` bind. No new endpoint, no baked credential; **#2075 unchanged**; `/auth/verify` still fail-closed (merged `4fa4ea76`).
       - **#2397** post-#2396 auth/secret hardening (this file's ref, merged `1120b7516355`): least-privilege browser credential — the demo seed now mints an internal **setup** JWT plus a narrow **browser** JWT (`governance:read,governance:meeting:write`, the only token handed to the member shell; drops `coop:*`, broad `governance:write`, `entity:write`); a **minimal `runuser` child environment** (explicit allowlist strip; secret via the environment, never argv) in seed/verify/firstboot; seed/smoke diagnostics never reproduce the session JWT; **one canonical HMAC key derivation** shared by the daemon and `icnctl --local-mint` (`icn_gateway::auth::signing_key_bytes` / `AuthManager::from_secret_string`); an `icn-http-kit` scope-boundary unit test; and the `icn-demo-verify` item-id path corrected to `--local-mint`. Follow-ups: **#2398** (recurring assembled-image smoke — the "component CI green while the assembled image was broken" process finding), **#2399** (secret-free local-issuance audit record), **#2400** (completion-only action-item scope decomposition). **#2080 remains open** — `--local-mint` is appliance-local operator bootstrap, NOT the production trusted-issuance architecture.
     APPLIANCE WITNESSES (real KVM boots; evidence retained under `~/artifacts/icn/`, NOT committed — repo-safe summary only): the complete canonical Rehearsal Node path was witnessed twice on freshly built demo-profile images under an enforced `restrict=on` no-outbound posture — post-#2396 (image git `4fa4ea76`) and post-#2397 hardening (image git `1120b751`). Each witness: firstboot → trusted-local seed → standing → Action Card → discharge (PUT completed) → completion receipt (record_hash binding) → card cleared; authenticated pending-publish `GET` → 200, `origin: committed_fixture`; member-shell live render via a real Playwright-**observed** (not intercepted) gateway request; member-shell demo-mode fixture render; steward `icn-demo-verify --pending-publish` validates `:v1` + honesty invariants; tampered packet **REJECTED** (fail-closed); in-guest outbound-isolation canary **held**. The post-#2397 witness additionally proved the browser-JWT least-privilege boundary on a live VM: it completes the member loop yet is denied `GET /v1/entities/{id}` (**403**, no `entity:read`). Each evidence set was redaction-audited clean (no tokens, secrets, DIDs, or non-loopback IPs).
     `Refs #2386 #2389 #2391 #2392 #2394 #2396 #2397 #2398 #2399 #2400 #2075 #2080 #2393 #1728 #1726 #1729 #2041 #1703 #1746`. No close keywords. **No production / pilot / organizer-ready / live-federation / current-deployment-health / human-accessibility / Phase-2-completion claim.** A preview row is evidence for review, not authorization; the appliance is `non_production=true, signed=false`; trusted-local issuance is appliance-local, not production enrollment. Human gates unchanged and still owed: **#2041** (assistive-technology), **#1703/#1746** (organizer presentation / first operator rehearsal). Phase 2 status ⏳ unchanged. -->

<!-- [sync edit] 2026-07-10 (#1728 preview/review read-model — first runtime projection of the pending-publish-summary contract served over a gateway; branch `demo/preview-review-read-model`; this PR bundles code + this docs truth-sync). Append-only/newest-first; the truth-root catch-up block immediately below remains accurate as of when it was written. **This PR adds `GET /v1/gov/me/pending-publish-summary`, the runtime projection of `urn:icn:contract:pending-publish-summary:v1` (#1998) — the generic read-model *for this contract* served over the gateway, which the #1728 maintainer note (2026-06-14) named as still-owed before closure. (Other member read-models — `/v1/gov/me/standing`, `/v1/gov/me/action-cards` — already serve over the gateway; what was missing was a gateway projection of the pending-publish/preview-review shape, not gateway read-models in general.)** New generic types in `icn/apps/governance/src/http/models.rs` (`PendingPublishSummaryResponse` + `PendingPublishRow` + closed, fail-closed enums: `PendingPublishOrigin`/`RowKind`/`RowStatus`/`RiskLevel`/`Provenance`/`ReceiptCategory`), a `/me/*`-family handler `get_my_pending_publish_summary` (`governance:read`, self-scoped by token subject, no membership gate — same posture as `/me/standing` and `/me/action-cards`), one route registration, and OpenAPI/TS-type registration alongside the two sibling member reads. Vocabulary mirrors the landed `pending-publish-summary:v1` contract and the `ActionCard` axes (`authority_basis`/`risk_level`/`accessibility_hint`); NO institution-specific nouns, NO DIDs in rows, NO bridge/custody nouns. **Read model only** — no mutation, no publish, no export, no action-card creation, no authority granted; `receipt_expected` labels an evidence EXPECTATION, not authority. **Build-mode gated:** in the `production` build mode the endpoint returns NO rows (`origin: live_runtime`) — fictional rehearsal rows never appear on a production surface; non-production (bootstrap/test) modes return deterministic, fictional, `origin: committed_fixture`-labeled rows so the organizer rehearsal shell has real gateway-served rows. Reuses the existing `ICN_GOVERNANCE_BUILD_MODE` gate (no new dev flag, no auth bypass; Production `configure` still fail-closes on missing standing/mandate deps per #2075). 5 integration tests (`me_pending_publish_summary.rs`: fixture rows labeled, deterministic, regulatory-safe + no package nouns, fail-closed unknown status, self-scoped) + 1 inline unit test (production-empty vs non-production gate). Witnessed proof adjacent (not in this PR): PR #2388's restrict=on demo-smoke canary was witnessed on a real KVM boot 2026-07-10 (guest could not reach a host listener while the loopback demo loop worked) — evidence in ~/artifacts, not committed. `Refs #1728` (recommend-close after human review; the runtime-serving deliverable is met, but #1726 shell consumption + #1703/#1746 remain), `Refs #1726`, `Refs #1746`. No close keywords. No production / pilot / organizer-ready / live-federation / Phase-2-completion claim; a preview row is evidence for review, not authorization; fixture rows are not live participant state. Phase 2 status ⏳ unchanged. -->

<!-- [sync edit] 2026-07-10 (truth-root catch-up for the post-2026-07-02 window; branch `docs/truth-sync-post-20260702`; docs-only truth-sync — no code, schema, route, or auth-decision change lands with it, except a warn-only advisory added to `scripts/check-state-lag.py` in the same PR). This file is append-only and newest-first; the 2026-07-02 DecisionRecordedReceipt block immediately below remains accurate as of when it was written. **This block repairs a truth-sync convention lapse: 62 commits merged to `main` between the 2026-07-02 sync (`2247041d`) and `5b7075e6` (2026-07-09) without any canonical truth-root block.** The existing lag guard could not see this class of drift — `check-state-lag.py` only detects stale "not merged / not on main" claims *inside* the newest block, not *missing* blocks, and its CI trigger is path-filtered to changes touching `docs/STATE.md` itself; this PR adds a warn-only commits-since-last-STATE-edit advisory to the script and documents the limitation (a scheduled/mechanical guard is follow-up work, not claimed here).
     WHAT LANDED SINCE THE PREVIOUS SYNC (grouped; evidence = merged squashes on `main`, PR numbers where present in the subject, SHAs otherwise):
     (1) **Process receipt-request hardening + membership timestamp determinism** — unknown-field rejection on the four routed process-receipt endpoints (`f4dbfbc4`); local wall-clock dropped from the membership `state_change_hash` (`e8057cc6`, cross-node hash determinism) plus durable `Member` timestamp semantics and `effective_at` carried through durable records (`41352410`, #2288).
     (2) **Organizer-steward evidence surface (fixture-only)** — design + runtime dogfood slice for the member-shell `?mode=demo&set=process-evidence` set (`7a42db89`, `b28fbeb2`): all process receipt classes render from committed fictional fixtures in demo mode. The member-shell human-AT packet was EXTENDED for process evidence (`0f4fa895`) — packet extension only; the #2041 human assistive-technology pass remains NOT performed.
     (3) **ADR-0026 Layer-2 receipt ladder completed as receipt classes** — six additional `ProcessTransitionReceipt` classes (5th–10th) landed, each as design-contract → decision-rung → runtime emit → member-shell render: `ActivationCrossedReceipt` (`a170d8e7`/`61d726e4`/`7d9ca7eb`/`2652a8d6`), `MutationPlanRecordedReceipt` (`0a84dc86`/`1a2ca6e0`/#2303/#2305), `MutationAppliedReceipt` (#2307/#2309/#2310/#2312), `EvidencePacketProducedReceipt` (#2314/#2316/#2318/`cf3e7d47`), `EvidencePacketExportPreparedReceipt` (`87b425cf`/`38fdd3a0`/#2326/#2328), and `EvidencePacketMadeAvailableReceipt` (#2329 private disclosure/access boundary, #2331, #2333, #2335, plus the made-available federation/access boundary map #2337). Boundaries that hold for all six: reachable through the in-process `GovernanceManager` seam ONLY (gateway HTTP write paths still exist only for the first four classes — gate-result/session-open/deliberation-entry/decision-recorded); NO served-OpenAPI/SDK publication for any process receipt class; receipt bodies are never stored (32-byte hash fingerprints only); member-shell renders are committed-fixture demo mode, not live state; `made_available` witnesses a decision to disclose — NOT retrieval, NOT access, NOT delivery. #1792 CLOSED completed by the 10th-class rung; runtime disclosure ENFORCEMENT (`ScopedVault`/`DisclosurePolicy`) remains design-only — rehearsal privacy is still by exclusion, not enforcement.
     (4) **`governance:write` decomposition + broad-fallback observability (observe-only)** — decomposition design (#2338), process scope gate (#2340), fallback-observability controls map (#2342), broad-fallback scope-admission classifier + matcher tightening (#2344/#2345), bounded observation sink (#2347), closed label vocabulary (#2349), emission helper (#2351), and charter-family wiring of two constitutional handlers (#2353). Observe-only throughout: handler call sites use the no-op emitter, NO route-outcome or enforcement change, the broad `governance:write` fallback is retained pending observation; wiring a real sink is future work.
     (5) **Control-plane / org-truth records + agent hardening** — preflight root-guidance hardening (#2354); org repo registry + truth-spine validator (#2356); reusable claim lint (#2358, consumed by the partner repo); cross-repo PR stack protocol manifest (#2359); control-plane adoption records for the nycn caller, pr7, and the infra dashboard (#2360/#2361/#2362); member-standing shipped-subset clarification (#2363 — corrects the stale MEMBER_STANDING.md header that inverted shipped-vs-future for `/v1/gov/me/standing`).
     (6) **Governed-bridge conformance lane (fake-fixture only)** — NYCN airlock requirements for governed tools (#2364); spec docs for governed-bridge receipt vocabulary, ToolManifest bridge modes, binding custody mapping, external reference observations, and the steward review surface (#2370/#2371/#2372/#2373/#2374); committed conformance fixtures + the conformance validator wired into the standard docs checks (#2375/#2378); NYCN intake and relationship fake-handoff fixtures (#2379/#2382); binding-required receipts enforced in fixtures (#2380); generic governed object receipt support (#2381); and v0 record-state custody `condition` predicates (#2383 — validator shape-checks the predicate; nothing evaluates it or routes custody). ALL of this lane is offline shape-conformance over committed FAKE fixtures: a green "governed bridge conformance" check proves fixture shape, not bridge behavior. The RFC-0017 object family these specs extend (`ToolManifest`/`ToolBinding`/`ServiceIdentity`/capability registry) still has NO Rust implementation, and RFC-0017 remains `active`, not accepted. The umbrella rehearsal issue #2377 is CLOSED completed.
     ISSUE EFFECTS: `Refs #1748` + `Refs #2141` (no close keywords) — **both remain OPEN**: receipt-class coverage is not an operable process spine (classes 5–10 unrouted, accessibility/privacy gates open, no action-card triggers from process state, no OpenAPI/SDK publication). #1792, #2330, #2332, #2334 CLOSED completed (10th-class rung). #2377 CLOSED completed. Bridge spec issues #2365–#2369/#2376 map 1:1 to merged PRs #2370–#2378 but remain OPEN pending human verify-and-close. `Refs #1703` — the organizer-presentation → pilot-formalization → first-operator-rehearsal gate is unchanged and human-gated, as are partner-repo gates NYCN #41/#52. Partner-repo housekeeping recorded for cross-reference: NYCN #97 CLOSED completed 2026-07-10 (decision: defer requiring the `airlock-fixtures` check until NYCN adopts a broader branch-protection/ruleset posture; evidence PR nycn#98 merged; no settings changed). #2080/#2081/#2274 untouched.
     NON-CLAIMS: no runtime bridge; no connector; no live sync; no real Drive / Sheets / Gmail / SimpleTix import; no private operational data handling (disclosure enforcement is design-only; privacy remains by exclusion); NYCN operations are NOT ICN-native today; no production readiness; no pilot readiness (NYCN is the intended first partner, not a formalized pilot); no live federation; no Phase-2 completion; receipts record institutional facts and grant zero authority; fixture-backed demo surfaces are not live participant state; a green conformance check is shape-validation of fake fixtures, nothing more. Phase 2 status ⏳ unchanged. -->

<!-- [sync edit] 2026-07-02 (#1748/#2141 process spine — DecisionRecordedReceipt runtime slice; branch `feat/decision-recorded-receipt`; this PR bundles code + this docs truth-sync). This file is append-only and newest-first; the DeliberationEntryRecordedReceipt block immediately below remains accurate as of when it was written — its "this PR" is **PR #2279, MERGED as squash `d0e87aec`**. Since then: the decision-recorded design/audit contract landed as **PR #2280 MERGED (`2b2622ca`**, `docs/design/decision-recorded-receipt.md`**)** and the Q4 boundary decision as **PR #2281 MERGED (`6a1f00ae`**, `docs/design/decision-recorded-q4-decision.md` — Option A: generic recorded-decision fact, parallel to and explicitly non-convergent with the proposal/vote `GovernanceDecisionReceipt` lineage; no lineage-reference field, no deciding-body handle, no typed outcome in `:v1`; `resolution` stays deferred with discriminant 10 reserved; hash-layout consequence NONE**)**. **This PR implements the receipt-only `DecisionRecordedReceipt` slice — the fourth `ProcessTransitionReceipt` class (ADR-0026 Layer 2) after #2144's gate result, #2276's session anchor, and #2279's deliberation entry.** New proof type under tag `icn:gov:decision_recorded:v1` (must NEVER converge with `icn:gov:decision:v1/v2/v3` — triple-tag separation pinned by test; the proposal/vote lineage, its typed store, effect dispatch, and action cards are UNTOUCHED). Fields are exactly the #2280 §4 set: `domain_id`, `session_id`, caller-opaque `decision_id`, **`recorded_by` (recorder evidence, deliberately not `decider`/`author`/`approver`)**, `recorded_at`, 32-byte `body_hash`, `record_hash` — no outcome, tally, vote, proposal, mandate, kind, or deciding-body field, enforced by payload-audit tests. Recording REQUIRES an already-opened session (fail-closed `decision_recorded_session_not_opened` → 404; NO silent session creation; deliberation entries are NOT a precondition — that is charter/gate territory); atomic `(domain_id, session_id, decision_id)` uniqueness reuses the landed `put_opaque_if_absent` primitive under an injective netstring composite key1 sibling (no gateway change); retry with identical stable identity (`recorded_by` + `body_hash` — `recorded_at`/`record_hash` are NOT identity) returns the ORIGINAL receipt (never restamped); any mismatch = fail-closed `decision_recorded_conflict` 409. Multiple decisions per session are permitted at the substrate layer (`decision_id` is the unit of uniqueness; how many is charter policy). **The decision body is never stored** — the receipt carries a caller-supplied 32-byte `body_hash` fingerprint only. Route `POST /gov/domains/{id}/process-sessions/{sid}/decisions/{did}/record` mirrors the sibling authz posture (`governance:write` + domain membership; 403 non-member; unopened session → 404; no new capability). Session-open, gate-result, and deliberation-entry behavior UNCHANGED (all sibling suites re-run green). NO stored `DecisionRecord`, NO `HumanDecisionSet` runtime or read-model, NO bindingness/validity/quorum claim, NO activation-crossed/mutation-plan/mutation-applied/evidence-packet objects, NO action-card triggers, NO accessibility/privacy gate completion, NO charter/CCL gating, NO served-OpenAPI publication (family-wide decision stays a future rung). 27 new tests across proof/manager/route layers (7 proof incl. golden vector `029e5bac…` + triple-tag separation; 14 runtime-slice incl. an 8-thread race, composite-key anti-aliasing, and persisted-payload field audit; 6 HTTP route). `Refs #1748` + `Refs #2141` (no close keywords); **both remain OPEN** (#1748's privacy/visibility and accessibility-gate runs stay open; remaining receipt classes: activation-crossed, mutation-plan, mutation-applied, evidence-packet). #2081/#2080/#2274 untouched. A receipt records an institutional fact and grants zero authority. No production / pilot / organizer / member / live-federation / Phase-2-completion readiness. Phase 2 status ⏳ unchanged; #1703 human gate unchanged. -->

<!-- [sync edit] 2026-07-02 (#1748/#2141 process spine — DeliberationEntryRecordedReceipt runtime slice; branch `feat/deliberation-entry-recorded-receipt`; this PR bundles code + this docs truth-sync). This file is append-only and newest-first; the ProcessSessionOpenedReceipt block immediately below remains accurate as of when it was written — its "this PR" is **PR #2276, MERGED as squash `5e1b7c97`**. Since then: the deliberation-entry design/audit contract landed as **PR #2277 MERGED (`e02c8519`**, `docs/design/deliberation-entry-recorded-receipt.md`**)** and the Q3 taxonomy decision as **PR #2278 MERGED (`907b3d21`**, `docs/design/deliberation-entry-kind-taxonomy.md`**)**. **This PR implements the receipt-only `DeliberationEntryRecordedReceipt` slice — the third `ProcessTransitionReceipt` class (ADR-0026 Layer 2) after #2144's gate result and #2276's session anchor.** New proof type under tag `icn:gov:deliberation_entry_recorded:v1` with the #2278 closed `DeliberationEntryKind` taxonomy (ten kinds, explicit u8 discriminants 0–9 hashed as one byte; serde snake_case wire; `resolution` deferred as Q4-ambiguous; retired kinds must stay decodable forever — emission-side retirement only). Recording REQUIRES an already-opened session (fail-closed `deliberation_entry_session_not_opened`; NO silent session creation); atomic `(domain_id, session_id, entry_id)` uniqueness reuses the landed `put_opaque_if_absent` primitive under an injective netstring composite key1 (no gateway change); retry with identical stable identity (`author` + `body_hash` + `entry_kind`) returns the ORIGINAL receipt (never restamped); any mismatch = fail-closed `deliberation_entry_conflict` 409. **The deliberation body is never stored** — the receipt carries a caller-supplied 32-byte `body_hash` fingerprint only. Route `POST /gov/domains/{id}/process-sessions/{sid}/deliberation-entries/{eid}/record` mirrors the session-open authz posture (`governance:write` + domain membership; 403 non-member; unopened session → 404). Session-open and gate-result behavior UNCHANGED (their suites re-run green; gate results still neither require nor create sessions). NO stored `DeliberationThread`, NO discussion/chat/comments/moderation system, NO decision records, NO mutation plans, NO evidence packets, NO action-card triggers, NO accessibility/privacy gate completion, NO charter/CCL gating. 33 new tests across proof/manager/route layers incl. an 8-thread race and composite-key anti-aliasing. `Refs #1748` + `Refs #2141` (no close keywords); **both remain OPEN** (#1748's privacy/visibility and accessibility-gate runs stay open). #2081/#2080/#2274 untouched. A receipt records an institutional fact and grants zero authority. No production / pilot / organizer / member / live-federation / Phase-2-completion readiness. Phase 2 status ⏳ unchanged; #1703 human gate unchanged. -->

<!-- [sync edit] 2026-07-02 (#1748/#2141 process spine — ProcessSessionOpenedReceipt runtime slice; branch `feat/process-session-opened-receipt`; this PR bundles code + this docs truth-sync). This file is append-only and newest-first; the 12b block immediately below remains accurate as of when it was written — that lane closed (#2273 merged `3e387b1f`, #2082 CLOSED completed with follow-up #2274 filed). **PR #2275 MERGED as squash `31bb52b0`** (`docs/design/process-session-receipt-anchor.md`, the implementation contract for this PR). **This PR implements the receipt-only `ProcessSessionOpenedReceipt` slice — the second `ProcessTransitionReceipt` class (ADR-0026 Layer 2) after #2144's `ProcessGateResultReceipt` — plus the contract's domain-scoped gate-result read. No enforcement default or route-outcome change to any existing surface.**

     WHAT LANDS HERE: (1) `icn-governance` proof type `ProcessSessionOpenedReceipt{session_id,domain_id,opened_by,opened_at,record_hash}` under new domain tag `icn:gov:process_session_opened:v1` (length-prefixed blake3, family-separated from the gate-result tag). (2) `GovernanceReceiptBackend`: new atomic opaque primitive `put_opaque_if_absent` (fail-closed default) + typed `put/get_process_session_opened` defaults routing through it (class `process_session_opened`, key1=domain_id, key2=session_id) + `list_process_gate_results_for_session_in_domain` (domain-scoped read filtering on the hash-bound `domain_id`; legacy `(session_id, gate_kind)` reads byte-identical). (3) Gateway `ReceiptStore::put_opaque_if_absent`: point-keyed unique marker checked-and-set INSIDE the sled transaction (sled txns are point-read-only, so the append-chain index cannot express uniqueness) — concurrent opens serialize to exactly one persisted opening. (4) `GovernanceManager::record_process_session_opened`: at most one opening per `(domain_id, session_id)`; same-opener retry returns the ORIGINAL receipt (never restamped); different-opener = fail-closed `process_session_open_conflict`; REQUIRES a wired receipt store (uniqueness cannot be faked in memory). (5) Route `POST /gov/domains/{domain_id}/process-sessions/{session_id}/open` mirroring the gate-result authz posture (`governance:write` + domain membership; 403 non-member; 409 different-opener). Gate-result recording is UNCHANGED: it neither requires nor silently creates opened sessions.

     EVIDENCE (this branch, pre-merge): 9 new icn-governance proof unit tests (determinism, per-field + family domain separation, length-prefix aliasing, serde, vocabulary); 4 gateway store tests incl. an 8-thread concurrent race (exactly one winner); 11 manager runtime-slice tests (retry idempotency, conflict, fail-closed store/backend, no-silent-session-creation, domain-scoped no-mixing, 8-thread manager-level race); 5 HTTP route tests (200 open, idempotent retry, 409 conflict, 403 non-member with nothing persisted, 400 whitespace). Route inventory regenerated (`route_inventory.py --check` OK). Full commands + results in the PR.

     ISSUE EFFECTS: this PR uses `Refs #1748` and `Refs #2141` (no close keywords). **#1748 remains OPEN** (its privacy/visibility evidence-export and accessibility-gate runs remain open; this slice only provides the anchor they need). **#2141 remains OPEN.** #2081/#2080/#2274 untouched.

     This sync explicitly does NOT claim: a stored `ProcessSession` object; a session lifecycle; a full process runtime; deliberation/decision/mutation-plan/evidence-packet objects or receipts; charter/CCL sequencing or existence gating; accessibility-gate or privacy/redaction completion; #1748 closure; #2141 closure; mapping- or receipt-as-authority (a receipt records an institutional fact and grants zero authority; `opened_by` is actor evidence, not permission); production readiness; pilot readiness; member readiness; organizer readiness; live federation; Phase 2 completion. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-07-02 (#2082 gap 12b — vertical-slice proof surface redirected to icn-coop; branch `test/membership-coop-core-redirect-to-icn-coop`; test/deps/docs only). This file is append-only and newest-first; the 12a block immediately below remains accurate as of when it was written — its "this PR" is **PR #2271, MERGED as squash `480f294a`** (CreateTreasury trusted-binding consultation in `icn-coop`), and the 12b decision contract landed as **PR #2272, MERGED as squash `4c726593`** (`docs/design/membership-coop-core-map-parity.md`). **This PR changes NO production behavior: it implements #2272's Option B by migrating `icn-core`'s `vertical_slice_integration.rs` from the `apps/membership` `coop_core` fixture to the real `icn_coop::CoopActor`, so the flagship integration test now exercises the current #2082 semantics** (#2104 activation binding path, #2266 activation-time treasury `entity_id` population — asserted in-test via the no-map projection outcome with byte-exact `coop_id` — and the #2271-era post-activation `CreateTreasury` rejection guard).

     WHAT LANDS HERE: (1) `vertical_slice_integration.rs` imports/spawns `icn_coop::CoopActor` (identical constructor/handle surface) and gains #2082 assertions (treasury `coop_id` byte-exact; `entity_id` populated at activation; post-activation `CreateTreasury` rejected). (2) `icn-core`'s `[dev-dependencies]` drops `icn-membership-app` (no remaining consumer) and adds `icn-coop` — test-only, following the existing `icn-governance`/`icn-trust` dev-dep precedent; the meaning firewall (no domain crates in kernel `src/`) is untouched. (3) `apps/membership` `coop_core` remains in tree (its own `src/coop.rs`/lib re-exports consume it) but its actor is now explicitly **FROZEN** by module doc: pre-#2104 semantics, NOT a #2082 proof surface, not to be extended with mapping/identity logic.

     ISSUE EFFECTS (verified against the tracker): this PR uses `Refs #2082` (no close keyword). **With 12a merged (#2271) and 12b resolved by this redirect, #2082's structural scope is complete — but #2082 remains OPEN unless Matt explicitly authorizes closure.** **#2081** (treasury entity-auth enforcement cutover) remains OPEN and **blocked**; **#2080** (trusted positive token issuance) remains OPEN and separate.

     This sync explicitly does NOT claim: any production code behavior change; any map or provenance write beyond what the real activation path already does under its existing merged semantics; any projection fallback added to CreateTreasury; trust of `UnknownLegacy`; mapping-as-authority (a binding grants no membership, role, capability, mandate, standing, or permission); any gateway/default enforcement or route-outcome change; positive `entity_id`/`entity_type` token issuance; #2081 progress; #2080 progress; #2082 closure; production readiness; pilot readiness; member readiness; organizer readiness; live federation; Phase 2 completion. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-07-01 (#2082 rung 12a — CreateTreasury trusted-binding consultation; branch `feat/create-treasury-trusted-entity-binding`; this PR bundles code + this docs truth-sync). This file is append-only and newest-first; the #2267-era block immediately below remains accurate as of when it was written — note its "#2267 (this PR, OPEN)" phrasing was pre-merge context, not a semantic claim: **PR #2267 MERGED as squash `d37d7d43`**, and the follow-on design rung **PR #2270 MERGED as squash `39e95987`** (`docs/design/create-treasury-entity-id-semantics.md`, the implementation contract for this PR). **This PR changes exactly one behavior: `CreateTreasury` in `icn-coop` now consults the canonical `coop_id ↔ EntityId` map READ-ONLY and, only when a trusted binding already exists, populates the new treasury's `entity_id` from it. No enforcement default or route outcome changes.**

     WHAT LANDS HERE: at `CreateTreasury` time (icn-coop actor only; see sequencing note), after the plain `register_treasury` files the row under the byte-exact original `coop_id`, a read-only consultation (`trusted_binding_for_creation`, delegating to the #2267 `report_unknown_legacy` classifier so "trusted" cannot drift from the operator report) checks for an existing binding that is trusted (`is_trusted_for_resolution`), reverse-consistent byte-for-byte, and targets a well-formed cooperative `EntityId`. Only then is `entity_id` populated, through the new `TreasuryManager::populate_entity_id_at_creation` thin wrapper over the same fail-closed populate seam activation and the ADR-0084 apply already use (byte-for-byte `coop_id` re-check, never overwrites, entity-uniqueness `EntityIdConflict` fail-closed). Everything else — not bound, `UnknownLegacy`/missing provenance, reverse mismatch, malformed/non-cooperative target, storage error, no map wired — leaves `entity_id: None` exactly as before. Per the merged #2270 contract: `register_treasury_with_entity` is NOT used (it would re-derive `coop_id` from `entity_id.identifier()` and mis-file surrogate-bound rows); there is NO projection fallback (deliberately stricter than activation — `CreateTreasury` is not an institutional act and owns no provenance); the map is never written; no provenance is recorded; the post-activation "already has a treasury" rejection is unchanged.

     SEQUENCING (12b): the `apps/membership` `coop_core` duplicate actor is NOT touched — it has no `icn-entity` dependency and no map integration at all (not even activation binding), so consultation there is meaningless until its own map-parity rung (activation bind + populate + creation consult together). That remains the next #2082 rung.

     EVIDENCE (this branch, pre-merge): 13 new icn-coop tests (each trusted provenance populates incl. a divergent-surrogate case proving legacy `coop_id` preservation and that no row appears under the surrogate slug; UnknownLegacy/reverse-mismatch/non-cooperative/no-map/conflict all left `None`; post-activation rejection unchanged; read-only test doubles panic on any `bind_*`, proving no map write); `cargo fmt --all --check` / targeted clippy `-D warnings` / `cargo test -p icn-coop -p icn-ledger` recorded in the PR.

     ISSUE EFFECTS (verified against the tracker): this PR uses `Refs #2082` (no close keyword). **#2082 remains OPEN** (remaining rung: apps/membership coop_core map parity, then close-or-followup with explicit authorization). **#2081** (treasury entity-auth enforcement cutover) remains OPEN and **blocked**; **#2080** (trusted positive token issuance) remains OPEN and separate.

     This sync explicitly does NOT claim: any map write or provenance write; any projection fallback; trust of `UnknownLegacy` (untrusted unless a future evidence-bearing workflow repairs it); mapping-as-authority (a binding grants no membership, role, capability, mandate, standing, or permission); any gateway/default enforcement or route-outcome change; positive `entity_id`/`entity_type` token issuance; #2081 progress; #2080 progress; full #2082 completion; production readiness; pilot readiness; member readiness; organizer readiness; live federation; Phase 2 completion. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-07-01 (#2082 treasury entity_id backfill/apply/activation lane + UnknownLegacy report — branch `feat/entity-unknown-legacy-report`; PR #2267 carries this catch-up sync for the merged #2258/#2262/#2265/#2266 rungs plus its own read-only report). This file is append-only and newest-first; the #2254 A2e block immediately below remains accurate as of when it was written — this block is its successor, capturing the treasury-consumer #2082 rungs the canonical docs had not yet recorded. **Every rung here is read-only or off-the-request-path except the #2265/#2266 treasury `entity_id` writes, which populate an identity target only under fail-closed, trusted, non-ambiguous provenance and change NO enforcement default or route outcome.**

     LANE SUMMARY (the honest current state): after the A2c provenance substrate (#2190) and the A2e enforce-mode seam (#2252/#2254, off by default), the treasury-consumer side of the canonical `coop_id ↔ EntityId` map (#2082) advanced through four merged rungs and one open read-only report. None grants authority; a mapping binds an identity target only.

       - #2258 (`df847075`) feat(ledger): **read-only treasury entity_id backfill PLAN** — `TreasuryManager::plan_entity_id_backfill` classifies which legacy treasuries (`entity_id: None`) could be populated from a trusted, non-ambiguous binding; fail-closed on `UnknownLegacy` / unprovenanced / ambiguous / non-cooperative. Writes nothing.
       - #2262 (`8e62df35`) feat(icnctl): **operator report** — `icnctl treasury entity-backfill-report [--json]` surfaces the planner against persisted treasuries; read-only.
       - #2265 (`528d54df`) feat(treasury): **controlled apply (ADR-0084)** — `icnctl treasury entity-backfill-apply [--apply --confirm-apply]` mutates ONLY the `entity_id` of planner `WouldPopulate` rows, from the trusted binding, re-verifying forward + provenance + reverse (byte-for-byte) before each write; dry-run default; never writes the map; entity-id uniqueness enforced.
       - #2266 (`98810b5a` = current `origin/main` tip) feat(treasury): **activation-time population** — the `icn-coop` activation path registers the treasury `entity_id: None`, commits activation, records the `Activation`-provenance map binding LAST, then populates the treasury `entity_id` from that recorded binding (pure projection if no map). Non-projectable `coop_id`s stay `None` (reject-not-normalize). Closes the activation-time gap only.
       - #2267 (this PR, OPEN) feat(entity): **read-only UnknownLegacy repair-candidate report** — `icn_entity::report_unknown_legacy` + `icnctl coop entity-unknown-legacy-report [--json]` classify each bound `coop_id` as trusted / not-bound / untrusted-provenance (`UnknownLegacy`) / reverse-mismatch / malformed-target / storage-error, and name the evidence class a repair would require. NO trust upgrade, NO bind, NO write, NO map / treasury / gateway change. `UnknownLegacy` stays untrusted until an explicit, evidence-bearing repair.

     REMAINING #2082 WORK (not in this lane): the `CreateTreasury` message path still creates `entity_id: None` treasuries and owns no `Activation` provenance (trust semantics must be drafted before it may populate); the `apps/membership` `coop_core` activation path is a parallel actor that does not yet bind the canonical map. Both are distinct future rungs.

     EVIDENCE (PR #2267, pre-merge, off `origin/main` `98810b5a`): `cargo test -p icn-entity` **263/0** (incl. 15 new UnknownLegacy unit tests) + `cargo test -p icnctl` green (incl. 3 new integration tests); `cargo clippy -p icn-entity -p icnctl --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean; `git diff --check` clean. The #2258/#2262/#2265/#2266 evidence is recorded in each merged PR.

     ISSUE EFFECTS (verified against the tracker): every rung used `Refs #2082` (no close keyword) and `closingIssuesReferences` is empty for each. **#2082 remains OPEN** (the canonical `coop_id ↔ EntityId` store/mapping tracker). **#2081** (treasury entity-auth enforcement cutover) remains OPEN and **blocked** (clean observation metrics + membership/entity backfill + mapping evidence). **#2080** (trusted positive token issuance) remains OPEN and separate.

     This sync explicitly does NOT claim: full #2082 completion; trust of `UnknownLegacy`; trust of gossip-originated / unprovenanced mappings; upgrade of pre-existing legacy rows; mapping-as-authority (`CoopEntityMap` / resolver data is target evidence / a trust qualifier, never authority); any default route-outcome or enforcement change; treasury route cutover to `require_entity_access`; positive `entity_id` / `entity_type` token-claim issuance; #2081 readiness; #2080 progress; production readiness; pilot readiness; organizer readiness; member readiness; live federation; Phase 2 completion. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-29 (A2e treasury route ENFORCEMENT consumption — branch `claude/a2e-treasury-route-enforcement-consumption`; PR #2254; bundles code + this docs truth-sync). This file is append-only and newest-first; the #2252 config-parser block immediately below remains accurate as of when it was written — this block is its successor in the entity-aware-authorization (A2e) lane. **This PR consumes the treasury entity-auth gate decision at the route layer ONLY under the explicit, off-by-default `ICN_TREASURY_ENTITY_AUTH_MODE=enforce-trusted-resolver`; it changes NO default route outcome.**

     LANE SUMMARY (the honest current state): #2252 (merged to main as squash `ec1992e1`) added the operator-gated, OFF-BY-DEFAULT config seam (`ICN_TREASURY_ENTITY_AUTH_MODE` + `parse_treasury_entity_auth_mode` + `active_treasury_entity_auth_mode`) that *selects* the treasury entity-auth mode, but NO route consumed the gate decision. This slice wires that consumption: under the explicit `EnforceTrustedResolver` mode the treasury route helpers evaluate the gate INLINE and deny on `WouldDeny`. The default (`ObserveOnly`) is unchanged — the observation stays a detached, fire-and-forget task whose result is discarded, so default route allow/deny is **byte-identical** to the flat `require_coop_access` guard (which remains the enforced baseline, not retired). Entity membership remains the authority signal; resolver mappings / `CoopEntityMap` remain target evidence / trust qualifiers, never authority.

       - (this PR) feat(authz): **consume treasury entity-auth gate in enforce mode** — `icn/crates/icn-gateway/src/authority.rs` + `icn/crates/icn-gateway/src/api/treasury.rs`. `authority.rs`: adds `TreasuryAccessOutcome { decision, observation }`, the pure `treasury_gate_enforcement_denial(TreasuryGateDecision) -> Option<GatewayError>` (`ProceedUnchanged` → `None`; `WouldDeny(reason)` → `Some(GatewayError::Forbidden)` carrying the stable `reason.label()`), and splits the old `observe_treasury_entity_access` body into `evaluate_treasury_entity_access(...) -> TreasuryAccessOutcome` with `observe_treasury_entity_access` kept as a thin wrapper returning just the observation (identical signature/behavior; existing observe tests/callers unchanged). `treasury.rs`: `observe_treasury` / `observe_treasury_by_did` now return `Result<()>` and branch on `active_treasury_entity_auth_mode()` — `ObserveOnly` keeps the detached spawn and returns `Ok(())` (never denies); `EnforceTrustedResolver` evaluates inline and returns `Err(GatewayError::Forbidden)` on `WouldDeny`, `Ok(())` on `ProceedUnchanged`. All 10 treasury handler call sites changed `.await;` → `.await?;`, each verified to sit in GUARD POSITION (before any mutation), so an enforce-mode denial returns 403 before any treasury write. 4 new pure unit tests; existing gate/observe suites unchanged.

       ENFORCEMENT SEMANTICS (decision-driven, no ad hoc route auth): under `EnforceTrustedResolver` the route denies EXACTLY when `decide_treasury_gate` returns `WouldDeny` — i.e. a trusted `Agree`/`ResolverOnly` basis with a verified membership target and an affirmative member proceeds; every other case (non-member, indeterminate/error membership, `Disagree` collision, and any untrusted basis — `LegacyOnly`/`NeitherResolved`, which is how `UnknownLegacy` / unprovenanced / gossip-originated rows read back) fails closed. A resolver/agreed mapping is only a trust qualifier + target verifier; it never grants access on its own. Inline-await under enforcement is deliberate: an unverified entity gate must not let a slow/erroring `EntityManager` pass — a stall/error surfaces as `WouldDeny(ObservationError)` and denies.

     EVIDENCE (from this branch, pre-merge): `cargo fmt --all --check` clean; `cargo test -p icn-gateway --lib --all-features` **681/0** (incl. 4 new `enforcement_denial_*` / `enforce_mode_*` / `observe_mode_never_*` cases); `cargo clippy -p icn-gateway --all-targets --all-features -- -D warnings` clean; `ops/scripts/drift-check.sh` PASS.

     ISSUE EFFECTS (verified against the tracker): the PR uses `Refs #2082` (no close keyword) and its `closingIssuesReferences` is empty. **#2082 remains OPEN** (REOPENED).

     This sync explicitly does NOT claim: that enforcement is on by default (it is off — `ObserveOnly`); any default route-outcome change; completion of entity-aware authorization; retirement of the flat `require_coop_access` baseline by default; treasury route cutover to `require_entity_access`; positive `entity_id` / `entity_type` token-claim issuance; mapping-as-authority (`CoopEntityMap` / resolver data is target evidence / a trust qualifier, never authority); trust of `UnknownLegacy`; trust of gossip-originated / unprovenanced mappings; upgrade of pre-existing legacy rows; full #2082 completion; #2113 completion; any CodeQL setup or alert-state change; production readiness; pilot readiness; organizer readiness; member readiness; live federation; Phase 2 completion. Human AT remains OPEN under #2041. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-29 (A2e treasury entity-auth MODE config parser — branch `claude/a2e-treasury-entity-auth-mode-seam`; PR #2252; bundles code + this docs truth-sync). This file is append-only and newest-first; the A2d block below (2026-06-26, #2199) remains accurate as of when it was written and is not contradicted by this block — it is the immediate predecessor in the entity-aware-authorization lane. **This PR is a real config-seam code change in `icn-gateway` only; it changes NO default route outcome.**

     LANE SUMMARY (the honest current state): this slice adds the operator-gated, OFF-BY-DEFAULT configuration seam that selects the treasury entity-auth *measurement* mode. It replaces the hardcoded `ACTIVE_TREASURY_ENTITY_AUTH_MODE` constant with a tiny env-config seam, and is **parser + measurement-wiring only — NO treasury route consumes the gate decision**. The default remains `ObserveOnly`; default route allow/deny outcomes remain **byte-identical** to the flat `require_coop_access` guard (which remains the enforced baseline, not retired); `EnforceTrustedResolver` remains **decision-only** (now operator-selectable for the observe-only measurement, but wired to no route denial); `observe_treasury_entity_access`'s return value is still discarded by callers (the route runs it as a detached, fire-and-forget task). Entity membership remains the authority signal; resolver mappings / `CoopEntityMap` remain target evidence / trust qualifiers, never authority.

       - (this PR) feat(authz): **treasury entity-auth config parser** — `icn/crates/icn-gateway/src/authority.rs` only. Adds `const TREASURY_ENTITY_AUTH_MODE_ENV = "ICN_TREASURY_ENTITY_AUTH_MODE"`, the pure `parse_treasury_entity_auth_mode(Option<&str>) -> TreasuryEntityAuthMode`, and the thin env reader `active_treasury_entity_auth_mode()`. Removes the hardcoded `ACTIVE_TREASURY_ENTITY_AUTH_MODE` constant and routes its single use site (the `active_gate` computation inside `observe_treasury_entity_access`) through the new seam. The `would_enforce` measurement still evaluates `EnforceTrustedResolver` explicitly, unchanged. 6 new `parse_mode_*` unit tests; the existing `decide_treasury_gate` / `gate_*` suite stays green.

     PARSER SEMANTICS (fail-safe by construction — enforcement can NEVER be enabled by accident): `None` / empty / whitespace-only → `ObserveOnly`; `observe` / `observe-only` / `observe_only` (case-insensitive, trimmed) → `ObserveOnly`; `enforce-trusted-resolver` / `enforce_trusted_resolver` → `EnforceTrustedResolver`; **any other value** (typo, bare `enforce`, separator-less `enforcetrustedresolver`, `true`/`1`/`on`) → `ObserveOnly` with a diagnostic `warn!`. There is **no fuzzy match toward enforcement**.

     ROUTE-SAFETY TRUTH: because no treasury route consumes the gate decision in this slice, setting the env var to enforcement changes only which mode the observe-only measurement records as `active` — it CANNOT alter any route outcome on its own. Wiring the decision to a route would require inverting the deliberate detached-task observe design (await inline → return the decision → deny in the handler) across `observe_treasury` / `observe_treasury_by_did` and the treasury handlers; that route-consumption cutover is a deliberate, separate follow-up, NOT done here.

     EVIDENCE (from this branch, pre-merge): `cargo fmt --all --check` clean; `cargo test -p icn-gateway --lib --all-features` green (incl. 6 new parser cases; `gate_`/`parse_mode_` 26/0); `cargo clippy -p icn-gateway --all-targets --all-features -- -D warnings` clean; `ops/scripts/drift-check.sh` PASS.

     ISSUE EFFECTS (verified against the tracker): the PR uses `Refs #2082` (no close keyword) and its `closingIssuesReferences` is empty. **#2082 remains OPEN** (REOPENED).

     This sync explicitly does NOT claim: enforcement is enabled (ships observe-only; `EnforceTrustedResolver` is decision-only, consumed by no route); any default route-outcome change; treasury route cutover to `require_entity_access` (the flat `require_coop_access` guard remains the enforced baseline, not retired); completion of entity-aware authorization; positive `entity_id` / `entity_type` token-claim issuance; mapping-as-authority (`CoopEntityMap` / resolver data is target evidence / a trust qualifier, never authority); trust of `UnknownLegacy`; trust of gossip-originated / unprovenanced mappings; upgrade of pre-existing legacy rows; full #2082 completion; A2e enforcement-cutover completion; #2113 completion; any CodeQL setup or alert-state change; production readiness; pilot readiness; organizer readiness; member readiness; live federation; Phase 2 completion. Human AT remains OPEN under #2041. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-26 (A2d target verification — #2199 merged to main as squash `08acb0e5`, parent `4d5e027f` (#2198); docs-only, branch `docs/sync-a2d-target-verification`):
     Records PR #2199 — the A2d **target-verification** follow-up that the #2197/#2198 block below named as the next step ("carry the agreed/resolved `EntityId` target into membership evaluation"). This file is append-only and newest-first; the block below remains accurate as of when it was written (after #2197/#2198), but this block is the current view and **explicitly supersedes** its statement that "no resolution currently yields `ProceedUnchanged` under `EnforceTrustedResolver`". **This is a docs-only truth-sync — no code, schema, route, or auth-decision change lands with it.**

     LANE SUMMARY (the honest current state): #2199 carries treasury *target evidence* through the still-observe-only A2d path so the pure `decide_treasury_gate` can verify whether the entity-membership observation was evaluated against the same `EntityId` target trusted resolution established. The active mode remains `ObserveOnly`; default route allow/deny outcomes remain **byte-identical** to the flat `require_coop_access` guard (which remains the enforced baseline, not retired); `EnforceTrustedResolver` remains **decision-only** (wired to no route or production config); and `observe_treasury_entity_access`'s return value is still discarded by callers. Entity membership remains the authority signal; resolver mappings / `CoopEntityMap` remain *target evidence / trust qualifiers*, never authority.

       - #2199 (merged to main as squash `08acb0e5`) feat(authz): **verify treasury gate resolver target** — `icn-gateway/src/authority.rs` plus one test assertion in `icn-gateway/src/coop_entity_resolver.rs` for the changed return type (+822/−217). Adds internal target-evidence types `MembershipTargetSource`, `TreasuryMembershipObservation`, `CoopResolutionEvidence`, `TreasuryGateEvidence`. `compute_treasury_observation` now returns a target-aware membership observation (the `EntityAccessObservation`, the `membership_target: Option<EntityId>`, and the membership target source); the membership verdict logic was factored into `evaluate_membership` with **authorization logic unchanged**. `observe_coop_entity_resolution` now returns `CoopResolutionEvidence` (classification + legacy target when present + resolver target when present). `decide_treasury_gate` now takes `&TreasuryGateEvidence`. `record_treasury_gate` logs the membership target source and whether the target verified; `ResolverConflict` still logs at `warn!`, ordinary hypothetical denies still at `debug!`.

     UPDATED DECISION-ONLY SEMANTICS (supersedes the prior "no resolution currently yields `ProceedUnchanged`" statement below — still **simulation/decision-only**, NOT enforced by any route): under `EnforceTrustedResolver`, `Agree` + checked trusted target + `membership_target == agreed target` + `AgreesAllow` can now return `ProceedUnchanged`; `ResolverOnly` + checked trusted target + `membership_target == resolver target` + `AgreesAllow` can now return `ProceedUnchanged`. `ObserveOnly` still **always** returns `ProceedUnchanged` (byte-identical to the flat guard, for every observation × resolution).

     REVIEW-FIX TRUTH (#2199, defense-in-depth): `Agree`/`ResolverOnly` are NOT trusted from classification alone — `decide_treasury_gate` requires **checked trusted-target evidence** (`CoopResolutionEvidence::trusted_target()`: for `Agree`, legacy and resolver targets both present AND equal; for `ResolverOnly`, a present resolver target) before treating either as a trusted basis. Malformed/missing target evidence fails closed as `WouldDeny(UntrustedResolution)` **before** any target-unverified or membership reason. Only after a checked trusted target exists does the gate distinguish `AgreeTargetUnverified` / `ResolverOnlyTargetUnverified` (would-allow with the membership target missing/mismatched) / `NotMember` / `IndeterminateMembership` / `ObservationError`. `Disagree` → `WouldDeny(ResolverConflict)`. `LegacyOnly` / `NeitherResolved` / source-unavailable / backend-error / untrusted provenance / `UnknownLegacy` / gossip-originated rows → `WouldDeny(UntrustedResolution)`.

     EVIDENCE (from #2199, on pre-merge head `c0d0df30`): `cargo fmt --all --check` clean; `cargo test -p icn-gateway --lib --all-features` 671/0 (incl. the new malformed-evidence cases); `cargo clippy -p icn-gateway --all-targets --all-features -- -D warnings` clean; `ops/scripts/drift-check.sh` PASS. Merged non-admin with all 11 required checks green; the two Copilot review threads (missing-semicolon style + the defense-in-depth fix) were replied to and resolved.

     ISSUE EFFECTS (verified against the tracker): #2199 used `Refs #2082` (no close keyword) and its `closingIssuesReferences` is empty. **#2082 remains OPEN** (REOPENED). #2199 is MERGED.

     This sync explicitly does NOT claim: enforcement is enabled (ships observe-only; `EnforceTrustedResolver` is decision-only, wired to no route or production config); any default route-outcome change; treasury route cutover to `require_entity_access` (the flat `require_coop_access` guard remains the enforced baseline, not retired, and `require_entity_access` is not decisive for treasury); positive `entity_id` / `entity_type` token-claim issuance; mapping-as-authority (`CoopEntityMap` / resolver data is target evidence / a trust qualifier, never authority); trust of `UnknownLegacy`; trust of gossip-originated / unprovenanced mappings; upgrade of pre-existing legacy rows; full #2082 completion; A2e enforcement-cutover completion; production readiness; pilot readiness; live federation; Phase 2 completion. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-25 (A2d treasury entity gate scaffold + #2196 observe-wiring truth-sync — `origin/main` HEAD `0ef541c5` (#2197), parent `ced25f30` (#2196); docs-only, branch `docs/sync-a2d-treasury-gate-scaffold`):
     Records the two entity-aware-authorization PRs that merged after the #2187–#2194 block below (which was written by the #2195 docs-sync, merge commit `d9361640`, and predates them; that block recorded the lane as of main being at `39c51376` = #2194's squash): #2196 (`ced25f30`) wired the resolver into the treasury observation, and #2197 (`0ef541c5` = current `origin/main` tip) added the A2d treasury gate scaffold. This file is append-only and newest-first; older blocks below describe earlier states accurate when written. **This is a docs-only truth-sync — no code, schema, route, or auth-decision change lands with it.**

     LANE SUMMARY (the honest current state): treasury observe-mode now consults a *trusted, fail-closed* store-backed resolver (#2196), and there is now an explicit, tested **observe → measure → gate** seam over that observation (#2197). Both ship **observe-only by default** — the active mode is `ObserveOnly`, default route allow/deny outcomes are **byte-identical** to the flat `require_coop_access` guard, and enforcement is reachable **only** through a pure decision function that is wired to **no route and no config knob**. Entity-aware authorization is still **not enforced**, no positive entity claims are issued, and `CoopEntityMap` / resolver data is a trust qualifier, never authority.

       - #2196 (`ced25f30`) feat(authz): **store-backed resolver wired into treasury observation** — when the daemon provides the canonical, provenance-aware `CoopEntityMap`, treasury observe classification consults a trusted `StoreBackedCoopEntityResolver` instead of the unwired default; the resolver result is **discarded** (observe-only `let _ =`), so the `EntityAccessObservation` and every route allow/deny outcome are computed exactly as before. Only trusted provenance (`Activation`/`OperatorBackfill`/`Surrogate`/`GovernanceReceipt`) resolves; `UnknownLegacy`/missing/ambiguous/entity-type-mismatch/backend-error all fail closed; default (no store) stays fail-closed/unwired. **No route outcome change.**
       - #2197 (`0ef541c5` = current `origin/main` tip) feat(authz): **A2d treasury entity gate scaffold** — a single-file change in `icn/crates/icn-gateway/src/authority.rs` (+463/−5; `icn-core` untouched). Adds `TreasuryEntityAuthMode { ObserveOnly (default), EnforceTrustedResolver }` with `ACTIVE_TREASURY_ENTITY_AUTH_MODE = ObserveOnly` (the only mode shipped wired); a pure `decide_treasury_gate(mode, observation, resolution) -> TreasuryGateDecision` (`ProceedUnchanged | WouldDeny(reason)`); and observe-only gate measurement/logging via `record_treasury_gate` that records the active decision plus what `EnforceTrustedResolver` *would* decide, returning the unchanged observation (the caller still discards it). `ObserveOnly` **always** returns `ProceedUnchanged` (byte-identical to the flat guard, for every observation × resolution). The resolution is used **only as a trust qualifier**, never as authority; an untrusted resolution fails closed **before** membership is consulted.

     REVIEW-FIX TRUTH (decision-only; preserved because it bounds what `EnforceTrustedResolver` can do today, even though it is wired to nothing): under the enforcement simulation, **`ResolverOnly` is measured but not enforceable** — it fails closed with `ResolverOnlyTargetUnverified`; and the **`Agree` would-allow path also fails closed** with `AgreeTargetUnverified`. Reason: the entity-membership observation is computed against `treasury.entity_id()` / the legacy projection, **not** against the resolver's resolved/agreed target — so allowing on `Agree`/`ResolverOnly` would trust an unverified target. Consequently **no resolution currently yields `ProceedUnchanged` under `EnforceTrustedResolver`** (every path is measured fail-closed). That is intentional and conservative. A `Disagree` resolution is `WouldDeny(ResolverConflict)`; an untrusted/unverified basis is `WouldDeny(UntrustedResolution)` (covers `UnknownLegacy`, gossip/unprovenanced rows, ambiguous, entity-type-mismatch, backend/source error); membership denies still surface unchanged (`NotMember`/`IndeterminateMembership`/`ObservationError`). Logging: only the high-signal `ResolverConflict` logs at `warn!`; ordinary hypothetical denies (`ResolverOnlyTargetUnverified`/`AgreeTargetUnverified`/`UntrustedResolution`/`NotMember`/data gaps) log at `debug!`. **Next implementation follow-up:** carry the agreed/resolved `EntityId` target into membership evaluation and compare it against `treasury.entity_id()` before **either** `Agree` or `ResolverOnly` can become enforceable.

     EVIDENCE (from #2197): `cargo fmt --all --check` clean; `cargo test -p icn-gateway --lib` 656/0 (`gate_` 14/0, new gate tests across ObserveOnly / enforcement / route-safety / trust-boundary cases); `cargo clippy -p icn-gateway --all-targets --all-features -- -D warnings` clean; `ops/scripts/drift-check.sh` PASS.

     ISSUE EFFECTS (verified against the tracker): both PRs used `Refs #2082` (no close keyword) and `closingIssuesReferences` is empty for each. **#2082 remains OPEN** (the canonical `coop_id ↔ EntityId` store/mapping tracker); this lane consumes its store and the #2190 provenance substrate but does not complete it. #2195–#2197 are MERGED. RFC-0018 / #2061 / #2080 / #1868 remain the surrounding authority-side work and none is closed by this lane.

     This sync explicitly does NOT claim: entity-aware authorization enforcement (ships observe-only; `EnforceTrustedResolver` is decision-only, wired to no route or config); any route outcome change; treasury route cutover to `require_entity_access`; that `ResolverOnly` or `Agree` is enforceable today; positive `entity_id` / `entity_type` token-claim issuance; mapping-as-authority (`CoopEntityMap` / resolver data is a trust qualifier, never authority); trust of `UnknownLegacy`; trust of gossip-originated / unprovenanced mappings; upgrade of pre-existing legacy rows; full #2082 completion; production readiness; pilot readiness; live federation; Phase 2 completion. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-25 (A2c coop_id→EntityId provenance/resolver lane truth-sync — #2187–#2194 on `origin/main` HEAD `39c51376`; docs-only, branch `docs/sync-a2c-provenance-lane`):
     Records the entity-aware-authorization resolver lane (#2187–#2194), all merged to `origin/main` after the #2180 DomainPolicy block below, which the canonical current-state docs had not yet captured. This file is append-only and newest-first: older blocks below describe earlier states accurate when written; this newest block is the current view. **This is a docs-only truth-sync — no code, schema, route, or auth-decision change lands with it.**

     LANE SUMMARY (the honest current state): the A2c lane now has the persisted provenance substrate, a fail-closed store-backed resolver *source*, observe-only treasury classification, and trusted provenance *producers* for local coop activation and operator surrogate backfill. That makes trusted resolver observation meaningful **once the source is wired to an appropriate read path**, but it still does **not** enforce entity-aware authorization or issue trusted entity claims. Every binding remains non-authority (the `CoopEntityMap` module doc: a binding "grants zero standing, role, capability, membership, mandate, or permission").

       - #2187 (`aaf867a1`) docs(authz): **define governed `coop_id → EntityId` resolver seam** — design only (`docs/design/coop-id-entity-resolver.md`). Identified the keystone: the gateway already had the entity model and flat `coop_id` guards plus a canonical persisted `CoopEntityMap` store (#2082), but no governed, provenance-aware, fail-closed read path consuming it. No code.
       - #2188 (`e1810fc8`) feat(authz): **fail-closed resolver seam** — adds the `CoopEntityResolver` trait + by-value resolution result + the fail-closed `UnwiredCoopEntityResolver` default (always Unavailable, reads no input, never fabricates an `EntityId`). Route/observe twin of the issuance `DenyUntilWired` seam. No route wiring, no store consumption, no provenance — no behavior change by itself.
       - #2189 (`8368ce28`) feat(authz): **observe-only treasury wiring** — consults the resolver alongside the existing flat-`coop_id` projection inside the already-fire-and-forget treasury access observation, classifying/logging legacy-vs-resolver discrepancy. Existing authorization is preserved unchanged; **no route outcome changes**.
       - #2190 (`6798f7c4`) feat(entity): **persisted provenance substrate** — adds `CoopEntityBindingProvenance`, `bind_resolved_with_provenance`, and `binding_for_coop` / `binding_for_entity` on `CoopEntityMap` (InMemory + Sled, persisted under a `coop_entity_provenance:{coop_id}` key space). Existing unprovenanced rows read back as the fail-closed `UnknownLegacy` sentinel, which remains untrusted for resolution. `icn-entity` substrate only — no gateway/resolver consumption.
       - #2191 (`0a520096`) docs(spec): **institutional powers / legitimacy invariants** — doctrine-level spec (`docs/spec/institutional-powers.md`); introduces no code/schema/runtime behavior. Names the legitimacy circuit: authority basis → adopted policy → bounded effect → receipt → challenge/repair path.
       - #2192 (`f48d9898`) feat(authz): **store-backed resolver source** — adds `StoreBackedCoopEntityResolver` reading `binding_for_coop`; resolves **only** trusted provenance variants and fails closed on missing, untrusted/`UnknownLegacy`, ambiguous (one-sided / reverse-index conflict), entity-type-mismatch, or store/backend error. The trust gate `provenance_is_trusted_for_resolution` is exhaustive (a future variant is a compile error, not silent trust). Additive source type only — **not wired into any handler/app-state enforcement**; the observe path still uses the unwired default.
       - #2193 (`a0f2fbf3`) feat(coop): **activation provenance producer** — the local *authoritative* coop activation path records `Activation` provenance via `bind_resolved_with_provenance`; the gossip-mirror / untrusted replication path stays unprovenanced (`UnknownLegacy`). Provenance is a trust assertion: only the node that authoritatively activated may write `Activation`. No enforcement or route behavior change.
       - #2194 (`39c51376` = current `origin/main` tip) feat(coop): **operator backfill provenance producer** — `icnctl coop entity-backfill-surrogates --apply` now records `OperatorBackfill` provenance through `bind_resolved_with_provenance` (previously an unprovenanced `bind_resolved`). Dry-run unchanged; collision / storage-error / already-bound behavior unchanged; the original `coop_id` is preserved byte-for-byte. A durable Sled-backed test proves `OperatorBackfill` persists across reopen; a gateway interop test proves an operator-backfilled surrogate resolves through the existing trust gate (the missing-provenance case still fails closed). `OperatorBackfill` was already a trusted variant — **no resolver-trust broadening**. Does **not** upgrade pre-existing `UnknownLegacy` rows; does **not** complete #2082.

     MEANING FIREWALL (verified): the lane lives in `icn-entity` (substrate), `icn-gateway` / `icn-authz` (resolver source + observe), `icn-coop` + `icnctl` (producers), and docs; no kernel domain-import widening; no NYCN/Summit nouns. Regulatory-safe vocabulary preserved (authority / provenance / binding / resolver / entity / capability-token claim — never payment / wallet / balance / currency).

     ISSUE EFFECTS (verified against the tracker): every lane PR used `Refs` (no close keyword) and `closingIssuesReferences` is empty for each. **#2082 remains OPEN** (the canonical `coop_id ↔ EntityId` store/mapping tracker); this lane consumes and produces against its store but does not complete it. #2187–#2194 are MERGED. RFC-0018 / #2061 / #2080 / #1868 remain the surrounding authority-side work and none is closed by this lane.

     STILL FORWARD WORK: observe-only wiring of the store-backed resolver into a read path consuming the `Activation` / `OperatorBackfill` rows (non-enforcement); per-route-family observe → measure → (only then) enforce gates; explicit enforcement-cutover criteria; upgrade/repair of pre-existing `UnknownLegacy` rows to recorded provenance; a positive trusted issuance source for `entity_id` / `entity_type` token claims (the #2080 lane); full #2082 completion; cross-process / multi-writer transactional store safety.

     This sync explicitly does NOT claim: entity-aware authorization enforcement; any route outcome change; treasury route cutover; `require_entity_access` migration; positive `entity_id` / `entity_type` token-claim issuance; `StoreBackedCoopEntityResolver` handler/app-state wiring; mapping-as-authority; trust of `UnknownLegacy`; trust of gossip-originated mappings; upgrade of pre-existing legacy rows; full #2082 completion; production readiness; pilot readiness; live federation; Phase 2 completion. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-23 (wire DomainPolicy body store into adoption — #2180, branch `feat/2180-domain-policy-body-adoption`; bundles code + docs truth-sync). This file is append-only and newest-first: older blocks below describe earlier states (e.g. #2178 as "store-only, route not yet wired") that were accurate when written; this newest block is the current view.
     Records the rung after #2178/#2179 (the body store, merged `a41958cf`): the persisted DomainPolicy adoption path now persists the policy body and can resolve it. App-side only (`apps/governance` = `icn-governance-actor`); `icn-governance` pure-core unchanged; no new HTTP route; no new actor field; route path unchanged; no auth-model change.

       - #2180 feat(governance): **persist domain policy bodies during adoption** — `GovernanceManager::adopt_domain_policy_persisted` is refactored into `adopt_domain_policy_persisted_with_body(domain_id, policy, policy_content: Option<&[u8]>, actor, now)` with the old name kept as a thin `None` wrapper (all existing callers/tests unchanged). When `policy_content` is `Some`, inside the existing per-domain critical section the order is `load → gate-adopt → save body → save domain`: the body is persisted via the #2179 `save_domain_policy_body` (re-hash + fail-closed content/`DomainPolicyId` integrity check) **after** `DefaultMandateGate` resolves and **before** the `current_policy` update — so a gate rejection persists neither, and a body-store failure leaves `current_policy` unchanged. Adds `GovernanceManager::get_domain_policy_body(&DomainPolicyId) -> Result<Option<Vec<u8>>>` (fail-closed `MissingDomainStore` without a store) as the manager-level resolve. The `POST /gov/domains/{domain_id}/domain-policy/adopt` handler now calls the `_with_body` form with `Some(policy_content.as_bytes())` — the same bytes the id is content-addressed from, so integrity always holds on the route path; existing `MAX_DOMAIN_POLICY_CONTENT_BYTES` cap + empty/whitespace rejection (400-before-hash) and the per-domain lock are preserved unchanged. **No HTTP body-read route added.** Tests: 4 new route tests (body stored + resolves via manager; oversize/empty/unauthorized do not store a body) + 2 manager tests (happy-path with-body resolve; content/id hash-mismatch fails closed and leaves `current_policy` unchanged, the latter red-green verified by reordering save-domain-before-body). Route suite 12/12; `domain_policy_adoption` 34; full `icn-governance-actor` lib green.

     MEANING FIREWALL (verified): `icn-governance` pure-core untouched; adoption + body persistence stay in `apps/governance`. No new authority primitive; no NYCN/Summit nouns; the required `Meaning Firewall Check` + `Kernel Forbidden Dependencies` gates stay green (no new violation).

     ISSUE EFFECTS: tracked by new implementation issue **#2180** (`type:impl` + `epic:arch-invariants`); the issue/PR use `Refs #2179 Refs #2178 Refs #1817` with no close keyword. **#2178 was closed-completed (the body-store seam it tracked landed in #2179)**; #2179 is merged; #2142 and #1817 stay closed.

     STILL FORWARD WORK: a public HTTP body-read/GET route; the first read-only CCL registry/evaluator seam; the full CCL policy registry / evaluator-selection / CCL evaluation runtime (#1817); a durable policy-version/registry catalog; the full `InstitutionalDomain` / `DomainPolicy` lifecycle; service-binding runtime; package activation; entity-aware auth cutover; cross-process / multi-writer transactional store safety.

     This sync explicitly does NOT claim: a public body-read route; full CCL runtime or policy-evaluator selection; a durable policy registry/catalog; service-binding runtime; package activation; any auth-model change or entity-aware auth cutover; cross-process / multi-writer transactional safety; full InstitutionalDomain/DomainPolicy lifecycle; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-23 (durable DomainPolicy body store seam — #2178, branch `feat/2178-domain-policy-body-store`; this PR bundles the code change and its docs truth-sync, unlike the split #2142 rungs that each had a separate post-merge docs PR):
     Records the first implementation rung after #2142 (closed-completed 2026-06-23): a durable `DomainPolicy` **body store** seam. App-side only (`apps/governance` = `icn-governance-actor`); `icn-governance` pure-core unchanged; no HTTP route; no manager, route, or auth-model change. (This file is append-only and newest-first: the older `[sync edit]` blocks below still describe #2142 as open because that was accurate when each was written, before its 2026-06-23 closure; this newest block is the current view and is not contradicted by that preserved history.)

       - #2178 feat(governance): **persist domain policy bodies** — `GovernanceStateStore` gains default-implemented, **fail-closed** (`anyhow::bail!`) `get_domain_policy_body(&DomainPolicyId) -> Result<Option<Vec<u8>>>` / `save_domain_policy_body(&DomainPolicyId, &[u8]) -> Result<()>` trait methods — a store not implementing them must never masquerade as "no body stored". `SledGovernanceStateStore` overrides both under a distinct **content-addressed** `gov:domain-policy-body:{DomainPolicyId}` key space (sibling to `gov:institutional-domain:`), storing the raw policy body bytes whose blake3 hash is the key. Both directions re-hash with `DomainPolicyId::from_content` and **fail closed on key/body mismatch** (`save` refuses an id that does not hash the content; `get` refuses a stored body that no longer hashes to its key — corruption never yields mismatched bytes). Idempotent by construction (identical content → identical key → natural dedup). 6 focused state-store tests (round-trip; missing → `Ok(None)`; content-addressed/idempotent; corrupted-read fails closed; save id/content-mismatch fails closed; default-impl fails closed via a non-overriding store double), red-green verified — both integrity tests were confirmed to fail when the check is removed. Full `icn-governance-actor` lib suite green.

     WHY: #2142 stored only the adopted `current_policy: DomainPolicyRef` pointer and dropped the policy body after hashing (ADR-0083 Q4 deferred the body to #1817). This rung lets an adopted ref be resolved back to the bytes that produced it — the first safe step toward the CCL policy registry without implementing it.

     MEANING FIREWALL (verified): `icn-governance` pure-core untouched; the store seam stays in `apps/governance`. No new authority primitive; no NYCN/Summit nouns; the required `Meaning Firewall Check` + `Kernel Forbidden Dependencies` gates stay green (no new violation).

     ISSUE EFFECTS: tracked by new implementation issue **#2178** (`type:impl` + `epic:arch-invariants`); the issue/PR use `Refs #1817 Refs #2142` with no close keyword — neither #1817 nor #2142 is reopened or closed by this lane (#2142 already closed-completed; #1817 already closed as the spec-level registry design `docs/spec/ccl-policy-registry.md`).

     STILL FORWARD WORK: wiring the adoption route / `adopt_domain_policy_persisted` to persist + resolve bodies (a separate later rung that changes route behavior); the first read-only CCL registry/evaluator seam; the full CCL policy registry / evaluator-selection / CCL evaluation runtime (#1817); a durable policy-version/registry catalog; the full `InstitutionalDomain` / `DomainPolicy` lifecycle (standing, services, routing, federation, exit); service-binding runtime; package activation; entity-aware auth cutover; cross-process / multi-writer transactional store safety.

     This sync explicitly does NOT claim: route-level body persistence; full CCL runtime or policy-evaluator selection; a durable policy registry/catalog; service-binding runtime; package activation; any auth-model change or entity-aware auth cutover; cross-process / multi-writer transactional safety; full InstitutionalDomain/DomainPolicy lifecycle; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-23 (post-merge: gated InstitutionalDomain declaration HTTP route — `origin/main` HEAD `9a9afe94`: #2176 — NARROW scope, branch `docs/sync-institutional-domain-declare-route`):
     Records the #2142 follow-up that exposes the #2174 mandate-gated declaration seam over HTTP, merged to `origin/main` after the #2174 block below. NARROW sync covering only #2176. **#2142 advances; its MVP acceptance criteria appear satisfied, but it remains OPEN pending explicit acceptance review.**

       - #2176 (`9a9afe94` = squash-merge commit, current `origin/main` tip) feat(governance): **expose institutional domain declaration route** — app-side only (`apps/governance` = `icn-governance-actor`); `icn-governance` pure-core unchanged. Adds ONE route `POST /gov/domains/{domain_id}/institutional-domain/declare` (handler `declare_institutional_domain`) driving the existing gated seam: existing `governance:write` scope → `GovernanceManager::declare_institutional_domain_gated(...)` → real `DefaultMandateGate` (resolves the `institutional_domain:declare` Execution grant) **before any store write** → persist → declared-domain projection. The declaring **actor is the authenticated token subject**, never a body field. Body: `entity_type` (closed `BootstrapEntityType`; out-of-taxonomy → 400) + optional 64-hex `charter_id` (length-checked before decoding; malformed → 400). **The route NEVER calls the ungated bootstrap `declare_institutional_domain` seam.** Intentionally **no `check_domain_membership`**: declaration may *establish* the governed domain, so requiring prior membership could wrongly block the first authorized declaration — the gate grant is the decisive, stronger check (documented in the handler). Error mapping: gate denial → 403; already declared → 409; missing backend/store or gate backend fault → 500; malformed → 400. Because the gate resolves before the duplicate check, an **unauthorized attempt on an already-declared domain returns 403, not 409** (no existence leak). 11 route tests + regenerated `route-inventory.md` (82→83 governance candidates). One Copilot review round (clippy `clone_on_copy` on `req.entity_type`; charter_id length-check-before-decode) fixed in `5b4e7228`/`da900e66` before merge.

     ISSUE EFFECTS (verified against the tracker): **#2142 remains OPEN** — #2176 used `Refs #2142` (not a close keyword); `closingIssuesReferences` is empty. The MVP acceptance criteria appear met (see the acceptance-review note); closure awaits explicit human authorization.

     STILL OPEN / FORWARD WORK (#2142): the full `InstitutionalDomain` / `DomainPolicy` lifecycle (standing, services, routing, federation, exit); durable `DomainPolicy` body storage + CCL policy registry / evaluator selection / CCL evaluation (#1817); service-binding runtime; package activation; entity-aware auth cutover; cross-process / multi-writer transactional safety.

     This sync explicitly does NOT claim: #2142 complete (closure is a human decision); full InstitutionalDomain/DomainPolicy lifecycle; durable `DomainPolicy` body storage; CCL runtime or policy-evaluator selection; service-binding runtime; package activation; any auth-model change or entity-aware auth cutover; cross-process / multi-writer transactional safety; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. The non-required `Security Audit` (cargo-audit dependency backlog) check was red on #2176 and did NOT block merge — it is not in the branch-protection required set, and #2176 changed no dependency manifest/lock. -->

<!-- [sync edit] 2026-06-23 (post-merge: mandate-gated InstitutionalDomain declaration — `origin/main` HEAD `f8a821e5`: #2174 — NARROW scope, branch `docs/sync-gate-institutional-domain-declare`):
     Records the #2142 follow-up that mandate-gates `InstitutionalDomain` declaration at the manager seam, merged to `origin/main` after the #2172 block below. NARROW sync covering only #2174. **#2142 advances but is NOT complete and remains OPEN.**

       - #2174 (`f8a821e5` = squash-merge commit, current `origin/main` tip) feat(governance): **gate institutional domain declaration** — app-side only (`apps/governance` = `icn-governance-actor`); `icn-governance` pure-core unchanged; **no HTTP route**. Declaring a governed domain is an authority-bearing act (ADR-0083's flagged sub-question), now settled in favor of gating. Adds `MandateAct::DeclareInstitutionalDomain` (Execution class; act token `institutional_domain:declare` / proposal-class `InstitutionalDomain`; wire token `declare_institutional_domain`), reusing the existing `MandateTarget::Domain` resolver path — no resolver logic duplicated; the `act_wire_tokens_are_distinct_and_snake_case` guard now enumerates every act (also adding `AdoptDomainPolicy`, missing since #2164). Adds `GovernanceManager::declare_institutional_domain_gated(domain_id, owning_entity_class, charter_ref, actor, now)` → `DeclareInstitutionalDomainError`: it resolves authority through the real `DefaultMandateGate::require()` **before any store write**, then delegates to the existing ungated `declare_institutional_domain`. The error taxonomy preserves the gate's `Rejected`/`Backend` split — `Unauthorized(MandateRejection)` is a 403-class denial, `Backend(String)` is a 5xx-class gate read failure (distinct, never conflated), plus `MissingReceiptBackend` and `Store(InstitutionalDomainStoreError)` (covers `AlreadyDeclared`). The ungated `declare_institutional_domain` is retained as a documented **bootstrap/in-process-only** seam that must never be wired to a routable surface (a future declare HTTP route must call the gated seam). 10 new tests (gated success; missing backend; wrong actor/domain/act; expired/revoked; duplicate still refused; gate `Backend` → `Backend` not `Unauthorized`; ungated bootstrap still works without a backend). One Copilot review round (wire-token coverage + `Rejected`/`Backend` split + `Display`/rustdoc) fixed in `c0adcbc7` before merge.

     ISSUE EFFECTS (verified against the tracker): **#2142 remains OPEN** — #2174 used `Refs #2142` (not a close keyword); `closingIssuesReferences` is empty.

     STILL OPEN / FORWARD WORK (#2142): the thin **declare/create HTTP route** (the next rung — must call `declare_institutional_domain_gated`, never the ungated seam); the full `InstitutionalDomain` / `DomainPolicy` lifecycle (standing, services, routing, federation, exit); durable `DomainPolicy` body storage + CCL policy registry / evaluator selection / CCL evaluation (#1817); service-binding runtime; package activation; cross-process / multi-writer transactional safety.

     This sync explicitly does NOT claim: #2142 complete; a declare/create HTTP route; full InstitutionalDomain/DomainPolicy lifecycle; durable `DomainPolicy` body storage; CCL runtime or policy-evaluator selection; service-binding runtime; package activation; any auth-model change or entity-aware auth cutover; cross-process / multi-writer transactional safety; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. The non-required `Security Audit` (cargo-audit dependency backlog) check was red on #2174 and did NOT block merge — it is not in the branch-protection required set, and #2174 changed no dependency manifest/lock. -->

<!-- [sync edit] 2026-06-23 (post-merge: persisted gated DomainPolicy adoption HTTP route — `origin/main` HEAD `15dcdac1`: #2172 — NARROW scope, branch `docs/sync-domain-policy-adoption-route`):
     Records the #2142 follow-up that exposes the #2170 persisted gated adoption seam over HTTP, merged to `origin/main` after the #2170 block below. NARROW sync covering only #2172. **#2142 advances but is NOT complete and remains OPEN.**

       - #2172 (`15dcdac1` = squash-merge commit, current `origin/main` tip) feat(governance): **expose domain policy adoption route** — app-side only (`apps/governance` = `icn-governance-actor`); `icn-governance` pure-core unchanged. Adds ONE route `POST /gov/domains/{domain_id}/domain-policy/adopt` (handler `adopt_domain_policy`) that drives the existing persisted seam end-to-end: existing `governance:write` scope + existing coarse `check_domain_membership` guard → `GovernanceManager::adopt_domain_policy_persisted(...)` → load `InstitutionalDomain` → real `DefaultMandateGate` authority resolution (the authoritative adopt check; membership is additive, not a substitute) → pure-core `InstitutionalDomain::adopt_policy()` defense-in-depth → save → adopted `DomainPolicyRef` projection (`{policy_id, domain_id}`). The adopting **actor is the authenticated token subject**, never a body field; the candidate policy is **content-addressed server-side** from `policy_content` and bound to the path `domain_id`, so policy↔domain identity is correct by construction (the pure-core `PolicyForOtherDomain` rejection is unreachable from the route). Request body is bounded by `MAX_DOMAIN_POLICY_CONTENT_BYTES` (262_144) via a centralized `validate_domain_policy_content` (DoS guard mirroring the charter-YAML cap) — empty/whitespace and oversize → 400 before hashing. Error mapping: a missing `GovernanceDomain` (membership guard, which runs first) or an undeclared `InstitutionalDomain` → 404; missing store / backend read-write failure → 500; a non-member or a gate / structural authority refusal → 403; malformed/empty/oversize body → 400. New `AdoptDomainPolicyRequest` / `AdoptDomainPolicyResponse` models; regenerated `route-inventory.md` (81→82 governance candidates). 8 HTTP route integration tests (mounted + succeeds; persists `current_policy` + survives reload; requires write scope; non-member 403; undeclared 404; wrong actor 403 via gate; revoked mandate 403 via gate; empty 400; oversize 400). Full `icn-governance-actor` suite green; no regression in `process_gate_result_http_route` (5).

     CONCURRENCY (now IMPLEMENTED — the #2170-flagged prerequisite): `adopt_domain_policy_persisted` serializes its `load → adopt → save` **per domain** via an in-process lock keyed by `GovernanceDomainId` (`GovernanceManager::domain_adoption_lock`, `Mutex<HashMap<GovernanceDomainId, Arc<Mutex<()>>>>`; the outer map lock is held only for lookup/insert; the guarded section is fully synchronous, so the lock is never held across an `.await`). Same-domain adoptions serialize, different domains stay concurrent; an 8-thread same-domain contention test proves it. This is a **single-node** guarantee only — a transactional / multi-writer store primitive (atomic compare-and-swap `get`+`put`) for cross-process safety remains future work.

     ISSUE EFFECTS (verified against the tracker): **#2142 remains OPEN** — #2172 used `Refs #2142` (not a close keyword); `closingIssuesReferences` is empty.

     STILL OPEN / FORWARD WORK (#2142): **no declare/create route** — `declare_institutional_domain` persists a domain but is NOT mandate-gated yet (a flagged ADR-0083 sub-question), so it must not be exposed on a routable surface as-is (exposing it would be an authority bypass); the full `InstitutionalDomain` / `DomainPolicy` lifecycle (standing, services, routing, federation, exit); durable `DomainPolicy` body storage + CCL policy registry / evaluator selection / CCL evaluation (#1817); service-binding runtime; package activation; cross-process transactional adoption safety.

     This sync explicitly does NOT claim: #2142 complete; a declare/create route; full InstitutionalDomain/DomainPolicy lifecycle; durable `DomainPolicy` body storage; CCL runtime or policy-evaluator selection; service-binding runtime; package activation; any auth-model change or entity-aware auth cutover; cross-process / multi-writer transactional safety; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. The non-required `Security Audit` (cargo-audit dependency backlog) check was red on #2172 and did NOT block merge — it is not in the branch-protection required set, and #2172 changed no dependency manifest/lock; `Compare Against Base` (benchmark variance) is likewise non-required. -->

<!-- [sync edit] 2026-06-23 (post-merge: InstitutionalDomain persistence seam — `origin/main` HEAD `fb4796c3`: #2170 — NARROW scope, branch `docs/sync-institutional-domain-store`):
     Records the #2142 follow-up that makes `InstitutionalDomain` durably load/save-able and adds persisted gated adoption, merged to `origin/main` after the #2166 block below. NARROW sync covering only #2170. **#2142 advances but is NOT complete and remains OPEN.** It implements the persistence model decided in the ADR-0083 addendum (#2169).

     WHAT LANDED (squash tip `fb4796c3` on `origin/main`):
       - #2170 (`fb4796c3` = squash-merge commit, current `origin/main` tip) feat(governance): **persist institutional domain state** — app-side only (`apps/governance` = `icn-governance-actor`); `icn-governance` pure-core unchanged; no HTTP route. `GovernanceStateStore` gains `get_institutional_domain` / `save_institutional_domain` as **default-implemented, fail-closed** (`anyhow::bail!`) trait methods — a store not implementing them must never masquerade as "no domain declared"; `SledGovernanceStateStore` overrides both, keyed by the existing `GovernanceDomainId` under a **distinct `gov:institutional-domain:` key space** (sibling to `gov:domain:`, not embedded), persisting the record incl. its adopted `current_policy: DomainPolicyRef` only — no `DomainPolicy` body (#1817). `GovernanceManager` gains `with_domain_store` + `pub(crate) domain_state_store()` (mirror `with_receipt_store`/`receipt_backend`); `declare_institutional_domain` (persist; `AlreadyDeclared` on duplicate; `MissingDomainStore` fail-closed without a store) and `adopt_domain_policy_persisted` (`load → existing gated adopt_domain_policy (real DefaultMandateGate + pure-core commit) → save`; state unchanged on rejection). `InstitutionalDomainStoreError { MissingDomainStore, AlreadyDeclared, NotDeclared, Store, Adopt }`. Store tests (round-trip, distinct key space, fail-closed defaults via a non-overriding store double) + 8 persisted-manager tests; full `icn-governance-actor` suite (297 lib + integration) and `institutional_domain` (14) green. A pre-squash branch commit (`f4b41548`, folded into the `fb4796c3` squash) qualified two intra-doc links and added a `# Concurrency` caveat to `adopt_domain_policy_persisted` per Copilot review.

     CONCURRENCY (recorded, not yet fixed): `adopt_domain_policy_persisted`'s `load → mutate → save` is not atomic and has no per-domain serialization. There is no concurrent caller yet (no HTTP route). Per-domain locking and/or an atomic store primitive is a **prerequisite for the HTTP-route lane**, documented on the method.

     MEANING FIREWALL (verified): `icn-governance` pure-core untouched; persistence + authority resolution stay in `apps/governance`. No new authority primitive. No NYCN/Summit nouns. The required `Meaning Firewall Check` + `Kernel Forbidden Dependencies` gates passed.

     ISSUE EFFECTS (verified against the tracker): **#2142 remains OPEN** — #2170 used `Refs #2142` (not a close keyword); `closingIssuesReferences` is empty.

     STILL OPEN / FORWARD WORK (#2142): the thin HTTP adoption route (now unblocked — calls `adopt_domain_policy_persisted`, must add per-domain concurrency control first); declare-act mandate gating (the ADR-flagged sub-question — declare is not yet authority-gated, so it must not be exposed on a routable surface as-is); the full `InstitutionalDomain`/`DomainPolicy` lifecycle; CCL policy registry / evaluator selection / CCL evaluation (#1817); service-binding runtime; package activation.

     This sync explicitly does NOT claim: #2142 complete; an HTTP route; durable `DomainPolicy` body storage; full lifecycle; CCL runtime or policy-evaluator selection; service-binding runtime; package activation; declare-act mandate gating; any auth-model change or entity-aware auth cutover; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. The non-required `Security Audit` (cargo-audit dependency backlog) check was red on #2170 and did NOT block merge — it is not in the branch-protection required set, and #2170 changed no dependency manifest/lock. -->

<!-- [sync edit] 2026-06-23 (post-merge: DomainPolicy adoption GovernanceManager seam — `origin/main` HEAD `fe956146`: #2166 — NARROW scope, branch `docs/sync-domain-policy-manager-seam`):
     Records the #2142 follow-up that exposes the #2164 gated adoption helper through the governance app boundary, merged to `origin/main` after the #2164 block below. NARROW sync covering only #2166. **#2142 advances but is NOT complete and remains OPEN.**

     WHAT LANDED (squash tip `fe956146` on `origin/main`):
       - #2166 (`fe956146` = current tip) feat(governance): **add domain policy adoption manager seam** — app-side only (`apps/governance` = `icn-governance-actor`); `icn-governance` pure-core unchanged; no HTTP route. Adds `GovernanceManager::adopt_domain_policy(&self, &mut InstitutionalDomain, &DomainPolicy, &Did, now)` (in the `domain_policy_adoption` module via `impl GovernanceManager`): it resolves authority through this manager's wired `GovernanceReceiptBackend` + the real `DefaultMandateGate` by delegating to `adopt_domain_policy_gated(...)`, then commits through pure-core `InstitutionalDomain::adopt_policy` as **defense-in-depth**. **Fails closed** with `DomainPolicyAdoptionError::MissingReceiptBackend` when no backend is wired (a manager that cannot resolve authority must never allow adoption); other failures wrap as `DomainPolicyAdoptionError::Gated(AdoptDomainPolicyError)`. Adds a tiny `pub(crate) GovernanceManager::receipt_backend()` accessor. **Option B (honest):** there is no durable `InstitutionalDomain` store yet, so the seam operates on a caller-held `&mut InstitutionalDomain` and returns the adopted `DomainPolicyRef`; persistence is a later domain-store lane. 6 manager-seam tests over the real `DefaultMandateGate` (success; missing-backend fail-closed; wrong actor/domain; revoked authority; pure-core defense-in-depth) — `domain_policy_adoption` now 13 tests total.

     MEANING FIREWALL (verified): authority resolution stays in `apps/governance`; `icn-governance` pure-core is untouched and never imports the gate. No new authority primitive (reuses `Mandate` / `AuthorityGrant` / `TypedScope` / `MandateGate` / `GovernanceReceiptBackend`). No NYCN/Summit nouns. The required `Meaning Firewall Check` + `Kernel Forbidden Dependencies` gates passed.

     ISSUE EFFECTS (verified against the tracker): **#2142 remains OPEN** — #2166 used `Refs #2142` (not a close keyword); `closingIssuesReferences` is empty. This is the app-boundary seam rung, not issue closure.

     STILL OPEN / FORWARD WORK (#2142): no HTTP/route surface for domain-policy adoption (the next lane: a thin governance route that calls `GovernanceManager::adopt_domain_policy`, not bypassing it); no durable `InstitutionalDomain` persistence; no full `InstitutionalDomain`/`DomainPolicy` lifecycle (standing, services, routing, federation, exit); no CCL policy registry / evaluator selection / CCL evaluation; no service-binding runtime; no package activation.

     This sync explicitly does NOT claim: #2142 complete; full InstitutionalDomain/DomainPolicy lifecycle; durable InstitutionalDomain persistence; a domain-policy HTTP/route surface; CCL runtime or policy-evaluator selection; package activation; any auth-model change or entity-aware auth cutover; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. The non-required `Security Audit` (cargo-audit dependency backlog) check was red on #2166 and did NOT block merge — it is not in the branch-protection required set, and #2166 changed no dependency manifest/lock. -->

<!-- [sync edit] 2026-06-23 (post-merge: DomainPolicy adoption gate-wired to MandateGate — `origin/main` HEAD `8594cd98`: #2164 — NARROW scope, branch `docs/sync-domain-policy-gate`):
     Records the #2142 follow-up that wires `DomainPolicy` adoption to the existing app-side authority resolver, merged to `origin/main` after the #2162 block below. NARROW sync covering only #2164. **#2142 advances but is NOT complete and remains OPEN.**

     WHAT LANDED (squash tip `8594cd98` on `origin/main`):
       - #2164 (`8594cd98` = current tip) feat(governance): **gate domain policy adoption** — app-side only (`apps/governance` = `icn-governance-actor`); `icn-governance` pure-core unchanged. Adds `MandateAct::AdoptDomainPolicy` (Execution class; act token `domain_policy:adopt` / proposal-class `DomainPolicy`; wire token `adopt_domain_policy`), reusing the existing `MandateTarget::Domain` resolver path — no resolver logic duplicated. Adds `apps/governance::domain_policy_adoption::adopt_domain_policy_gated(backend, domain, policy, actor, at)` + `AdoptDomainPolicyError { Unauthorized, Backend, Core }`: it builds a `MandateRequest` and runs the **real** `DefaultMandateGate::require()` (actor → active grants → `TypedScope.domain` + Execution class + act-token + mandate lifecycle) against the existing `GovernanceReceiptBackend`, then commits through the pure-core `InstitutionalDomain::adopt_policy` as **defense-in-depth**. Adoption is no longer shape-only — it now requires real authority resolution against the receipt/grant store. Fails closed on any gate rejection, backend read failure, or structural rejection, leaving policy state unchanged. 7 unit tests over the real `DefaultMandateGate` (success; wrong domain/actor/act/expired-grant/revoked-mandate; pure-core defense-in-depth). A follow-up doc-only commit (`be70269e`) qualified one module-doc intra-doc link to `Mandate` (to a fully-qualified `icn_governance::Mandate` path) per Copilot review.

     MEANING FIREWALL (verified): authority resolution lives in `apps/governance`; `icn-governance` stays pure types + structural validation and never imports the gate. No new authority primitive (reuses `Mandate` / `AuthorityGrant` / `TypedScope` / `MandateGate` / `GovernanceReceiptBackend`). No NYCN/Summit nouns. The required `Meaning Firewall Check` + `Kernel Forbidden Dependencies` gates passed.

     ISSUE EFFECTS (verified against the tracker): **#2142 remains OPEN** — #2164 used `Refs #2142` (not a close keyword); `closingIssuesReferences` is empty. This is the authority-resolution wiring rung, not issue closure.

     STILL OPEN / FORWARD WORK (#2142): no HTTP/route surface for domain-policy adoption; no `GovernanceManager` adoption seam (the gated function is standalone, not yet wired into the manager); no full `InstitutionalDomain`/`DomainPolicy` lifecycle (standing, services, routing, federation, exit); no CCL policy registry / evaluator selection / CCL evaluation; no service-binding runtime; no package activation.

     This sync explicitly does NOT claim: #2142 complete; full InstitutionalDomain/DomainPolicy lifecycle; CCL runtime or policy-evaluator selection; a domain-policy HTTP/route surface; GovernanceManager integration for adoption; package activation; any auth-model change or entity-aware auth cutover; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. The non-required `Security Audit` (cargo-audit dependency backlog) check was red on #2164 and did NOT block merge — it is not in the branch-protection required set, and #2164 changed no dependency manifest/lock. -->

<!-- [sync edit] 2026-06-23 (post-merge: InstitutionalDomain / DomainPolicy minimal runtime root — `origin/main` HEAD `e9e87f3c`: #2162 — NARROW scope, branch `docs/sync-institutional-domain-runtime`):
     Records the minimal runtime root from ADR-0083 (#2161) / issue #2142, merged to `origin/main` after the #2158/#2159 block below. NARROW sync covering only #2162.

     WHAT LANDED (squash tip `e9e87f3c` on `origin/main`):
       - #2162 (`e9e87f3c` = current tip) feat(governance): **add institutional domain policy root** — new `icn-governance::institutional_domain` module adding `InstitutionalDomain`, `DomainPolicy`, `DomainPolicyRef`, `DomainPolicyId`, and `InstitutionalDomainError` (re-exported from the crate root). `InstitutionalDomain` is keyed by the existing `GovernanceDomainId` (it **references**, never renames or forks, `GovernanceDomain`); `owning_entity_class` reuses `BootstrapEntityType`; `charter_ref` is `Option<CharterId>`; `current_policy` is `Option<DomainPolicyRef>`. `DomainPolicy` / `DomainPolicyRef` / `DomainPolicyId` are **blake3 content-addressed** and store/interpret **no CCL text**. **Policy inertness is structural** — only the single adopted `current_policy` confers authority; any other ref yields none (`has_adopted` compares against `current_policy`). `adopt_policy` is **fail-closed**, reusing the same primitives the app-side `DefaultMandateGate::validate_mandate_lifecycle` uses in the same order (status liveness → deadline → empty-grant): it refuses a policy authored for another domain, and a **missing**, **ambiguous**, **inactive/revoked/past-deadline**, or **unbound** authority basis, leaving policy state unchanged. 14 unit tests (declare, adopt, replace, structural inertness, content-addressing, serde round-trip, and the precedence/fail-closed authority cases). No new authority primitive (reuses `Mandate` / `AuthorityGrant` / `TypedScope`), no kernel import, no NYCN/Summit nouns; the required `Meaning Firewall Check` + `Kernel Forbidden Dependencies` gates passed. A post-open Copilot review fix (`28d16c9a`) reordered the liveness-before-unbound checks to match the gate's precedence (added test `inactive_status_takes_precedence_over_unbound_grants`).

     ISSUE EFFECTS (verified against the tracker, not inferred from PR keywords): **#2142 remains OPEN** — #2162 used `Refs #2142` (not a close keyword); `closingIssuesReferences` is empty. The landed slice is the **Declare + Adopt-policy** runtime root only.

     STILL OPEN / FORWARD WORK (#2142): gate-wired grant→domain resolution — matching a grant's `TypedScope.domain` against the target domain and invoking the app-layer `MandateGate::require()` (needs the governance receipt/grant store) — remains follow-up; this slice enforces presence / uniqueness / boundedness / liveness of the authority `Mandate` only. Also forward: full `InstitutionalDomain` / `DomainPolicy` lifecycle (standing, services, routing, federation, exit), CCL policy registry / evaluator selection / CCL evaluation, package activation, service binding runtime.

     This sync explicitly does NOT claim: #2142 complete; full InstitutionalDomain/DomainPolicy lifecycle; CCL runtime or policy-evaluator selection; grant→domain TypedScope resolution; MandateGate::require() wired to adoption; package activation; any auth-model change or entity-aware auth cutover; production / pilot / organizer / federation readiness. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. The non-required `Security Audit` (cargo-audit dependency backlog) check was red on #2162 and did NOT block merge — it is not in the branch-protection required set, and #2162 changed no dependency manifest/lock. The required `Test` check flaked once on an unrelated `icn-core` two-node batch-determinism test (`test_two_node_effect_batch_determinism`, membership-batch state-hash); it passed on a single rerun before merge. -->

<!-- [sync edit] 2026-06-23 (post-merge truth-sync for two runtime slices — main HEAD `6acb666a`: #2158 ProcessGateResultReceipt HTTP route + #2159 storage access validation — NARROW scope, branch `docs/sync-storage-process-slices`):
     Records two runtime slices merged to `origin/main` since the post-#2129 block below. This is a **narrow** sync covering only #2158 and #2159; it deliberately does NOT reconcile the intervening #2134–#2157 truth-layer/ops window (invariants catalog, document-registry refresh, ops-MCP fixes, the `ICN_OPERATING_MODEL.md` doctrine #2139), which remains for a separate sync.

     WHAT LANDED (squash tips on `origin/main`, oldest→newest):
       - #2158 (`839a93b4`) feat(process): **mount `ProcessGateResultReceipt` over HTTP** — adds ONE governance route `POST /gov/domains/{domain_id}/process-sessions/{session_id}/gate-results` that records a typed process-gate result through the pre-existing `GovernanceManager::record_process_gate_result` and returns the persisted receipt (deterministic blake3 `record_hash`). The receipt class, manager method, and receipt-store record kind already existed (#1755/#1759); this PR adds only the HTTP surface, reusing the existing `governance:write` scope + domain-membership gate. **No new receipt type, no process runtime, no auth-decision change.** The PR body used `Refs #2144`; **#2144 is independently verified `CLOSED` (`COMPLETED`) in the issue tracker** (closed at the #2158 merge, 2026-06-22).
       - #2159 (`6acb666a` = current tip) feat(kernel-api): **wire storage access validation** — adds `StorageSpec { class, locality }` and `validate_storage_access(task, data, canonical_output) -> Result<(), StorageValidationError>` to `icn-kernel-api` (re-exported from the crate root), making the previously-decorative `StorageValidationError` taxonomy callable. `ComputeTask::validate()` now enforces the canonical-output rule via the helper — a Canonical-determinism task that declares a non-Canonical `storage_class` is rejected, reached live through `ComputeActor::handle_submit` — replacing a prior SHOULD-only no-op. Fires only when `storage_class` is explicitly declared (the `None`-default behavior is preserved). All three `StorageValidationError` variants are reachable through the helper in kernel unit tests. **Closes #2143** (the issue #1131 enforcement gap; `docs/state/storage-governance-spec.md` status updated in the same PR). This is a **storage-governance enforcement slice, NOT a storage backend redesign.**

     MEANING FIREWALL (verified): `canonical_output` is caller-supplied; no domain types or thresholds enter `icn-kernel-api`. The required `Kernel Forbidden Dependencies` + `Meaning Firewall Check` gates passed on #2159.

     ISSUE EFFECTS (verified against the issue tracker, not inferred from PR keywords): **#2143 CLOSED `COMPLETED`** (by #2159's `Closes #2143`). **#2144 CLOSED `COMPLETED`** (verified; the underlying runtime is the #1755/#1759 receipt emission/persistence plus the #2158 HTTP surface).

     This sync explicitly does NOT claim: production / pilot / organizer / federation readiness; a complete storage system or storage backend redesign; encrypted/distributed storage; a workflow engine or complete Institutional Process Substrate; CCL runtime; InstitutionalDomain/DomainPolicy; any auth-model change or entity-aware auth cutover. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. The non-required `Security Audit` (cargo-audit dependency backlog) and `Compare Against Base` (benchmark variance) checks were red on #2159 and did NOT block merge — neither is in the branch-protection required set, and #2159 changed no dependency manifest/lock. -->

<!-- [sync edit] 2026-06-21 (post-#2129 truth-layer/control-plane window — main HEAD `b63dc13c`: agent context spine + live-state overlay + generated-truth drift gate + convergent file-record check — docs/control-plane + generated-orientation + CI tooling only, branch `docs/state-truth-refresh-2128-2133`):
     Records the five PRs merged on `origin/main` after the #2129 sync tip (`git log d444f945..b63dc13c`, exclusive): #2128, #2130, #2131, #2132, #2133. **No runtime Rust in this window** — every change is a generated orientation artifact, its on-demand generator, a CI drift gate, or a docs note. No schema, no contract URN, no ADR, no RFC, no ADR-0026 receipt class, no kernel/gateway API change, no auth-decision change, no K3s/DNS/Forgejo/GitHub-settings mutation, no NYCN partner data; no production-readiness / live-federation / formal-NYCN-pilot / Phase-2-completion claim.

     ⚠️ CORRECTION to the #2129 sync block below: at its write-time (squash `d444f945`, merged moments before #2128) it recorded the Agent Context Spine v0 as still living on a feature branch and not yet on the main line. **#2128 has since MERGED (squash `41c7082b`) and the spine artifact, its checker, and the `icn_ops_agent_context_spine` MCP tool are on `main`** — verified this pass: `python3 scripts/check-agent-context-spine.py` exits 0 (131 nodes / 92 edges, no drift). Do not read the #2129 block's spine note as current; this block supersedes it.

     ⚠️ HARD INVARIANT (verified, unchanged from the #2129 + #2111 syncs): **no enforced authorization decision changed in this window, and no auth code was touched at all.** `require_coop_access` remains flat `claims.coop_id == coop_id` string equality; the entity-aware treasury path stays OBSERVE-mode; the #2121 `DenyUntilWired` `TokenAuthoritySource` seam (recorded in the #2129 block) is unchanged and still wired to NO production issuance route.

     WHAT LANDED (squash tips on `origin/main`, oldest→newest):
       - #2128 (`41c7082b`) chore(agent): **Agent Context Spine v0** — a generated, evidence-grounded repo-orientation artifact `docs/reference/project-index/generated/agent-context-spine.json` (`Canonical: no`; 131 nodes / 92 edges, every node/edge carrying a source-of-truth + evidence path), its stdlib generator `scripts/generate-agent-context-spine.py`, validator `scripts/check-agent-context-spine.py`, and the `icn_ops_agent_context_spine` MCP tool. Orientation/reference only — NOT a truth root.
       - #2130 (`75976ebb`) docs(project-index): **refresh the generated repo file-record snapshot** `docs/reference/project-index/generated/icn-file-record.{json,md}` after the #2124/#2125 website-mirror cleanup. Generated-only commit (regenerated by `generate_repo_record.py`, never hand-edited). This is the snapshot-refresh side of issue **#2126**.
       - #2131 (`80a18c24`) docs(ai): **Live-State Overlay session-start grounding** — `scripts/generate-live-state-overlay.py`, an on-demand stdlib generator (13 required sections, every claim source/freshness-bound, no committed snapshot) broadened into a whole-repo/project orientation layer. **Closes issue #2115** (overlay activation).
       - #2132 (`0b336839`) ci(docs): **observational generated-truth drift gate** — `.github/workflows/generated-truth.yml` runs spine + plugin + plugin-root + overlay `--check` + `generate_repo_record.py --check`, plus the reference doc `docs/ci/GENERATED_TRUTH_DRIFT.md`. Drift is `::warning::` and the job SUCCEEDS; it only FAILS if a checker itself errors (`generate_repo_record.py --check` exit ≥2, distinct from `1`=stale). Not a branch-protection-required check — does not block merge.
       - #2133 (`b63dc13c` = current tip) fix(project-index): **make the file-record check convergent** — `generate_repo_record.py --check` now excludes the generator's own canonical outputs from inventory and normalizes generation-moment metadata (`generated_at`/`branch`/`head`/`working_tree_dirty`) so `--check` compares CONTENT only. Removes the structural blocker that a committed artifact can never record the SHA of the commit that contains it. Regenerated `icn-file-record.{json,md}`. This is the convergent-check side of issue **#2126**.

     ISSUE EFFECTS: **#2115 CLOSED** (by #2131). **#2126** is satisfied in substance by #2130 (snapshot refresh) + #2133 (the `--check` now converges) — pending a maintainer close-with-evidence. STILL-OPEN, unchanged by this window: #2112 route-inventory/OpenAPI/public-API proof-level tagging; #2113 role-based `icnctl` command map; #2114 invariants catalog; #2047 architecture-freshness (the §04-eight-primitives + §18-institution-primitives sections were reviewed-and-corrected this pass — see the freshness change in this branch); #2041 human accessibility (assistive-technology) pass; #1703 NYCN organizer presentation / first operator rehearsal (the Phase 2 human gate, in the partner `nycn` repo); #1955 CI sled disk-space flake.

     TRUTH BOUNDARY (the point of this window): every artifact added here — the agent context spine, the file-record snapshot, the live-state overlay, the drift gate — is `Canonical: no`. They are **orientation / reference layers that navigate TOWARD canonical state; they are NOT truth roots and must not exceed it.** Canonical project state remains `docs/STATE.md` + `docs/PHASE_PROGRESS.md`. This window adds session-start grounding and makes generated-artifact drift visible in CI; it changes neither phase posture nor any enforced behavior.

     This sync explicitly does NOT claim: production readiness; live federation; a formal NYCN pilot; Phase 2 completion; that entity-aware auth is enforced; that `entity_id`/`entity_type` claims are authoritative; treasury enforcement cutover; a production `/auth/verify` trusted positive issuance path; first-admin bootstrap fixed; OpenAPI completeness; or that member-shell / the July Demo Candidate 0.1 is organizer- or pilot-ready (the #2041 human AT pass remains owed). `docs/PHASE_PROGRESS.md` receives only a one-line post-June-14 truth/control-plane note — no phase-posture change. Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-21 (post-#2111 truth-sync → main HEAD `880b85db`: route-inventory lane + #2121 trusted token authority seam + portable agent pack — docs/control-plane only, this PR, branch `docs/state-truth-sync-2116-2127`):
     Records the eleven PRs merged on `origin/main` after the #2111 tip (2026-06-20 → 2026-06-21) that this doc had not yet captured (`git log 8513fbd5..880b85db` — #2111's squash tip, exclusive: #2116–#2125 plus #2127, since #2126 is an issue not a PR). **Mixed truth class, but only ONE is runtime Rust (#2121)**; the rest are generated-map + CI + public-claim-grounding docs, website cleanup, and additive agent tooling. This is a **docs/control-plane truth-sync** — no Rust, no schema, no contract URN, no ADR, no RFC, no ADR-0026 receipt class, no kernel/gateway API change, no K3s/DNS/Forgejo/GitHub-settings mutation, no NYCN partner data; no production-readiness / live-federation / formal-NYCN-pilot / Phase-2-completion claim.

     ⚠️ HARD INVARIANT (verified, unchanged from the #2111 + 2026-06-17 syncs): **no enforced authorization decision changed in this window.** `require_coop_access` remains flat `claims.coop_id == coop_id` string equality; the entity-aware treasury path stays OBSERVE-mode; the new authority seam ships fail-closed and is wired to **NO** production issuance route (no `/auth/verify`, invite, session, SDIS-enrollment, or bootstrap caller consults a `TokenAuthoritySource`).

     AUTH — #2080 lane PR2 (#2121 `feat(auth): add trusted token authority source seam`, squash `bbc82566`): real gateway code, additive + fail-closed. Adds `icn/crates/icn-gateway/src/token_authority.rs`:
       - `TokenAuthoritySource` async trait (`can_issue_entity_token(subject, coop_id, entity_id, scopes) -> IssuanceAuthorityDecision`); the decision is returned BY VALUE (`Allow { basis }` / `Deny { reason }`), so a deny — or an unhealthy/unwired source — can never be mistaken for a mint.
       - `DenyUntilWired` — the **only** production source shipped; denies every request with `NoTrustedSourceWired` and reads none of its inputs. `IssuanceAuthorityBasis::{Membership, Standing, Bootstrap, AuthorityGrant, TestOnly}` names future positive sources; only `TestOnly` is ever constructed (by test doubles).
       - `AuthManager::issue_entity_token_checked(...)` — checked counterpart to #2111's raw `issue_entity_token`; mints only on `Allow`, else returns 403 `AuthorizationFailed`. **Not wired into any issuance route in this PR.**
       This advances #2080 beyond PR1 (#2111), but **#2080 itself REMAINS OPEN**: the production positive `/auth/verify` trusted-issuance replacement and the first-admin bootstrap trusted path are NOT built. **First-admin bootstrap is NOT fixed** (not verified in current code). A real positive source (likely `icn-entity` membership, converging on the source `require_entity_access` already enforces) waits on a wired `coop_id → EntityId` resolver — see RFC-0018 (#2074; spec #2061) and ADR-0035 ("claim vs lookup — we choose lookup; fail-closed only on a wired resolver"). Status: `implemented but partial` for the fail-closed seam; positive issuance is `docs-only / design-direction`.

     ROUTE-INVENTORY LANE (#2116–#2123; tracks issue #2112): a **generated, non-canonical** orientation artifact plus its CI guard and public-claim grounding, all landed after #2111.
       - #2116 (`ab3cf611`) add `docs/reference/project-index/generated/route-inventory.md` (`Canonical: no`; generator `docs/scripts/route_inventory.py`); #2118 (`06cf308b`) expand coverage; #2119 (`4b47261a`) surface OpenAPI paths unmatched to a discovered route; #2120 (`3e3dfdec`) + #2123 (`9138f51b`) cross-reference the governance-app route-registration candidates and ground public-surface claims against the inventory.
       - #2117 (`7f241718`) + #2122 (`3996e6a2`) CI: a `route_inventory` job in `.github/workflows/docs-freshness.yml`, also triggered on governance source. **Drift is `::warning::`, non-blocking** (only a broken checker fails CI).
       - Current generated counts (verify with `python3 docs/scripts/route_inventory.py --check`, exit 0 = fresh; **do not hand-edit** — rerun the generator): **287 gateway route macros · 5 OpenAPI-documented paths (~0.7%) · 80 governance-app registration candidates · 4 non-macro gateway candidates.** This proves route **declarations exist in source** at a snapshot; it does NOT prove auth, mounting, tests, runtime health, or production readiness. **OpenAPI coverage is partial — do NOT claim the API is documented / complete / fully specified.**

     WEBSITE CLEANUP (#2124 `287794e3` / #2125 `6e2379cb`): remove the obsolete `src/content/docs` mirror and drop its stale row from `website-truth-map.md`. Website/docs only.

     AGENT TOOLING: #2127 (`880b85db`, current tip) merged the **portable Claude Code agent pack plugin** (`tools/claude-code/plugins/icn-agent-pack/`; skills `authority-spine, doctor, navigator, preflight, route-impact, truth-sync`). Additive; does NOT replace `.claude/`. Its structure validators (`scripts/check-claude-plugin.py`, `scripts/check-claude-plugin-root-resolution.py`) pass on main but are **not yet wired into CI** (only `check-mcp-portability.py` is, via `mcp-portability.yml`). **Agent Context Spine v0 (#2128) is OPEN, not merged** — its generator/checker/MCP tool/`agent-context-spine.json` live only on branch `chore/agent-context-spine-v0` and are **not present on `main`**; do not cite them as on-main.

     STILL-OPEN (issues, not merged work): #2112 route-inventory/OpenAPI/public-API claims; #2113 role-based `icnctl` command map; #2114 invariants catalog; #2115 live-state overlay activation; #2126 generated repo-file-record refresh; #2047 architecture-freshness / stale sections; #2041 human accessibility (assistive-technology) pass; #1703 NYCN organizer presentation / first operator rehearsal — the **Phase 2 human gate, which lives in the partner `nycn` repo** (there is no in-repo `docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md`).

     This sync explicitly does NOT claim: production readiness; live federation; a formal NYCN pilot; that entity-aware auth is enforced; that `entity_id`/`entity_type` claims are authoritative; treasury enforcement cutover; a production `/auth/verify` trusted positive issuance path; first-admin bootstrap fixed; OpenAPI completeness; or that member-shell / the July Demo Candidate 0.1 is organizer- or pilot-ready (the #2041 human AT pass remains owed). `docs/PHASE_PROGRESS.md` is intentionally untouched — no phase-posture change (matching the #2111 STATE.md-only precedent). Phase 2 status remains ⏳ (partner-bound); phase model unchanged; the #1703 human gate is unchanged. -->

<!-- [sync edit] 2026-06-20 (RFC-0018 token-claim groundwork — optional NON-ENFORCING `entity_id`/`entity_type` claim + mint seam — PR #2111, #2080 lane PR1, branch `feat/2080-entity-id-token-claim` off `origin/main` HEAD `d2aa7ecb`):
     PR1 of the #2080 lane (trusted positive token issuance), building the RFC-0018 (#2074; spec #2061) migration rail the prior #2079 observe-mode slice deferred. **Real gateway code** (two optional struct fields + one mint helper) — additive and non-enforcing. **Supersedes the precondition recorded in the 2026-06-17 sync below** ("this slice does not include an `entity_id` token claim"): the optional claim shape now EXISTS but remains non-authoritative and unpopulated. No new ADR, no new RFC, no new contract URN, no new ADR-0026 receipt class, no NYCN partner data, no K3s/DNS/GitHub-settings mutation, no production-readiness claim.

     ⚠️ HARD INVARIANT (verified): **no authorization decision changes.** `require_coop_access` remains flat `claims.coop_id == coop_id` string equality and ignores the new claim; treasury stays OBSERVE-mode. No guard reads `entity_id`.

     WHAT LANDED:
       - `TokenClaims` (`icn-gateway/src/auth.rs`) gains optional `entity_id` / `entity_type` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) — pre-#2080 tokens still decode; legacy mints stay byte-identical (keys omitted when `None`).
       - `AuthManager::issue_entity_token(did, coop_id, Option<EntityId>, scopes)` mint seam; `issue_token` delegates with `None`. `entity_type` is derived from the canonical `EntityId` so the two claims cannot disagree.
       - SAFETY: no production caller passes `Some(..)` yet; the dev/self-asserted path (`verify_challenge` → `issue_token`) always mints `None`, so a self-asserting DID cannot fabricate typed entity authority a later enforcement slice would trust. This is why it does NOT recreate the #2075 self-asserted-claim problem ADR-0035 guards against.

     EVIDENCE: `cargo test -p icn-gateway --features sled-storage` lib 617 + all integration suites green (incl. 6 new claim tests: back-compat decode, legacy-mint-`None`, typed round-trip, `None`-equals-legacy, dev-path-never-mints, `require_coop_access`-ignores-`entity_id`); `cargo clippy -p icn-gateway --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.

     This sync explicitly does NOT claim: any change to enforced authorization; population of the claim from membership/standing (no trusted caller passes `Some` yet); a canonical/persisted `coop_id ↔ EntityId` mapping wired into issuance; treasury enforcement cutover (#2081); first-admin bootstrap replacement; production readiness; a formal NYCN pilot. Tracked follow-ups: populate `entity_id` from a trusted source (PR2); membership/standing-backed issuance after resolving the authority-source fork (PR3); first-admin bootstrap trusted path (PR4); then #2081 treasury enforcement cutover. Phase 2 status remains ⏳ (partner-bound); phase model unchanged. -->

<!-- [sync edit] 2026-06-17 (RFC-0018 first slice — entity-aware authorization primitive + treasury OBSERVE-mode wiring — this PR, branch `feat/entity-access-treasury-observe` off `origin/main` HEAD `7eba7f8a`):
     First implementation slice of RFC-0018 (#2074; spec #2061), the entity-aware request-authorization model, following the #2075/#2077 self-asserted-coop issuance fix. **Real gateway code** (a new authorization primitive + observation wiring + one metric) plus a new ADR. Adds **ADR-0035** (`docs/adr/ADR-0035-entity-aware-request-authorization.md`, status accepted, implementation_status partial). No new contract URN, no new RFC, no new ADR-0026 receipt class, no NYCN partner data, no K3s/DNS/Forgejo/GitHub-settings mutation, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim.

     ⚠️ HARD INVARIANT (verified): **no live treasury authorization decision changes in this slice.** The flat `require_coop_access` guard remains the sole *enforced* gate on every treasury endpoint (all 10 `require_coop_access` call sites intact, none removed — grep-verified against `origin/main`). The entity-aware path runs ALONGSIDE it in **observation-only** mode and can never deny a request.

     WHAT LANDED:
       - `require_entity_access(entity_mgr, caller, target, action)` in `icn-gateway/src/authority.rs` — generalizes the existing live `require_entity_write_access` (`api/entity.rs`). `EntityAction` is intentionally minimal (`ModifyEntity` | `TreasuryRead` | `TreasuryWrite`). Authority bases are heterogeneous and deliberate: `ModifyEntity` = role threshold (`Founder`/`BoardMember`), preserved role-only with no active-standing gate to exactly match the historical behavior; `TreasuryWrite` = `TreasuryAccess` **capability** (not a role-name shortcut); `TreasuryRead` = active membership.
       - `require_entity_write_access` now **delegates** to the primitive (behavior-preserving; existing entity-write tests green).
       - Treasury OBSERVE-mode wiring: all 10 treasury handlers compute `observe_treasury_entity_access` AFTER the flat guard + treasury load, recording agreement/divergence via the new `icn_gateway_entity_authz_observation_total{family,action,result,reason}` counter (bounded labels; no per-coop cardinality). The observation result is discarded — structurally incapable of denying. Caller entity is resolved **at the guard** from `claims.sub` (DID → `EntityId::from_did`); it is NOT carried as a token claim (avoids recreating the #2075 self-asserted-claim trust).
       - `legacy_coop_id_to_entity_id_fallback` (`icn-gateway/src/entity_map.rs`): a loudly-second-class, reject-not-normalize projection used only when a treasury has no stored `entity_id`. `coop_A` is rejected, never normalized into the distinct `coop-a` (collision guard).

     EVIDENCE: `cargo test -p icn-gateway` 600 lib tests pass (incl. 12 new entity-access tests — 7-case primitive matrix with capability-beats-role + 5 observe-mode tests incl. "entity-deny is observation-only, never denies"; 8 fallback rejection-matrix tests) + integration/doc-tests green; `cargo clippy -p icn-gateway -p icn-obs --all-targets -D warnings` clean; `cargo fmt` clean.

     This sync explicitly does NOT claim: any change to enforced authorization; an `entity_id` token claim; a canonical/persisted `coop_id ↔ EntityId` mapping (fallback is best-effort only); migration of any other endpoint family off the flat guard; delegation / federation / community cross-entity authority (RFC-0018 Step 5); production readiness; a formal NYCN pilot; live federation; multi-person governance. Tracked follow-ups (filed with this PR): trusted positive issuance path / first-admin bootstrap; treasury enforcement cutover; canonical `coop_id ↔ EntityId` mapping/backfill; SDK auth docs; Community Proof Spine 0.1. Phase 2 status remains ⏳ (partner-bound); phase model unchanged. -->

<!-- [sync edit] 2026-06-14 (July Demo Candidate 0.1 — accessibility / language-access / hand-off window, PRs #2037, #2043, #2039, #2040 on `origin/main` HEAD `687625cd`):
     Continuation of the prior 2026-06-14 sync (which was PR #2038, recording window #2021 → #2036 at HEAD `c0845c0e`). Records the four PRs that landed after #2038 — #2037, #2043, #2039, #2040 (not a contiguous numeric range; #2043 sorts after #2040) — all merged 2026-06-14; the post-#2038 merge order was #2037, then #2043 → #2039 → #2040. **Mixed truth class**: one small dependency-free web feature (the i18n seam) plus demo-reliability, accessibility-evidence, and hand-off docs. No new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no kernel/gateway domain-import widening, no K3s/DNS/Forgejo/GitHub-settings mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim.

     The window (squash tips on `origin/main`):
       - #2037 fix(demo) (de572997): make the nycn-dogfood gossip-port preflight reliable when `ss` is absent — `lsof`/local-bind fallback, a loud warning instead of a silent skip, a local-only `free_port` filter, and a README prerequisite note. Demo-reliability only; no protocol or gossip behavior change.
       - #2043 feat(web) (a573801d): **member-shell i18n / language-modularity seam** (`web/member-shell/i18n.js`, contract `docs/spec/member-shell-i18n-v0.md`; closes #2042). Dependency-free, no build step, no network. This is the **infrastructure for language, not a set of translations**: `en` is the source-of-truth catalog and the per-key fallback target; `qps-ploc` is a generated pseudo-locale used as an extraction-coverage test; `ar` ships an intentionally empty catalog that demonstrates `dir=rtl` mirroring while falling back to English (its document `lang` stays `en` so assistive tech is not told the fallback English is Arabic). **Real translations remain owed** — adding a language is a catalog entry, not a code change. Regulatory-safe vocabulary preserved (no payment/wallet/balance/currency/token vocabulary in the catalog).
       - #2039 docs(demo) (eb8355ea): **automated rendered-browser accessibility walkthrough evidence** (`docs/demo/JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md`) plus the reproducible audit harness `web/member-shell/a11y-walkthrough.cjs` (Playwright + `@axe-core/playwright` + headless Chromium; a dev/audit tool, not shipped, not loaded by `index.html`). This is **machine-generated evidence only** — it does not substitute for a human assistive-technology pass. **#2041 (human screen-reader + keyboard-only + zoom + contrast + switch/non-pointer pass) remains OPEN and owed.**
       - #2040 docs(demo) (687625cd = current tip): **July Demo Candidate 0.1 release / hand-off packet** (`docs/demo/JULY_DEMO_CANDIDATE_0.1_RELEASE_PACKET.md`). "Release / hand-off" means a **review and operator hand-off document, NOT a software release** and NOT a partner-distributable artifact. It references the merged i18n seam (#2043, infra-only — translations owed) and the accessibility evidence (#2039), and preserves the truth boundary plus the build-time-vs-operate-time network caveat (building/staging the appliance image may need network access for OS packages and dependency fetches; operating an already-staged local image can be local/offline-ish). Forbidden-claims grep clean at merge.

     What this window changes about the candidate's honest description: **language access is now represented as infrastructure, not as completed multilingual coverage**; accessibility has **automated rendered-browser evidence but not yet a human AT pass**; and the candidate now has an explicit operator hand-off packet. It does NOT change the proof spine, the seal, or any maturity claim.

     This sync explicitly does NOT claim: production readiness; a formal NYCN pilot; partner deployment or partner-distributable artifacts; NYCN approval / adoption / activation; live federation; multi-organization or multi-person governance (the proof spine is still single-actor — no two-member flow exists); private-data handling; signed/immutable production receipts; completed translations; or a human accessibility validation (the #2041 gate is open). Fixture/local/dev data only.

     Phase 2 status remains ⏳ (still partner-bound). This window strengthens the demo's accessibility evidence, language-access seam, and operator hand-off; it does NOT satisfy the #1703 human gate (organizer presentation → explicit proceed/revise/defer/reject → pilot formalization → first operator rehearsal), which is unchanged. Next move is **not selected here**; optionality preserved (a two-member action flow remains a named, not-started candidate in `~/icn-dev/state/merge-queue.md`). Phase model unchanged. -->

<!-- [sync edit] 2026-06-14 (July Demo Candidate 0.1 truth sync — local proof-spine demo milestone, window #2021 → #2036 on `origin/main` HEAD `c0845c0e`):
     Truth-sync recording the July Demo Candidate 0.1 work that landed 2026-06-10 → 2026-06-13, after the prior 2026-06-10 sync (which closed at #2020). **Mixed truth class**: a new reference web client plus gateway/appliance demo plumbing (real code) alongside demo/operator docs and one CI guard. No new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no kernel/gateway domain-import widening, no K3s/DNS/Forgejo/GitHub-settings mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim.

     WHAT THE JULY DEMO CANDIDATE 0.1 IS: a **local, single-actor proof spine** a human can drive end to end — standing → action card → discharge/completion → completion receipt → local evidence/audit verification — surfaced through a reference member-shell client and packaged as a single-VM DEV/DEMO appliance image. It is a **demo/dev milestone, not a production or pilot milestone.** All data is fixture/local/dev; the live-local 13/13 receipt-chain proof runs against a single local daemon with a dev-gated self-trust seed (`ICN_DEV_SELF_TRUST`; `min_trust_for_entry` stays 0.1 and enforced), so its receipts are dev-attested, not production-signed.

     The window (squash tips on `origin/main`):
       - #2021 ci (c86bbd95): guard rehearsal fixtures against drift — fixture_bundle job + fail-closed summary gate. Merged 2026-06-10 after the prior sync's write-time; recorded here for completeness.
       - #2022 docs(state) (e056fae6): one-line rehearsal-script path fix (`scripts/`, not `demo/scripts/`).
       - #2023 chore(docs) (246291e4): redact expired local dev tokens from archived session docs + sanitize-ok marker on a synthetic test fixture.
       - #2024 fix(demo) (e1c22a55): dev-gated member-standing bootstrap for the multi-node governance demo (standing-enforcement drift had broken run-all); dev-gate only, no production auth/standing weakening.
       - #2025 docs(demo) (515c5299): rewrite `demo/SELF_SERVE.md` as a local-attendee quickstart with explicit maturity tiers (fixture / 13-of-13 / dogfood / devnet); removes the cluster/internal-IP dependency from the attendee path.
       - #2026 feat(web) (0491b104): **member-shell v0 reference client** at `web/member-shell/` (static `index.html` + `shell.js` + `shell.css` + `fixtures/`). Fixture mode and live-local mode. Renders member **standing**, **action cards**, and **completion receipts**. 14 review findings fixed; post-merge same-day sanity PASS (13/13, run-all 2/2, static smoke 6/6, shell.js syntax OK). Reference client only — not a shipped product, not an organizer/steward shell, not multi-person.
       - #2027 feat(gateway) (434a5511): **member-shell surface documented in the served OpenAPI** (`docs/api/openapi.generated.yaml`, `icn/crates/icn-gateway/src/openapi.rs`) — standing, action-cards, and completion-receipt endpoints on the `/v1` base. Drift-chain regen (Cargo.lock + generated TS types) included. Meaning-firewall ratchet held; no new typed governance import.
       - #2028 feat(appliance) (f4c68e68): **DEV/DEMO appliance image profile** — the member loop end to end inside one VM. 11 review findings across 5 Codex/Copilot rounds individually re-verified (endpoints, reset marker, verify-honesty wording, `vocab-ok:` marker, seed verify hint, `jurisdiction_id` standing bootstrap with fail-closed smoke check, build-time jsonschema assert, dual-origin CORS preflight). Post-merge from-main image `de2e9a2a…` passed `check.sh` 20/20 + build + `smoke --real --demo` + in-VM seed/discharge/verify/reset/reseed + 13/13 chain (logs under `~/artifacts/icn/demo-image-20260612/`). The image is a DEV/DEMO profile — **not signed, not immutable, not partner-distributable, not a production node**.
       - #2030 docs(reference) (7952aef5): refresh the proof-level capability matrix for the candidate — member surface now in served OpenAPI; new single-actor DEV/DEMO appliance-image row (single-actor L5); image-vs-running-instance-vs-hypervisor terminology note.
       - #2031 feat(appliance) (170b53f8): **one-command demo launcher + no-paste session flow** (`open-proxmox-demo.sh`, `icn-demo-session.py`, member-shell "Start local demo" button) so an operator does not paste a credential or type a gateway URL; includes a security fix routing demo helpers through `runuser` so the keystore passphrase no longer leaks via the sudo journal.
       - #2032 docs(demo) (69760a05): harden the demo rehearsal recovery docs (HANDS_ON §12 + DEMO_QUICKSTART panic/fallback rows).
       - #2033 fix(appliance) (23c684b5): correct the launcher port-conflict guidance message (text only, no behavior change).
       - #2034 docs(demo) (1a78d70a): **`docs/demo/JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md`** — the operator + reviewer handoff runbook, cross-linked from JULY_DEMO_OPERATOR_CHECKLIST, DEMO_QUICKSTART, and the capability matrix.
       - #2035 docs(demo) (54d5cff0): route `docs/demo/README.md` (the demo index) to the four July Candidate 0.1 docs with DEV/DEMO honesty labels; older one-click/devnet sections left intact.
       - #2036 docs(demo) (c0845c0e = current tip): clarify the reset and reseed paths — reset proves nothing and does not reseed; the launcher path is recommended (reload the launcher URL first, since the Start button hides after the first session); the manual `seed --json` fallback prints a local DEV credential kept off public artifacts; "fresh session," not "fresh member."

     SEAL: the from-main demo image `de2e9a2a…` was sealed as **ICN July Demo Candidate 0.1** on 2026-06-13; the seal + full PASS-line table live in `~/artifacts/icn/demo-image-20260612/EVIDENCE-MAP.md` (operator-run, outside the repo per the evidence-log convention). In-VM 13/13 chain proof confirmed PASS (exit 0). **Release dry-run was NOT run.**

     This sync explicitly does NOT claim: production readiness; a formal NYCN pilot; partner deployment or partner-distributable artifacts; NYCN approval / adoption / activation of anything; live federation; multi-organization or multi-person governance (the proof spine is single-actor); private-data handling; signed/immutable production receipts (the 13/13 path uses a dev-gated self-trust seed and dev-attested receipts); appliance fail-closed coverage beyond the single missing-firstboot-exec scenario verified 2026-06-10; resolved licensing (#1692). Fixture/local/dev data only.

     Phase 2 status remains ⏳ (still partner-bound). The July Demo Candidate strengthens the local proof spine and the human demo path; it does NOT satisfy the #1703 human gate (organizer presentation → explicit proceed/revise/defer/reject → pilot formalization → first operator rehearsal), which is unchanged. Next move is **not selected here**; optionality preserved (named candidate in `~/icn-dev/state/merge-queue.md`: a two-member action flow, not started).

     Hard rule preserved: this sync edit does NOT change any contract field, mint a contract URN, add an ADR/RFC, widen gateway typed governance imports, retire `governance:write`, touch K3s/DNS/Forgejo/GitHub state, handle private partner/member/organizer data, claim Phase 2 completion, claim a formal NYCN pilot, claim production readiness, or claim live federation. Phase model unchanged. -->

<!-- [sync edit] 2026-06-10 (appliance negative firstboot smoke verified — this PR):
     Truth-sync for one narrow appliance slice. **Mixed truth class**: one new operator-run verification (real VM boot evidence) plus the script/docs that produced it. No Rust change, no contract URN, no ADR, no RFC, no ADR-0026 receipt class, no K3s/DNS/Forgejo/GitHub-settings mutation, no NYCN partner data.

     What changed: the appliance proof matrix moves from "positive path verified (#1900); negative fail-closed path NOT verified" to "positive path verified (#1900, re-verified as baseline 2026-06-10); ONE negative fail-closed scenario verified (missing-firstboot-exec, operator-run 2026-06-10)". All other appliance failure modes remain unverified.

     The slice:
       - deploy/appliance/smoke/negative-firstboot-smoke.sh (new): deletes /usr/local/sbin/icn-appliance-firstboot (the ExecStart= of icn-appliance-firstboot.service) from a DISPOSABLE OVERLAY via virt-customize, boots the tampered overlay, and asserts fail-closed: firstboot unit `failed`, marker /var/lib/icn/.firstboot-complete absent, icnd never `active` during the observation window, /v1/health never answering. Any icnd activation or health answer exits non-zero as FAIL-OPEN. Source image never modified.
       - deploy/appliance/smoke/README.md: documents the scenario, prerequisites (virt-customize; readable /boot/vmlinuz-* for libguestfs), and non-claims.
       - docs/dev/handoff-2026-06-10-appliance-negative-firstboot-smoke.md: full operator evidence — host (icn-dev, Ubuntu 24.04.4, kernel 6.8.0-124-generic, QEMU 8.2.2 under TCG), image identity (the same #1900-built artifact, SHA256 re-verified against its manifest: e6888dd512d4...6f51), tamper hashes, command transcript, and journal excerpts.

     Operator-run evidence (2026-06-10, host icn-dev, same artifact image #1900 built):
       - Positive baseline re-run FIRST on the untampered image: PASS (SSH → marker → icnd active → /v1/health 200), so the negative run differs from a passing baseline by exactly one variable (the tamper).
       - Negative run: firstboot failed with status=203/EXEC ("Unable to locate executable"); journal shows the gate's Requires= propagation verbatim — "Dependency failed for icnd.service" / "Job icnd.service/start failed with result 'dependency'"; marker ABSENT; icnd ActiveState=inactive SubState=dead throughout; health never answered. Exit 0 (fail-closed verified).
       - Mechanism note recorded honestly: the boot-path block came from Requires= failure propagation; the ConditionPathExists= marker belt was not reached on the boot path (start job cancelled first) and is what would block a later manual start.

     This sync explicitly does NOT claim: appliance fail-closed certification beyond the single verified scenario (tampered-but-present firstboot script, corrupted /etc/icn inputs, partial identity material, identity-init warn-and-continue path, disk-full, clock-skew all remain unverified); production readiness; signed/immutable/partner-distributable images; live federation; formal NYCN pilot; Phase 2 completion. Phase 2 status remains ⏳ (partner-bound); the #1703 human gate is unchanged.

     Candidate-list delta vs the prior sync block: candidate (a) "appliance negative firstboot smoke" is now DONE (this PR). Candidates (b) Dependabot queue triage, (c) #1868 broad-fallback retirement criteria, (d) #1703 human gate, (e) issue hygiene #1704/#1727/#1728, (f) strategy-doc deep refresh carry forward unchanged. Next move is **not selected here**; optionality preserved.

     Hard rule preserved: this sync does NOT change any contract field, mint a URN, add an ADR/RFC, widen gateway typed governance imports, migrate handlers, retire governance:write, touch K3s/DNS/Forgejo, handle private partner data, or claim Phase 2 completion / NYCN pilot / production readiness / live federation. One documented host-state change on icn-dev (chmod 0644 of the running kernel image for libguestfs, per the #1900 precedent) is recorded in the handoff. Phase model unchanged. -->

<!-- [sync edit] 2026-06-10 (post #1903 → #2016 window, 91 commits, plus gap correction #1843 → #1874):
     Truth-sync recording two windows against `origin/main` HEAD `9012ba5c`. **Mixed truth class** — substantial real Rust runtime work plus docs/control-plane. No new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no K3s/DNS/Forgejo/GitHub-settings mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim.

     ── PART A — gap correction (2026-05-15 → 2026-05-17, 17 PRs, previously unrecorded) ──
     The 2026-05-22 sync (#1902) verified an external review brief covering #1875 → #1901 and did not sweep the full git window; the following merges between the 2026-05-15 architecture-sprint sync (#1841/#1842) and #1875 were never recorded in STATE.md until now:
       - #1843 schema(network): AntiEntropyProbe + StateDigest records (closed #1834).
       - #1844 schema(network): DivergenceEvidence + RepairPlan records (closed #1835).
       - #1845 test(devnet): receipt-index anti-entropy Slice A fixture (closed #1838).
       - #1846 test(devnet): steward cockpit divergence-render Slice A fixture (closed #1840).
       - #1847 fix(deps): lettre 0.11.19 → 0.11.22 (RUSTSEC-2026-0141).
       - #1848 test(devnet): member shell read-only Slice A fixture (closed #1839). Fixture member identity + LocalDomain, four ActionCards, plain-language receipt summaries, opaque PrivateEvidence rendered as existence + scope + access path only (no body bytes), closed seven-string member sync vocabulary locked verbatim against the spec, ADR-0028 twelve-category accessibility checklist asserted on each rendered view; 21 fixture tests.
       - #1850 schema(network): RepairReceipt record; #1851 test(devnet): Slice A repair fixtures retrofitted to consume it.
       - #1853 schema(network): PeerSyncReport record; #1855 refactor(devnet): receipt-index fixture consumes it.
       - #1857 schema(network): SyncDegradedStatus record (issue #1856); #1859 refactor(devnet): cockpit + member-shell fixtures consume it.
       - #1861 schema(network): QuorumSyncCheck + FederationSyncWindow records.
       - #1863 schema(network): RoutingProof + RedundancyProof records (issue #1862).
       - #1874 test(devnet): RedundancyProof Slice B fixture.
       - #1865 scaffold(deploy): Debian appliance / installable node image substrate (the scaffold the #1879/#1900 appliance docs reference).
       - #1867 docs(design): consolidate design principles and Claude Design setup.
     Effect of Part A on prior beliefs: all three Slice A fixture rehearsals (#1838 / #1839 / #1840) were CLOSED as completed by 2026-05-16 — fixture-backed deterministic proofs exist for receipt-index anti-entropy, member-shell read-only rendering, and steward-cockpit divergence rendering. The "first fixture rehearsal" candidate the 2026-05-22 sync block enumerated as open was already satisfied when that block was written.

     ── PART B — main window (2026-05-23 → 2026-06-10, #1903 → #2016) ──

     (1) governance:write decomposition EXECUTED with accepted-also fallback (#1868 steps 2–6) — **real Rust**:
       - #1903 test: lock accepted-is-not-applied ReconciliationStatus invariants.
       - #1905 authz(gateway): bind decomposed governance scope allowlist to canonical constants (step 2).
       - #1918 migrate charter family to governance:charter:write (step 3); #1919 label direct charter activation as bootstrap path.
       - #1922 migrate assign_role to governance:steward:write (step 4).
       - #1923 migrate federation proposals; #1924 migrate meeting/activity/comment families.
       - #1946 migrate proposal close scope; #1947 migrate HTTP proposal family.
       - #1948 decompose cast-vote + meeting-create gateway scopes; #1949 decompose governance JSON-RPC scopes; #1950 reconcile vote-cast handler authorization; #1951 fix stale `gov:*` icnctl scope defaults to `governance:*`.
       - #1984 cleanup(api): remove dead governance scope constants.
       Verified result state at `9012ba5c`: 42 `require_any_scope` call sites in `icn/apps/governance/src/http/handlers.rs` across all seven families (charter 4, proposal 8, steward 1, federation 1, meeting 12, activity 8, comment 5), each pairing its class scope with broad `"governance:write"` as accepted-also fallback; ZERO bare broad-scope mutation gates remain. Broad `governance:write` is NOT retired — every site still accepts it during the migration; retirement criteria are a separate future decision.

     (2) MandateGate + mandate-attested receipts (#1868 steps 5–6) — **real Rust**:
       - #1925 docs(governance): MandateGate act-time resolver design; #1926 resolver primitives; #1927 MandateGate context guard (`icn/apps/governance/src/mandate_gate.rs`, wired in `configure.rs`, `manager.rs`, `icn-gateway/src/server.rs`); #1928 MandateGrantRef wire primitive.
       - #1929 / #1930 / #1931 mandate attestation added to decision, action-item, and meeting-attendance receipts.
       - #1934 / #1935 v2 meeting-attendance + action-item receipts; #1936 decision-receipt authority + grant-minting design; #1937 / #1938 v3 process-authorized decision receipts.
       MandateGate landing does NOT mean the authority migration is complete: capability scopes still authorize every handler (with broad fallback), and mandate attestation enriches receipts rather than replacing scope checks.

     (3) TrustThreshold fail-open closures — **real Rust**, P0 items from docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md:
       - #1911 resolve TrustThreshold direct membership mutations; #1916 close TrustThreshold fail-open in check_domain_membership (closes #1913); #1917 follow-up assertion/docs cleanup.
       - #1920 / #1921 cover the non-atomic put_mandate_with_grants orphan boundary (closes #1872).

     (4) Receipt-chain audit completion + dispatch-evidence durability — **real Rust**:
       - #1979 feat(cli): authenticate `icnctl audit verify` against secured gateways.
       - #1985 fix(gateway): complete live receipt-chain audit path to 13/13 — `icnctl audit verify --token` reaches 13/13 against the live local daemon. The path includes a dev-gated kernel self-trust seed (`ICN_DEV_SELF_TRUST`; min_trust_for_entry stays 0.1 and enforced) — this is a LOCAL/dev-gated proof, not a partner deployment.
       - #1986 recover incomplete cross-store closes; #1988 page decision ledger lookups; #1993 page dispatch evidence backfill scan; #1996 index ledger entries by decision_hash.
       - #1990 feat(core): durably persist EffectDispatchEvidence across crash/restart using the dispatcher's ExecutionRecord as the durable write-ahead recovery handle (closes #1987; no DispatchEvidenceSink trait change, no effect re-execution).

     (5) Demo / rehearsal layer — **real code, dev-gated**:
       - #1980 local member-standing bootstrap; #1981 dev-gated standing bridge for live gateway demos.
       - #1997 one-command local 13/13 receipt-chain rehearsal (`scripts/local_receipt_chain_13of13_rehearsal.sh`).
       - #1999 fixture-backed rehearsal shell demo mode (relates to open #1727).
       - #1953 / #1954 nycn-dogfood keystone demo kit (work → obligation → receipt loop; receipts persisted, card asserted, receipt binding verified).
       All demo-layer standing/trust shortcuts are explicitly dev-gated; none weakens audit, auth, trust, or ledger gates in production configuration, and none constitutes live federation.

     (6) Vocabulary / meaning-firewall remediation (passport-keyring migration) — **mixed docs + SDK/UI code + CI**:
       - #1957 docs/ci: reinforce meaning firewall around readiness and fintech vocabulary (adds `.github/scripts/readiness_overclaim_linter.py` alongside the compliance linter).
       - #1958 align legacy payment/balance API doc surfaces with ICN semantic primitives; #1960 / #1961 TS SDK surface alignment + example typecheck.
       - #1963 passport/keyring boundary definition; #1964–#1978 migration across UI copy, architecture docs, SDK (keyring aliases for legacy wallet-named APIs, keyring config option, storage-key canonicalization, identity-reset concurrency hardening, deprecation bridge replacement, operator-DID rename, CoopWallet example reframe, member allocation receipt language, node-mode terminology). This is custody-surface and vocabulary remediation — key custody (keyring) and identity (passport) are now named separately; no new custody feature is claimed.
       - #1909 / #1910 React Native validation + deprecation-bridge fixes; #1976 React Native client lifecycle teardown.

     (7) Contracts / spec / index — docs/control-plane:
       - #1998 spec(contracts): pending-publish summary row contract (relates to open #1728).
       - #2000 docs(project-index): proof-level taxonomy + rehearsal capability matrix; #2001 / #2007 INDEX.generated.md drift-mirror regenerations.
       - #1952 docs(summit): workshop proposal claims aligned to verified runtime.

     (8) Introduction / strategy materials — docs:
       - #2002 honest ICN introduction + hard-question materials (ICN_FOR_COOPERATIVE_MOVEMENT.md, ICN_FOR_EVERYONE.md, ICN_HANDBILL.md, ICN_HARD_QUESTIONS.md, ICN_INTRODUCTION_EVIDENCE_MAP.md — the evidence map binds every introduction claim to merged artifacts and states what each does NOT prove).
       - #2003 Cooperative Codebase meeting brief for the 2026-06-11 call; #2008 hard-questions naming alignment.

     (9) Repo / agent hygiene — docs/process:
       - #2009 agent worktree policy (docs/dev/AGENT_WORKTREE_POLICY.md) + AGENTS.md pointer; #2004 / #2005 / #2011 dead docs/dev-journal pointer fixes in CLAUDE.md and agent-instruction trees; #2010 homelab agent doc path fixes.

     (10) Dependencies / CI / security — maintenance:
       - #1894–#1898 ts-sdk Dependabot batch merged 2026-05-25 (flatted 3.4.2, minimatch 3.1.5, picomatch 2.3.2, fast-uri 3.1.2, dev-deps group) — the queue the 2026-05-22 sync left open is cleared.
       - #1904 / #1994 / #2016 npm bumps (#2016 shell-quote 1.8.4, critical); #1908 react-native handlebars lockfile.
       - #1983 chore(ci): ignore unmaintained pqcrypto-* advisories to unblock main — an explicit accepted-risk posture decision, not a fix; the advisories remain real.
       - #1932 CI runner disk headroom guard + bounded Docker build cache (relates to open #1955).
       - #1906 fix(compute): use public exports in cost doctest (relates to open #1704).
       - #1982 website legacy cooperative URL redirects.

     Issue-state record at sync write-time:
       - #1868 OPEN — but materially advanced far beyond the 2026-05-22 record: steps 1–6 landed (#1881 constants; #1905 gateway allowlist; #1918/#1922/#1923/#1924/#1946/#1947 handler families; #1948/#1949 gateway+RPC; #1925–#1931 MandateGate; #1934–#1938 receipts v2/v3). Remaining: broad-fallback retirement decision and any leftover checklist items.
       - CLOSED in/around this window: #1913 (by #1916), #1872 (by #1920/#1921), #1987 (by #1990), #1834 (by #1843), #1835 (by #1844), #1838 (by #1845), #1839 (by #1848), #1840 (by #1846).
       - OPEN but possibly satisfied by landed work (verification + closure is a maintainer call, not claimed here): #1704 (vs #1906), #1727 (vs #1999), #1728 (vs #1998).
       - Newly OPEN: #1955 (CI disk-space flake), #1956 (nycn-dogfood gossip-port preflight skips silently without `ss`).
       - Unchanged OPEN gates: #1703 (NYCN organizer presentation → pilot formalization → first operator rehearsal), #1746, #1748, #1692 (licensing), #1742 ($id review by 2026-06-30), #1837, #1836.
     Open PRs at sync write-time: #2018 / #1995 (npm dev-deps groups), #1940–#1945 (Rust deps — NOTE: #1944 rand 0.8.5 → 0.9.4 is a breaking major and #1942 ml-dsa 0.1.0-rc.4 → 0.1.1 touches the post-quantum surface; neither is a fast-track candidate), #1907 (icn.zone dev-portal scaffold, human-authored).

     Cross-cycle disciplines preserved verbatim:
       - No new ADR, no new RFC, no new contract URN, no new ADR-0026 receipt class (the v2/v3 receipt work versions existing governance receipt envelopes; the network schema records are devnet/fixture-level records, not ADR-0026 classes).
       - The meaning firewall is not widened: scope constants stay kernel-side enforcement primitives; MandateGate lives in apps/governance; the gateway gained no typed governance imports.
       - Settlement / position / obligation / allocation / unit / receipt / provenance / evidence — never payment / wallet / currency / balance / token / crypto / blockchain / timebank — for ICN-native flows. The passport/keyring migration (stack 6) actively remediates legacy wallet-named surfaces toward this discipline.
       - Privacy is posture, not content. PrivateEvidence body bytes never reach any rendering layer (#1848 asserts this at fixture level).
       - When cockpit shows degraded, member shell must show degraded too (#1859 wires both fixtures to the shared SyncDegradedStatus record).

     Phase 2 status remains ⏳ (still partner-bound). Nothing in either window completes a phase, removes a partner-binding gate, activates NYCN, or implements live federation. The 13/13 receipt-chain proof, MandateGate, scope decomposition, fixture rehearsals, and introduction materials all strengthen Phase 2 machinery and presentation honesty; the human gate (#1703: organizer presentation → pilot formalization → first operator rehearsal) is unchanged.

     This sync explicitly does NOT claim: Phase 2 completion; formal NYCN pilot; production readiness; live federation; an implemented Civic Shell, member shell, or steward cockpit (fixture-level proofs only); retirement of broad governance:write; a verified appliance fail-closed firstboot path (the #1900 negative-path gap is STILL open); signed/immutable/partner-distributable appliance images; resolved licensing (#1692); partner data handling.

     Truth-class corrections recorded by this sync:
       - The 2026-05-22 sync block's candidate enumeration listed "first fixture rehearsal (#1838/#1839/#1840)" among open candidates; all three were already closed by then (Part A above).
       - docs/strategy/ICN-Technical-Whitepaper.md line 207 said "38 Rust workspace crates"; verified count from icn/Cargo.toml [workspace].members at 9012ba5c is 37 library crates (44 workspace members = 37 crates + 3 bins + 4 apps; icn-baseline-lock-guest remains excluded). Fixed in this PR. The same line's LOC/test figures date to the 2026-03-17 gap analysis and were NOT re-verified here.
       - docs/status.toml total_crates ("44 workspace members") re-verified as still correct; no status.toml change needed this cycle.

     Next pre-RFC architecture move is **not yet selected**; this sync preserves optionality. Named candidates (descriptive only, not selected here):
       (a) Appliance negative firstboot smoke — exercise the fail-closed path #1900 left unverified; still the only named proof-matrix gap on the appliance.
       (b) Dependabot queue triage — fast-track patch-level Rust bumps (#1940/#1941/#1943/#1945) and npm groups (#1995/#2018) after full workspace checks; hold #1944 (rand major) and #1942 (ml-dsa PQ surface) for dedicated review.
       (c) #1868 endgame — define broad-fallback retirement criteria per the #1936 design (token-issuance migration first).
       (d) #1703 human gate — organizer presentation → pilot formalization → first operator rehearsal (the 2026-06-11 Cooperative Codebase call is introduction-lane, not the NYCN organizer gate itself).
       (e) Issue hygiene — verify and close #1704/#1727/#1728 against #1906/#1999/#1998 with evidence.
       (f) Strategy-doc deep refresh — re-verify whitepaper LOC/test figures and ICN-Roadmap-Live.md currency (last bumped 2026-04-29).

     Hard rule preserved: this sync edit does NOT change any contract field, does NOT mint a new contract URN, does NOT add an ADR, does NOT add an RFC, does NOT widen gateway typed governance imports, does NOT migrate or un-migrate any handler, does NOT retire `governance:write`, does NOT touch K3s / DNS / GitHub / Forgejo state, does NOT handle private partner / member / organizer data, does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, does NOT verify the appliance fail-closed firstboot path. Phase model unchanged. -->

<!-- [sync edit] 2026-05-22 (post #1875 → #1901 cycle: stale-count fix, governance:write decomposition design + class-level scope constants, governance production guard, appliance Debian-13 real smoke + verified host toolchain handoff, Civic Shell v0 spec, Claude Design protocol + truth-label appendix + icon candidate review + federation operator surface concept + member-shell action-card refinements, NYCN/Launch strategy reframes, SECURITY.md + issue templates + cross-repo map, three Dependabot dev-deps bumps):
     Truth-sync recording 21 PR merges plus one bare handoff commit (22 commits total) between 2026-05-17 and 2026-05-22 on `main`. **Mixed truth class** (this is not pure docs/control-plane): #1901 is real Rust runtime change in `icn/apps/governance/src/http/configure.rs` (new `GovernanceContextBuildMode` enum + `ICN_GOVERNANCE_BUILD_MODE` env var + fail-closed Production-mode validation + warn-only Bootstrap/Test); #1881 is real Rust change in `icn/crates/icn-rpc/src/auth.rs` (seven new class-level governance write-scope constants + `GOVERNANCE_CLASS_WRITE` slice + wire-string assertion tests). Everything else in this batch is docs/control-plane only. No new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no kernel/gateway/runtime API surface widening, no K3s/DNS/Forgejo mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim.

     Phase 2 deliverables list extended to record:
       - #1875 docs(strategy): add Thursday meeting truth packet. Internal strategy brief with decision context and talking points; carried over from earlier session.
       - #1876 docs(appliance): operator handoff — verified host toolchain + base-image staging gap. Handoff notes on verified toolchain and staging gap between image build and production; foundation for the #1900 real-smoke verification.
       - #1877 deps(ts-sdk): dev-dependencies group across 1 directory with 5 updates. Dependabot maintenance.
       - #1790 deps(pilot-ui): dev-dependencies group across 1 directory with 3 updates. Late-merge of an earlier Dependabot PR.
       - #1878 docs(strategy): fix stale crate count in Thursday brief and CLAUDE.md. Two-line correction to align with the actual `[workspace].members` count.
       - #1879 docs(appliance): reconcile README with landed scaffold + real build/smoke. Drift fix after #1865/#1866 landed.
       - #1880 docs(design): governance:write decomposition — pick hybrid path (Refs #1868). Design doc landed at `docs/design/governance/governance-write-decomposition.md` picking the hybrid path (class-level scopes plus app-side MandateGate for higher-risk actions). Enumerates 38 affected handlers. Non-claims on production. Does not migrate any handler; does not retire broad `governance:write`; does not introduce MandateGate in code.
       - #1881 feat(rpc,gateway): mint governance class-level scope constants (#1868 step 1). Seven new constants minted in `icn/crates/icn-rpc/src/auth.rs:966-972`: `GOVERNANCE_CHARTER_WRITE`, `GOVERNANCE_PROPOSAL_WRITE`, `GOVERNANCE_STEWARD_WRITE`, `GOVERNANCE_FEDERATION_WRITE`, `GOVERNANCE_MEETING_WRITE`, `GOVERNANCE_ACTIVITY_WRITE`, `GOVERNANCE_COMMENT_WRITE`, plus a `GOVERNANCE_CLASS_WRITE: &[&str]` slice and wire-string assertion tests. Gateway allowlist entries added. Comment at `auth.rs:963`: "`GOVERNANCE_WRITE` remains in place during the migration." First implementation slice of #1868 decomposition. Does NOT migrate handlers (41 broad `require_scope::<BasicClaims>(&http_req, "governance:write")` call sites in `icn/apps/governance/src/http/handlers.rs` are unchanged); does NOT retire broad `governance:write`; does NOT add MandateGate. #1868 remains open.
       - #1882 docs(strategy): retarget Thursday packet to formation-to-governance conversation. Internal strategy reframe.
       - #1883 docs(strategy): NYCN/Summit as ICN reference institution. Strategy doc positioning NYCN as case-study institution; preserves NYCN-not-activated non-claim.
       - #1885 docs(design): Claude Design seed review and handoff protocol. Adds `docs/design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md`, handoff template, `MUST_NOT_SHIP.md` floor, seed directory workflow. Design-process scaffold only; no design system promotion.
       - #1886 docs(design): candidate icon family review package. Adds `docs/design/icons/CANDIDATES.md` plus contact sheet. Candidate icons proposed for promotion; no icons promoted.
       - #1887 docs(design): truth-label and rejected-pattern appendix. Adds appendix documenting truth labels and anti-patterns.
       - #1888 docs(spec): clarify member-shell action card sync and threshold rendering. Refines `docs/spec/member-shell-v0.md` on action-card sync boundaries and threshold-render semantics; preserves all existing non-claims.
       - #1889 docs(design): federation operator surface concept. Concept doc for a federation operator shell (operational/control-plane facing surface). Concept-level — no spec, no implementation.
       - #1890 chore(deps): bump npm_and_yarn group across `examples/mobile-app` (2 updates). Dependabot dev-deps minor.
       - #1892 chore(repo): add SECURITY.md, issue templates, cross-repo map. Repo hygiene only.
       - #1893 docs(strategy): Launch + ICN meeting kit for 2026-05-21. Internal strategy reframe of the Launch Cooperative meeting from pitch to learner-first reciprocal.
       - #1899 docs(spec): define ICN Civic Shell v0 composition surface. Lands `docs/spec/icn-civic-shell-v0.md`. The first draft used the rejected "ICN Headquarters" metaphor; the v0 name is "ICN Civic Shell." Composition contract that ties existing surfaces (public website per ADR-0032/ADR-0033, `docs/spec/member-shell-v0.md`, `docs/spec/steward-cockpit-v0.md`, no-CLI organizer/member workflow, service-hosting model, auth-bridge model, Sovereign Forge, Forgejo deployment plan) into a single top-level public-plus-logged-in institutional operating shell. Composition only; does NOT supersede the Member Shell, Steward Cockpit, public website, service-hosting model, or auth-bridge model. Names a ten-room model, domain-and-route doctrine, status/proof labels anchored to #1796. Explicit non-goals enumerated. No app implementation, no new endpoint, no auth implementation, no Keycloak/Forgejo/Matrix deployment, no n8n workflow build, no DNS/K3s/VLAN/network mutation, no public admin surfaces, no private data in repo. Registry row landed in `docs/registry.toml`, INDEX row landed in `docs/INDEX.md`.
       - #1900 docs(appliance): record Debian 13 real-smoke verification. Operator-verified end-to-end build + boot of real QCOW2 on Debian 13 trixie host; SHA512 chain recorded. Positive path verified: SSH reachable, firstboot marker present, `icnd.service` active, `/v1/health` returned 200. **Negative / fail-closed firstboot path NOT verified.** Appliance non-claims preserved verbatim (not production, not signed, not immutable, not partner-distributable).
       - #1901 feat(governance): add production guard for standing checker configuration (closes #1871). Adds `GovernanceContextBuildMode { Bootstrap, Production, Test }` at `icn/apps/governance/src/http/configure.rs:58`; reads `ICN_GOVERNANCE_BUILD_MODE` at line 95 (`production` / `bootstrap` / `test`, case-insensitive, with `Bootstrap` fallback on unrecognized values); Production mode hard-fails at line 455 (`Err(GovernanceContextValidationError)`) when a checker dependency is missing; Bootstrap/Test mode emits `tracing::warn!` at line 459 and continues. Unit tests at line 802+. Does NOT fix #1870 (closed separately by prior work); does NOT add MandateGate; does NOT migrate handlers off broad `governance:write`.

     Closure batch: #1870 and #1871 are both CLOSED. #1868 remains OPEN (decompose `governance:write`) — step 1 landed in #1881; step 2 (handler migration) is pending.

     Cross-cycle disciplines preserved verbatim:
       - No new ADR, no new RFC, no new contract URN, no new ADR-0026 receipt class.
       - The meaning firewall is not widened. #1901 adds an enum and an env-var read inside `apps/governance`; #1881 adds capability-string constants inside `icn-rpc` (kernel-side enforcement primitive layer). Neither widens domain-meaning into the kernel.
       - Settlement / position / obligation / allocation / receipt / provenance — never payment / wallet / currency / balance / token / crypto / blockchain / timebank — for ICN-native compute / settlement / federation surfaces.
       - "Civic Shell" is the v0 name; "Headquarters" was the rejected first-draft metaphor. The merged spec is `docs/spec/icn-civic-shell-v0.md`.
       - Privacy is posture, not content. PrivateEvidence body bytes never reach any rendering layer.
       - When cockpit shows degraded, member shell must show degraded too.

     Open Dependabot PRs at sync write-time (all in `sdk/typescript/`, none merged):
       - #1894 flatted 3.3.3 → 3.4.2 (CWE-1321 patch).
       - #1895 dev-dependencies group (4 updates: `@types/node`, `@typescript-eslint/*`, `ts-jest`).
       - #1896 minimatch 3.1.2 → 10.2.5 (major jump, requires Node ≥20, adds install-time `prepare` script).
       - #1897 picomatch 2.3.1 → 4.0.4 (major + two CVE fixes: CVE-2026-33671, CVE-2026-33672).
       - #1898 fast-uri 3.1.0 → 3.1.2 (security patch GHSA-v39h-62p7-jpjc).

     Phase 2 status remains ⏳ (still partner-bound). Nothing in this cycle completes a phase, removes a partner-binding gate, activates NYCN, or implements live federation. The Civic Shell spec, governance production guard, and class-level scope minting all materially strengthen the *design* and *startup-time legibility* of Phase 2 machinery; they do not change the partner gate.

     Truth-class corrections to prior beliefs surfaced by this verification:
       - `docs/status.toml` line 241 said `total_crates = "34 crates + 4 apps + 3 binaries = 41 workspace members"`. Actual count from `icn/Cargo.toml` `[workspace].members` is 37 crates + 3 bins + 4 apps = **44 workspace members**. Fixed in this PR.
       - `docs/strategy/ICN-Technical-Whitepaper.md` line 207 says "38 Rust workspace crates." Actual lib-crate count is 37. Defer correction to a strategy-doc refresh PR.
       - `docs/deployment/DEPLOYMENT_READY.md` lines 26-27 and 130-144 use wallet/balance/payment language. The file is not in `[control].canonical_doc_paths`. Recommend follow-up archive or banner; do not silently delete.
       - The first draft of #1899 was titled "ICN Headquarters v0." Reviewer-driven rename to "Civic Shell" landed in the merged spec.

     Next pre-RFC architecture move is **not yet selected**; this sync preserves optionality. Named candidates (descriptive only, not selected here):
       (a) #1868 step 2 — migrate the *charter* handler family from broad `"governance:write"` to `GOVERNANCE_CHARTER_WRITE`, keeping broad scope as an accepted-also fallback per the hybrid design in #1880. One handler family per PR (charter → proposal → steward → federation → meeting → activity → comment).
       (b) Appliance negative firstboot smoke — exercise the `10-firstboot-gate.conf` drop-in as a fail-closed scenario; complete the proof matrix #1900 left at positive-only.
       (c) Receipt evidence carries presented scope + mandate grant — conditional on #1880's open question about administrative receipt class.
       (d) MandateGate trait + types + persistence — defer until handler migration is far enough that the gating need is concrete.
       (e) Dependabot triage — fast-track #1898/#1894/#1895 after `cd sdk/typescript && npm ci && npm test && npm run build && npm run typecheck`. Hold #1897 for careful CVE-changelog read. Defer #1896 pending Node ≥20 decision.
       (f) Strategy-doc crate-count refresh and `DEPLOYMENT_READY.md` regulatory-vocabulary handling.

     Hard rule preserved: this sync edit does NOT change any contract field, does NOT mint a new contract URN, does NOT add an ADR, does NOT add an RFC, does NOT widen gateway typed governance imports, does NOT migrate any handler, does NOT add MandateGate, does NOT retire `governance:write`, does NOT touch K3s / DNS / GitHub / Forgejo state, does NOT handle private partner / member / organizer data, does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, does NOT verify the appliance fail-closed firstboot path. Phase model unchanged. -->

<!-- [sync edit] 2026-05-16 (abuse-case hardening strategy doc):
     Truth-sync recording the landing of `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`. **Doc/control-plane only**: no Rust code, no schema fields changed, no new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no kernel/gateway/runtime mutation, no K3s/DNS/Forgejo mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim.

     The session landed five files in one commit on branch `docs/abuse-case-hardening-strategy` (worktree-isolated):
       - docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md (new, 608 lines): institutional-failure-mode hardening doctrine. Ten one-line doctrine rules (receipts prove events not legitimacy; authority shortcuts must label themselves as shortcuts; unresolved standing is not standing in production; accepted is not applied; convenience paths must not become authority paths; bootstrap is not democracy; a capability token is not a mandate; a UI must not launder uncertainty into confidence; privacy posture is not private content; index absence is not record absence). Ten code-anchored abuse stories and matching hardening tracks against current main (d57ff1d6e): broad `governance:write` scope at icn-rpc/src/auth.rs:947 covering ~12 mutation handlers; direct membership mutation at apps/governance/src/http/handlers.rs:548-635 with fail-open TrustThreshold at :572/:617; direct charter activation at handlers.rs:694-744 with synthetic `direct-activation:` provenance; optional checker wiring at apps/governance/src/http/configure.rs:197-264; reconciliation status enum at apps/governance/src/dispatch_evidence.rs:131-169 with EmittedOnly/ExecutionEvidenced/ExecutionFailed; receipt_backend.rs:178-190 documenting put_mandate_with_grants default as NOT atomic; kernel-api/src/proofs.rs:646-661 ScopedVault digest-only rendering boundary. Closed lifecycle vocabulary, production invariants, authority-shortcut policy, resolver fail-closed policy, fixture-matrix plan, and a P0–P3 candidate issue roster are all named without filing.
       - docs/dev/handoff-2026-05-16-abuse-case-hardening.md (new): session handoff with verified anchor table, decisive-test fail criteria, deferred-items list, preserved-boundaries list, unsafe-assumptions list, next-move PR command, and checks-run / checks-not-run tables.
       - docs/INDEX.md: one architecture-section line under KERNEL_APP_SEPARATION.md.
       - docs/registry.toml: one [docs."docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md"] control-plane row, schema matching ARCHITECTURE_DUE_DILIGENCE.md.
       - docs/STATE.md: this sync-edit block.

     Cross-sprint disciplines preserved verbatim:
       - No new ADR, no new RFC, no new contract URN. The strategy doc is process / principle, not an architectural decision; specific design picks (scope decomposition vs mandate gating, administrative receipt class, closed-vocabulary owner, degraded-state record class, direct-mutation lifetime) are explicitly listed as open questions deferred to follow-up PRs.
       - No new ADR-0026 receipt class. The candidate `BootstrapCharterActivationReceipt` and `BootstrapMembershipMutationReceipt` artifacts are draft names; whether they are new receipt classes or discriminators on existing administrative receipt types is itself an open question (§15).
       - The meaning firewall is not widened. §4.1 keeps mandate-bundle gating as the ICN-native authority path where it fits; kernel-side capability strings are kept for kernel enforcement only.
       - Settlement / position / obligation / allocation / receipt / provenance — never payment / wallet / currency / balance / token / crypto / blockchain / timebank — for ICN-native compute / settlement / federation surfaces.
       - Privacy is posture, not content. PrivateEvidence body bytes never reach any rendering layer; the §4.9 / §12 regression-test set asserts this against the existing digest-only boundary at icn-kernel-api/src/proofs.rs:646-661.
       - When cockpit shows degraded, member shell must show degraded too — the §4.8 / §11 fixture-matrix plan formalizes this against the existing member-shell-v0 / steward-cockpit-v0 v0 violation rule.

     Phase 2 status remains ⏳ (still partner-bound). Nothing in this session completes a phase, removes a partner-binding gate, or implements any of the named hardening tracks. The strategy doc is descriptive of what production must look like; the implementation that achieves it is sequenced in §16 (Stages A–E) and unfiled.

     Next pre-RFC architecture move is **not yet selected**; this sync preserves optionality. The strategy doc's §14 P0 roster (scope split design, BootstrapMembershipMutationReceipt design, BootstrapCharterActivationReceipt design, production startup guard for optional checkers / resolvers) is the named first candidate set, but neither selection nor issue filing is part of this session.

     Hard rule preserved: this sync does NOT change any contract field, does NOT mint a new contract URN, does NOT add an ADR, does NOT add an RFC, does NOT widen gateway typed governance imports, does NOT increase the meaning-firewall ratchet, does NOT touch K3s / DNS / GitHub / Forgejo state, does NOT handle private partner / member / organizer data, does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, does NOT start runtime work. Phase model unchanged. -->

<!-- [sync edit] 2026-05-15 (post architecture-spec sprint, PRs #1814 / #1819 / #1820 / #1821 / #1822 / #1823 / #1824 / #1825 / #1826 / #1827 / #1829 / #1830 / #1831 / #1832 / #1833):
     Truth-sync for the architecture-spec sprint completion. **Doc/control-plane only**: no Rust code, no schema fields changed, no new contract URN, no new ADR, no new RFC, no new ADR-0026 receipt class, no kernel/gateway/runtime mutation, no K3s/DNS/Forgejo mutation, no NYCN partner data, no production-readiness claim, no live-federation claim, no formal NYCN pilot claim, no Phase 2 completion claim.

     The sprint landed fifteen PRs over 2026-05-14 → 2026-05-15 — thirteen design-level architecture-spec PRs, one process-doc PR (#1827), and one wrap-up review PR (#1833):
       - #1814 docs(architecture): integrated cooperative operating model spine (the ladder root).
       - #1819 docs(spec): accepted-proposal effect dispatch contract (closed #1797 on merge).
       - #1820 docs(spec): institutional domain and policy primitive.
       - #1821 docs(spec): CCL policy registry and hook contract.
       - #1822 docs(spec): governed service binding, workload manifest, and runtime provider.
       - #1823 docs(spec): storage durability policy objects.
       - #1824 docs(spec): ArtifactRegistry v0 and ScopedVault boundary.
       - #1825 docs(architecture): entity-scope vocabulary boundary (LocalDomain not Coop).
       - #1826 docs(spec): compute placement policy.
       - #1827 docs(agents): reconcile handoff path with template (process-doc).
       - #1829 docs(spec): network anti-entropy proof loops.
       - #1830 docs(spec): member shell v0.
       - #1831 docs(spec): steward cockpit v0 (drift-fix follow-up landed as #1832 after late post-merge reviewer feedback).
       - #1832 fix(spec): correct steward cockpit review drift (four rounds of fixes: 8→9 field count, ADR-0027 14-field requirement, PlacementFallbackReceipt attribution + handoff timing, IA-row no longer routes through ADR-0027).
       - #1833 docs(dev): wrap architecture spec sprint closure review (the sprint closure handoff with paste-ready closure-comment drafts, deduplicated follow-up roster, and recommended next-decision sequence).

     Following #1833 the closure batch landed: nine sprint sibling issues closed with the paste-ready closure comments — #1794 (institutional domain), #1795 (steward cockpit), #1798 (ArtifactRegistry/ScopedVault), #1799 (network anti-entropy), #1801 (compute placement), #1815 (governed service binding), #1816 (storage durability), #1817 (CCL policy registry), #1818 (member shell v0). #1797 (effect dispatch) was already closed by #1819. All ten sprint sibling issues are now CLOSED at docs/spec level; none of the closures implies runtime-implementation completion.

     First-batch follow-up issues filed from the deduplicated wrap-up roster (seven of the wrap-up's thirty-four drafts):
       - #1834 schema(network): define AntiEntropyProbe and StateDigest records.
       - #1835 schema(network): define DivergenceEvidence and RepairPlan records.
       - #1836 schema(compute): wire-stable PlacementDecision and ExecutorAdmissionDecision schemas.
       - #1837 spec(contracts): define steward required-action card contract (the #1831/#1832 gap — ADR-0027 is member-only; steward cards need a separate or amended contract).
       - #1838 test(devnet): receipt-index anti-entropy fixture (Slice A).
       - #1839 test(devnet): member shell read-only rendering rehearsal (Slice A).
       - #1840 test(devnet): steward cockpit divergence-render fixture (Slice A).
     Twenty-seven additional follow-up drafts remain in the wrap-up doc's deduplicated roster for separate batch decisions.

     Cross-sprint disciplines preserved verbatim:
       - No new ADR-0026 receipt classes. The sprint introduced design-level proof-artifact identifiers (PlacementDecision, RepairReceipt, DivergenceEvidence, PlacementFallbackReceipt, etc.) that ride inside existing Stage 5 EffectDispatchEvidence or Layer 2 ArtifactReceipt envelopes.
       - No ADR-0027 support for steward required-action cards. ADR-0027 covers member ActionCards; the steward cockpit's fourteen operator scenarios cannot be represented by its closed enums. #1837 carries the gap.
       - LocalDomain vocabulary (not Coop) per #1825 §C3. Existing serialized Coop-prefixed identifiers (DataLocality::CoopReplicated, ADR-0030 Coop(coop_id), bonds:payments gossip topic) are preserved with naming notes pending the rename follow-up.
       - Execution budget is policy-facing; fuel_limit is the runtime field; capacity is reserved for executor/node resource availability.
       - Settlement / position / obligation / allocation / receipt / provenance — never payment / wallet / currency / balance / token / crypto / blockchain / timebank — for ICN-native compute / settlement / federation surfaces.
       - Member shell shows plain participation status; steward cockpit shows technical detail. When cockpit shows degraded, member shell must show degraded too — v0 violation otherwise (#1831 Design principle 9 + failure-table row).
       - Privacy is posture, not content. Body bytes of PrivateEvidence artifacts never reach any rendering layer.
       - The kernel never imports app-side rendering. Member shell, steward cockpit, and policy oracle outputs are all app-side per docs/architecture/KERNEL_APP_SEPARATION.md.

     Phase 2 status remains ⏳ (still partner-bound). The Phase 2 *machinery* is now substantially richer at the design-level layer (twelve merged specs covering the integrated operating model spine, effect dispatch, institutional domain, CCL registry, governed service binding, storage durability, ArtifactRegistry/ScopedVault, scope vocabulary, compute placement, anti-entropy proof loops, member shell, and steward cockpit); what remains for Phase 2 is the human procedure — present, formalize, rehearse against organizer material — plus the implementation work the follow-up roster names. The next concrete human gate is unchanged from the prior sync: organizer presentation → pilot formalization → first operator rehearsal per the NYCN rehearsal gate (in the partner repo).

     Next pre-RFC architecture move is **not yet selected**; this sync preserves optionality. Candidate next moves enumerated descriptively only (not selected here): (a) batch-file the remaining twenty-seven follow-up drafts from the wrap-up roster; (b) implementation slice — `feat(compute): policy oracle for placement decisions (read-only proof-loop)` per #1826's named first implementation slice; (c) first fixture rehearsal — pick one of the three already-filed Slice A fixtures (#1838 / #1839 / #1840); (d) next spec-ladder doc — #1837 steward required-action card contract; (e) DAP runtime dogfood emitting at least one receipt under ADR-0026 for one DAP primitive (carried from previous sync); (f) idea-0019 runtime dogfood emitting additional ProcessTransitionReceipt classes (carried from previous sync); (g) idea-0019 visibility/privacy-boundary run with redaction in evidence export (carried); (h) idea-0019 accessibility-gate ProcessGateResult on a real surface (carried); (i) idea-0019 open-question triage (carried). Phase model classification is unchanged; see PHASE_PROGRESS.md for phase definitions.

     Hard rule preserved: this sync does NOT change any contract field, does NOT mint a new contract URN, does NOT add an ADR, does NOT add an RFC, does NOT widen gateway typed governance imports, does NOT increase the meaning-firewall ratchet, does NOT touch K3s / DNS / GitHub / Forgejo state, does NOT handle private partner / member / organizer data, does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, does NOT start runtime work, and does NOT start any Stage 1.5 / Stage 2 / Stage 3 / Stage 4 / Stage 5 work. Phase model unchanged. -->

<!-- [sync edit] 2026-05-07 (post-#1761 / #1762 / #1763 / #1764):
     Truth-sync for the May-7 close-out cycle plus the ActionCard
     contract publication landing. This is **doc/control-plane only**:
     no Rust code, no contract field changes, no new contract URN, no
     new schema, no new ADR, no new RFC. Phase 2 status remains ⏳
     (still partner-bound) — the next concrete human gate remains the
     partner-bound sequence in the NYCN rehearsal gate (in the partner repo).
     Landings since the previous sync edit (2026-05-07 mid-day, the
     opaque receipt storage stack post-#1755/#1756/#1757/#1758/#1759
     captured in #1762):
       - #1761 fix(commons): retry sled open on WouldBlock to bridge
         flusher shutdown. Bounded retry-with-backoff in
         `SledCommonsStore::open` (8 attempts max, 500ms total budget
         cap, 10ms initial backoff, only matches
         `io::ErrorKind::WouldBlock`). Closes #1760 (sled 0.34
         flusher-thread shutdown race surfaced by #1759's CI Test
         job). Two new unit tests pin the new behavior. Single-file
         change in `icn/crates/icn-commons/src/store.rs`. Was "open at
         this sync write-time" in #1762; now MERGED 2026-05-07.
       - #1762 docs(state): sync opaque receipt storage stack landing.
         Records #1755/#1756/#1757/#1758/#1759 in STATE.md and
         PHASE_PROGRESS.md and adds session handoff
         `docs/dev/handoff-2026-05-07.md`. Doc/control-plane only.
       - #1763 deps(ts-sdk): bump the dev-dependencies group across 1
         directory with 4 updates. Dependabot maintenance of
         `sdk/typescript/`. No runtime change.
       - #1764 docs(contracts): publish ActionCard contract for
         institution packages. Adds `docs/contracts/institution-package/action-card.example.json`
         (fictional sample card) and `docs/scripts/validate-action-card.py`
         (draft-2020-12 JSON Schema validator), and expands
         `docs/contracts/institution-package/README.md` with stability
         rationale (cites ADR-0027 § Card kind taxonomy "growable by
         ADR amendment"), schema-id-audit linkage (#1737 / #1742
         retain-temporarily decision; review by 2026-06-30), explicit
         CLI validation commands, regulatory-safe vocabulary
         enumeration, and explicit "institution-specific semantics
         belong in institution packages, not in ICN core" guidance.
         Mirrors the existing convention used by
         `validate-preview-review.py` and
         `validate-rehearsal-evidence.py`. Closes #1713 (all six
         acceptance criteria met by the merged PR; manually closed
         after merge with the README's Files table, validator, and
         example as evidence). One substantive Copilot review finding
         addressed pre-merge: CLI arg `packet` -> `card`,
         `DEFAULT_PACKET` -> `DEFAULT_CARD`, internal vars and error
         messages aligned (commit `ffb4d791` on the branch, squashed
         into `f7c3bf73`). No schema fields change. Schema's `$id`
         remains DNS-backed (`https://intercooperative.network/contracts/institution-package/action-card.schema.json`)
         per the audit's retain-temporarily decision; migration to
         `urn:icn:contract:action-card:v<N>` is a separate
         single-schema PR under audit §5 rules, gated on the
         2026-06-30 review tracked by #1742.
     `idea-0019` (#1748) acceptance gate (a) status is unchanged from
     #1762: still partially satisfied (one `ProcessTransitionReceipt`
     class — `ProcessGateResultReceipt` — emitted via #1755 and durably
     persisted via the opaque cascade since #1759); gates (b)-(d)
     unchanged: not started. #1713 closure is the only acceptance-gate
     change in this cycle; it is independent of #1748's gates. Phase 2
     status remains ⏳ (still partner-bound). Hard rule preserved:
     this cycle does NOT widen gateway typed governance imports, does
     NOT increase the meaning-firewall ratchet (baseline 10 known
     violations preserved, 0 new), does NOT mint a new contract URN,
     does NOT add an ADR, does NOT add an RFC, does NOT touch K3s /
     DNS / GitHub / Forgejo state, does NOT handle private partner /
     member / organizer data, does NOT claim Phase 2 completion, does
     NOT claim formal NYCN pilot, does NOT claim production readiness,
     does NOT claim live federation, and does NOT start any
     Stage 1.5 / Stage 2 / Stage 3 / Stage 4 / Stage 5 work. Next
     pre-RFC architecture move is **not yet selected**; this sync
     deliberately preserves optionality. Candidate next moves remain
     as enumerated in the post-#1755/#1759 sync below; one less open
     follow-up since #1761 closed #1760. -->

<!-- [sync edit] 2026-05-07 (post-#1755 / #1756 / #1757 / #1758 / #1759):
     Truth-sync for the opaque receipt storage stack landing.
     Unlike the May-5 sync edits, this is **runtime/implementation
     truth**, not doc/control-plane: real Rust changes landed in
     `icn-gateway` and `apps/governance`. Phase 2 status remains
     ⏳ (still partner-bound) — the next concrete human gate
     remains the partner-bound sequence in
     the NYCN rehearsal gate (in the partner repo).
     Landings since the previous sync edit (2026-05-05 evening,
     post-#1753):
       - #1755 feat(governance): add first process-transition
         receipt runtime slice. Adds `ProcessGateResultReceipt`
         (one of eight named `ProcessTransitionReceipt` classes
         in the `idea-0019` framing brief). Emitted by
         `GovernanceManager::record_process_gate_result` and
         persisted through the `GovernanceReceiptBackend` trait.
         **First real runtime dogfood emitting a
         `ProcessTransitionReceipt` class** — partial credit
         toward #1748 acceptance gate (a). Surfaced a production
         durability gap: the sled-backed `ReceiptStore` had not
         yet overridden `put_process_gate_result`, so production
         callers received an explicit fail-closed sentinel
         (`process_gate_result_backend_not_implemented`) rather
         than a silent commit-without-persistence — addressed by
         the #1757-#1759 stack below.
       - #1756 chore(hooks): fix scope-guard / todo-guard exec
         bit and todo-guard pipeline. Repository-tooling fix; no
         runtime, contract, schema, or API change.
       - #1757 feat(gateway): add meaning-blind opaque receipt
         storage primitive. Adds the `put_opaque` /
         `get_latest_opaque` / `list_opaque_for` inherent methods
         on `ReceiptStore` plus the supporting `OPAQUE_REC_PREFIX`
         and `OPAQUE_BY_KEY_PREFIX` keyspaces. The gateway stores
         payloads under a caller-supplied `(class, key1, key2_opt,
         recorded_at, record_hash)` tuple without learning the
         typed shape — the apps layer is the single source of
         truth for the closed taxonomy of class strings. New
         classes can be added in apps/ without expanding the
         gateway's typed governance imports (no firewall ratchet
         increase). Three substantive review findings addressed
         in `cb9d6daf` before merge: write-once-by-hash on the
         primary record (same `(class, record_hash)` + identical
         payload is idempotent; different payload aborts with
         stable sentinel `opaque_record_hash_collision`),
         atomic primary + secondary index writes via a single
         sled transaction, distinct `key2 = None` vs `key2 =
         Some("")` encoding (tag-byte scheme), and deterministic
         tie-breaker for equal `recorded_at` (sorted by
         `(recorded_at, record_hash)`). One additional codex P2
         raised against `cb9d6daf` and addressed in `a8fbb1a6`
         before merge: a new `OPAQUE_HASH_BIND_PREFIX` keyspace
         binds each `(class, record_hash)` to exactly one
         canonical `(key1, key2_opt, recorded_at)` tuple on first
         write; divergent re-binds abort with stable sentinel
         `opaque_record_hash_index_collision`. Without this, an
         identical-payload replay under different index tuples
         could surface one canonical receipt across multiple
         audit chains or appear under `get_latest_opaque` for the
         wrong tuple. Bind, primary, and secondary writes are all
         enforced atomically inside the same sled transaction.
       - #1758 feat(governance): expose opaque storage on
         `GovernanceReceiptBackend` trait. Adds
         `put_opaque` / `get_latest_opaque` / `list_opaque_for`
         to the trait surface in
         `apps/governance/src/receipt_backend.rs`, each with a
         fail-closed default returning the stable sentinel
         `opaque_storage_not_implemented`. The sled-backed
         `ReceiptStore` overrides them via thin delegates to its
         inherent opaque methods. Existing typed test backends
         (which override the typed `put_*`/`get_*`/`list_*`
         methods) are unaffected — the opaque methods are only
         exercised when callers explicitly route through them.
         Validates dynamic dispatch with a `Box<dyn
         GovernanceReceiptBackend>` round-trip test.
       - #1759 feat(governance): route `ProcessGateResultReceipt`
         through opaque storage cascade. Updates the trait
         default for `put_process_gate_result` to attempt the
         opaque cascade first (encoding the typed envelope as
         JSON, calling `put_opaque` with class
         `"process_gate_result"`, `key1 = session_id`,
         `key2 = Some(gate_kind)`, the typed `recorded_at` and
         `record_hash`), and to surface the explicit
         `process_gate_result_backend_not_implemented` sentinel
         only when the underlying `put_opaque` itself returns the
         opaque-not-implemented sentinel. Production gateway-backed
         `ReceiptStore` therefore now durably persists
         `ProcessGateResultReceipt` through the opaque cascade
         without any new typed governance import on
         `icn-gateway` (no firewall ratchet increase).
         Test-backend coverage: a new `OpaqueOnlyBackend` that
         overrides only `put_opaque` exercises the
         typed-default → opaque cascade end-to-end. Test-suite
         determinism follow-up was applied in the same PR
         (Copilot review): three tests previously used
         `std::thread::sleep(Duration::from_millis(1100))` to
         force `recorded_at` to advance one second between writes
         — replaced with explicit, strictly-increasing
         `recorded_at` timestamps on directly-constructed
         `ProcessGateResultReceipt` values fed through the
         backend trait. Suite now finishes in 0.01s, deterministic.
     New invariant added during the merge cycle:
       - **`OPAQUE_HASH_BIND_PREFIX`** keyspace in
         `icn/crates/icn-gateway/src/receipt_store.rs`. Each
         `(class, record_hash)` is bound to exactly one canonical
         `(key1, key2_opt, recorded_at)` tuple. Divergent re-binds
         abort with stable sentinel
         `opaque_record_hash_index_collision`. Closes a
         secondary-index fan-out hole that the original
         write-once-by-hash check on `OPAQUE_REC_PREFIX` did not
         catch.
     Surfaced flake → real bug filed:
       - Issue #1760 `fix(commons): add CommonsManager::close()`
         (later updated; correct diagnosis: sled 0.34's flusher
         thread holds the OS `flock(LOCK_EX)` past `Db::drop`).
         Fired on #1759's Test job (run `25491262579`,
         `test_commons_charter_survives_sled_drop_and_reopen`
         panicked at `crates/icn-gateway/tests/commons_integration.rs:472:59`
         with `EAGAIN/WouldBlock`). The diff on #1759 was
         entirely in `apps/governance` and never touched the
         commons stack — pre-existing race surfaced under CI
         load. The `Test` job was rerun on the same SHA without
         code changes and went green, confirming the load-
         dependent classification.
       - PR #1761 `fix(commons): retry sled open on WouldBlock to
         bridge flusher shutdown` opened on
         `fix/commons-sled-open-retry-on-wouldblock`. Bounded
         retry-with-backoff in `SledCommonsStore::open` (8
         attempts max, 500ms total budget cap, 10ms initial
         backoff). Only matches `io::ErrorKind::WouldBlock` so
         genuine errors (NotFound, PermissionDenied, etc.) are
         not masked. Two new unit tests pin the new behavior.
         CI in flight at sync write-time. Open at this sync.
     Acceptance-gate posture for `idea-0019` (#1748) acceptance
     criteria, restated for clarity:
       - (a) **runtime dogfood emitting at least one
         `ProcessTransitionReceipt` class under `ADR-0026`** —
         partially advanced: #1755 emits
         `ProcessGateResultReceipt`; #1759 makes that emission
         durable through the opaque cascade on the production
         gateway-backed `ReceiptStore`. The receipt envelope's
         relationship to `ADR-0026`'s receipt-and-provenance
         proof envelope is not separately re-stated here.
       - (b) **real visibility/privacy-boundary run with
         redaction in evidence export** — unchanged: not
         started.
       - (c) **accessibility-gate `ProcessGateResult` produced
         through `ORGANIZER_MEMBER_ACCESSIBILITY_GATE` on a real
         surface** — unchanged: not started.
       - (d) **open-question triage (Q1, Q3, or Q4)** — unchanged:
         not started.
     This sync explicitly does NOT claim:
       - Phase 2 completion; Phase 2 remains ⏳ partner-bound.
       - Formal NYCN pilot authorization.
       - Production readiness, live federation, live cloud sync,
         K3s/DNS/GitHub/Forgejo mutation, or NYCN private-data
         handling.
       - That `idea-0019`'s receipt-backed promotion to RFC has
         been satisfied — three of the four acceptance gates
         remain open, and the runtime-dogfood gate is partial.
       - That `idea-0020` (DAP) has any new runtime advancement —
         unchanged from the post-#1753 sync; promotion gate
         unchanged.
       - That gateway typed governance imports were widened —
         the opaque storage primitive is bytes-in / bytes-out,
         and adds zero new domain types.
       - That the meaning-firewall ratchet changed — baseline
         10 known violations preserved, 0 new.
     Open coordination/control issues at this sync (unchanged
     from post-#1753):
       - #1748 milestone(process): define Institutional Process
         Substrate. Acceptance criteria advanced on gate (a)
         (partial).
       - #1746 milestone(showcase): make NYCN organizer rehearsal
         operable before first presentation. Unchanged.
       - #1744 ci(review): make substantive AI review findings
         merge-gating. Unchanged.
     Open PR queue at this sync:
       - #1761 fix(commons): retry sled open on WouldBlock to
         bridge flusher shutdown — CI in flight.
       - #1736 / #1735 — Dependabot dev-dependency bumps.
     Next pre-RFC architecture move: **NOT YET SELECTED**. The
     candidate enumeration from the post-#1753 sync stands,
     reduced as follows:
       (a) DAP **runtime** dogfood emitting at least one receipt
           under `ADR-0026` for one DAP primitive — unchanged,
           unchanged scope.
       (b) `idea-0019` runtime dogfood emitting **additional**
           `ProcessTransitionReceipt` classes under `ADR-0026`
           — the gate is no longer "first" but "additional";
           candidates remain `ProcessSessionOpenedReceipt`,
           `DeliberationEntryRecordedReceipt`,
           `DecisionRecordedReceipt`,
           `ActivationCrossedReceipt`,
           `MutationPlanRecordedReceipt`,
           `MutationAppliedReceipt`,
           `EvidencePacketProducedReceipt`. All are eligible
           through the same opaque storage cascade landed in
           this sync.
       (c) `idea-0019` visibility/privacy-boundary run with
           redaction in evidence export — unchanged.
       (d) `idea-0019` accessibility-gate `ProcessGateResult`
           produced through `ORGANIZER_MEMBER_ACCESSIBILITY_GATE`
           on a real surface — unchanged.
       (e) `idea-0019` open-question triage — unchanged.
       (f) DAP §17 follow-up framing briefs — unchanged.
       (g) Control-plane cleanup, including unresolved/stale
           review-thread hygiene — unchanged.
     None is selected here. -->

<!-- [sync edit] 2026-05-05 (post-#1753):
     Truth-sync for the Democratic Authority Primitives read-model
     fixture-walk dogfood landing. Doc/control-plane and
     idea-refinery only — no runtime, no schema, no contract URN,
     no ADR, no RFC, no implementation issue, no runtime dogfood,
     no Phase 2 advance.
     Landings since the previous sync edit (2026-05-05 evening,
     post-#1751):
       - #1753 docs(ideas): add read-model dogfood slice for
         Democratic Authority Primitives (idea-0020). Adds
         `ops/ideas/dogfood/democratic-authority-primitives-mvp.md`
         and updates the matching `ops/ideas/ideas.yaml` row.
         Read-model fixture-walk variant per
         `ops/ideas/README.md` § "Dogfood slice variants" (the
         convention added in #1749). Composes the six DAP
         primitive families named in the framing brief's §17
         follow-up (`AuthorityBasis`, `ParticipationRole`,
         `FacilitatorSummary`, `ConflictDisclosure`,
         `MinorityReport`, `DeliberationContext` — the latter
         exercising three of its twelve reference families:
         `CharterRuleReference`, `PriorDecisionReference`,
         `AccessibilityNote`) end-to-end against the merged
         `idea-0019` read-model fixture walk
         (`ops/ideas/dogfood/institutional-process-substrate-mvp.md`),
         plus referencing `OperatorExecutionAuthority` as the
         strictly-downstream-of-decision operator handle at the
         activation gate. Walks `Step 0` through `Step 7` of the
         existing `idea-0019` slice without re-describing the
         spine; only DAP primitive additions are recorded.
         Composes orthogonally with `idea-0019`: the spine names
         *what gets processed*; the primitives fill the spine's
         records with authority and context typing the spine
         deliberately deferred. Emits no receipts, contacts no
         gateway, performs no mutation, introduces no new contract
         URN, modifies no kernel/runtime/contract/schema/ADR file.
         Per `ops/ideas/README.md` § "Dogfood slice variants" and
         per the DAP framing brief's §16.1 strict RFC promotion
         gate, **a read-model fixture walk does NOT satisfy
         receipt-backed promotion thresholds**; promotion of
         `idea-0020` to RFC still requires (1) a separate runtime
         dogfood that emits at least one receipt under `ADR-0026`
         for one of the named primitives (preferably a
         `ConflictDisclosure` accept receipt or a `MinorityReport`
         recorded receipt — the framing brief's §16.1 names these
         generically without attaching concrete class identifiers,
         and the slice's slice-local class candidates
         `ConflictDisclosureAcceptedReceipt` and
         `MinorityReportRecordedReceipt` are not committed as
         canonical), (2) a real visibility/privacy-boundary run
         with redaction in evidence export, (3) an
         accessibility-gate `ProcessGateResult` produced through
         `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` on a
         real surface, and (4) Q1 (`AuthorityBasis` polymorphism
         vs typed family) or Q5 (`ConflictDisclosure` and
         `MinorityReport` placement) **resolved** in writing
         (deferral is not sufficient for the RFC gate per §16.1;
         the lenient resolved-or-deferred standard at §16.3
         applies only to the broader runtime-justification
         threshold).
     Open coordination/control issues at this sync (unchanged):
       - #1748 milestone(process): define Institutional Process
         Substrate. `epic:arch-invariants` + `type:spec`. Four
         acceptance gates remain unchecked.
       - #1746 milestone(showcase): make NYCN organizer rehearsal
         operable before first presentation. Unchanged.
       - #1744 ci(review): make substantive AI review findings
         merge-gating. Unchanged.
     Open PR queue at this sync: only Dependabot dev-dependency
     bumps (#1735 pilot-ui axe-core/playwright; #1736 TypeScript
     SDK dev-deps). Unchanged from prior sync.
     Next pre-RFC architecture move: **NOT YET SELECTED**. The
     prior sync (post-#1751) deliberately preserved optionality
     and named four candidate classes; #1753 landed the artifact
     that was the most directly named candidate from class (1)
     (the DAP brief's `[x]` next artifact, §17 follow-ups), so
     this sync removes class (1) from the open-candidate
     enumeration. The remaining candidate classes the next
     session may pick from, listed descriptively only:
       (a) DAP **runtime** dogfood emitting at least one receipt
           under `ADR-0026` for one DAP primitive — the next
           artifact called for by the slice's promotion gate; the
           framing brief's §16.1 names a `ConflictDisclosure`
           accept receipt and a `MinorityReport` recorded receipt
           generically; `ConflictDisclosureAcceptedReceipt` and
           `MinorityReportRecordedReceipt` appear as slice-local
           class candidates in the dogfood artifact and are not
           committed as canonical.
       (b) `idea-0019` runtime dogfood emitting at least one
           `ProcessTransitionReceipt` class under `ADR-0026` (one
           of four #1748 acceptance gates).
       (c) `idea-0019` visibility/privacy-boundary run with
           redaction in evidence export (one of four #1748
           acceptance gates).
       (d) `idea-0019` accessibility-gate `ProcessGateResult`
           produced through `ORGANIZER_MEMBER_ACCESSIBILITY_GATE`
           on a real surface (one of four #1748 acceptance
           gates).
       (e) `idea-0019` open-question triage: at least one of Q1
           (`ProcessTargetRef` polymorphism), Q3
           (`DeliberationEntry` kind taxonomy), or Q4
           (`HumanDecisionSet` vs proposal/vote) resolved or
           explicitly deferred in writing (one of four #1748
           acceptance gates).
       (f) DAP §17 follow-up framing briefs — pre-RFC,
           decompose-only: CCL hook-point catalog;
           expert/advisory across institution types; conflict
           object model connecting `ConflictDisclosure` to
           `idea-0016`/ADR-0029; federation tally semantics
           composing `RepresentationMandate` with #1609;
           delegation runtime gated on #1632.
       (g) Control-plane cleanup, including unresolved/stale
           review-thread hygiene if inspection confirms it.
     None is selected here.
     Phase 2 framing unchanged: NYCN remains the intended first
     cooperative partner, not a formally committed pilot. The
     concrete next gate remains the partner-bound sequence in
     the NYCN rehearsal gate (in the partner repo):
     organizer presentation -> pilot formalization -> first
     operator rehearsal. This sync does not claim production
     readiness, live federation integration, implemented service
     hosting, K3s/DNS/GitHub/Forgejo mutation, NYCN private-data
     handling, live Google Drive/Groups/Sheets sync, or resolved
     licensing. -->

<!-- [sync edit] 2026-05-05 (post-#1751):
     Truth-sync for the Democratic Authority Primitives framing
     landing. Doc/control-plane and idea-refinery only — no runtime,
     no schema, no contract URN, no ADR, no RFC, no implementation
     issue, no runtime dogfood, no Phase 2 advance.
     Landings since the previous sync edit (2026-05-05 morning,
     post-#1749):
       - #1751 docs(ideas): name Democratic Authority Primitives
         as `idea-0020` with framing brief at
         `ops/ideas/framing/democratic-authority-primitives.md`.
         Pre-RFC framing only. Names two generic primitive families
         — authority/participation primitives (`AuthorityBasis`,
         `ParticipationRole`, `DelegationGrant`,
         `RepresentationMandate`, `ExpertStatement`,
         `AdvisoryOpinion`, `ConflictDisclosure`,
         `FacilitatorSummary`, `StewardReview`,
         `OperatorExecutionAuthority`, `MinorityReport`,
         `ChallengePath`, `RevocationPath`, `RecallPath`) and
         deliberation-context / educational-reference primitives
         (`DeliberationContext`, `ContextReference`,
         `LearningReference`, `EvidenceReference`,
         `PriorDecisionReference`, `CharterRuleReference`,
         `CCLRuleReference`, `AccessibilityNote`, `PrivacyNote`,
         `RiskNote`, `CounterargumentReference`,
         `GlossaryReference`). All names are candidates only;
         no schema, no URN, no implementation issue is opened.
         Composes orthogonally with `idea-0019` (Institutional
         Process Substrate): the spine names *what gets processed*;
         these primitives fill the spine's records with authority
         and context typing the spine deliberately deferred.
     Open coordination/control issues at this sync:
       - #1748 milestone(process): define Institutional Process
         Substrate. Unchanged from prior sync. Four acceptance
         gates remain unchecked.
       - #1746 milestone(showcase): make NYCN organizer rehearsal
         operable before first presentation. Unchanged.
       - #1744 ci(review): make substantive AI review findings
         merge-gating. Unchanged.
     Open PR queue at this sync: only Dependabot dev-dependency
     bumps (#1735 pilot-ui axe-core/playwright; #1736 TypeScript
     SDK dev-deps). Unchanged from prior sync.
     Next pre-RFC architecture move: **NOT YET SELECTED**. The
     prior sync named Democratic Authority Primitives as next; that
     framing now landed in #1751. This sync deliberately preserves
     optionality for the next session rather than smuggling in a
     new commitment. The candidate next moves the next session may
     pick from are listed descriptively below in the "Current
     status" paragraph; none is selected here.
     Phase 2 framing unchanged: NYCN remains the intended first
     cooperative partner, not a formally committed pilot. The
     concrete next gate remains the partner-bound sequence in
     the NYCN rehearsal gate (in the partner repo):
     organizer presentation -> pilot formalization -> first
     operator rehearsal. This sync does not claim production
     readiness, live federation integration, implemented service
     hosting, K3s/DNS/GitHub/Forgejo mutation, NYCN private-data
     handling, live Google Drive/Groups/Sheets sync, or resolved
     licensing. -->

<!-- [sync edit] 2026-05-05 (post-#1734 / #1739 / #1741 / #1743 /
     #1745 / #1747 / #1749, with open #1748):
     Truth-sync for the May-5 institutional-process-substrate
     sequence. Doc/control-plane only — no runtime, no schema,
     no contract URN beyond what already shipped, no implementation
     issue, no Phase 2 advance.
     Landings since the previous sync edit (2026-05-04):
       - #1734 docs(contracts): rehearsal evidence export schema
         (`urn:icn:contract:rehearsal-evidence-export:v1`).
       - #1739 docs(architecture): codify due-diligence checklist
         (`docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md`).
       - #1741 docs(contracts): audit schema identifiers
         (`docs/contracts/schema-id-audit.md`).
       - #1743 docs(design): organizer/member accessibility gate
         (`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`).
       - #1745 docs(contracts): preview/review read-model contract
         (`urn:icn:contract:preview-review:v1`).
       - #1747 docs(ideas): name Institutional Process Substrate
         as `idea-0019` with framing brief at
         `ops/ideas/framing/institutional-process-substrate.md`.
       - #1749 docs(ideas): read-model fixture-walk dogfood slice
         for `idea-0019` at
         `ops/ideas/dogfood/institutional-process-substrate-mvp.md`,
         plus the `ops/ideas/README.md` "Dogfood slice variants"
         section that formalizes this variant. Read-model only —
         emits no receipts, contacts no gateway, performs no
         mutation, introduces no new contract URN, and does NOT
         satisfy receipt-backed promotion thresholds.
     Open coordination/control issue:
       - #1748 milestone(process): define Institutional Process
         Substrate. `epic:arch-invariants` + `type:spec`. Acceptance
         criteria already record #1747 framing as merged and #1749
         read-model dogfood as the smallest-safe slice; runtime
         dogfood, visibility/privacy-boundary run, accessibility-
         gate `ProcessGateResult`, and open-question triage remain
         unchecked. No implementation work is opened from #1748
         until a runtime dogfood slice is explicitly scoped.
     Open PR queue at this sync: only Dependabot dev-dependency
     bumps (#1735 pilot-ui axe-core/playwright; #1736 TypeScript
     SDK dev-deps).
     Next pre-RFC architecture move (not started in this sync):
     Democratic Authority Primitives — generic delegation,
     representation, expert/advisory input, deliberation context /
     educational references, conflict disclosure, facilitator and
     steward/operator authority, and revocation/recall/challenge
     paths that institutions adopt and constrain through CCL,
     charters, and institution packages. Not an ICN app feature.
     Not an RFC by itself. Not a runtime commitment.
     Phase 2 framing unchanged: NYCN remains the intended first
     cooperative partner, not a formally committed pilot. The
     concrete next gate remains the partner-bound sequence in
     the NYCN rehearsal gate (in the partner repo):
     organizer presentation -> pilot formalization -> first
     operator rehearsal. This sync does not claim production
     readiness, live federation integration, implemented service
     hosting, K3s/DNS/GitHub/Forgejo mutation, NYCN private-data
     handling, live Google Drive/Groups/Sheets sync, or resolved
     licensing. -->

<!-- [sync edit] 2026-05-04 (post-#1725 / NYCN-#53 / #1732):
     Documentation and public-surface truth-sync. ICN #1725
     landed the generic no-CLI organizer/member rehearsal
     workflow spec at docs/pilots/no-cli-organizer-member-
     rehearsal-workflow.md — review-first, mutation-last,
     three roles distinguished (organizer / steward-operator /
     future member), accessibility baseline release-blocking
     for any user-facing shell, evidence export repo-safe by
     default. NYCN #53 landed the NYCN companion at
     docs/NO-CLI-ORGANIZER-REHEARSAL-WORKFLOW.md (in
     fahertym/nycn). ICN #1732 landed the website README
     civic design truth-sync, replacing stale Lexend / hex
     palette / "modern design system" / Tailwind framing
     with pointers to docs/design-language/ and
     website/src/styles/global.css as the single token
     surface. These are documentation and presentation-
     readiness changes only. No Phase 2 status change. NYCN
     remains the intended first cooperative partner, not a
     formally committed pilot. Implementation follow-ups
     remain open: ICN #1726-#1731 plus #1713 (organizer
     rehearsal shell, fixture-backed demo mode, generic
     preview/review read-model contract, repo-safe evidence
     export schema, private-overlay/DID-binding activation
     flow, accessibility review gate, ActionCard schema
     stabilization); NYCN #54-#58 (presentation wireframe
     deck, fixture demo packet, evidence packet example,
     holder-label/DID activation policy, accessibility/
     privacy checklist). Next substantive implementation
     path remains ICN #1729 (repo-safe evidence export
     schema). Do not read this sync as production readiness,
     live federation integration, implemented service
     hosting, K3s/DNS/Forgejo mutation, NYCN private-data
     handling, or resolved licensing. -->

<!-- [sync edit] 2026-05-02 (post-#1695/#1696/#1697/#1698/#1699/#1700/#1701):
     Follow-up May-cycle queue is now merged on main: Dependabot
     Actions major-version bumps (#1695-#1698), wasmtime security bump
     (#1699), unified dev-environment bootstrap (#1700), and the prior
     state sync (#1701). Open PR queue is empty at this sync. Phase 2
     remains in progress. NYCN remains the intended first cooperative
     partner, not a formally committed pilot. The exact next gate is
     defined in the NYCN rehearsal gate (in the partner repo):
     organizer presentation -> pilot formalization -> first operator
     rehearsal. Do not read this sync as production readiness, live
     federation integration, implemented service hosting, K3s/DNS/
     GitHub/Forgejo mutation, NYCN private-data handling, or resolved
     licensing. -->

<!-- [sync edit] 2026-04-29 (post-NYCN-#32, ICN PR queue clean):
     NYCN drive-ingest work has continued past the operator
     ladder + runbook landed in #28. Now also merged on the
     NYCN side: organizer briefing + simple summit demo (#29);
     start-here onboarding pass with quickstarts and glossary
     (#30); one-command local preflight runner (#31); whole-
     NYCN operating-surfaces inventory + Google-Groups boundary
     policy + repo-safe communication-groups fixture (#32). A
     small steward-facing communication-groups directory tool
     was open as NYCN #33 at last sync and may have merged
     since — verify before reading. ICN PR #1665 (Dependabot
     TS SDK dev-deps) merged 2026-04-29; ICN open-PR queue is
     empty as of this sync. The cooperator-developer prep
     brief landed alongside this sync at
     docs/strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md.
     Phase 2 framing unchanged from the prior sync: NYCN is
     the intended first cooperative partner (not yet formally
     committed); the next concrete step is presenting the
     merged ladder + ICN proof-loop machinery to NYCN
     organizers; partnership formalization and first operator
     pilot rehearsal remain. Issue #1646 still open;
     signal_rule and obligation_lifecycle source paths remain
     RFC-gated. -->

<!-- [sync edit] 2026-04-29 (post-#1675/#1677, post-NYCN-#28):
     Action-item completion-receipt retrieval endpoint is now live
     (`GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`,
     #1675). Local HTTP proof loop closure for the action-item path
     is recorded in #1676; the operator-authorized K3s NYCN smoke
     proof closure against deployed image 91a63eec is recorded in
     #1677. NYCN's drive-ingest operator ladder (#21–#28 in
     fahertym/nycn) is now merged: parser → review → decisions →
     publish dry-run → assignee binding → local publisher → local
     proof runner → federation surface bridge → operator pilot
     runbook + ladder checker. The procedural spine that walks
     organizer material into ICN action-item proofs is real.
     Phase 2 framing change: NYCN is the intended first
     cooperative partner; not yet a formally committed pilot.
     The next concrete step is **presenting the merged ladder
     + ICN proof-loop machinery to NYCN organizers** to
     formalize the pilot partnership. Subsequent gates are
     partnership formalization and the first operator pilot
     rehearsal against real (or fixture-equivalent) organizer
     material. Phase 2 remains ⏳ until those happen and are
     recorded. Issue #1646 still open; signal_rule and
     obligation_lifecycle source paths remain RFC-gated. -->

<!-- [sync edit] 2026-04-27 (post-#1663): Action-card runtime now has
     proof-bearing receipt loops for all three currently emitted source
     paths: proposal/vote (#1660), action_item/complete (#1661), and
     meeting/attend (#1663). Issue #1646 remains open with two RFC-
     gated paths still pending: signal_rule (gated on #1631) and
     obligation_lifecycle (gated on #1634). Phase model unchanged. -->

<!-- [sync edit] 2026-04-27: Append the action-card runtime sequence
     (/me/action-cards endpoint, proposal/vote receipt linkage,
     action_item completion receipt seam) landed via #1659/#1660/#1661.
     Issue #1646 remains open; meeting/attend, signal_rule, and
     obligation_lifecycle source paths remain pending. Phase model
     unchanged. -->

<!-- [sync edit] 2026-04-26: Append the institutional-operability sequence
     (live charter activation, person-directory overlay, /me/standing,
     authority_scope plumbing) and the doctrine/ADR canonicalization that
     landed since 2026-04-15. Open-PR table updated; 4-15 entries kept
     intact below for continuity. Phase model unchanged. -->

<!-- [sync edit] 2026-04-15: Consolidated stacked changelog into single current snapshot.
     Aligned crate list, merged PRs, and metrics to verified repo state.
     Phase model unchanged — phase classification is governance territory (PR C). -->

## Current status (2026-07-27 snapshot)

**Current phase:** Phase 2 — Pilot Launch (in progress, partner-bound). This snapshot is refreshed per truth-sync, not per commit, so it does not pin a `main` SHA — read git for the current head, and the newest-first `[sync edit]` blocks above for exact per-merge SHAs (this file is append-only; the comment blocks ARE the per-PR record). The architecture and deployment facts below were reconciled through the 2026-07-28 merge train recorded in the newest block.

**Architecture (A1 + B0 merged; B1 no-go; B2 not started).** The meaning firewall now has one authoritative crate taxonomy and a gate that can actually fail (A1, `4bdae326`) — this **measures** kernel/app separation honestly, it does not complete it, and it removes no dependency edge. B0 (`c1ea355e`) inverted the community edge: `icn-core` has **zero direct `icn-community` dependency and zero direct `icn_community::` source references**, with construction and LWW gossip-merge ownership moved to the `icnd` composition root. A transitive path `icn-core → icn-gateway → icn-community` **still exists** — B0 removed the direct edge and the ownership, not the graph reachability. **B1 (removing `icn-core → icn-ledger`) failed its design gate on 2026-07-25 and was never implemented**; it may not be built on the B0 opaque-handle pattern, and resuming it requires one authoritative composition root, one authoritative ledger implementation, typed recovery commands, explicit authority, and durable workflow evidence. Composition-root consolidation is the prerequisite tranche. **B2 has not begun.**

**Deployment boundary (public CI no longer touches private infrastructure).** Public `main` merges no longer invoke a private registry, SSH, K3s rollout, self-hosted runner, or scheduled homelab cleanup (`75d15750`, #2455); the replacement lane is a GitHub-hosted generic OCI build with `push: false` — build validation only. **Only the automatic private deployment from public CI was retired.** Kubernetes, K3s, and Helm remain available as optional operator material and are not removed from ICN.

**Appliance witness (2026-07-26).** The appliance demo-payload mode defect is fixed and merged (`425f513f`, #2456 — merged, not pending). The assembled single-node appliance was witnessed at integrated build head `67a6566e`, whose tree (`d3604c4c…`) is **byte-identical to the content that squash-merged as `425f513f`** — an ancestor of `main`, so the witness covers exactly that content and nothing merged after it, so the witness covers exactly this content: clean boot and firstboot, `icnd` under systemd returning health, organizer and member rehearsal flows, least-privilege negatives, wrong-digest rejection, completion receipt created and re-fetched, outbound isolation, service restart, and full VM reboot (`check.sh` 40/0). **Durability boundary:** node identity, machine ID, and config/genesis hashes survived reboot (boot ID changed, proving a real reboot) and the completion receipt stayed re-fetchable; the rehearsal **workspace view is intentionally ephemeral** and is reconstructed by reseeding. Durable identity + durable receipt is what was earned — not general workspace durability.

**Deployment profiles: recorded, proposed, not adopted.** ADR-0086 is **on `main`** (merged `9ca12148`, #2458) and linked from `docs/INDEX.md`. It records the four-profile direction — appliance = canonical sovereign node; Compose = disposable devnet; Kubernetes/K3s = optional operator infrastructure; native Linux = advanced install — at `status: proposed`, `implementation_status: partially implemented`. **Writing or merging that ADR does not adopt it**; adoption is a human decision and the `status:` field in `docs/adr/` is the owner of that fact. Only the appliance profile has a retained build-and-boot witness, and independent appliance restoration is **blocked** because `icnctl backup` omits `/etc/icn/icnd.env`, so a node restored from a backup alone cannot reopen its keystore.

**Evaluator package identity: corrected on `main`.** The name the evaluator lane previously shipped under — "Common Sense (bootable) vertical slice" — was **never an ICN-ratified identity**; it arrived with an externally assembled distribution. The correction landed as `1af341cb` (#2435): from **0.0.4** forward the identity is **`icn-portable-evaluator`**. **The owner of this fact is `deploy/appliance/evaluator/package-spec.env` (`PKG_STEM`) — read it there rather than inferring it from this file.** Independent of the correction: the affected release *payloads* are genuine (manifest `git_commit` values are real commits of this repository), and tags/assets at or below 0.0.3 are **retained unchanged** so published checksums keep verifying — never rename or delete them, and never reintroduce the old name into new material.

**Next executable gaps (scoped, not started).** Community gossip topic `community:updates` is still never created before subscribe in production wiring, so cross-node community gossip is dormant and publish failures are logged after local mutation — a pre-existing defect B0 neither caused nor fixed, filed as **#2457**, which withholds authorization for an opportunistic patch pending topic-ownership and failure-semantics decisions. **No two-node appliance proof has been executed**; two development nodes exchanging data would not be live federation.

**NYCN adoption state:** partner repo pinned to ICN `8c0fe926`; this window does not move that pin. NYCN remains the intended first partner — an active track, not a committed pilot.

**Open human gates (the project's primary gates):** the real organizer presentation → pilot formalization → first operator rehearsal (#1703/#1746; partner-side nycn#41/#52), and the member-shell human assistive-technology pass (#2041). Automated and assembled-image evidence closes none of these.

**Open software lanes (selected):** production trusted issuance (#2080); recurring assembled-image smoke (#2398); provider-boundary slice 3 (#2393); RPC credential lifetime (#2445); SDIS capability-vs-trust authority (#2447); unauthenticated anchor key rotation (#2448); community topic ownership (#2457).

**Non-claims:** no production readiness, no formal pilot, no organizer acceptance, no human-accessibility sign-off, no live federation, no two-node proof, no signed or immutable appliance, no independent appliance restoration, no adopted deployment ADR, and no claim that kernel/app separation is complete.

## Historical: Current status (2026-07-17 snapshot)

**Current phase (as of 2026-07-17):** Phase 2 — Pilot Launch (in progress, partner-bound). The **Rehearsal Node organizer→member loop** is merged (#2406 runtime, #2407 browser surface, #2408 appliance wiring, #2409 smoke driver) and was witnessed end-to-end on a fresh assembled image built from clean `main` `8c0fe926` on 2026-07-13: restrict=on boot → organizer no-paste session → review/edit/assign → digest-bound confirm (wrong digest → 409, fail-closed) → real ADR-0026 ladder → member completion → durable receipt → evidence export (`dids_exported=false`) → in-VM steward verify → outbound canary held. Per-PR detail: the newest-first `[sync edit]` blocks above (this file is append-only; the comment blocks ARE the per-PR record).

**LAN workstation witness (2026-07-15, merged-main):** the appliance's LAN single-origin profile is **merged to main** (#2424, #2425) and deployed as a dedicated hypervisor VM on operator-controlled LAN infrastructure from the **merged-main image `icn-appliance-0.0.3-lan-e74a8915`** (git_commit `e74a8915`, non-production, unsigned; supersedes the earlier `916629d7` branch image). A review-caught authority-boundary defect was fixed before merge: the LAN profile's gateway/member-shell/session are now all loopback-bound so the in-VM nginx is the only LAN HTTP surface. The full organizer→member loop was driven from a real Windows workstation browser against the merged-main deployment — one-click role-scoped sessions (no terminal, no credential paste, no credential in any URL), digest-bound confirm with server-verified wrong-digest 409, exactly one action item per confirm, fresh member session, completion, idempotent retry, in-VM steward verify PASS (negative capability matrix + value-withheld evidence), unattended recovery across service restart and VM reboot (node identity + sled durable; the rehearsal workspace view is intentionally ephemeral). A LAN development rehearsal, not production and not any human gate — see the 2026-07-15 sync block above.

**Portable evaluator release (2026-07-17, current-main; identity corrected 2026-07-19):** distinct from the LAN Rehearsal Node, the **ICN portable evaluator** (bootable vertical slice) is produced by a repository-owned, fail-closed generation lane (`deploy/appliance/evaluator/`, merged `f34f9f29`, PR #2428) rather than assembled ad hoc. Package stem: `icn-portable-evaluator` (from 0.0.4). The canonical release (tag `common-sense-vertical-slice-0.0.3-amd64` — the tag retains an externally introduced, since-corrected name that was never an ICN-ratified identity; see the 2026-07-19 sync block) was built from a current-main demo-profile image (git_commit `f34f9f29`, `non_production=true, signed=false, demo_profile=true`), generated through the merged lane, and **runtime-witnessed on a real KVM boot of the exact published bytes**: QEMU user-net with localhost-only host forwards (LAN address refused), disposable overlay (source image unchanged), one-command `setup-and-run.sh`, one-click role sessions with no credential in any URL, organizer review/assign/preview/digest-bound confirm (wrong digest → 409; correct → one action item + ADR-0026 ladder), member completion + idempotent retry, clean teardown, repeat run; the downloaded release bytes match the witnessed bytes. This is the reviewer-on-their-own-machine profile — a different threat model and audience from the operator-controlled LAN node, and the two profiles are deliberately not collapsed. The four ad-hoc 0.0.2 pre-releases were reconciled (two recipient-name-contaminated demo releases deleted; the two 0.0.2 vertical-slice releases marked superseded). Unsigned, non-production; not a pilot, not organizer acceptance, not accessibility completion, not federation.

**NYCN adoption state:** partner repo pinned to `8c0fe926`; facilitator gate package independently steward-operable (nycn#100/#101); `human_review: pending`. NYCN remains the intended first partner — an active track, not a committed pilot.

**Open human gates (the project's primary gates):** the real organizer presentation → pilot formalization → first operator rehearsal (#1703/#1746; partner-side nycn#41/#52), and the member-shell human assistive-technology pass (#2041). The assembled-image witness is automated evidence and closes none of these.

**Open software lanes (selected):** production trusted issuance architecture (#2080 — the appliance's `--local-mint` is operator bootstrap, not enrollment); recurring assembled-image smoke lane (#2398 — the harness is committed, a KVM-capable runner is owed); provider-boundary slice 3 (#2393); local-issuance audit-record implementation (spec merged `1ac5fd58`).

**Non-claims:** no production readiness, no formal pilot, no live federation, no organizer acceptance, no human-accessibility sign-off; the appliance ships `non_production=true, signed=false`; receipts record facts and grant zero authority.

## Historical: Current status (2026-05-15 snapshot)

**Current phase (as of 2026-05-15):** Phase 2 — Pilot Launch. NYCN is the intended first cooperative partner (active partnership track, not yet a formally committed pilot). The next concrete step is presenting the merged drive-ingest ladder + ICN proof-loop machinery + the now-complete architecture-spec ladder to NYCN organizers. Subsequent gates are pilot formalization, then first operator rehearsal against real (or fixture-equivalent) organizer material. The exact gate definition lives in the partner NYCN repo. The Phase 2 *machinery* is in place end-to-end at the runtime layer; the *contract* layer is now substantially richer at the design-level after the May-14/May-15 architecture-spec sprint. What remains is the human procedure — present, formalize, rehearse — and recording each step, plus the implementation work the closure batch's follow-up roster names.

The 2026-05-14 → 2026-05-15 architecture-spec sprint landed thirteen design-level spec PRs plus one process-doc PR plus one wrap-up PR (fifteen PRs total): #1814 integrated cooperative operating model spine, #1819 accepted-proposal effect dispatch contract (closed #1797 on merge), #1820 institutional domain and policy primitive, #1821 CCL policy registry and hook contract, #1822 governed service binding / workload manifest / runtime provider, #1823 storage durability policy objects, #1824 ArtifactRegistry v0 and ScopedVault boundary, #1825 entity-scope vocabulary boundary (LocalDomain not Coop), #1826 compute placement policy, #1827 reconciled the AGENTS.md handoff path with the actual template convention, #1829 network anti-entropy proof loops, #1830 member shell v0, #1831 steward cockpit v0, #1832 steward cockpit drift fix (four rounds of post-merge fixes addressing late reviewer feedback that landed after #1831 merged), and #1833 architecture-spec sprint closure-review wrap-up. After #1833 merged, the nine remaining sprint-related sibling issues were closed at the docs/spec level: #1794, #1795, #1798, #1799, #1801, #1815, #1816, #1817, #1818. The first batch of follow-up issues from the wrap-up's deduplicated roster was filed (#1834–#1840): three schema follow-ups (AntiEntropyProbe/StateDigest, DivergenceEvidence/RepairPlan, PlacementDecision/ExecutorAdmissionDecision), the steward required-action card contract (#1837 — the ADR-0027-doesn't-cover-operator-scenarios gap surfaced by #1831 / #1832), and three first-slice fixture follow-ups (anti-entropy Slice A, member shell Slice A, cockpit Slice A). Twenty-seven additional follow-up drafts remain in the wrap-up doc's deduplicated roster for separate batch decisions.

No closure of any sprint sibling issue implies runtime-implementation completion: every closure comment names the docs/spec-level scope explicitly. The deferred items (DataLocality::CoopReplicated rename, FuelLimit/fuel_limit code-level alignment with the `execution budget` policy-facing term, payment_rate/payment_currency legacy reconciliation on ComputeTask, PrivacyClass taxonomy reconciliation between ADR-0030 and the in-code variants, bonds:payments gossip-topic legacy preservation, AGENTS.md auto-commit-handoff behavioral rule reconciliation) are all carried in the wrap-up roster and remain explicit out-of-scope for the sprint closure. Phase 2 status is unchanged; the sprint did not implement, deploy, or claim partner pilot.

The prior May-7 close-out cycle context is preserved below.

The May-7 close-out cycle landed: #1761 closed the surfaced sled-flusher race (#1760), #1762 truth-synced STATE.md and PHASE_PROGRESS.md for the opaque receipt storage stack, #1763 / #1735 bumped Dependabot dev dependencies, and #1764 published the generic ActionCard contract surface for institution packages (bundled fictional example + draft-2020-12 validator script + expanded README mirroring the convention used by `validate-preview-review.py` and `validate-rehearsal-evidence.py`). #1764 closed #1713 with all six acceptance criteria met. No schema fields changed; the schema's `$id` remains DNS-backed under the schema-id audit's retain-temporarily decision (#1742 tracks the 2026-06-30 review). Phase 2 status is unchanged.

Active execution since the previous sync is mixed: the May-5 sequence was entirely doc/control-plane (#1734 rehearsal evidence export schema; #1739 architecture due-diligence checklist; #1741 contract schema-identifier audit; #1743 organizer/member accessibility gate definition; #1745 preview/review read-model contract `urn:icn:contract:preview-review:v1`; #1747 `idea-0019` Institutional Process Substrate framing brief; #1748 coordination/control milestone for spine composition; #1749 read-model fixture-walk dogfood slice for `idea-0019` plus the new `ops/ideas/README.md` "Dogfood slice variants" section; #1751 `idea-0020` Democratic Authority Primitives framing brief; #1753 read-model fixture-walk dogfood slice for `idea-0020`); the May-6/May-7 sequence is **runtime/implementation truth**, not doc/refinery — real Rust changes landed in `icn-gateway` and `apps/governance`. The first runtime dogfood emitting one of the eight named `ProcessTransitionReceipt` classes from the `idea-0019` framing brief landed as #1755 (`ProcessGateResultReceipt`), surfacing a production durability gap on the sled-backed `ReceiptStore` because no opaque storage path existed without expanding gateway typed governance imports. The opaque receipt storage stack (#1757 → #1758 → #1759) closed that gap: the gateway gained a meaning-blind `put_opaque` / `get_latest_opaque` / `list_opaque_for` primitive keyed on `(class, key1, key2_opt, recorded_at, record_hash)` (#1757); the `GovernanceReceiptBackend` trait gained a fail-closed opaque method surface and the sled-backed `ReceiptStore` overrode it via thin delegates to its inherent opaque methods (#1758); and `put_process_gate_result`'s trait default was rewritten to attempt the opaque cascade first and fall back to the explicit `process_gate_result_backend_not_implemented` sentinel only when the underlying `put_opaque` itself returns the opaque-not-implemented sentinel (#1759). Production gateway-backed `ReceiptStore` therefore now durably persists `ProcessGateResultReceipt` through the opaque cascade without any new typed governance import on `icn-gateway`. A new invariant landed inside the merge cycle: the `OPAQUE_HASH_BIND_PREFIX` keyspace binds each `(class, record_hash)` to exactly one canonical `(key1, key2_opt, recorded_at)` tuple at first write; divergent re-binds abort with stable sentinel `opaque_record_hash_index_collision` to prevent one canonical receipt from fanning out across multiple audit chains. Bind, primary-record, and secondary-index writes are enforced atomically inside a single sled transaction. CI on #1759 surfaced a pre-existing sled-flusher race on the unrelated `test_commons_charter_survives_sled_drop_and_reopen` integration test (sled 0.34's flusher thread holds the OS `flock(LOCK_EX)` past `Db::drop`); filed as issue #1760 with corrected diagnosis and a follow-up bounded-retry fix opened on `fix/commons-sled-open-retry-on-wouldblock` as PR #1761 (merged 2026-05-07; closed #1760). Carrying forward: rehearsal evidence export schema (#1734); architecture due-diligence checklist (#1739); contract schema-identifier audit (#1741); organizer/member accessibility gate definition (#1743); preview/review read-model contract `urn:icn:contract:preview-review:v1` (#1745); `idea-0019` framing brief (#1747) + read-model fixture-walk dogfood (#1749); `idea-0020` framing brief (#1751) + read-model fixture-walk dogfood (#1753). Carrying forward: institutional-operability runtime (live charter activation, person-directory overlay, `/me/standing`, `authority_scope` plumbing) plus the action-card runtime (`/me/action-cards` endpoint with proof-loop linkage to `GovernanceDecisionReceipt` for proposal/vote, `ActionItemCompletionReceipt` for action_item/complete, and `MeetingAttendanceReceipt` for meeting/attend). The action-item completion-receipt retrieval endpoint shipped as #1675; the local HTTP proof loop closure is documented in #1676 and the K3s smoke proof closure is recorded in #1677. NYCN's drive-ingest operator ladder (NYCN #21–#28 in `fahertym/nycn`) is merged end-to-end, with subsequent NYCN #29–#32 also merged. The May-5 process-substrate and authority-primitive sequence is documentation/refinery only: no runtime executes; no kernel, gateway, ledger, governance, or SDK code changed; no new contract URN beyond `urn:icn:contract:preview-review:v1` (#1745) was minted; no implementation issue was opened from #1748; and a read-model fixture walk does not satisfy receipt-backed promotion thresholds per `ops/ideas/README.md` § "Dogfood slice variants". Democratic Authority Primitives now has both pieces of its idea-refinery surface: framing brief landed in #1751 as `idea-0020` with brief at `ops/ideas/framing/democratic-authority-primitives.md`, and the read-model fixture-walk dogfood slice landed in #1753 at `ops/ideas/dogfood/democratic-authority-primitives-mvp.md`. The dogfood slice composes the six DAP primitive families named in the framing brief's §17 follow-up (`AuthorityBasis`, `ParticipationRole`, `FacilitatorSummary`, `ConflictDisclosure`, `MinorityReport`, `DeliberationContext` exercising three of its twelve reference families: `CharterRuleReference`, `PriorDecisionReference`, `AccessibilityNote`) plus references `OperatorExecutionAuthority` as the strictly-downstream-of-decision operator handle at the activation gate, all attached end-to-end to the merged `idea-0019` read-model fixture walk without modifying any kernel, runtime, gateway, ledger, governance, SDK, or contract file. Both DAP framing (#1751) and DAP read-model dogfood (#1753) are pre-RFC framing/refinery only; together they do not claim runtime validity, do not emit receipts, do not contact gateway, do not create schema, do not create a contract URN, do not promote to RFC, do not open implementation issues, do not start runtime dogfood, and do not claim Phase 2 completion, formal NYCN pilot, production readiness, or live federation. Per `ops/ideas/README.md` § "Dogfood slice variants" and per the DAP framing brief's §16.1, **the read-model fixture walk does NOT satisfy receipt-backed promotion thresholds**; promotion of `idea-0020` to RFC still requires (1) a separate runtime dogfood that emits at least one receipt under `ADR-0026` for one of the named primitives — the framing brief's §16.1 names a `ConflictDisclosure` accept receipt and a `MinorityReport` recorded receipt generically; the dogfood artifact references slice-local class candidates `ConflictDisclosureAcceptedReceipt` and `MinorityReportRecordedReceipt` at the right transition points but does not commit them as canonical, (2) a real visibility/privacy-boundary run with redaction in evidence export, (3) an accessibility-gate `ProcessGateResult` produced through `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` on a real surface, and (4) Q1 (`AuthorityBasis` polymorphism vs typed family) or Q5 (`ConflictDisclosure` and `MinorityReport` placement) **resolved** in writing (deferral is not sufficient for the RFC gate per §16.1; the resolved-or-deferred standard at §16.3 applies only to the broader runtime-justification threshold). Next pre-RFC architecture move is **not yet selected**; this sync deliberately preserves optionality for the next session rather than smuggling in a new commitment. The prior sync (post-#1751) named the DAP read-model composition slice as the most directly named candidate; #1753 has now landed it, so the candidate enumeration is reduced. The candidate next moves the next session may pick from, listed descriptively only: (a) DAP **runtime** dogfood emitting at least one receipt under `ADR-0026` for one DAP primitive — the next artifact called for by the slice's promotion gate; (b) `idea-0019` runtime dogfood emitting **additional** `ProcessTransitionReceipt` classes (the first — `ProcessGateResultReceipt` — landed in #1755 and is durably persisted via the opaque cascade since #1759; remaining candidates are `ProcessSessionOpenedReceipt`, `DeliberationEntryRecordedReceipt`, `DecisionRecordedReceipt`, `ActivationCrossedReceipt`, `MutationPlanRecordedReceipt`, `MutationAppliedReceipt`, `EvidencePacketProducedReceipt` — all eligible through the same opaque storage cascade); (c) `idea-0019` visibility/privacy-boundary run with redaction in evidence export (one of four #1748 acceptance gates); (d) `idea-0019` accessibility-gate `ProcessGateResult` produced through `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` on a real surface (one of four #1748 acceptance gates); (e) `idea-0019` open-question triage: at least one of Q1 (`ProcessTargetRef` polymorphism), Q3 (`DeliberationEntry` kind taxonomy), or Q4 (`HumanDecisionSet` vs proposal/vote) resolved or explicitly deferred in writing (one of four #1748 acceptance gates); (f) one of the DAP §17 follow-up framing briefs — pre-RFC, decompose-only (CCL hook-point catalog; expert/advisory across institution types; conflict object model connecting `ConflictDisclosure` to `idea-0016`/ADR-0029; federation tally semantics composing `RepresentationMandate` with #1609; delegation runtime gated on #1632); (g) control-plane cleanup, including unresolved/stale review-thread hygiene if inspection confirms it. None is selected here. Phase model classification is unchanged; see PHASE_PROGRESS.md for phase definitions.

### Recently merged (since 2026-04-15)

| PR | Title | Merged |
|----|-------|--------|
| #1833 | docs(dev): wrap architecture spec sprint closure review | 2026-05-15 |
| #1832 | fix(spec): correct steward cockpit review drift | 2026-05-15 |
| #1831 | docs(spec): define steward cockpit v0 | 2026-05-15 |
| #1830 | docs(spec): define member shell v0 | 2026-05-15 |
| #1829 | docs(spec): define network anti-entropy proof loops | 2026-05-15 |
| #1827 | docs(agents): reconcile handoff path with template | 2026-05-15 |
| #1826 | docs(spec): define compute placement policy | 2026-05-15 |
| #1825 | docs(architecture): define entity-scope vocabulary boundary | 2026-05-15 |
| #1824 | docs(spec): define ArtifactRegistry v0 and ScopedVault boundary | 2026-05-15 |
| #1823 | docs(spec): define storage durability policy objects | 2026-05-14 |
| #1822 | docs(spec): define governed service binding, workload manifest, and runtime provider | 2026-05-14 |
| #1821 | docs(spec): define CCL policy registry and hook contract | 2026-05-14 |
| #1820 | docs(spec): define institutional domain and policy primitive | 2026-05-14 |
| #1819 | docs(spec): add accepted-proposal effect dispatch contract | 2026-05-14 |
| #1814 | docs(architecture): add integrated cooperative operating model spine | 2026-05-14 |
| #1764 | docs(contracts): publish ActionCard contract for institution packages | 2026-05-07 |
| #1763 | deps(ts-sdk): bump the dev-dependencies group across 1 directory with 4 updates | 2026-05-07 |
| #1735 | deps(pilot-ui): bump @axe-core/playwright in /web/pilot-ui | 2026-05-07 |
| #1762 | docs(state): sync opaque receipt storage stack landing | 2026-05-07 |
| #1761 | fix(commons): retry sled open on WouldBlock to bridge flusher shutdown | 2026-05-07 |
| #1759 | feat(governance): route ProcessGateResultReceipt through opaque storage cascade | 2026-05-07 |
| #1758 | feat(governance): expose opaque storage on GovernanceReceiptBackend trait | 2026-05-07 |
| #1757 | feat(gateway): add meaning-blind opaque receipt storage primitive | 2026-05-06 |
| #1756 | chore(hooks): fix scope-guard/todo-guard exec bit and todo-guard pipeline | 2026-05-06 |
| #1755 | feat(governance): add first process-transition receipt runtime slice | 2026-05-06 |
| #1754 | docs(state): sync Democratic Authority Primitives read-model dogfood landing | 2026-05-06 |
| #1753 | docs(ideas): add read-model dogfood slice for Democratic Authority Primitives (idea-0020) | 2026-05-05 |
| #1752 | docs(state): sync Democratic Authority Primitives landing and agent handoff | 2026-05-05 |
| #1751 | docs(ideas): name Democratic Authority Primitives (idea-0020 + framing brief) | 2026-05-05 |
| #1750 | docs(state): sync process substrate landings and agent handoff | 2026-05-05 |
| #1749 | docs(ideas): add read-model dogfood slice for Institutional Process Substrate (idea-0019) | 2026-05-05 |
| #1747 | docs(ideas): name Institutional Process Substrate (idea-0019 + framing brief) | 2026-05-05 |
| #1745 | docs(contracts): define preview review contract | 2026-05-05 |
| #1743 | docs(design): define organizer member accessibility gate | 2026-05-05 |
| #1741 | docs(contracts): audit schema identifiers | 2026-05-05 |
| #1739 | docs(architecture): codify due-diligence checklist | 2026-05-04 |
| #1734 | docs(contracts): define rehearsal evidence export schema | 2026-05-04 |
| #1733 | docs(state): sync no-CLI and website cleanup tranche | 2026-05-04 |
| #1732 | docs(website): align README with current civic design system | 2026-05-04 |
| #1725 | docs(pilots): add no-CLI organizer/member rehearsal workflow spec | 2026-05-04 |
| #1701 | docs(state): sync May-cycle project truth | 2026-05-02 |
| #1700 | chore: unify dev environment setup into scripts/bootstrap.sh | 2026-05-02 |
| #1699 | fix(compute): bump wasmtime for RUSTSEC-2026-0114 | 2026-05-02 |
| #1698 | ci: bump actions/setup-node from 4 to 6 | 2026-05-02 |
| #1697 | ci: bump actions/checkout from 4 to 6 | 2026-05-02 |
| #1696 | ci: bump actions/github-script from 8 to 9 | 2026-05-02 |
| #1695 | ci: bump softprops/action-gh-release from 2 to 3 | 2026-05-02 |
| #1694 | docs(architecture): add sovereign service hosting stack | 2026-05-02 |
| #1693 | docs(licensing): add autonomy-focused strategy matrix | 2026-05-02 |
| #1691 | docs(project-index): add generated repo record snapshot | 2026-05-01 |
| #1690 | docs(project-index): add full repo record protocol | 2026-05-01 |
| #1688 | docs(rfcs): RFC-0017 draft → active (Tool Install Infrastructure) | 2026-05-01 |
| #1686 | docs(licensing): document current license metadata and open questions | 2026-05-01 |
| #1678 | docs(state): sync to post-#1675/#1677 and post-NYCN-#28 reality | 2026-04-29 |
| #1665 | deps(ts-sdk): bump the dev-dependencies group in /sdk/typescript with 2 updates | 2026-04-29 |
| #1677 | docs(dev): record K3s NYCN action-item receipt proof path | 2026-04-29 |
| #1676 | docs(dev): record action-item completion receipt endpoint | 2026-04-29 |
| #1675 | feat(governance): add completion-receipt endpoint for action items | 2026-04-29 |
| #1663 | feat(governance): add meeting attendance receipts | 2026-04-27 |
| #1662 | docs(state): record action-card runtime landing (#1659/#1660/#1661) | 2026-04-27 |
| #1661 | feat(governance): add action item completion receipts | 2026-04-27 |
| #1660 | feat(governance): connect action cards to receipts | 2026-04-27 |
| #1659 | feat(gateway): add member action cards endpoint | 2026-04-27 |
| #1658 | docs(sync): record ICN Academy repo creation | 2026-04-27 |
| #1656 | docs(site): add curated docs pathways | 2026-04-27 |
| #1637 | docs: reframe feedback doctrine and canonicalize ADR location | 2026-04-26 |
| #1630 | feat(governance): plumb authority_scope through assign_role end-to-end | 2026-04-25 |
| #1627 | feat(governance): add GET /me/standing read model | 2026-04-25 |
| #1626 | feat(governance): person-directory overlay for bootstrap role assignment | 2026-04-25 |
| #1625 | fix(coop): release sled db lock before reopen test | 2026-04-25 |
| #1624 | feat(governance): live charter activation endpoint | 2026-04-25 |
| #1622 | docs(strategy): institutional ecosystem arc — NYCN as first ecosystem seed | 2026-04-24 |
| #1621 | fix(governance): persist domains across gateway restart in standalone mode | 2026-04-24 |
| #1620 | fix(web): derive steward dashboard gateway URL from request context | 2026-04-24 |
| #1619 | feat(infra): add soft pod anti-affinity for ICN daemons | 2026-04-23 |
| #1618 | feat(ci): add Atlas-backed sccache setup for ci-runner | 2026-04-23 |
| #1617 | fix(bootstrap): treat remaining create conflicts as idempotent | 2026-04-22 |
| #1616 | docs(monitoring): document Helm access path for kube-prometheus-stack upgrade | 2026-04-22 |
| #1614 | fix(monitoring): move Prometheus to Atlas-backed persistent storage | 2026-04-22 |
| #1593 | docs(nycn): live-validate bootstrap apply and rewrite runbook | 2026-04-19 |
| #1592 | test(icnctl): NYCN bootstrap apply integration tests | 2026-04-19 |
| #1591 | fix(gateway): colon-safe proposal index keys with one-shot migration | 2026-04-19 |
| #1590 | fix(governance): close residual acceptance-closure atomicity hazards | 2026-04-18 |
| #1586 | feat(governance): add generic institution bootstrap package path | 2026-04-18 |

### Recently merged (2026-04-15 snapshot, retained)

| PR | Title | Merged |
|----|-------|--------|
| #1547 | feat(governance): notification digest + action-item/meeting events | 2026-04-15 |
| #1546 | docs(dev): session handoff 2026-04-15 | 2026-04-15 |
| #1545 | docs(strategy): correct NYCN-Institutional-Design entity tree | 2026-04-15 |
| #1544 | docs(strategy): NYCN repo-shaped architecture spec + matrix + tranches | 2026-04-15 |
| #1543 | feat(governance): Meeting management primitive | 2026-04-15 |
| #1542 | chore(security): fix Security Audit CI failure | 2026-04-14 |
| #1540 | feat(governance): institutional structure + event model (Tranche 2, part 1) | 2026-04-14 |
| #1534 | docs(strategy): NYCN federation charter draft (CCL YAML) | 2026-04-14 |
| #1533 | feat(governance): consent-based decision mode | 2026-04-14 |
| #1532 | feat(governance): decision-to-action bridge | 2026-04-14 |
| #1529 | chore(repo): add GitHub Sponsors funding button | 2026-04-14 |
| #1527 | fix(ci): add timeout-minutes to docker-build-deploy jobs | 2026-04-11 |
| #1526 | docs: full refresh — archive 21 stale files | 2026-04-11 |
| #1525 | docs(architecture): Constitutional Genesis | 2026-04-11 |
| #1524 | fix(ci): add has_rust dual-signal guard | 2026-04-11 |

### Open PRs

| PR | Title |
|----|-------|

(none open at this sync write-time; verified via `gh pr list --state open` returning `[]`. Dependabot may surface follow-on bumps automatically.)

Open implementation follow-ups at this sync:

| Issue | Title |
|-------|-------|

(none — #1760 was closed by the #1761 merge.)

Open coordination/control issues at this sync (not implementation):

| Issue | Title |
|-------|-------|
| #1748 | milestone(process): define Institutional Process Substrate (`epic:arch-invariants` + `type:spec`) |
| #1746 | milestone(showcase): make NYCN organizer rehearsal operable before first presentation |
| #1744 | ci(review): make substantive AI review findings merge-gating |

### What landed since Phase 1 (Charter Engine)

ActionCard contract publication for institution packages (added 2026-05-07; **doc/control-plane only** — no runtime change, no schema fields changed, no new contract URN, no new ADR, no new RFC):
- Bundled fictional example landed at `docs/contracts/institution-package/action-card.example.json` — a single representative `proposal`/`vote` `ActionCard` with all required fields plus optional `deadline` and `domain_id`. Uses fictional ids (`prop-example-2026-05-07-001`, `domain-example-fictional-cooperative`); contains no NYCN-specific nouns. Validates against the existing schema. — #1764.
- Tiny draft-2020-12 JSON Schema validator landed at `docs/scripts/validate-action-card.py`. Mirrors the existing convention used by `validate-preview-review.py` and `validate-rehearsal-evidence.py`. Defaults to validating the bundled example; accepts a partner-side card path positional; supports `--schema` for pinned-version validation. CLI argument is `card`/`DEFAULT_CARD` (terminology aligned with the schema, ADR-0027, README, and runtime struct after a substantive Copilot review finding addressed pre-merge). Stdlib-only `format: date-time` and `format: uri` checkers registered for symmetry with the other contract validators (the action-card schema does not currently use either format; future format additions will be enforced without touching this file). — #1764.
- `docs/contracts/institution-package/README.md` expanded — #1764: Files table now lists the schema + example + validator. Stability section cites ADR-0027 § Card kind taxonomy ("growable by ADR amendment") to explain why `"x-icn-status": "rfc"` is honest, and documents the schema's current DNS-backed `$id` retention per `docs/contracts/schema-id-audit.md` (review by 2026-06-30 tracked by #1742; migration to `urn:icn:contract:action-card:v<N>` is a separate single-schema PR under audit §5 rules). Validation guidance now includes the explicit emitted-vs-gated source kind enumeration with tracking issues (`signal_rule` → #1631 / #1711; `obligation_lifecycle` → #1634 / #1712), the regulatory-safe vocabulary list (obligation, allocation, settlement, unit, position, receipt, provenance, evidence — explicitly not payment / wallet / balance / currency), the explicit "institution-specific semantics belong in institution packages, not in ICN core" boundary, a worked CLI command block for the new validator, and partner-package vendor-or-invoke-from-CI guidance.
- `docs/registry.toml` — #1764: `last_updated` and `last_reviewed` advanced 2026-05-04 → 2026-05-07 for the institution-package README entry; `description` refreshed to mention the new example, validator, and schema-id-audit retention decision.
- Closes #1713: all six acceptance criteria met by the merged PR (generic ActionCard schema exists and matches current runtime fields; honest stability/status marker; source kinds distinguish shipped vs gated variants; NYCN-specific nouns absent; regulatory-safe vocabulary preserved; package validation path documented for NYCN and future institution packages). Manually closed after merge with a comment enumerating each gate met by the merged PR. The schema and ADR-0027 existed before this PR; #1764 added only the example, validator, README expansions, and registry metadata.
- Hard rule preserved: this publication does NOT change the schema fields, does NOT mint a new contract URN, does NOT add new ADR / RFC content, does NOT touch runtime code, does NOT widen gateway typed governance imports, does NOT increase the meaning-firewall ratchet, does NOT touch K3s / DNS / Forgejo state, does NOT handle private partner data, does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, and does NOT start any Stage 1.5 / Stage 2 / Stage 3 / Stage 4 / Stage 5 work.

May-7 close-out cycle (added 2026-05-07; doc/control-plane and dependency maintenance only, plus one runtime fix):
- Sled-open retry-on-`WouldBlock` shipped — #1761 (closed #1760). Bounded retry-with-backoff in `SledCommonsStore::open` (8 attempts max, 500ms total budget cap, 10ms initial backoff, only matches `io::ErrorKind::WouldBlock` so genuine errors are not masked). Two new unit tests pin the new behavior. Single-file change in `icn/crates/icn-commons/src/store.rs`. Diagnosis was corrected pre-merge from initial actor-drop hypothesis to sled-flusher-flock-shutdown.
- Truth-sync of opaque receipt storage stack landing — #1762. Records #1755/#1756/#1757/#1758/#1759 in `docs/STATE.md` and `docs/PHASE_PROGRESS.md`. Adds `docs/dev/handoff-2026-05-07.md`. Doc/control-plane only.
- Dependabot dev-dependency maintenance — #1763 (`sdk/typescript/`, four updates) and #1735 (`web/pilot-ui/`, `@axe-core/playwright` 4.11.2 → 4.11.3). No runtime change.

Opaque receipt storage stack (added 2026-05-06 → 2026-05-07; **runtime/implementation truth** — real Rust changes in `icn-gateway` and `apps/governance`; no firewall ratchet increase; no new typed governance imports on `icn-gateway`):
- First runtime dogfood emitting one of the eight named `ProcessTransitionReceipt` classes from the `idea-0019` framing brief landed as #1755 (`feat(governance): add first process-transition receipt runtime slice`). Adds `ProcessGateResultReceipt`, emitted by `GovernanceManager::record_process_gate_result`, persisted through the `GovernanceReceiptBackend` trait. Surfaced a production durability gap on the sled-backed `ReceiptStore` because no opaque storage path existed without expanding gateway typed governance imports — addressed by the #1757 → #1758 → #1759 stack.
- Meaning-blind opaque receipt storage primitive landed at `icn/crates/icn-gateway/src/receipt_store.rs` — #1757. Adds `put_opaque(class, key1, key2_opt, recorded_at, record_hash, payload)` plus `get_latest_opaque` and `list_opaque_for` inherent methods on `ReceiptStore`. The gateway stores payloads under a caller-supplied `(class, key1, key2_opt, recorded_at, record_hash)` tuple without learning the typed shape; the apps layer is the single source of truth for the closed taxonomy of class strings. Adding a new receipt class becomes a one-file change in apps. Three substantive review findings addressed in `cb9d6daf` before merge (write-once-by-hash on the primary record with stable sentinel `opaque_record_hash_collision`; atomic primary + secondary index writes via single sled transaction; distinct `key2 = None` vs `key2 = Some("")` tag-byte encoding; deterministic `(recorded_at, record_hash)` tie-breaker). One additional codex P2 raised against `cb9d6daf` and addressed in `a8fbb1a6` before merge: the new `OPAQUE_HASH_BIND_PREFIX` keyspace binds each `(class, record_hash)` to exactly one canonical `(key1, key2_opt, recorded_at)` tuple at first write; divergent re-binds abort with stable sentinel `opaque_record_hash_index_collision`. Bind, primary, and secondary writes are enforced atomically inside the same sled transaction.
- Opaque storage exposed on the `GovernanceReceiptBackend` trait at `icn/apps/governance/src/receipt_backend.rs` — #1758. Adds `put_opaque` / `get_latest_opaque` / `list_opaque_for` to the trait surface, each with a fail-closed default returning the stable sentinel `opaque_storage_not_implemented`. The sled-backed `ReceiptStore` overrides them via thin delegates to its inherent opaque methods. Existing typed test backends are unaffected; opaque methods are only exercised when callers explicitly route through them. Validates dynamic dispatch via a `Box<dyn GovernanceReceiptBackend>` round-trip test.
- `ProcessGateResultReceipt` routed through opaque storage cascade — #1759. Updates the trait default for `put_process_gate_result` to attempt the opaque cascade first (encoding the typed envelope as JSON, calling `put_opaque` with class `"process_gate_result"`, `key1 = session_id`, `key2 = Some(gate_kind)`, the typed `recorded_at` and `record_hash`), and to surface the explicit `process_gate_result_backend_not_implemented` sentinel only when the underlying `put_opaque` itself returns the opaque-not-implemented sentinel. Production gateway-backed `ReceiptStore` therefore now durably persists `ProcessGateResultReceipt` through the opaque cascade. Test-backend coverage: a new `OpaqueOnlyBackend` overrides only `put_opaque` and exercises the typed-default → opaque cascade end-to-end. Test-suite determinism follow-up was applied in the same PR (Copilot review): three tests previously used `std::thread::sleep(Duration::from_millis(1100))` to force `recorded_at` to advance one second between writes — replaced with explicit, strictly-increasing `recorded_at` timestamps on directly-constructed `ProcessGateResultReceipt` values fed through the backend trait. Suite now finishes in 0.01s, deterministic.
- New invariant: `OPAQUE_HASH_BIND_PREFIX` keyspace in `icn/crates/icn-gateway/src/receipt_store.rs`. Each `(class, record_hash)` is bound to exactly one canonical `(key1, key2_opt, recorded_at)` tuple. Divergent re-binds abort with stable sentinel `opaque_record_hash_index_collision`. Closes a secondary-index fan-out hole that the original write-once-by-hash check on `OPAQUE_REC_PREFIX` did not catch. Bind, primary, and secondary writes are atomic inside the same sled transaction.
- Surfaced flake → real bug filed and fix opened: a pre-existing race on `test_commons_charter_survives_sled_drop_and_reopen` (sled 0.34's flusher thread holds the OS `flock(LOCK_EX)` past `Db::drop`) fired on #1759's CI Test job. Filed as issue #1760 with corrected diagnosis (initial actor-drop hypothesis was wrong; `CommonsHandle` is synchronous `Arc<RwLock<CommonsInner>>` with no spawned tasks). Fix opened as PR #1761 (`fix(commons): retry sled open on WouldBlock to bridge flusher shutdown`) — bounded retry-with-backoff in `SledCommonsStore::open`, 8 attempts max, 500ms total budget cap, 10ms initial backoff, only matches `io::ErrorKind::WouldBlock` so genuine errors (NotFound, PermissionDenied, etc.) are not masked. Two new unit tests pin the new behavior. Open at this sync write-time.
- Hook tooling fix: scope-guard / todo-guard exec bit + todo-guard pipeline failures observed in earlier sessions resolved in #1756. Repository tooling only; no runtime, contract, schema, or API change.
- Hard rule preserved: this stack does NOT widen gateway typed governance imports, does NOT increase the meaning-firewall ratchet (baseline 10 known violations preserved, 0 new), does NOT claim Phase 2 completion, does NOT claim formal NYCN pilot, does NOT claim production readiness, does NOT claim live federation, does NOT touch K3s/DNS/GitHub/Forgejo state, does NOT handle private partner/member/organizer data, and does NOT satisfy more than acceptance gate (a) of `idea-0019` (#1748) — the visibility/privacy-boundary run, accessibility-gate `ProcessGateResult` on a real surface, and open-question triage gates remain open.

Democratic Authority Primitives read-model fixture-walk dogfood (added 2026-05-05; doc/control-plane and idea-refinery only, not runtime; no kernel, gateway, ledger, governance, or SDK code touched):
- Read-model fixture-walk dogfood slice for `idea-0020` landed at `ops/ideas/dogfood/democratic-authority-primitives-mvp.md` alongside an `ops/ideas/ideas.yaml` row update — #1753. Read-model fixture-walk variant per `ops/ideas/README.md` § "Dogfood slice variants" (formalized in #1749). Composes the six DAP primitive families named in the framing brief's §17 follow-up (`AuthorityBasis`, `ParticipationRole`, `FacilitatorSummary`, `ConflictDisclosure`, `MinorityReport`, `DeliberationContext` — the latter exercising three of its twelve reference families: `CharterRuleReference`, `PriorDecisionReference`, `AccessibilityNote`) end-to-end against the merged `idea-0019` read-model fixture walk (`ops/ideas/dogfood/institutional-process-substrate-mvp.md`). Walks `Step 0` through `Step 7` of the existing `idea-0019` slice without re-describing the spine; only DAP primitive additions are recorded. References `OperatorExecutionAuthority` as the strictly-downstream-of-decision operator handle at the activation gate (Step 5), typed to point at the `DecisionRecord` plus the `ProcessGateResult` set plus the steward's `RoleAssignment`. Composes orthogonally with `idea-0019`: the spine names *what gets processed*; the primitives fill the spine's records with the authority and context typing the spine deliberately deferred. Emits no receipts, contacts no gateway, performs no mutation, introduces no new contract URN, modifies no kernel/runtime/contract/schema/ADR file. Receipt class candidates `FacilitatorSummaryRecordedReceipt`, `ConflictDisclosureAcceptedReceipt`, and `MinorityReportRecordedReceipt` are referenced at the right transition points as slice-local candidates only — the framing brief's §16.1 names a `ConflictDisclosure` accept receipt and a `MinorityReport` recorded receipt generically without attaching concrete class identifiers, and the slice does not commit any of these names as canonical. Per `ops/ideas/README.md` § "Dogfood slice variants" and per the DAP framing brief's §16.1, **a read-model fixture walk does NOT satisfy receipt-backed promotion thresholds**; promotion of `idea-0020` to RFC still requires (1) a separate runtime dogfood emitting at least one receipt under `ADR-0026` for one of the named primitives, (2) a real visibility/privacy-boundary run with redaction in evidence export, (3) an accessibility-gate `ProcessGateResult` produced through `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` on a real surface, and (4) Q1 (`AuthorityBasis` polymorphism vs typed family) or Q5 (`ConflictDisclosure` and `MinorityReport` placement) **resolved** in writing — deferral is not sufficient for the RFC gate per §16.1; the resolved-or-deferred standard at §16.3 applies only to the broader runtime-justification threshold. The DAP brief's other open questions (Q2 through Q4, Q6 through Q10) are not surfaced by this slice and remain open. Hard rule preserved per DAP framing brief §14: not runtime, not a schema, not an RFC by itself, not a voting-system decision, not a liquid-democracy commitment, not expertocracy, not anti-expertise, not chat, not social media, not a moderation platform, not an identity directory implementation, not a credential verification implementation, not a private-overlay implementation, not NYCN-specific, not a production-readiness claim, not a Phase 2 completion claim, not a formal NYCN pilot authorization, not a live federation claim, not a live cloud sync claim, not a K3s/DNS/Forgejo mutation claim, not a private-data-handling claim, not a binding on partner repositories.

Democratic Authority Primitives framing (added 2026-05-05; doc/control-plane and idea-refinery only, not runtime; no kernel, gateway, ledger, governance, or SDK code touched):
- `idea-0020` Democratic Authority Primitives framing brief landed at `ops/ideas/framing/democratic-authority-primitives.md` and the matching idea-refinery row in `ops/ideas/ideas.yaml` — #1751. Pre-RFC framing only; not an RFC, not an ADR, not a schema, not a contract URN, not a backlog commitment. Names two generic primitive families (authority/participation: `AuthorityBasis`, `ParticipationRole`, `DelegationGrant`, `RepresentationMandate`, `ExpertStatement`, `AdvisoryOpinion`, `ConflictDisclosure`, `FacilitatorSummary`, `StewardReview`, `OperatorExecutionAuthority`, `MinorityReport`, `ChallengePath`, `RevocationPath`, `RecallPath`; deliberation context / educational reference: `DeliberationContext`, `ContextReference`, `LearningReference`, `EvidenceReference`, `PriorDecisionReference`, `CharterRuleReference`, `CCLRuleReference`, `AccessibilityNote`, `PrivacyNote`, `RiskNote`, `CounterargumentReference`, `GlossaryReference`). Composes orthogonally with `idea-0019` (Institutional Process Substrate): the spine names *what gets processed*; these primitives fill the spine's records with the authority and context typing the spine deliberately deferred. Hard rule preserved: institutions adopt and constrain through CCL, charters, and institution packages — not as ICN app features. Promotion to RFC requires (per the brief's §16.1 promotion gate) a read-model composition slice with `idea-0019`, a runtime dogfood emitting at least one receipt under `ADR-0026`, a real visibility/privacy-boundary run, an accessibility-gate `ProcessGateResult` on a real surface, and at least one open question — Q1 (`AuthorityBasis` polymorphism vs typed family) or Q5 (`ConflictDisclosure` and `MinorityReport` placement) — **resolved** in writing. Deferral is **not** sufficient for the RFC gate per §16.1; the lenient resolved-or-deferred standard at §16.3 applies only to the broader runtime-justification threshold, not to RFC promotion. None of those follow-ups is started in this sync; the next move is **not yet selected**.

Institutional Process Substrate framing and read-model dogfood (added 2026-05-04 → 2026-05-05; doc/control-plane and idea-refinery only, not runtime; no kernel, gateway, ledger, governance, or SDK code touched):
- Rehearsal evidence export schema landed under `docs/contracts/rehearsal-evidence-export.md` and `docs/contracts/rehearsal-evidence-export.schema.json` defining `urn:icn:contract:rehearsal-evidence-export:v1` — #1734. Contract definition only; no live evidence export pipeline runs.
- Architecture due-diligence checklist landed at `docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md` — #1739. Reflex/process artifact only; no architectural change.
- Contract schema-identifier audit table landed at `docs/contracts/schema-id-audit.md` — #1741. Inventory/discipline only; no schema change.
- Organizer/member accessibility gate definition landed at `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` — #1743. PR-time gate definition only; no UI/runtime change.
- Preview/review read-model contract `urn:icn:contract:preview-review:v1` landed under `docs/contracts/preview-review.md`, `docs/contracts/preview-review.schema.json`, and `docs/contracts/preview-review.example.json` — #1745. Read-model contract definition only; no read-model serves over a gateway today.
- `idea-0019` Institutional Process Substrate framing brief landed at `ops/ideas/framing/institutional-process-substrate.md` and the matching idea-refinery row in `ops/ideas/ideas.yaml` — #1747. Pre-RFC framing only; not an RFC, not an ADR, not a schema, not a backlog commitment.
- Read-model fixture-walk dogfood slice for `idea-0019` landed at `ops/ideas/dogfood/institutional-process-substrate-mvp.md`, alongside the new `ops/ideas/README.md` § "Dogfood slice variants" section that formalizes this variant convention — #1749. Fictional Example Cooperative process session walked end-to-end against the SAME shipping contract URNs as the committed examples (`urn:icn:contract:preview-review:v1`, `urn:icn:contract:rehearsal-evidence-export:v1`); emits no receipts, contacts no gateway, performs no mutation, introduces no new contract URN. A read-model fixture walk does NOT satisfy receipt-backed promotion thresholds; receipt-backed promotion of `idea-0019` to RFC still requires (1) a separate runtime dogfood slice that emits at least one `ProcessTransitionReceipt` class under `ADR-0026`, (2) a real visibility/privacy-boundary run with redaction in the evidence export, (3) a real accessibility-gate `ProcessGateResult` produced through the accessibility-gate checklist, and (4) at least one framing-brief open question among Q1/Q3/Q4 resolved or explicitly deferred in writing.
- Coordination/control milestone issue #1748 (`milestone(process): define Institutional Process Substrate`) is open with `epic:arch-invariants` + `type:spec`. Acceptance criteria record #1747 framing as merged and #1749 read-model dogfood as the smallest-safe slice; runtime dogfood, visibility-boundary run, accessibility-gate `ProcessGateResult`, and open-question triage remain unchecked. No implementation work is opened from #1748 until a runtime dogfood slice is explicitly scoped.
- Next pre-RFC architecture move: **Democratic Authority Primitives** (delegation, representation, expert/advisory input, deliberation context / educational references, conflict disclosure, facilitator and steward/operator authority, and revocation/recall/challenge paths) as generic primitives institutions adopt and constrain through CCL, charters, and institution packages. Not started in this sync. Not an ICN app feature. Not an RFC by itself. Not a runtime commitment.

May-cycle repo governance and strategy documentation (added 2026-05-01 → 2026-05-02; documentation/control-plane only, not runtime deployment):
- Licensing metadata and open questions documented — #1686.
- RFC-0017 moved from draft to active for Tool Install Infrastructure — #1688. Active means accepted for implementation; it does not mean the tool install infrastructure is implemented.
- Full repo-record protocol/generator added — #1690.
- Generated ICN repo-record snapshot added — #1691. This is a mechanical inventory snapshot, not an interpretive atlas.
- Licensing/autonomy strategy matrix added — #1693. Planning only; no relicensing happened.
- Sovereign service hosting stack added — #1694. Design direction only; no Forgejo deployment, DNS mutation, K3s mutation, hosted-service rollout, or GitHub cutover happened.
- Follow-up maintenance/state queue merged — #1695-#1701. This includes CI action bumps, a wasmtime security bump, unified bootstrap setup, and a prior state sync; none of these changes starts a NYCN pilot or completes Phase 2.
- NYCN organizer/operator rehearsal gate defined (lives in the partner NYCN repo). The gate remains organizer presentation -> pilot formalization -> first operator rehearsal.

Action-card runtime (added 2026-04-27 → 2026-04-29, all currently emitted source paths now proof-bearing — issue #1646 remains open for the two RFC-gated paths):
- `GET /v1/gov/me/action-cards` member endpoint with closed source/action enums — #1659
- Proposal/vote action card → `GovernanceDecisionReceipt` proof linkage, end-to-end test — #1660
- `action_item`/`complete` source path emits append-only `ActionItemCompletionReceipt` (ADR-0026 Layer 2); persist-before-commit semantics; full-update handler routes status changes through receipt-bearing path — #1661
- `meeting`/`attend` source path emits append-only `MeetingAttendanceReceipt` (ADR-0026 Layer 2) keyed by `(meeting_id, attendee_did)`; `Present` and `Remote` are receipt-bearing transitions, `Absent` is not; `recorded_by` is the authenticated caller (distinct from `attendee_did` for steward-recorded attendance); persist-before-commit semantics — #1663
- `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` retrieval endpoint — #1675; closes the proof loop on the read side so a holder shell that completed an `action_item`/`complete` action card can fetch the persisted `ActionItemCompletionReceipt` over HTTP instead of relying on in-process tests or on-disk Sled inspection. Authorization mirrors the rest of the action-item read surface (`governance:read` scope plus domain membership; the receipt's bound `domain_id` is asserted to match the path parameter so cross-domain probes are rejected).
- Local HTTP proof loop closure recorded — #1676.
- K3s smoke proof closure (operator-authorized, against deployed image `91a63eec`) recorded — #1677. K3s smoke records remain durable devnet proof artifacts; full namespaced teardown semantics are not yet specified (tracking issue planned).
- Source paths currently emitted by `/me/action-cards`: `proposal`/`vote`, `meeting`/`attend`, `action_item`/`complete`
- **Proof loop verified end-to-end for all three currently emitted source paths, both locally and on K3s.**
- Pending under #1646 (RFC-gated): `signal_rule` source path (gated on #1631); `obligation_lifecycle` source path (gated on #1634)

NYCN drive-ingest operator ladder (added 2026-04-29; lives in `fahertym/nycn`):
- Parser → review artifact (`drive-ingest-review/v1`) — NYCN #21, #22
- Review decisions YAML (organizer-authored)
- Publish dry-run (`drive-ingest-action-item-publish-dry-run/v1`) — NYCN #23
- Assignee binding (`drive-ingest-action-item-publish-dry-run-bound/v1`) — NYCN #24
- Local publisher (`drive-ingest-local-publish-plan/v1`; preflight default, execute fenced behind two operator flags + localhost-only `--gateway`) — NYCN #25
- Local proof runner (`drive-ingest-local-proof/v1`; walks `/me/action-cards` → `PUT .../status` → `GET .../completion-receipt`) — NYCN #26
- Federation surface bridge (`drive-ingest-federation-surface/v1`; pure file-in/file-out summary records keyed on the cross-node deterministic blake3 `record_hash` from `ActionItemCompletionReceipt`) — NYCN #27
- Operator pilot runbook + no-network ladder checker — NYCN #28
- Organizer briefing + simple summit demo (partner-facing, civic tone, anti-pitch) — NYCN #29
- Start-here onboarding pass (`START_HERE.md`, `ORGANIZER_QUICKSTART.md`, `STEWARD_QUICKSTART.md`, `GLOSSARY.md`) — NYCN #30
- One-command local preflight runner (`local_preflight_runner` orchestrating the full chain in a single deterministic, no-network run; preserves both human-review boundaries) — NYCN #31
- Whole-NYCN operating-surfaces inventory + Google-Groups boundary policy + repo-safe communication-groups fixture (no live sync, no private data committed) — NYCN #32
- Steward-facing communication-groups directory tool (`tools/nycn-ops`; pure file-in / file-out validator + renderer) — NYCN #33 (open at last sync; verify status before reading)
- The ladder defends a hard mutation boundary: every layer is either pure (no network) or localhost-only operator-gated. K3s mutation is never allowed by NYCN-side tools. The ICN-side K3s exercise (#1677) sits on the ICN repo side of the boundary, not in the NYCN repo.

Institutional-operability runtime (added 2026-04-22 → 2026-04-26):
- Generic institution bootstrap package path — #1586
- Bootstrap-apply 409 idempotency for repeated bootstrap runs — #1617
- Persistent governance domains across gateway restart in standalone mode — #1621
- Live charter activation endpoint — #1624
- Person-directory overlay for bootstrap role assignment (DID binding) — #1626
- `GET /me/standing` read model — #1627
- `authority_scope` plumbed end-to-end through `assign_role` — #1630
- Feedback/support doctrine rename + ADR canonicalization under `docs/adr/` — #1637
- NYCN bootstrap apply integration tests + live-validate runbook — #1592, #1593

Governance institutional primitives:
- Governance domains, structures, activities, parent (scope container) — #1540
- Decision-to-action bridge: accepted proposals create linked action items — #1532
- Consent-based decision mode — #1533
- Meeting management (schedule, agenda, attendance, minutes) — #1543
- Notification digest (pending votes, overdue items, upcoming meetings) — #1547
- NYCN architecture docs (repo-shaped spec, implementation matrix, execution tranches) — #1544
- NYCN institutional design correction (layered ontology) — #1545
- Residual acceptance-closure atomicity hazards closed — #1590
- Colon-safe proposal index keys with one-shot migration — #1591

Infrastructure:
- Atlas-backed Prometheus persistent storage — #1614
- Atlas-backed sccache for ci-runner — #1618
- Soft pod anti-affinity for ICN daemons — #1619
- Helm path documented for kube-prometheus-stack — #1616
- Steward dashboard derives gateway URL from request context — #1620
- Security Audit CI fix (wasmtime bump) — #1522, #1542
- CI dual-signal guard — #1524
- Docker-build-deploy timeout fix — #1527
- 21-file doc refresh and archive — #1526

### Architectural decisions in force

- **Layered ontology (locked 2026-04-14):** Entities (sovereign) / Structures (non-sovereign, entity-owned) / Activities (time-bounded, entity-owned). Committees are Structures. Summit is Activity.
- **Program is a separate primitive** (not Activity extension): Milestones with machine-readable checks, parent_program_id for cycle-handoff. Spec lives in the partner NYCN repo.
- **Authority is capability-string based today, typed model frozen for migration:** `RoleAssignment.authority_scope: Vec<String>` remains the shipped surface; the constitutional object model (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`) is frozen in [ADR-0014](adr/ADR-0014-constitutional-object-model.md) and is the target of a subsequent additive migration. No behavior change has shipped yet.
- **Sled key convention:** primary `<thing>:{id}`; secondary `<thing>_by_<scope>:{scope_id}:{id}`.
- **Gateway event naming:** `Governance<Thing><Verb>`.
- **Meaning Firewall:** CI ratchet enforces no new kernel/domain import regressions. Pre-existing domain imports in icn-core and icn-gateway remain; full extraction is ongoing work.

## Architecture notes

- Repo root is not a Cargo workspace; Rust workspace lives in `icn/`.
- Workspace: 35 crates in `icn/crates/` + 4 app crates in `icn/apps/` + 3 binaries = 42 packages.
  - **Crates (in `icn/crates/`):** icn-api, icn-authz, icn-ccl, icn-commons, icn-community, icn-compute, icn-coop, icn-core, icn-crypto, icn-crypto-pq, icn-encoding, icn-entity, icn-federation, icn-gateway, icn-gossip, icn-governance, icn-http-kit, icn-identity, icn-kernel-api, icn-ledger, icn-naming, icn-net, icn-obs, icn-privacy, icn-protocol, icn-rpc, icn-security, icn-services, icn-snapshot, icn-steward, icn-store, icn-testkit, icn-time, icn-trust, icn-zkp.
  - **App crates (in `icn/apps/`):** icn-governance-actor, icn-ledger-actor, icn-membership-app, icn-charter-app.
  - **Binaries:** icnd, icnctl, icn-console.
- Web UI: web/pilot-ui (PWA), web/dashboard (static).
- SDKs: sdk/typescript, sdk/react-native.
- Deployment: native/systemd, Docker Compose, Kubernetes, Helm (deploy/README.md).

## Decisions (durable)

- Mutual TLS with client certificates enabled (2025-12-18).
- DID-TLS binding verification enabled.
- Some QUIC/chaos tests ignored in CI due to timing; run manually as needed.

## Constraints (durable)

- Run Rust build/test commands from `icn/`.
- Tokio async only; avoid blocking operations in async paths.
- No panics in protocol/network/actor runtime paths.
- Demo status docs note STUN discovery disabled for local-only testing.

## References

- docs/PHASE_PROGRESS.md — phase tracking
- docs/architecture/THE_COMMONS.md — Capital-C Commons doctrine (what ICN exists to enable)
- docs/architecture/MEMBER_STANDING.md — `/me/standing` design contract (member-facing standing + accessibility)
- docs/architecture/KERNEL_APP_SEPARATION.md — kernel/app boundary
- docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md — due-diligence reflex checklist (#1739)
- docs/adr/ADR-0027-action-card-contract.md — ActionCard contract ADR (referenced by #1713 / #1764)
- docs/contracts/institution-package/README.md — institution-package ActionCard contract notes + validation guidance (#1764)
- docs/scripts/validate-action-card.py — bundled draft-2020-12 validator for the ActionCard schema (#1764)
- docs/contracts/preview-review.md — `urn:icn:contract:preview-review:v1` (#1745)
- docs/contracts/rehearsal-evidence-export.md — `urn:icn:contract:rehearsal-evidence-export:v1` (#1734)
- docs/contracts/schema-id-audit.md — contract schema-identifier audit (#1741)
- docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md — organizer/member accessibility gate (#1743)
- docs/pilots/no-cli-organizer-member-rehearsal-workflow.md — no-CLI organizer/member rehearsal workflow spec (#1725)
- ops/ideas/framing/institutional-process-substrate.md — `idea-0019` framing brief (#1747)
- ops/ideas/dogfood/institutional-process-substrate-mvp.md — read-model fixture-walk dogfood slice for `idea-0019` (#1749)
- ops/ideas/framing/democratic-authority-primitives.md — `idea-0020` framing brief (#1751)
- ops/ideas/dogfood/democratic-authority-primitives-mvp.md — read-model fixture-walk dogfood slice for `idea-0020` (#1753)
- ops/ideas/README.md § "Dogfood slice variants" — read-model fixture-walk variant convention (#1749)
- docs/dev/handoff-2026-05-07-a.md — latest session handoff (post-#1764 sync)
- docs/dev/handoff-2026-05-07.md — prior same-day handoff (post-opaque receipt storage stack sync)
- deploy/README.md — deployment options

---

## Historical snapshots

<details>
<summary>2026-04-11 snapshot (PR #1520–#1522)</summary>

- **PR #1520** (website cleanup) merged 2026-04-10
- **PR #1522** (`fix/coop-store-sled-lock`) merged 2026-04-11 — wasmtime bump + sled lock fix
- **PR #1521** closed as superseded by #1522
- Pilot Vertical Slice Hardening sprint complete: #1214, #1221, #1220, #1222
- Issue #862 (naming) closed as superseded — implemented as `icn-naming`
- Issue #1401 (hung docker CI) closed — root cause already removed in #1403

</details>

<details>
<summary>2026-03-18 snapshot (Phase 0 + Phase 1 complete)</summary>

- Phase 1 (Charter Engine) complete — PRs #1336 + #1337
- Charter bridge, CharterPolicyOracle, 5 CCL templates, icnctl charter CLI, ratification flow all landed
- Phase 0 (Close the Demo) complete — all 4 flows passing on K3s cluster
- 4,287 tests, ~420K Rust LOC

</details>

<details>
<summary>2026-03-14 snapshot (Governance Demo Sprint)</summary>

- Fixed: Gateway governance routes 404 (actix-web scope ordering)
- Fixed: Vote tally (CastVote missing voter DID)
- Built: demo pipeline (start-demo.sh, demo-governance.py, demo.html)
- 547 tests passing, cold-start demo 18/18

</details>

<details>
<summary>2026-02-18 snapshot (Economics Consolidation)</summary>

- Sprint 8-10 complete: deterministic economic receipt chain
- CanonicalReceipt, AllocationReceipt, SettlementIntent, ReceiptStore
- 6 REST endpoints for receipt/ledger provenance
- Pilot UI Receipts tab, icnctl receipts commands

</details>

<details>
<summary>2026-01-20 snapshot (Code review findings)</summary>

- Repo-wide TODO scan captured
- Large module candidates: icnctl/main.rs (9445 lines), icn-ledger (5447), icn-gateway governance (4650), icn-core governance_handlers (4243)

</details>
