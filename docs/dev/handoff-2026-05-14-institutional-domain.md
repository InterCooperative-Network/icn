# Session Handoff — 2026-05-14 (institutional domain spec)

## Session Goal

Define `InstitutionalDomain` and `DomainPolicy` as design-level forward-direction primitives so that the next layer of work — schemas, identity recovery, agreement registry, route-binding contracts — can be filed safely against a clear authority model.

## Decisive Test

Can a downstream implementer — looking only at this spec plus the existing canon it harmonizes with — name (1) what a domain is and what it is not, (2) which fields the domain object carries vs which adjacent concepts are deferred, (3) where `DomainPolicy` is evaluated and where it is forbidden to run, (4) which lifecycle stage requires which authority basis and which receipt class? If yes, the spec has done its job.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`494dce9fa` — `docs(spec): add accepted-proposal effect dispatch contract (#1819)` (merged earlier today).

### Open PRs
| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|
| #1820 | `spec/institutional-domain-policy` | OPEN | All required checks pass after the initial push at commit `880d06105`; a review-feedback fix commit followed and must re-run CI before merge | Awaiting CI re-run on the fix commit, then mergeability re-check |
| #1790 | dependabot (pilot-ui dev deps) | OPEN | n/a | Out of session scope |
| #1791 | dependabot (ts-sdk dev deps) | OPEN | n/a | Out of session scope |

### Branches
- `spec/institutional-domain-policy` — pushed to origin. Initial head `880d06105` (spec doc, registry, handoff). A review-feedback commit lands after the initial PR and changes the head; check `git rev-parse HEAD` on the branch for the current SHA after that commit.
- Local `main` synced to `494dce9fa` (unchanged this session).

### Issues
- #1794 — OPEN. This session writes the candidate spec doc. PR will use `Refs:`, not `Closes:`.
- #1793 — closed (merged via #1814).
- #1797 — closed (merged via #1819, user closed manually).
- #1815, #1816, #1817, #1818 — sibling spec issues, all OPEN.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. Spec doc drafted (`docs/spec/institutional-domain.md`, ~310 lines)

The doc:

- **Names what a domain is and what it is not.** Boundary table separating domain from DNS name, node, federation, institution package, app/tool, member account, storage bucket, CCL document, and tenant.
- **Places `InstitutionalDomain` in the civic loop at the Authority stage.** Every later stage derives legitimacy from a decision rooted in some domain.
- **Outlines `InstitutionalDomain` fields at design granularity.** Identity (DID + owning entity class), constitutional inputs (charter refs, adopted CCL policy refs), membership/standing/role-authority, device/service identities, workspace/tool/agreement registries, storage/compute/routing policy refs, receipt policy, accessibility/translation policy, export/backup/recovery policy, federation agreements, challenge/repair/exit paths. No Rust types, no JSON schemas, no field order fixed.
- **Outlines `DomainPolicy` rule bundle.** Storage / replication / backup / privacy / compute / receipt retention / tool install authority / route authority / service identity authority / member recovery / accessibility / language / authority class / amendment / versioning / inertness rule. Adopted via governance; unadopted policy is inert.
- **Specifies `DomainPolicy` evaluation rules.** Consulted at decision admission, mandate composition (Stage 2 of effect dispatch), effect plan generation (Stage 3), and binding adoption. Never consulted inside the kernel. Cross-link to `effect-dispatch-contract.md` and `KERNEL_APP_SEPARATION.md`.
- **Specifies a nine-stage domain lifecycle table.** Declare → adopt charter/policy → initialize standing/membership → bind routes/services/tools → operate → amend → federate → suspend/repair → export/exit/archive. Each row names the authority required and the receipt class emitted. Lifecycle invariants: no transition without authority basis, no transition without receipt expectation, no silent transition, reversal is a separate transition (chain is append-only), exit is constitutional and unblockable.
- **Maps the relationship to effect dispatch.** Five-row table aligning `DomainPolicy`'s role to the five dispatch stages.
- **Cross-links every sibling future-work issue.** #1815, #1816, #1817, #1818, #1798, #1801, #1799, #1795. No duplication.
- **Names adjacent concepts as deferred.** `DomainSession`, `DeviceIdentity`/`DeviceEnrollment`, `ServiceIdentity`, `Workspace`, `AgreementRegistry`, `ToolRegistry`, `DnsBinding` — each named, none redefined.
- **Lists open questions and deferred decisions.** Nine items, each with a named follow-up location.

