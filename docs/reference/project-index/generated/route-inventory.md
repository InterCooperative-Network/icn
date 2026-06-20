---
Status: generated
Canonical: no
Generated: 2026-06-20T13:46:22+00:00
---

# Gateway Route Inventory (generated)

> Generated mechanically by [`docs/scripts/route_inventory.py`](../../../scripts/route_inventory.py). **Do not hand-edit** — rerun the script.
> Regenerate: `python3 docs/scripts/route_inventory.py --write`  ·  Check drift: `python3 docs/scripts/route_inventory.py --check`

## What this proves / does not prove

- **Proves:** these route declarations (Actix routing attribute macros) exist in the gateway source at the snapshot commit.
- **Does NOT prove:** route correctness, authentication/authorization, test coverage, runtime health, production readiness, or public-claim safety. `OpenAPI documented = yes` does **not** mean the contract is complete or correct.
- Defer to canonical truth/precedence: [`source-of-truth-map.md`](../source-of-truth-map.md) and proof levels in [`proof-level-taxonomy-capability-matrix.md`](../proof-level-taxonomy-capability-matrix.md). This is an orientation artifact (`Canonical: no`).

## Snapshot

- Source commit: `7f241718e55dfd27c8864b1e702d635cca5ce9ae`
- Gateway source scanned: `icn/crates/icn-gateway/src/**`
- OpenAPI spec: `docs/api/openapi.generated.yaml`

## Coverage summary

- **Discovered gateway route macros: 287** (DELETE 17 · GET 131 · POST 125 · PUT 14)
- **OpenAPI documented paths: 5**
- Matched as documented (best-effort path match): 2
- Not matched to OpenAPI (best-effort): 285 (~99% of discovered)
- Documented share of discovered routes: ~0.7%

> The gap is structural: only handlers hand-annotated for utoipa reach the OpenAPI spec. Of the OpenAPI paths, several belong to `icn-governance-actor` HTTP handlers that live outside the gateway crate and are not captured by this macro scan — so the documented/undocumented counts here are a best-effort comparison, while the two headline counts (discovered macros, OpenAPI paths) are the robust measured facts.

## Limitations (PR1)

