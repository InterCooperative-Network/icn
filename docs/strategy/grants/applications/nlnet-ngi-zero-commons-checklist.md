---
Status: working doc
Companion to: nlnet-ngi-zero-commons.md
Last Reviewed: 2026-05-22
---

# NLnet NGI Zero Commons — Pre-Submit Checklist

Companion to the submission draft [`nlnet-ngi-zero-commons.md`](nlnet-ngi-zero-commons.md).
Deadline: **2026-06-01, 12:00 CEST**.

## 1. Metrics verification log

Exact methods and results. The shell workspace could not mount the repo path this session, so
`cargo` / `tokei` / `wc` were unavailable; methods below note that and give the commands to run.

### Workspace members — VERIFIED, cited in draft

- **Method:** Read the `[workspace].members` array in `icn/Cargo.toml` and counted entries.
- **Result:** **44 workspace members** = 37 library crates (`crates/*`) + 4 application crates
  (`apps/charter`, `apps/governance`, `apps/membership`, `apps/ledger`) + 3 binaries
  (`bins/icnd`, `bins/icnctl`, `bins/icn-console`). One crate, `crates/icn-baseline-lock-guest`,
  is present on disk but listed under `exclude` — not a member.
- **Re-verify command:** `cd icn/icn && cargo metadata --no-deps --format-version 1 | jq '.packages | length'` → expect `44`.
- **Note:** This corrects the stale "34 crates" figure still present in `grant-narrative-core.md`
  and `grant-one-pager.md` (already flagged by PR #1878). Those two files should be updated to 44.

### Test count — NOT cited (ambiguous)

- **Method:** `cargo test` could not be run (no shell). Grepped `#[(tokio::)?test]` across
  `icn/icn/**/*.rs`.
- **Result:** test attributes are widespread (hundreds of files; per-file counts up to ~42),
  but an attribute grep is **not** a defensible total — it diverges from a real `cargo test`
  pass count (parametrised cases, macro-generated tests, doctests). No reliable number.
- **Decision:** Per the "don't cite shaky numbers" rule, the draft uses qualitative language
  ("a substantial automated test suite spanning unit and multi-node integration tests"). Do
  **not** insert a test number unless verified.
- **Command for a citeable figure:** `cd icn/icn && cargo test --workspace -- --list | grep -c ': test'`
  (test-case count), or run `cargo test --workspace --no-fail-fast` and read the summary lines.

### Lines of code — NOT cited (not countable cleanly this session)

- **Method:** No shell, so `tokei` / `cloc` / `wc` were unavailable.
- **Result:** Not counted. The draft uses qualitative language ("a working reference
  implementation", "the Rust workspace declares 44 members").
- **Decision:** Do not cite a LOC figure. If a number is wanted, it must be a clean, single,
  reproducible count.
- **Command for a citeable figure:** `tokei icn/icn` (cite the "Rust" code line, not totals),
  or `find icn/icn -name '*.rs' -not -path '*/target/*' | xargs wc -l | tail -1`.

## 2. Overclaim audit — changes made to the draft

| Was | Now | Why |
|-----|-----|-----|
| "a live multi-node deployment" | "a multi-node Kubernetes test cluster" | It is a test cluster, not a production service |
| NYCN "preparing to become first institutional pilot" / "a concrete first user" | "the intended first rehearsal partner"; "in active conversation with" | NYCN is not a formally committed pilot (per the nycn repo README and icn `STATE.md`) |
| Deliverable: "live two-cooperative anti-entropy run (ICN + NYCN nodes)" | "a controlled two-deployment anti-entropy rehearsal between independent ICN deployments (one configured as the NYCN rehearsal package)" | Avoids implying NYCN runs a live cooperative node |
| "not a blockchain (no tokens, no mining)" | "not a global-consensus system: no shared global ledger, no consensus protocol, no economic-incentive layer" | Removes blockchain/token framing per vocabulary discipline |
| "mutual-credit ledger" | "obligation-and-settlement ledger" | Vocabulary discipline (settlement / obligation, not credit) |

Preserved and kept honest: the substrate exists and runs; the proof-loop's wire-stable record
types are merged; the runtime **does not yet exist** — stated plainly, not hidden. Core pitch
intact: ICN is open, cooperative-owned institutional infrastructure, and this grant funds the
anti-entropy / federation proof-loop runtime.

## 3. Licensing assessment

- Root `LICENSE`: **AGPL-3.0**.
- `icn/Cargo.toml` `[workspace.package].license`: **`MIT OR Apache-2.0`**. Per `LICENSING.md`,
  of 49 canonical `Cargo.toml` files: 13 declare `MIT OR Apache-2.0` explicitly, 34 inherit it,
  2 declare no `license` field (`examples/wasm-compute`, `icn/crates/icn-ccl/fuzz`). No
  `Cargo.toml` declares AGPL-3.0.
- **Eligibility impact: none.** AGPL-3.0, MIT, and Apache-2.0 are all OSI-approved / FSF-
  recognised free licenses, so ICN satisfies NLnet's "recognised open source license"
  requirement either way. The only risk is a stage-2 reviewer question about the unresolved
  AGPL-vs-MIT/Apache split — which `LICENSING.md` itself records as an open question.
- **Smallest safe cleanup (recommended, not required for eligibility):** a short maintainer
  licensing decision — per `LICENSING.md`'s own rule that license changes land in a dedicated
  PR — recording the intended posture, e.g. "AGPL-3.0 governs the project source distribution
  and daemon; `MIT OR Apache-2.0` is the intentional permissive posture for reusable library
  crates." Also add a `license` field to the 2 crates that declare none. This is a tiny PR and
  lets the proposal state one coherent licensing sentence.
- **Docs/website licensing:** verified — ICN documentation falls under the root `LICENSE`
  (AGPL-3.0); there is no separate CC license on docs (`LICENSING.md` confirms only AGPL at
  root and `MIT OR Apache-2.0` for workspace/SDK/API metadata). AGPL is a recognised free
  license and NLnet does not strictly mandate a CC license on documentation, so this is **not
  an eligibility blocker**. Applying CC-BY-SA to docs remains a low-cost, recommended
  enhancement.

## 4. Pre-submit blockers

### Resolved in triage (2026-05-22)

- [x] **Stale crate count** — `grant-one-pager.md` and `grant-narrative-core.md` updated:
      "34 crates" → "44 workspace members (37 crates, 4 apps, 3 binaries)". The unverified
      "451,000 lines" figure was dropped from `grant-narrative-core.md` in the same edit.
- [x] **European dimension** — Gmail searched for "Decidim" and "NLnet / NGI Zero /
      Fediversity": no matches. No confirmed European contacts and no prior NLnet
      correspondence exist. The draft's framing — Decidim and the European cooperative-tech
      ecosystem as *prospective* review/engagement targets — is correct; do not name partners.
- [x] **Documentation licensing** — verified (see §3): docs fall under the root AGPL-3.0; not
      an eligibility blocker. CC-BY-SA on docs stays an optional enhancement.
- [x] **Test / LOC counts** — `cargo` / `tokei` unavailable in this environment (see §1). The
      draft already uses qualitative language; citeable numbers are optional, not required.

### Needs your input

- [ ] **Licensing decision** — the AGPL-vs-CAL comparison is written
      (`../licensing-decision-agpl-vs-cal.md`); the decision and a lawyer's review of CAL-1.0
      are yours. Land it as the dedicated PR/RFC `LICENSING.md` calls for — touching
      `LICENSING.md`, `icn/Cargo.toml`, and adding a `license` field to
      `examples/wasm-compute/Cargo.toml` and `icn/crates/icn-ccl/fuzz/Cargo.toml`. **Not
      required before June 1** — both licenses satisfy NLnet.
- [ ] **Fiscal sponsor** — confirm Open Source Collective vs Alchemical; confirm a
      milestone-reimbursement (not open-ended-salary) structure; review with an SSDI-aware
      accountant.
- [ ] **Optional — citeable metrics** — for hard numbers in the proposal, run
      `cd icn/icn && cargo test --workspace -- --list | grep -c ': test'` and `tokei icn/icn`,
      then add the verified figures to the draft's experience answer. Otherwise the
      qualitative language stands.
- [ ] **Transcribe to the form** — the draft is already structured by NLnet form field; copy
      each section into nlnet.nl/propose, keeping the main application concise (~2 pages).
- [ ] **NLnet office hour 2026-05-27** ("Ask us Anything") — optional sanity-check.
- [ ] **Submit** at [nlnet.nl/propose](https://nlnet.nl/propose/) before 2026-06-01 12:00
      CEST; record the application number in §5 and in the draft's submission log.

### Adjacent — flagged, not changed

Beyond the crate count, `grant-one-pager.md` (a March 2026 doc) and `grant-narrative-core.md`
still carry unverified or aspirational content: "5,933 passing tests" and "~75% complete" in
both; and in the one-pager a pilot-deployment timeline ("first external cooperative running
ICN" by Jun 2026; "3–5 cooperatives federated" in Q4 2026), the word "payments" in the
architecture section, and "Cooperative Fund of New England" (elsewhere "Cooperative Fund of
the Northeast"). These are reusable grant material, not the NLnet application, so they were
left unchanged — but they need a dedicated cleanup pass before they feed another application.

## 5. Submission log

- _(application number to be recorded here after submitting)_