### 2. Registered the doc (`docs/registry.toml`, +14 lines after the effect-dispatch entry)

`category = "architecture"`, `status = "draft"`, `title`, `description`, `last_updated`, `last_reviewed`, `owner`, `audiences`, `domain_tags`, `depends_on`, `recommended_action`. **No `truth_class` or `role` overrides** — the `docs/spec/` prefix default applies (`truth_class = "normative"`, `role = "spec"`, `canonical = "no"`, `freshness = "unknown"`), matching the convention the #1819 review settled on.

### 3. Followed the #1819 review lessons proactively

- YAML frontmatter is `Status: normative` (matches the prefix default; matches the doc's tone).
- Doc opening labeled "spec, work-in-progress" — honest about which clauses are forward-direction.
- Handoff written in `docs/dev/HANDOFF_TEMPLATE.md` structure from the start (not after review). Unsafe Assumptions section is concrete and lists five things.
- Used canon vocabulary verbatim (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`, `ServiceIdentity`, `ToolBinding`, `DnsBinding`, `StorageClass`, `DataLocality`, `EffectManifest`, `GovernanceDecisionReceipt`, etc.).
- No schema, no wire format, no MUST that requires fields the existing canon does not yet declare.

### 4. Drafted follow-up issues (not filed)

Per the user's instruction: drafts in handoff only, not auto-filed. See "Next Move" §5.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [x] Run validation suite. *(All four scripts: doc_control_check pass with 53 pre-existing unrelated warnings; lint-arch CLEAN; compliance_linter clean; freshness-check exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness.)*
- [x] Commit with scoped message. *(Initial commit `880d06105`; review-feedback commit follows.)*
- [x] Push branch.
- [x] Open PR with non-claims body. *(PR #1820.)*
- [x] Watch CI on the initial head — all required checks pass; non-required skip (docs-only).
- [ ] Re-run CI on the review-feedback fix commit and recheck `mergeStateStatus`.
- [ ] Reply to the two valid Copilot review threads (handoff "Final State" staleness; spec doc Federation row casing) with the fix commit SHA.
- [ ] Codex thread (handoff-removal) — already replied with verification chain; no further edit since `docs/dev-journal/` does not exist and the user's PR instructions explicitly directed the `docs/dev/` path. Leave as is; carry the AGENTS.md / HANDOFF_TEMPLATE.md drift as a separate follow-up.
- [ ] Squash-merge if clean.
- [ ] Decide whether to file the follow-up issue drafts listed in §"Next Move §5" (deferred to user).
- [ ] After #1794 lands, the next safe spec is `#1817` (CCL policy registry). #1817 operationalizes the §"DomainPolicy evaluation" hook and the §"Stage 2 / Stage 3" CCL hook points named in `effect-dispatch-contract.md`.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` is the canonical home for the domain-infrastructure overview.** I treated it as design-direction and extended it without superseding. If a more recent doc has been written that I did not catch, my "harmonize, don't duplicate" framing may need revisiting.
- **`InstitutionalDomain` and `DomainPolicy` are not yet defined in Rust types.** I assumed both are forward-direction primitives based on `ICN_INTEGRATED_SYSTEM_MODEL.md` line 106's explicit "tracked under #1794" framing. If types named `InstitutionalDomain` or `DomainPolicy` have landed in `icn-governance` or another crate since the spine merged, my "no Rust types defined" framing is wrong.
- **`ADR-0014`'s three `AuthorityClass` variants are still authoritative.** I cited `Representation` / `Execution` / `Attestation` as the closed set. If a later ADR has amended that enum, the spec needs adjustment.
- **`DnsBinding` is already adequately specified by `DOMAIN_ROUTING_AND_DNS_BINDINGS.md`.** I cross-linked rather than re-specified. If reviewers find that doc inadequate for the contract this spec implies, a `spec(routing): …` follow-up is needed.
- **The `docs/spec/` prefix default in the registry sets `truth_class = "normative"` and `role = "spec"`.** Confirmed via `grep` against `docs/registry.toml`'s `[[doc_path_defaults]]` block during the #1819 review. If that default has been amended since, the YAML `Status: normative` may produce a mismatch warning.
- **The 25 cross-link targets I cite all exist at session start.** Verified by file-existence sweep before committing. If a downstream PR renames or removes one of them before #1794 lands, the spec's link will rot.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. Run the validation suite:
   ```bash
   cd /home/matt/projects/icn
   python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
   python3 docs/scripts/lint-arch.py docs/spec/institutional-domain.md --cargo icn/Cargo.toml
   python3 .github/scripts/compliance_linter.py
   python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .
   ```
2. Commit with the scoped message `docs(spec): define institutional domain and policy primitive`.
3. Push the branch and open the PR with the non-claims body.
4. Monitor CI; reply to any AI-reviewer threads using the same pattern as #1819.
5. **Follow-up issue drafts (deferred; not filed in this PR):**
   - `schema(domain): define InstitutionalDomain and DomainPolicy persisted records` — the wire-stable schema that this spec deliberately does not introduce.
   - `spec(identity): define ServiceIdentity and DeviceIdentity issuance, rotation, recovery inside domains` — the full lifecycle for the identity primitives this spec only names.
   - `spec(agreement): define federation/domain AgreementRegistry shape and lifecycle` — the registry shape `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` named eight classes for.
   - `spec(routing): clarify DomainRouteBinding authority and receipt contract` — *optional;* `DOMAIN_ROUTING_AND_DNS_BINDINGS.md` may already be sufficient. Flagged for human triage.
6. After #1794 lands, the recommended next spec PR is #1817 (CCL policy registry / versioning / governance-effect hook contract). It operationalizes the §"DomainPolicy evaluation" hook this spec names.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

1. **Domain object stays at design granularity.** The spec deliberately does not name Rust types, JSON schemas, or wire formats. Schema work is a separate follow-up.
2. **`DomainPolicy` evaluation never runs in the kernel.** Confirms and extends the meaning firewall (per `KERNEL_APP_SEPARATION.md`). Evaluation happens at decision admission, mandate composition, effect plan generation, and binding adoption — all in the app layer.
3. **Unadopted `DomainPolicy` is inert.** Mirrors the rule already established for CCL documents (per `effect-dispatch-contract.md` §"CCL hook points") and for `Mandate`s (per ADR-0019 conservative-broadening invariants).
4. **The nine-stage lifecycle is the canonical sequence.** Each transition requires authority basis and emits a receipt; reversal is a separate transition; exit is constitutional and unblockable.
5. **Adjacent concepts (DomainSession, DeviceIdentity, ServiceIdentity, Workspace, AgreementRegistry, ToolRegistry, DnsBinding) are named, not redefined.** Where they have existing canonical sources, the spec cross-links and notes deferrals.
6. **Tenant is rejected as an analog.** A domain is a self-governing jurisdiction, not a billing entity inside someone else's product.

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
cd /home/matt/projects/icn

# Branch + PR state
git branch --show-current
gh pr view <pr-number> --json state,mergeable,mergeStateStatus

# Validation suite
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
python3 docs/scripts/lint-arch.py docs/spec/institutional-domain.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# Review thread state
gh api repos/InterCooperative-Network/icn/pulls/<pr-number>/comments \
  --jq '.[] | {id, user: .user.login, line, in_reply_to_id, created_at}'
```

Expected: doc_control passes with 53 pre-existing unrelated warnings, none on the new doc; lint-arch CLEAN (already verified); compliance_linter no violations; freshness-check exit 1 from pre-existing `docs/ARCHITECTURE.md` staleness (recorded honestly, not in this PR's scope).

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/STATE.md` (last reviewed 2026-05-07, current as of the #1819 merge earlier today). No conflict surfaced.
- **Implementation truth:** verified by reading `icn-governance/src/proof.rs`, `mandate.rs`, `authority.rs` for the constitutional types (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`) the spec leans on; checked that `InstitutionalDomain` and `DomainPolicy` do not yet have Rust types (consistent with the "forward-direction" framing).
- **Execution truth:** confirmed `main` HEAD `494dce9fa`, two dependabot PRs open, no other open PRs.
- **Known truth-plane conflict:** `freshness-check.py` reports `docs/ARCHITECTURE.md` as carrying stale sections. CI workflow treats this as informational and reports SUCCESS; the script itself exits 1. This PR does not broaden scope to address it (same pattern recorded in #1814 and #1819 handoffs).