- **Full mounted-path resolution is best-effort.** The `Group` column is `/v1` + a `web::scope("…")` segment associated from `server.rs` `.service(...)`/`.configure(...)` registration sites (matched at identifier boundaries, keyed by the handler's top-level `icn/crates/icn-gateway/src/api/<group>` module) + the relative macro path. It is **not** a real Rust parser and may be wrong for deeply-nested or conditional scopes. The relative macro `Path` and the `Source` file are authoritative.
- The route table is **attribute macros only**. Macro-less `web::resource("…")`/`.route(…)` registrations are now **flagged** below (see *Unparsed route-registration candidates*) but **not** parsed into routes; out-of-crate handlers (e.g. `icn-governance-actor::http`) are still not captured.
- This is **generated-map work, not API documentation completion** (issue #2112). It does not add or change any OpenAPI content or runtime behavior.

## Routes

Status vocabulary is the existing set from `source-of-truth-map.md`; no new labels are introduced. A static macro scan proves only that a routing macro is **declared** in source — not that the handler is mounted and served (registration happens elsewhere via `.service(...)` / `.configure(...)`, which this pass does not resolve). PR1 therefore **cannot** assert `gateway-backed`: every discovered route's status defaults to `unknown / needs local verification` and claim-safety to `needs review`, pending human or runtime confirmation.

| Method | Path (macro) | Group (best-effort) | Source | Handler | OpenAPI | Status | Claim safety |
|---|---|---|---|---|---|---|---|
| GET | `(empty)` | `/v1` | `icn/crates/icn-gateway/src/api/agreements.rs`:789 | `list_agreements` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1` | `icn/crates/icn-gateway/src/api/agreements.rs`:855 | `create_agreement` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}` | `/v1/{id}` | `icn/crates/icn-gateway/src/api/agreements.rs`:936 | `delete_agreement` | no | unknown / needs local verification | needs review |
| GET | `/{id}` | `/v1/{id}` | `icn/crates/icn-gateway/src/api/agreements.rs`:900 | `get_agreement` | no | unknown / needs local verification | needs review |
| GET | `/{id}/amendments` | `/v1/{id}/amendments` | `icn/crates/icn-gateway/src/api/agreements.rs`:1302 | `list_amendments` | no | unknown / needs local verification | needs review |
| POST | `/{id}/amendments` | `/v1/{id}/amendments` | `icn/crates/icn-gateway/src/api/agreements.rs`:1358 | `propose_amendment` | no | unknown / needs local verification | needs review |
| POST | `/{id}/amendments/{amendment_id}/sign` | `/v1/{id}/amendments/{amendment_id}/sign` | `icn/crates/icn-gateway/src/api/agreements.rs`:1419 | `sign_amendment` | no | unknown / needs local verification | needs review |
| POST | `/{id}/parties` | `/v1/{id}/parties` | `icn/crates/icn-gateway/src/api/agreements.rs`:971 | `add_party` | no | unknown / needs local verification | needs review |
| POST | `/{id}/propose` | `/v1/{id}/propose` | `icn/crates/icn-gateway/src/api/agreements.rs`:1092 | `propose_agreement` | no | unknown / needs local verification | needs review |
| POST | `/{id}/resume` | `/v1/{id}/resume` | `icn/crates/icn-gateway/src/api/agreements.rs`:1196 | `resume_agreement` | no | unknown / needs local verification | needs review |
| POST | `/{id}/sign` | `/v1/{id}/sign` | `icn/crates/icn-gateway/src/api/agreements.rs`:1126 | `sign_agreement` | no | unknown / needs local verification | needs review |
| POST | `/{id}/suspend` | `/v1/{id}/suspend` | `icn/crates/icn-gateway/src/api/agreements.rs`:1161 | `suspend_agreement` | no | unknown / needs local verification | needs review |
| POST | `/{id}/terminate` | `/v1/{id}/terminate` | `icn/crates/icn-gateway/src/api/agreements.rs`:1231 | `terminate_agreement` | no | unknown / needs local verification | needs review |
| PUT | `/{id}/terms` | `/v1/{id}/terms` | `icn/crates/icn-gateway/src/api/agreements.rs`:1033 | `set_terms` | no | unknown / needs local verification | needs review |
| POST | `/auth/challenge` | `/v1/auth/challenge` | `icn/crates/icn-gateway/src/api/auth.rs`:38 | `challenge` | no | unknown / needs local verification | needs review |
| POST | `/auth/verify` | `/v1/auth/verify` | `icn/crates/icn-gateway/src/api/auth.rs`:68 | `verify` | no | unknown / needs local verification | needs review |
| GET | `/budgets` | `/v1/gov/budgets` | `icn/crates/icn-gateway/src/api/budgets.rs`:87 | `list_budgets` | no | unknown / needs local verification | needs review |
| POST | `/budgets` | `/v1/gov/budgets` | `icn/crates/icn-gateway/src/api/budgets.rs`:35 | `create_budget` | no | unknown / needs local verification | needs review |
| DELETE | `/budgets/{id}` | `/v1/gov/budgets/{id}` | `icn/crates/icn-gateway/src/api/budgets.rs`:212 | `delete_budget` | no | unknown / needs local verification | needs review |
| GET | `/budgets/{id}` | `/v1/gov/budgets/{id}` | `icn/crates/icn-gateway/src/api/budgets.rs`:127 | `get_budget` | no | unknown / needs local verification | needs review |
| PUT | `/budgets/{id}` | `/v1/gov/budgets/{id}` | `icn/crates/icn-gateway/src/api/budgets.rs`:161 | `update_budget` | no | unknown / needs local verification | needs review |
| GET | `(empty)` | `/v1/charter` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:241 | `list_charters` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/charter` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:162 | `create_charter` | no | unknown / needs local verification | needs review |
| GET | `/by-domain/{domain_id}` | `/v1/charter/by-domain/{domain_id}` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:225 | `get_charter_by_domain` | no | unknown / needs local verification | needs review |
| GET | `/{charter_id}` | `/v1/charter/{charter_id}` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:209 | `get_charter` | no | unknown / needs local verification | needs review |
| POST | `/{charter_id}/activate` | `/v1/charter/{charter_id}/activate` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:308 | `activate_charter` | no | unknown / needs local verification | needs review |
| GET | `/{charter_id}/founders` | `/v1/charter/{charter_id}/founders` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:478 | `get_charter_founders` | no | unknown / needs local verification | needs review |
| POST | `/{charter_id}/sign` | `/v1/charter/{charter_id}/sign` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:267 | `sign_charter` | no | unknown / needs local verification | needs review |
| PUT | `/{charter_id}/status` | `/v1/charter/{charter_id}/status` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:346 | `update_charter_status` | no | unknown / needs local verification | needs review |
| GET | `/{charter_id}/summary` | `/v1/charter/{charter_id}/summary` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:462 | `get_charter_summary` | no | unknown / needs local verification | needs review |
| GET | `/{charter_id}/timeline` | `/v1/charter/{charter_id}/timeline` | `icn/crates/icn-gateway/src/api/charter/mod.rs`:517 | `get_charter_timeline` | no | unknown / needs local verification | needs review |
| GET | `/by-did/{did}` | `/v1/commons/by-did/{did}` | `icn/crates/icn-gateway/src/api/commons/anchor.rs`:123 | `get_anchor_by_did` | no | unknown / needs local verification | needs review |
| POST | `/dev/bootstrap-standing` | `/v1/commons/dev/bootstrap-standing` | `icn/crates/icn-gateway/src/api/commons/mod.rs`:429 | `dev_bootstrap_standing` | no | unknown / needs local verification | needs review |
| GET | `/holder/id/{holder_id}` | `/v1/commons/holder/id/{holder_id}` | `icn/crates/icn-gateway/src/api/commons/mod.rs`:180 | `get_holder_by_id` | no | unknown / needs local verification | needs review |
| GET | `/holder/{did}` | `/v1/commons/holder/{did}` | `icn/crates/icn-gateway/src/api/commons/mod.rs`:154 | `get_holder_by_did` | no | unknown / needs local verification | needs review |
| GET | `/holder/{did}/affiliations` | `/v1/commons/holder/{did}/affiliations` | `icn/crates/icn-gateway/src/api/commons/mod.rs`:226 | `list_affiliations` | no | unknown / needs local verification | needs review |
| POST | `/holder/{did}/affiliations` | `/v1/commons/holder/{did}/affiliations` | `icn/crates/icn-gateway/src/api/commons/mod.rs`:261 | `join_jurisdiction` | no | unknown / needs local verification | needs review |
| DELETE | `/holder/{did}/affiliations/{jurisdiction}` | `/v1/commons/holder/{did}/affiliations/{jurisdiction}` | `icn/crates/icn-gateway/src/api/commons/mod.rs`:342 | `leave_jurisdiction` | no | unknown / needs local verification | needs review |
| PUT | `/holder/{did}/affiliations/{jurisdiction}` | `/v1/commons/holder/{did}/affiliations/{jurisdiction}` | `icn/crates/icn-gateway/src/api/commons/mod.rs`:303 | `update_affiliation` | no | unknown / needs local verification | needs review |
| GET | `/status` | `/v1/commons/status` | `icn/crates/icn-gateway/src/api/commons/mod.rs`:116 | `get_commons_status` | no | unknown / needs local verification | needs review |
| GET | `/{anchor_id}` | `/v1/commons/{anchor_id}` | `icn/crates/icn-gateway/src/api/commons/anchor.rs`:107 | `get_anchor_by_id` | no | unknown / needs local verification | needs review |
| GET | `/{anchor_id}/attestations` | `/v1/commons/{anchor_id}/attestations` | `icn/crates/icn-gateway/src/api/commons/anchor.rs`:146 | `list_attestations` | no | unknown / needs local verification | needs review |
| POST | `/{anchor_id}/attestations` | `/v1/commons/{anchor_id}/attestations` | `icn/crates/icn-gateway/src/api/commons/anchor.rs`:168 | `add_attestation` | no | unknown / needs local verification | needs review |
| PUT | `/{anchor_id}/status` | `/v1/commons/{anchor_id}/status` | `icn/crates/icn-gateway/src/api/commons/anchor.rs`:280 | `update_anchor_status` | no | unknown / needs local verification | needs review |
| GET | `(empty)` | `/v1/gov` | `icn/crates/icn-gateway/src/api/communities.rs`:137 | `list_communities` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/gov` | `icn/crates/icn-gateway/src/api/communities.rs`:99 | `create_community` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}` | `/v1/gov/{id}` | `icn/crates/icn-gateway/src/api/communities.rs`:196 | `dissolve_community` | no | unknown / needs local verification | needs review |
| GET | `/{id}` | `/v1/gov/{id}` | `icn/crates/icn-gateway/src/api/communities.rs`:150 | `get_community` | no | unknown / needs local verification | needs review |
| PUT | `/{id}` | `/v1/gov/{id}` | `icn/crates/icn-gateway/src/api/communities.rs`:164 | `update_community` | no | unknown / needs local verification | needs review |
| POST | `/{id}/activate` | `/v1/gov/{id}/activate` | `icn/crates/icn-gateway/src/api/communities.rs`:182 | `activate_community` | no | unknown / needs local verification | needs review |
| GET | `/{id}/members` | `/v1/gov/{id}/members` | `icn/crates/icn-gateway/src/api/communities.rs`:283 | `list_members` | no | unknown / needs local verification | needs review |
| POST | `/{id}/members` | `/v1/gov/{id}/members` | `icn/crates/icn-gateway/src/api/communities.rs`:215 | `join_community` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}/members/{member_id}` | `/v1/gov/{id}/members/{member_id}` | `icn/crates/icn-gateway/src/api/communities.rs`:253 | `leave_community` | no | unknown / needs local verification | needs review |
| GET | `/{id}/resources` | `/v1/gov/{id}/resources` | `icn/crates/icn-gateway/src/api/communities.rs`:320 | `get_resource_pools` | no | unknown / needs local verification | needs review |
| POST | `/{id}/resources` | `/v1/gov/{id}/resources` | `icn/crates/icn-gateway/src/api/communities.rs`:297 | `allocate_resource` | no | unknown / needs local verification | needs review |
| POST | `/cancel/{task_hash}` | `/v1/compute/cancel/{task_hash}` | `icn/crates/icn-gateway/src/api/compute.rs`:310 | `cancel_task` | no | unknown / needs local verification | needs review |
| GET | `/settlement/receipt/{hash}` | `/v1/compute/settlement/receipt/{hash}` | `icn/crates/icn-gateway/src/api/compute.rs`:586 | `query_settlement_by_receipt` | no | unknown / needs local verification | needs review |
| GET | `/settlement/{task_id}` | `/v1/compute/settlement/{task_id}` | `icn/crates/icn-gateway/src/api/compute.rs`:565 | `query_settlement` | no | unknown / needs local verification | needs review |
| GET | `/status/{task_hash}` | `/v1/compute/status/{task_hash}` | `icn/crates/icn-gateway/src/api/compute.rs`:263 | `get_status` | no | unknown / needs local verification | needs review |
| POST | `/submit` | `/v1/compute/submit` | `icn/crates/icn-gateway/src/api/compute.rs`:117 | `submit_task` | no | unknown / needs local verification | needs review |
| GET | `/wasm` | `/v1/compute/wasm` | `icn/crates/icn-gateway/src/api/compute.rs`:501 | `list_wasm` | no | unknown / needs local verification | needs review |
| POST | `/wasm/upload` | `/v1/compute/wasm/upload` | `icn/crates/icn-gateway/src/api/compute.rs`:442 | `upload_wasm` | no | unknown / needs local verification | needs review |
| GET | `/wasm/{hash}` | `/v1/compute/wasm/{hash}` | `icn/crates/icn-gateway/src/api/compute.rs`:527 | `get_wasm_metadata` | no | unknown / needs local verification | needs review |
| GET | `/amendments` | `/v1/constitutional/amendments` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:690 | `list_amendments` | no | unknown / needs local verification | needs review |
| POST | `/amendments` | `/v1/constitutional/amendments` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:565 | `create_amendment` | no | unknown / needs local verification | needs review |
| GET | `/amendments/{id}` | `/v1/constitutional/amendments/{id}` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:731 | `get_amendment` | no | unknown / needs local verification | needs review |
| POST | `/amendments/{id}/changes` | `/v1/constitutional/amendments/{id}/changes` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:875 | `add_amendment_change` | no | unknown / needs local verification | needs review |
| GET | `/amendments/{id}/my-vote` | `/v1/constitutional/amendments/{id}/my-vote` | `icn/crates/icn-gateway/src/api/constitutional/voting.rs`:422 | `get_my_vote` | no | unknown / needs local verification | needs review |
| POST | `/amendments/{id}/ratify` | `/v1/constitutional/amendments/{id}/ratify` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:827 | `ratify_amendment` | no | unknown / needs local verification | needs review |
| GET | `/amendments/{id}/results` | `/v1/constitutional/amendments/{id}/results` | `icn/crates/icn-gateway/src/api/constitutional/voting.rs`:317 | `get_results` | no | unknown / needs local verification | needs review |
| POST | `/amendments/{id}/submit` | `/v1/constitutional/amendments/{id}/submit` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:759 | `submit_amendment` | no | unknown / needs local verification | needs review |
| POST | `/amendments/{id}/vote` | `/v1/constitutional/amendments/{id}/vote` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:793 | `open_amendment_voting` | no | unknown / needs local verification | needs review |
| GET | `/amendments/{id}/votes` | `/v1/constitutional/amendments/{id}/votes` | `icn/crates/icn-gateway/src/api/constitutional/voting.rs`:288 | `list_votes` | no | unknown / needs local verification | needs review |
| POST | `/amendments/{id}/votes` | `/v1/constitutional/amendments/{id}/votes` | `icn/crates/icn-gateway/src/api/constitutional/voting.rs`:193 | `cast_vote` | no | unknown / needs local verification | needs review |
| POST | `/amendments/{id}/withdraw` | `/v1/constitutional/amendments/{id}/withdraw` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:931 | `withdraw_amendment` | no | unknown / needs local verification | needs review |
| GET | `/appeals` | `/v1/constitutional/appeals` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:1069 | `list_appeals` | no | unknown / needs local verification | needs review |
| POST | `/appeals` | `/v1/constitutional/appeals` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:976 | `file_appeal` | no | unknown / needs local verification | needs review |
| GET | `/appeals/dashboard` | `/v1/constitutional/appeals/dashboard` | `icn/crates/icn-gateway/src/api/constitutional/appeals_ui.rs`:233 | `get_appeals_dashboard` | no | unknown / needs local verification | needs review |
| GET | `/appeals/{id}` | `/v1/constitutional/appeals/{id}` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:1108 | `get_appeal` | no | unknown / needs local verification | needs review |
| POST | `/appeals/{id}/assign-reviewer` | `/v1/constitutional/appeals/{id}/assign-reviewer` | `icn/crates/icn-gateway/src/api/constitutional/appeals_ui.rs`:174 | `assign_reviewer` | no | unknown / needs local verification | needs review |
| POST | `/appeals/{id}/evidence` | `/v1/constitutional/appeals/{id}/evidence` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:1136 | `add_appeal_evidence` | no | unknown / needs local verification | needs review |
| POST | `/appeals/{id}/resolve` | `/v1/constitutional/appeals/{id}/resolve` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:1282 | `resolve_appeal` | no | unknown / needs local verification | needs review |
| POST | `/appeals/{id}/respond` | `/v1/constitutional/appeals/{id}/respond` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:1210 | `add_appeal_response` | no | unknown / needs local verification | needs review |
| POST | `/appeals/{id}/review` | `/v1/constitutional/appeals/{id}/review` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:1255 | `begin_appeal_review` | no | unknown / needs local verification | needs review |
| GET | `/appeals/{id}/status` | `/v1/constitutional/appeals/{id}/status` | `icn/crates/icn-gateway/src/api/constitutional/appeals_ui.rs`:141 | `get_appeal_status` | no | unknown / needs local verification | needs review |
| GET | `/appeals/{id}/timeline` | `/v1/constitutional/appeals/{id}/timeline` | `icn/crates/icn-gateway/src/api/constitutional/appeals_ui.rs`:107 | `get_appeal_timeline` | no | unknown / needs local verification | needs review |
| POST | `/appeals/{id}/withdraw` | `/v1/constitutional/appeals/{id}/withdraw` | `icn/crates/icn-gateway/src/api/constitutional/mod.rs`:1317 | `withdraw_appeal` | no | unknown / needs local verification | needs review |
| GET | `(empty)` | `/v1/contracts` | `icn/crates/icn-gateway/src/api/contracts.rs`:406 | `list_contracts` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/contracts` | `icn/crates/icn-gateway/src/api/contracts.rs`:346 | `deploy_contract` | no | unknown / needs local verification | needs review |
| GET | `/name/{name}/versions` | `/v1/contracts/name/{name}/versions` | `icn/crates/icn-gateway/src/api/contracts.rs`:717 | `get_version_history` | no | unknown / needs local verification | needs review |
| DELETE | `/{hash}` | `/v1/contracts/{hash}` | `icn/crates/icn-gateway/src/api/contracts.rs`:561 | `revoke_contract` | no | unknown / needs local verification | needs review |
| GET | `/{hash}` | `/v1/contracts/{hash}` | `icn/crates/icn-gateway/src/api/contracts.rs`:450 | `get_contract` | no | unknown / needs local verification | needs review |
| POST | `/{hash}/deprecate` | `/v1/contracts/{hash}/deprecate` | `icn/crates/icn-gateway/src/api/contracts.rs`:636 | `deprecate_contract` | no | unknown / needs local verification | needs review |
| GET | `/{hash}/metadata` | `/v1/contracts/{hash}/metadata` | `icn/crates/icn-gateway/src/api/contracts.rs`:608 | `get_contract_metadata` | no | unknown / needs local verification | needs review |
| GET | `/{hash}/status` | `/v1/contracts/{hash}/status` | `icn/crates/icn-gateway/src/api/contracts.rs`:689 | `get_contract_status` | no | unknown / needs local verification | needs review |
| GET | `/{hash}/summary` | `/v1/contracts/{hash}/summary` | `icn/crates/icn-gateway/src/api/contracts.rs`:496 | `get_contract_summary` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/members` | `icn/crates/icn-gateway/src/api/coops.rs`:37 | `create_coop` | no | unknown / needs local verification | needs review |
| GET | `/coops/{id}/stats` | `/v1/members/coops/{id}/stats` | `icn/crates/icn-gateway/src/api/coops.rs`:337 | `get_coop_stats` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}` | `/v1/members/{id}` | `icn/crates/icn-gateway/src/api/coops.rs`:143 | `delete_coop` | no | unknown / needs local verification | needs review |
| GET | `/{id}` | `/v1/members/{id}` | `icn/crates/icn-gateway/src/api/coops.rs`:74 | `get_coop` | no | unknown / needs local verification | needs review |
| POST | `/{id}/members` | `/v1/members/{id}/members` | `icn/crates/icn-gateway/src/api/coops.rs`:174 | `add_member` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}/members/{did}` | `/v1/members/{id}/members/{did}` | `icn/crates/icn-gateway/src/api/coops.rs`:230 | `remove_member` | no | unknown / needs local verification | needs review |
| PUT | `/{id}/members/{did}/role` | `/v1/members/{id}/members/{did}/role` | `icn/crates/icn-gateway/src/api/coops.rs`:268 | `update_member_role` | no | unknown / needs local verification | needs review |
| PUT | `/{id}/settings` | `/v1/members/{id}/settings` | `icn/crates/icn-gateway/src/api/coops.rs`:89 | `update_settings` | no | unknown / needs local verification | needs review |
| GET | `/{did}` | `/v1/devices/{did}` | `icn/crates/icn-gateway/src/api/devices.rs`:134 | `list_devices` | no | unknown / needs local verification | needs review |
| POST | `/{did}` | `/v1/devices/{did}` | `icn/crates/icn-gateway/src/api/devices.rs`:77 | `register_device` | no | unknown / needs local verification | needs review |
| DELETE | `/{did}/{device_id}` | `/v1/devices/{did}/{device_id}` | `icn/crates/icn-gateway/src/api/devices.rs`:174 | `revoke_device` | no | unknown / needs local verification | needs review |
| GET | `/{did}/{device_id}` | `/v1/devices/{did}/{device_id}` | `icn/crates/icn-gateway/src/api/devices.rs`:222 | `get_device` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/entities` | `icn/crates/icn-gateway/src/api/entity.rs`:289 | `register_entity` | no | unknown / needs local verification | needs review |
| POST | `/audit/prune` | `/v1/entities/audit/prune` | `icn/crates/icn-gateway/src/api/entity.rs`:1739 | `prune_audit` | no | unknown / needs local verification | needs review |
| GET | `/audit/stats` | `/v1/entities/audit/stats` | `icn/crates/icn-gateway/src/api/entity.rs`:1829 | `audit_stats` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}` | `/v1/entities/{id}` | `icn/crates/icn-gateway/src/api/entity.rs`:632 | `delete_entity` | no | unknown / needs local verification | needs review |
| GET | `/{id}` | `/v1/entities/{id}` | `icn/crates/icn-gateway/src/api/entity.rs`:482 | `get_entity` | no | unknown / needs local verification | needs review |
| PUT | `/{id}` | `/v1/entities/{id}` | `icn/crates/icn-gateway/src/api/entity.rs`:513 | `update_entity` | no | unknown / needs local verification | needs review |
| GET | `/{id}/audit` | `/v1/entities/{id}/audit` | `icn/crates/icn-gateway/src/api/entity.rs`:1667 | `get_entity_audit` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}/dissolution` | `/v1/entities/{id}/dissolution` | `icn/crates/icn-gateway/src/api/entity.rs`:1521 | `cancel_dissolution` | no | unknown / needs local verification | needs review |
| POST | `/{id}/dissolution` | `/v1/entities/{id}/dissolution` | `icn/crates/icn-gateway/src/api/entity.rs`:1129 | `initiate_dissolution` | no | unknown / needs local verification | needs review |
| POST | `/{id}/dissolution/complete` | `/v1/entities/{id}/dissolution/complete` | `icn/crates/icn-gateway/src/api/entity.rs`:1335 | `complete_dissolution` | no | unknown / needs local verification | needs review |
| GET | `/{id}/members` | `/v1/entities/{id}/members` | `icn/crates/icn-gateway/src/api/entity.rs`:720 | `list_members` | no | unknown / needs local verification | needs review |
| POST | `/{id}/members` | `/v1/entities/{id}/members` | `icn/crates/icn-gateway/src/api/entity.rs`:832 | `add_membership` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}/members/{member_id}` | `/v1/entities/{id}/members/{member_id}` | `icn/crates/icn-gateway/src/api/entity.rs`:971 | `remove_membership` | no | unknown / needs local verification | needs review |
| GET | `/escrow` | `/v1/gov/escrow` | `icn/crates/icn-gateway/src/api/escrow.rs`:86 | `list_escrows` | no | unknown / needs local verification | needs review |
| POST | `/escrow` | `/v1/gov/escrow` | `icn/crates/icn-gateway/src/api/escrow.rs`:39 | `create_escrow` | no | unknown / needs local verification | needs review |
| GET | `/escrow/{id}` | `/v1/gov/escrow/{id}` | `icn/crates/icn-gateway/src/api/escrow.rs`:127 | `get_escrow` | no | unknown / needs local verification | needs review |
| POST | `/escrow/{id}/refund` | `/v1/gov/escrow/{id}/refund` | `icn/crates/icn-gateway/src/api/escrow.rs`:275 | `refund_escrow` | no | unknown / needs local verification | needs review |
| POST | `/escrow/{id}/release` | `/v1/gov/escrow/{id}/release` | `icn/crates/icn-gateway/src/api/escrow.rs`:156 | `release_escrow` | no | unknown / needs local verification | needs review |
| GET | `/records` | `/v1/execution/records` | `icn/crates/icn-gateway/src/api/execution.rs`:119 | `list_records` | no | unknown / needs local verification | needs review |
| GET | `/records/{decision_hash}` | `/v1/execution/records/{decision_hash}` | `icn/crates/icn-gateway/src/api/execution.rs`:137 | `get_record` | no | unknown / needs local verification | needs review |
| POST | `/attestations` | `/v1/federation/attestations` | `icn/crates/icn-gateway/src/api/federation.rs`:719 | `issue_attestation` | no | unknown / needs local verification | needs review |
| GET | `/attestations/{member_did}` | `/v1/federation/attestations/{member_did}` | `icn/crates/icn-gateway/src/api/federation.rs`:701 | `get_attestations` | no | unknown / needs local verification | needs review |
| GET | `/clearing` | `/v1/federation/clearing` | `icn/crates/icn-gateway/src/api/federation.rs`:790 | `list_agreements` | no | unknown / needs local verification | needs review |
| POST | `/clearing` | `/v1/federation/clearing` | `icn/crates/icn-gateway/src/api/federation.rs`:894 | `create_agreement` | no | unknown / needs local verification | needs review |
| POST | `/clearing/netting/{unit}` | `/v1/federation/clearing/netting/{unit}` | `icn/crates/icn-gateway/src/api/federation.rs`:1140 | `perform_multilateral_netting` | no | unknown / needs local verification | needs review |
| POST | `/clearing/netting/{unit}/apply` | `/v1/federation/clearing/netting/{unit}/apply` | `icn/crates/icn-gateway/src/api/federation.rs`:1176 | `apply_multilateral_netting` | no | unknown / needs local verification | needs review |
| POST | `/clearing/settle-scheduled` | `/v1/federation/clearing/settle-scheduled` | `icn/crates/icn-gateway/src/api/federation.rs`:1105 | `process_scheduled_settlements` | no | unknown / needs local verification | needs review |
| GET | `/clearing/{agreement_id}` | `/v1/federation/clearing/{agreement_id}` | `icn/crates/icn-gateway/src/api/federation.rs`:841 | `get_agreement` | no | unknown / needs local verification | needs review |
| GET | `/clearing/{agreement_id}/position` | `/v1/federation/clearing/{agreement_id}/position` | `icn/crates/icn-gateway/src/api/federation.rs`:1030 | `get_position` | no | unknown / needs local verification | needs review |
| POST | `/clearing/{agreement_id}/settle` | `/v1/federation/clearing/{agreement_id}/settle` | `icn/crates/icn-gateway/src/api/federation.rs`:1079 | `trigger_settlement` | no | unknown / needs local verification | needs review |
| POST | `/clearing/{id}/propose-adoption` | `/v1/federation/clearing/{id}/propose-adoption` | `icn/crates/icn-gateway/src/api/federation.rs`:972 | `propose_clearing_adoption` | yes | unknown / needs local verification | needs review |
| POST | `/connect` | `/v1/federation/connect` | `icn/crates/icn-gateway/src/api/federation.rs`:1210 | `federation_connect` | no | unknown / needs local verification | needs review |
| GET | `/coops` | `/v1/federation/coops` | `icn/crates/icn-gateway/src/api/federation.rs`:255 | `list_coops` | no | unknown / needs local verification | needs review |
| POST | `/coops` | `/v1/federation/coops` | `icn/crates/icn-gateway/src/api/federation.rs`:337 | `register_coop` | no | unknown / needs local verification | needs review |
| GET | `/coops/{coop_id}` | `/v1/federation/coops/{coop_id}` | `icn/crates/icn-gateway/src/api/federation.rs`:295 | `get_coop` | no | unknown / needs local verification | needs review |
| POST | `/coops/{coop_id}/vouch` | `/v1/federation/coops/{coop_id}/vouch` | `icn/crates/icn-gateway/src/api/federation.rs`:415 | `vouch_for_coop` | no | unknown / needs local verification | needs review |
| GET | `/coops/{coop_id}/vouches` | `/v1/federation/coops/{coop_id}/vouches` | `icn/crates/icn-gateway/src/api/federation.rs`:381 | `get_vouches` | no | unknown / needs local verification | needs review |
| POST | `/init` | `/v1/federation/init` | `icn/crates/icn-gateway/src/api/federation.rs`:208 | `init_federation` | no | unknown / needs local verification | needs review |
| GET | `/status` | `/v1/federation/status` | `icn/crates/icn-gateway/src/api/federation.rs`:166 | `get_status` | no | unknown / needs local verification | needs review |
| GET | `/topology` | `/v1/federation/topology` | `icn/crates/icn-gateway/src/api/federation.rs`:562 | `get_topology` | yes | unknown / needs local verification | needs review |
| POST | `/{coop_id}/proposals` | `/v1/coops/{coop_id}/proposals` | `icn/crates/icn-gateway/src/api/flow_c.rs`:22 | `propose_spend_alias` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/treasury/position` | `/v1/coops/{coop_id}/treasury/position` | `icn/crates/icn-gateway/src/api/flow_c.rs`:35 | `get_treasury_position_alias` | no | unknown / needs local verification | needs review |
| GET | `/{id}/proof` | `/v1/coops/{id}/proof` | `icn/crates/icn-gateway/src/api/flow_c.rs`:112 | `get_proof_alias` | no | unknown / needs local verification | needs review |
| POST | `/{id}/vote` | `/v1/coops/{id}/vote` | `icn/crates/icn-gateway/src/api/flow_c.rs`:46 | `cast_vote_alias` | no | unknown / needs local verification | needs review |
| GET | `/governance/{charter_id}/dashboard` | `/v1/gov/governance/{charter_id}/dashboard` | `icn/crates/icn-gateway/src/api/governance_dashboard.rs`:21 | `get_governance_dashboard` | no | unknown / needs local verification | needs review |
| GET | `/health` | `/v1/health` | `icn/crates/icn-gateway/src/api/health.rs`:169 | `health` | no | unknown / needs local verification | needs review |
| GET | `/health/detailed` | `/v1/health/detailed` | `icn/crates/icn-gateway/src/api/health.rs`:182 | `health_detailed` | no | unknown / needs local verification | needs review |
| GET | `/health/full` | `/v1/health/full` | `icn/crates/icn-gateway/src/api/health.rs`:284 | `health_full` | no | unknown / needs local verification | needs review |
| GET | `/healthz` | `/v1/healthz` | `icn/crates/icn-gateway/src/api/health.rs`:36 | `liveness` | no | unknown / needs local verification | needs review |
| GET | `/ready` | `/v1/ready` | `icn/crates/icn-gateway/src/api/health.rs`:100 | `ready` | no | unknown / needs local verification | needs review |
| GET | `/readyz` | `/v1/readyz` | `icn/crates/icn-gateway/src/api/health.rs`:48 | `readiness` | no | unknown / needs local verification | needs review |
| GET | `/health` | `/v1/identity/health` | `icn/crates/icn-gateway/src/api/identity.rs`:100 | `identity_health` | no | unknown / needs local verification | needs review |
| GET | `/resolve/{did}` | `/v1/identity/resolve/{did}` | `icn/crates/icn-gateway/src/api/identity.rs`:60 | `resolve_did` | no | unknown / needs local verification | needs review |
| GET | `(empty)` | `/v1/invites` | `icn/crates/icn-gateway/src/api/invites.rs`:101 | `list_invites` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/invites` | `icn/crates/icn-gateway/src/api/invites.rs`:25 | `create_invite` | no | unknown / needs local verification | needs review |
| POST | `/join` | `/v1/invites/join` | `icn/crates/icn-gateway/src/api/invites.rs`:146 | `join_via_invite` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/entries/by-decision` | `/v1/ledger/{coop_id}/entries/by-decision` | `icn/crates/icn-gateway/src/api/ledger.rs`:389 | `get_entries_by_decision` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/history` | `/v1/ledger/{coop_id}/history` | `icn/crates/icn-gateway/src/api/ledger.rs`:237 | `get_history` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/position/{did}` | `/v1/ledger/{coop_id}/position/{did}` | `icn/crates/icn-gateway/src/api/ledger.rs`:22 | `get_position` | no | unknown / needs local verification | needs review |
| POST | `/{coop_id}/settle` | `/v1/ledger/{coop_id}/settle` | `icn/crates/icn-gateway/src/api/ledger.rs`:55 | `create_settlement` | no | unknown / needs local verification | needs review |
| POST | `/{coop_id}/settle/convert` | `/v1/ledger/{coop_id}/settle/convert` | `icn/crates/icn-gateway/src/api/ledger.rs`:546 | `create_cross_settlement` | no | unknown / needs local verification | needs review |
| POST | `/{coop_id}/settle/convert/quote` | `/v1/ledger/{coop_id}/settle/convert/quote` | `icn/crates/icn-gateway/src/api/ledger.rs`:657 | `get_cross_settlement_quote` | no | unknown / needs local verification | needs review |
| GET | `(empty)` | `/v1/listings` | `icn/crates/icn-gateway/src/api/listings.rs`:637 | `list_listings` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/listings` | `icn/crates/icn-gateway/src/api/listings.rs`:576 | `create_listing` | no | unknown / needs local verification | needs review |
| GET | `/my` | `/v1/listings/my` | `icn/crates/icn-gateway/src/api/listings.rs`:1013 | `get_my_listings` | no | unknown / needs local verification | needs review |
| DELETE | `/{id}` | `/v1/listings/{id}` | `icn/crates/icn-gateway/src/api/listings.rs`:786 | `delete_listing` | no | unknown / needs local verification | needs review |
| GET | `/{id}` | `/v1/listings/{id}` | `icn/crates/icn-gateway/src/api/listings.rs`:681 | `get_listing` | no | unknown / needs local verification | needs review |
| PUT | `/{id}` | `/v1/listings/{id}` | `icn/crates/icn-gateway/src/api/listings.rs`:715 | `update_listing` | no | unknown / needs local verification | needs review |
| POST | `/{id}/interest` | `/v1/listings/{id}/interest` | `icn/crates/icn-gateway/src/api/listings.rs`:895 | `express_interest` | no | unknown / needs local verification | needs review |
| GET | `/{id}/interests` | `/v1/listings/{id}/interests` | `icn/crates/icn-gateway/src/api/listings.rs`:973 | `get_interests` | no | unknown / needs local verification | needs review |
| PUT | `/{id}/status` | `/v1/listings/{id}/status` | `icn/crates/icn-gateway/src/api/listings.rs`:830 | `update_listing_status` | no | unknown / needs local verification | needs review |
| GET | `/members/{coop_id}/{did}` | `/v1/identity/members/{coop_id}/{did}` | `icn/crates/icn-gateway/src/api/members.rs`:40 | `get_member_profile` | no | unknown / needs local verification | needs review |
| PUT | `/{coop_id}/{did}/profile` | `/v1/identity/{coop_id}/{did}/profile` | `icn/crates/icn-gateway/src/api/members.rs`:130 | `update_member_profile` | no | unknown / needs local verification | needs review |
| POST | `/apply` | `/v1/membership/apply` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:136 | `apply_for_membership` | no | unknown / needs local verification | needs review |
| POST | `/approve` | `/v1/membership/approve` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:225 | `approve_membership` | no | unknown / needs local verification | needs review |
| POST | `/ban` | `/v1/membership/ban` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:345 | `ban_member` | no | unknown / needs local verification | needs review |
| GET | `/capability/check` | `/v1/membership/capability/check` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:506 | `check_capability` | no | unknown / needs local verification | needs review |
| POST | `/capability/grant` | `/v1/membership/capability/grant` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:440 | `grant_capability` | no | unknown / needs local verification | needs review |
| POST | `/capability/revoke` | `/v1/membership/capability/revoke` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:473 | `revoke_capability` | no | unknown / needs local verification | needs review |
| POST | `/exit` | `/v1/membership/exit` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:703 | `exit_membership` | no | unknown / needs local verification | needs review |
| GET | `/list/{jurisdiction}` | `/v1/membership/list/{jurisdiction}` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:611 | `list_members` | no | unknown / needs local verification | needs review |
| POST | `/promote` | `/v1/membership/promote` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:255 | `promote_member` | no | unknown / needs local verification | needs review |
| POST | `/reinstate` | `/v1/membership/reinstate` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:315 | `reinstate_member` | no | unknown / needs local verification | needs review |
| POST | `/revoke` | `/v1/membership/revoke` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:387 | `revoke_membership` | no | unknown / needs local verification | needs review |
| POST | `/role/add` | `/v1/membership/role/add` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:536 | `add_role` | no | unknown / needs local verification | needs review |
| POST | `/role/remove` | `/v1/membership/role/remove` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:566 | `remove_role` | no | unknown / needs local verification | needs review |
| GET | `/status/{jurisdiction}` | `/v1/membership/status/{jurisdiction}` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:180 | `get_membership_status` | no | unknown / needs local verification | needs review |
| POST | `/suspend` | `/v1/membership/suspend` | `icn/crates/icn-gateway/src/api/membership/mod.rs`:285 | `suspend_member` | no | unknown / needs local verification | needs review |
| GET | `/{path:.*}` | `/v1/names/{path:.*}` | `icn/crates/icn-gateway/src/api/names.rs`:138 | `resolve_name` | no | unknown / needs local verification | needs review |
| GET | `/notifications` | `/v1/notifications/notifications` | `icn/crates/icn-gateway/src/api/notifications.rs`:118 | `list_notifications` | no | unknown / needs local verification | needs review |
| GET | `/notifications/count` | `/v1/notifications/notifications/count` | `icn/crates/icn-gateway/src/api/notifications.rs`:154 | `get_notification_count` | no | unknown / needs local verification | needs review |
| POST | `/notifications/read-all` | `/v1/notifications/notifications/read-all` | `icn/crates/icn-gateway/src/api/notifications.rs`:196 | `mark_all_read` | no | unknown / needs local verification | needs review |
| POST | `/notifications/register` | `/v1/notifications/notifications/register` | `icn/crates/icn-gateway/src/api/notifications.rs`:36 | `register_device` | no | unknown / needs local verification | needs review |
| DELETE | `/notifications/unregister` | `/v1/notifications/notifications/unregister` | `icn/crates/icn-gateway/src/api/notifications.rs`:57 | `unregister_device` | no | unknown / needs local verification | needs review |
| DELETE | `/notifications/{notification_id}` | `/v1/notifications/notifications/{notification_id}` | `icn/crates/icn-gateway/src/api/notifications.rs`:213 | `delete_notification` | no | unknown / needs local verification | needs review |
| POST | `/notifications/{notification_id}/read` | `/v1/notifications/notifications/{notification_id}/read` | `icn/crates/icn-gateway/src/api/notifications.rs`:171 | `mark_notification_read` | no | unknown / needs local verification | needs review |
| GET | `/notifications/{notification_id}/status` | `/v1/notifications/notifications/{notification_id}/status` | `icn/crates/icn-gateway/src/api/notifications.rs`:241 | `get_delivery_status` | no | unknown / needs local verification | needs review |
| POST | `/convert` | `/v1/oracle/convert` | `icn/crates/icn-gateway/src/api/oracle.rs`:280 | `convert_amount` | no | unknown / needs local verification | needs review |
| POST | `/rate` | `/v1/oracle/rate` | `icn/crates/icn-gateway/src/api/oracle.rs`:380 | `set_rate` | no | unknown / needs local verification | needs review |
| GET | `/rate/{from}/{to}` | `/v1/oracle/rate/{from}/{to}` | `icn/crates/icn-gateway/src/api/oracle.rs`:213 | `get_rate` | no | unknown / needs local verification | needs review |
| GET | `/sources` | `/v1/oracle/sources` | `icn/crates/icn-gateway/src/api/oracle.rs`:355 | `list_sources` | no | unknown / needs local verification | needs review |
| GET | `/allocations` | `/v1/receipts/allocations` | `icn/crates/icn-gateway/src/api/receipts.rs`:569 | `list_allocations` | no | unknown / needs local verification | needs review |
| GET | `/allocations/{hash}` | `/v1/receipts/allocations/{hash}` | `icn/crates/icn-gateway/src/api/receipts.rs`:282 | `get_allocation` | no | unknown / needs local verification | needs review |
| GET | `/chain` | `/v1/receipts/chain` | `icn/crates/icn-gateway/src/api/receipts.rs`:331 | `get_chain` | no | unknown / needs local verification | needs review |
| GET | `/chain/{decision_hash}` | `/v1/receipts/chain/{decision_hash}` | `icn/crates/icn-gateway/src/api/receipts.rs`:381 | `get_full_chain` | no | unknown / needs local verification | needs review |
| GET | `/intents` | `/v1/receipts/intents` | `icn/crates/icn-gateway/src/api/receipts.rs`:598 | `list_intents` | no | unknown / needs local verification | needs review |
| GET | `/intents/{hash}` | `/v1/receipts/intents/{hash}` | `icn/crates/icn-gateway/src/api/receipts.rs`:305 | `get_intent` | no | unknown / needs local verification | needs review |
| GET | `/settlements/recurring` | `/v1/gov/settlements/recurring` | `icn/crates/icn-gateway/src/api/recurring_settlements.rs`:94 | `list_recurring_settlements` | no | unknown / needs local verification | needs review |
| POST | `/settlements/recurring` | `/v1/gov/settlements/recurring` | `icn/crates/icn-gateway/src/api/recurring_settlements.rs`:44 | `create_recurring_settlement` | no | unknown / needs local verification | needs review |
| DELETE | `/settlements/recurring/{id}` | `/v1/gov/settlements/recurring/{id}` | `icn/crates/icn-gateway/src/api/recurring_settlements.rs`:214 | `cancel_recurring_settlement` | no | unknown / needs local verification | needs review |
| GET | `/settlements/recurring/{id}` | `/v1/gov/settlements/recurring/{id}` | `icn/crates/icn-gateway/src/api/recurring_settlements.rs`:135 | `get_recurring_settlement` | no | unknown / needs local verification | needs review |
| PUT | `/settlements/recurring/{id}` | `/v1/gov/settlements/recurring/{id}` | `icn/crates/icn-gateway/src/api/recurring_settlements.rs`:164 | `update_recurring_settlement` | no | unknown / needs local verification | needs review |
| GET | `/decisions` | `/v1/registry/decisions` | `icn/crates/icn-gateway/src/api/registry.rs`:702 | `list_decisions` | no | unknown / needs local verification | needs review |
| POST | `/decisions` | `/v1/registry/decisions` | `icn/crates/icn-gateway/src/api/registry.rs`:598 | `index_decision_endpoint` | no | unknown / needs local verification | needs review |
| GET | `/decisions/{id}` | `/v1/registry/decisions/{id}` | `icn/crates/icn-gateway/src/api/registry.rs`:745 | `get_decision` | no | unknown / needs local verification | needs review |
| GET | `/decisions/{id}/trace` | `/v1/registry/decisions/{id}/trace` | `icn/crates/icn-gateway/src/api/registry.rs`:781 | `get_decision_trace` | no | unknown / needs local verification | needs review |
| GET | `/meetings` | `/v1/registry/meetings` | `icn/crates/icn-gateway/src/api/registry.rs`:564 | `list_meetings` | no | unknown / needs local verification | needs review |
| POST | `/meetings` | `/v1/registry/meetings` | `icn/crates/icn-gateway/src/api/registry.rs`:491 | `create_meeting` | no | unknown / needs local verification | needs review |
| GET | `/summary` | `/v1/rights/summary` | `icn/crates/icn-gateway/src/api/rights.rs`:12 | `rights_summary` | no | unknown / needs local verification | needs review |
| POST | `/devices/add` | `/v1/sdis/devices/add` | `icn/crates/icn-gateway/src/api/sdis/anchor.rs`:332 | `add_device` | no | unknown / needs local verification | needs review |
| POST | `/enrollment/complete` | `/v1/sdis/enrollment/complete` | `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs`:501 | `complete_enrollment` | no | unknown / needs local verification | needs review |
| POST | `/enrollment/start` | `/v1/sdis/enrollment/start` | `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs`:281 | `start_enrollment` | no | unknown / needs local verification | needs review |
| POST | `/enrollment/verify/level1` | `/v1/sdis/enrollment/verify/level1` | `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs`:331 | `verify_level1` | no | unknown / needs local verification | needs review |
| POST | `/enrollment/verify/level2` | `/v1/sdis/enrollment/verify/level2` | `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs`:406 | `verify_level2` | no | unknown / needs local verification | needs review |
| POST | `/ephemeral/generate` | `/v1/sdis/ephemeral/generate` | `icn/crates/icn-gateway/src/api/sdis/mod.rs`:301 | `generate_ephemeral` | no | unknown / needs local verification | needs review |
| GET | `/health` | `/v1/sdis/health` | `icn/crates/icn-gateway/src/api/sdis/mod.rs`:175 | `sdis_health` | no | unknown / needs local verification | needs review |
| POST | `/reject/{enrollment_id}` | `/v1/sdis/reject/{enrollment_id}` | `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs`:913 | `reject_enrollment` | no | unknown / needs local verification | needs review |
| POST | `/rotate-keys` | `/v1/sdis/rotate-keys` | `icn/crates/icn-gateway/src/api/sdis/anchor.rs`:271 | `rotate_keys` | no | unknown / needs local verification | needs review |
| POST | `/start` | `/v1/sdis/start` | `icn/crates/icn-gateway/src/api/sdis/enrollment.rs`:258 | `start_enrollment` | no | unknown / needs local verification | needs review |
| POST | `/start` | `/v1/sdis/start` | `icn/crates/icn-gateway/src/api/sdis/recovery.rs`:216 | `start_recovery` | no | unknown / needs local verification | needs review |
| POST | `/verify/level1` | `/v1/sdis/verify/level1` | `icn/crates/icn-gateway/src/api/sdis/mod.rs`:189 | `verify_level1` | no | unknown / needs local verification | needs review |
| POST | `/verify/level2` | `/v1/sdis/verify/level2` | `icn/crates/icn-gateway/src/api/sdis/mod.rs`:209 | `verify_level2` | no | unknown / needs local verification | needs review |
| POST | `/vouch/{enrollment_id}` | `/v1/sdis/vouch/{enrollment_id}` | `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs`:824 | `steward_vouch` | no | unknown / needs local verification | needs review |
| GET | `/{anchor_id}` | `/v1/sdis/{anchor_id}` | `icn/crates/icn-gateway/src/api/sdis/anchor.rs`:246 | `get_anchor` | no | unknown / needs local verification | needs review |
| GET | `/{anchor_id}/devices` | `/v1/sdis/{anchor_id}/devices` | `icn/crates/icn-gateway/src/api/sdis/anchor.rs`:368 | `list_devices` | no | unknown / needs local verification | needs review |
| GET | `/{anchor_id}/history` | `/v1/sdis/{anchor_id}/history` | `icn/crates/icn-gateway/src/api/sdis/anchor.rs`:311 | `get_rotation_history` | no | unknown / needs local verification | needs review |
| GET | `/{ceremony_id}` | `/v1/sdis/{ceremony_id}` | `icn/crates/icn-gateway/src/api/sdis/enrollment.rs`:294 | `get_enrollment_status` | no | unknown / needs local verification | needs review |
| POST | `/{ceremony_id}/approve` | `/v1/sdis/{ceremony_id}/approve` | `icn/crates/icn-gateway/src/api/sdis/enrollment.rs`:368 | `approve_ceremony` | no | unknown / needs local verification | needs review |
| POST | `/{ceremony_id}/finalize` | `/v1/sdis/{ceremony_id}/finalize` | `icn/crates/icn-gateway/src/api/sdis/enrollment.rs`:318 | `finalize_enrollment` | no | unknown / needs local verification | needs review |
| GET | `/{recovery_id}` | `/v1/sdis/{recovery_id}` | `icn/crates/icn-gateway/src/api/sdis/recovery.rs`:252 | `get_recovery_status` | no | unknown / needs local verification | needs review |
| POST | `/{recovery_id}/approve` | `/v1/sdis/{recovery_id}/approve` | `icn/crates/icn-gateway/src/api/sdis/recovery.rs`:329 | `approve_recovery` | no | unknown / needs local verification | needs review |
| POST | `/{recovery_id}/complete` | `/v1/sdis/{recovery_id}/complete` | `icn/crates/icn-gateway/src/api/sdis/recovery.rs`:275 | `complete_recovery` | no | unknown / needs local verification | needs review |
| GET | `(empty)` | `/v1/services` | `icn/crates/icn-gateway/src/api/services.rs`:494 | `query_services` | no | unknown / needs local verification | needs review |
| POST | `/announce` | `/v1/services/announce` | `icn/crates/icn-gateway/src/api/services.rs`:238 | `announce_service` | no | unknown / needs local verification | needs review |
| GET | `/discover` | `/v1/services/discover` | `icn/crates/icn-gateway/src/api/services.rs`:388 | `discover_services` | no | unknown / needs local verification | needs review |
| DELETE | `/{service_id}` | `/v1/services/{service_id}` | `icn/crates/icn-gateway/src/api/services.rs`:347 | `withdraw_service` | no | unknown / needs local verification | needs review |
| GET | `/{service_id}` | `/v1/services/{service_id}` | `icn/crates/icn-gateway/src/api/services.rs`:550 | `get_service` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/sessions` | `icn/crates/icn-gateway/src/api/sessions.rs`:137 | `create_session` | no | unknown / needs local verification | needs review |
| GET | `/{session_id}` | `/v1/sessions/{session_id}` | `icn/crates/icn-gateway/src/api/sessions.rs`:184 | `get_session_status` | no | unknown / needs local verification | needs review |
| POST | `/{session_id}/approve` | `/v1/sessions/{session_id}/approve` | `icn/crates/icn-gateway/src/api/sessions.rs`:251 | `approve_session` | no | unknown / needs local verification | needs review |
| GET | `(empty)` | `/v1/steward` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:170 | `list_stewards` | no | unknown / needs local verification | needs review |
| POST | `(empty)` | `/v1/steward` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:72 | `register_steward` | no | unknown / needs local verification | needs review |
| GET | `/attesters` | `/v1/steward/attesters` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:195 | `list_attesters` | no | unknown / needs local verification | needs review |
| GET | `/by-did/{did}` | `/v1/steward/by-did/{did}` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:150 | `get_steward_by_did` | no | unknown / needs local verification | needs review |
| GET | `/{steward_id}` | `/v1/steward/{steward_id}` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:133 | `get_steward` | no | unknown / needs local verification | needs review |
| POST | `/{steward_id}/attestation` | `/v1/steward/{steward_id}/attestation` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:570 | `record_attestation` | no | unknown / needs local verification | needs review |
| POST | `/{steward_id}/bond/add` | `/v1/steward/{steward_id}/bond/add` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:432 | `add_bond` | no | unknown / needs local verification | needs review |
| POST | `/{steward_id}/bond/slash` | `/v1/steward/{steward_id}/bond/slash` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:484 | `slash_bond` | no | unknown / needs local verification | needs review |
| POST | `/{steward_id}/dispute` | `/v1/steward/{steward_id}/dispute` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:624 | `record_dispute` | no | unknown / needs local verification | needs review |
| POST | `/{steward_id}/dispute-won` | `/v1/steward/{steward_id}/dispute-won` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:686 | `record_dispute_won` | no | unknown / needs local verification | needs review |
| POST | `/{steward_id}/extend-term` | `/v1/steward/{steward_id}/extend-term` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:360 | `extend_steward_term` | no | unknown / needs local verification | needs review |
| POST | `/{steward_id}/retire` | `/v1/steward/{steward_id}/retire` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:319 | `retire_steward` | no | unknown / needs local verification | needs review |
| PUT | `/{steward_id}/status` | `/v1/steward/{steward_id}/status` | `icn/crates/icn-gateway/src/api/steward/mod.rs`:214 | `update_steward_status` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}` | `/v1/treasury/{coop_id}` | `icn/crates/icn-gateway/src/api/treasury.rs`:375 | `get_treasury_status` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/audit` | `/v1/treasury/{coop_id}/audit` | `icn/crates/icn-gateway/src/api/treasury.rs`:939 | `get_audit_trail` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/budgets` | `/v1/treasury/{coop_id}/budgets` | `icn/crates/icn-gateway/src/api/treasury.rs`:576 | `list_budgets` | no | unknown / needs local verification | needs review |
| POST | `/{coop_id}/budgets` | `/v1/treasury/{coop_id}/budgets` | `icn/crates/icn-gateway/src/api/treasury.rs`:732 | `create_budget` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/budgets/{budget_id}` | `/v1/treasury/{coop_id}/budgets/{budget_id}` | `icn/crates/icn-gateway/src/api/treasury.rs`:646 | `get_budget` | no | unknown / needs local verification | needs review |
| POST | `/{coop_id}/deposit` | `/v1/treasury/{coop_id}/deposit` | `icn/crates/icn-gateway/src/api/treasury.rs`:1029 | `deposit_to_treasury` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/nonce` | `/v1/treasury/{coop_id}/nonce` | `icn/crates/icn-gateway/src/api/treasury.rs`:521 | `get_treasury_nonce` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/position` | `/v1/treasury/{coop_id}/position` | `icn/crates/icn-gateway/src/api/treasury.rs`:450 | `get_treasury_position` | no | unknown / needs local verification | needs review |
| POST | `/{coop_id}/spend` | `/v1/treasury/{coop_id}/spend` | `icn/crates/icn-gateway/src/api/treasury.rs`:1163 | `propose_spend` | no | unknown / needs local verification | needs review |
| GET | `/{coop_id}/spending-rules` | `/v1/treasury/{coop_id}/spending-rules` | `icn/crates/icn-gateway/src/api/treasury.rs`:872 | `list_spending_rules` | no | unknown / needs local verification | needs review |
| POST | `/trust/attest` | `/v1/trust/trust/attest` | `icn/crates/icn-gateway/src/api/trust.rs`:103 | `create_trust_attestation` | no | unknown / needs local verification | needs review |
| POST | `/trust/revoke` | `/v1/trust/trust/revoke` | `icn/crates/icn-gateway/src/api/trust.rs`:179 | `revoke_trust_attestation` | no | unknown / needs local verification | needs review |
| GET | `/trust/{did}` | `/v1/trust/trust/{did}` | `icn/crates/icn-gateway/src/api/trust.rs`:35 | `get_trust_score` | no | unknown / needs local verification | needs review |
| GET | `/trust/{did}/edges` | `/v1/trust/trust/{did}/edges` | `icn/crates/icn-gateway/src/api/trust.rs`:72 | `get_trust_edges` | no | unknown / needs local verification | needs review |
| GET | `/trust/{did}/network` | `/v1/trust/trust/{did}/network` | `icn/crates/icn-gateway/src/api/trust.rs`:143 | `get_trust_network` | no | unknown / needs local verification | needs review |
| GET | `/ws/{coop_id}` | `/v1/ws/{coop_id}` | `icn/crates/icn-gateway/src/api/websocket.rs`:12 | `websocket` | no | unknown / needs local verification | needs review |

## Unparsed route-registration candidates

Beyond attribute macros, Actix can also register routes via `web::resource("…").route(web::<method>().to(handler))`. This scan **flags** such sites mechanically but does **not** parse them into routes: a regex cannot reliably tell a production registration from a **test fixture** (e.g. inside `#[cfg(test)]` / `test::init_service`), and it does not resolve the mounted path without a real parser. They are **not** counted in the route total above and remain `unknown / needs local verification` — treat them as leads for human review.

- **Candidates flagged: 4** (`web::resource("…")`: 2 · `.route(…)`: 2)

| Kind | Resource literal | Source |
|---|---|---|
| `web::resource` | `/notifications/register` | `icn/crates/icn-gateway/src/api/notifications.rs`:315 |
| `.route` | — | `icn/crates/icn-gateway/src/api/notifications.rs`:316 |
| `web::resource` | `/{session_id}/approve` | `icn/crates/icn-gateway/src/server.rs`:1968 |
| `.route` | — | `icn/crates/icn-gateway/src/server.rs`:1969 |

