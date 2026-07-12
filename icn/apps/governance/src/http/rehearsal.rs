//! HTTP surface for the rehearsal pending-publish review workspace
//! (#1726 / #1728 / #2386 organizer workflow slice).
//!
//! Mounted by [`configure`](super::configure::configure) ONLY when
//! `build_mode.allows_rehearsal_mutation()` — i.e. exclusively in
//! [`GovernanceContextBuildMode::Rehearsal`](super::GovernanceContextBuildMode).
//! In Production, Bootstrap (the unknown/missing-mode fallback), and Test the
//! routes do not exist at all, so the surface fails closed as 404 without
//! trusting any handler-level check.
//!
//! Capability model (sub-capability doctrine, #2400):
//!
//! - `governance:rehearsal:setup` — operator/fixture-setup acts: DESIGNATING
//!   a fictional rehearsal domain (its first workspace initialization) and
//!   binding labels to fictional identities. Held by the internal setup
//!   credential only — an organizer browser can neither turn an arbitrary
//!   domain into a rehearsal workspace nor bind identities, and never
//!   handles a DID.
//! - `governance:pending-publish:review` — the organizer surface: review
//!   decisions, bounded edits, label assignment (labels only), previews, and
//!   RE-resetting an already-designated workspace (repeating the fictional
//!   rehearsal is an organizer act; each reset retires the prior run's
//!   fictional action items through the normal status machinery).
//! - `governance:pending-publish:confirm` — executing the bound mutation.
//!   Held separately so a review-only credential can never mutate governance
//!   state, and a confirm-only credential can never manufacture the review
//!   state it confirms.
//! - Broad `governance:write` retains all three (tested), consistent with
//!   the existing sub-capability decomposition — operator compatibility
//!   only; browser credentials carry only their narrow scopes.
//!   `governance:read` grants read surfaces only.
//!
//! These routes are intentionally NOT part of the public OpenAPI document:
//! they exist only on an isolated Rehearsal Node and never in production, so
//! advertising them in the production API contract would be dishonest. The
//! contract is documented in `docs/contracts/rehearsal-review-workflow.md`.
//!
//! Every mutation requires domain membership; the path domain is the
//! authority context (a workspace, row, or binding is only reachable through
//! its own domain — #2052 object-context doctrine). No read surface exposes
//! a DID; previews are bound to state via a domain-separated BLAKE3 digest
//! (see [`crate::rehearsal_workspace`]); confirm walks the real ADR-0026
//! receipt ladder and creates a real action item through the existing
//! manager machinery. Receipts record process facts and grant no authority.

use actix_web::{web, HttpRequest, HttpResponse};
use icn_governance::{ActionItemPriority, ProcessGateKind, ProcessGateResult};
use icn_http_kit::auth::{require_any_scope, require_scope, BasicClaims};
use icn_http_kit::error::ApiError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::events::GovernanceEventEmitter;
use crate::manager::{
    ActivationCrossedOutcome, DecisionRecordOutcome, MutationAppliedOutcome,
    MutationPlanRecordedOutcome, ProcessSessionOpenOutcome,
};
use crate::rehearsal_workspace::{
    self as ws, hex32, plan_for_row, ApproveRef, DecisionLogEntry, ExecutionRecord, RehearsalRow,
    ReviewDecision, MAX_LABEL, MAX_NOTE, MAX_PLAIN_SUMMARY,
};

use super::configure::GovernanceContext;
use super::handlers::{check_domain_membership, demo_pending_publish_rows, parse_did};
use super::models::PendingPublishRowStatus;

use icn_governance::GovernanceDomainId;
use icn_identity::Did;

const REVIEW_SCOPES: &[&str] = &["governance:pending-publish:review", "governance:write"];
const CONFIRM_SCOPES: &[&str] = &["governance:pending-publish:confirm", "governance:write"];
/// Operator/fixture-setup capability: designating a fictional rehearsal
/// domain (its FIRST workspace initialization) and binding labels to
/// fictional identities. The organizer browser credential never holds this,
/// so it can neither turn an arbitrary domain into a rehearsal workspace nor
/// bind identities — it only sees and assigns labels.
const SETUP_SCOPES: &[&str] = &["governance:rehearsal:setup", "governance:write"];

// ── Request models: contract enforced by rejection, not omission ───────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RehearsalResetRequest {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RehearsalReviewRequest {
    pub decision: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RehearsalEditRequest {
    #[serde(default)]
    pub plain_summary: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RehearsalAssignRequest {
    /// `None` clears the assignment.
    #[serde(default)]
    pub assignee_label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RehearsalBindRequest {
    pub label: String,
    /// The fictional DID to bind. Accepted on write, never echoed on any
    /// read surface.
    pub did: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RehearsalConfirmRequest {
    /// 64-hex BLAKE3 preview digest from `GET .../preview`.
    pub preview_digest: String,
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Scope + DID + domain-membership gate shared by every rehearsal handler.
async fn gate<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: &GovernanceContext<E>,
    http_req: &HttpRequest,
    domain_id: &str,
    scopes: &[&str],
) -> Result<(GovernanceDomainId, Did), ApiError> {
    let claims = require_any_scope::<BasicClaims>(http_req, scopes)?;
    let caller = parse_did(&claims.sub, "Invalid DID in token")?;
    let domain = GovernanceDomainId(domain_id.to_string());
    check_domain_membership(ctx, &domain, &caller).await?;
    Ok((domain, caller))
}

/// Read-surface gate: `governance:read` + membership.
async fn read_gate<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: &GovernanceContext<E>,
    http_req: &HttpRequest,
    domain_id: &str,
) -> Result<(GovernanceDomainId, Did), ApiError> {
    let claims = require_scope::<BasicClaims>(http_req, "governance:read")?;
    let caller = parse_did(&claims.sub, "Invalid DID in token")?;
    let domain = GovernanceDomainId(domain_id.to_string());
    check_domain_membership(ctx, &domain, &caller).await?;
    Ok((domain, caller))
}

/// Serialize a value for a response. The row/enum types are plain field
/// structs whose serialization cannot fail in practice; if it ever does,
/// serve an explicit error value and log — never panic a worker.
fn to_value_or_error<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or_else(|e| {
        tracing::error!(detail = %e, "rehearsal response serialization failed");
        json!({ "error": "serialization failed" })
    })
}

fn workspace_not_initialized() -> ApiError {
    ApiError::NotFound(
        "No rehearsal workspace is initialized for this domain. Start one with \
         POST /domains/{domain_id}/rehearsal/reset."
            .to_string(),
    )
}

fn row_not_found() -> ApiError {
    ApiError::NotFound("No such proposed-work row in this domain's rehearsal workspace".to_string())
}

/// Ensure the workspace's process session is opened (recording the real
/// `ProcessSessionOpenedReceipt` on first use).
fn ensure_session<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: &GovernanceContext<E>,
    domain: &GovernanceDomainId,
    workspace: &mut ws::Workspace,
    actor: &Did,
) -> Result<(), ApiError> {
    if workspace.session_opened {
        return Ok(());
    }
    let session_id = workspace.session_id();
    let outcome = ctx
        .manager
        .record_process_session_opened(domain, &session_id, actor)
        .map_err(conflict_or_internal)?;
    let receipt = match outcome {
        ProcessSessionOpenOutcome::Opened(r) | ProcessSessionOpenOutcome::AlreadyOpened(r) => r,
    };
    workspace.session_opened = true;
    workspace.session_record_hash = Some(receipt.record_hash);
    Ok(())
}

/// The ladder fails closed with descriptive conflicts (e.g. a session opened
/// by a different actor). Map those to 409 and everything else to 500 with
/// PLAIN client messages — backend detail goes to the log, never the client.
fn conflict_or_internal(e: anyhow::Error) -> ApiError {
    let detail = e.to_string();
    if detail.contains("conflict") || detail.contains("already") {
        tracing::warn!(detail = %detail, "rehearsal receipt conflict");
        ApiError::Conflict(
            "A rehearsal process receipt for this step already exists with different \
             content (for example, a session opened by someone else). Reset the \
             rehearsal to start a fresh run."
                .to_string(),
        )
    } else {
        tracing::warn!(detail = %detail, "rehearsal receipt recording failed");
        ApiError::Internal("Recording a rehearsal process receipt failed.".to_string())
    }
}

/// JSON shape of one rehearsal row: the generic fixture row plus review
/// metadata. Never contains a DID.
fn row_json(workspace: &ws::Workspace, row: &RehearsalRow) -> Value {
    let mut v = to_value_or_error(&row.base);
    if let Some(obj) = v.as_object_mut() {
        obj.insert("version".into(), json!(row.version));
        obj.insert("note".into(), json!(row.note));
        obj.insert("executed".into(), json!(row.execution.is_some()));
        if let Some(exec) = &row.execution {
            obj.insert(
                "execution".into(),
                json!({
                    "action_item_id": exec.action_item_id,
                    "plan_record_hash": hex32(&exec.plan_record_hash),
                    "application_record_hash": hex32(&exec.application_record_hash),
                }),
            );
        }
    }
    let bound = row
        .base
        .assignee_label
        .as_deref()
        .map(|l| workspace.label_bound(l).unwrap_or(false));
    json!({ "row": v, "assignee_bound": bound })
}

fn listing_row_json(workspace: &ws::Workspace, row: &RehearsalRow) -> Value {
    // Flat shape for listings: the row fields at top level + review metadata.
    let mut v = to_value_or_error(&row.base);
    if let Some(obj) = v.as_object_mut() {
        obj.insert("version".into(), json!(row.version));
        obj.insert("executed".into(), json!(row.execution.is_some()));
        let bound = row
            .base
            .assignee_label
            .as_deref()
            .map(|l| workspace.label_bound(l).unwrap_or(false));
        obj.insert("assignee_bound".into(), json!(bound));
    }
    v
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// POST /domains/{domain_id}/rehearsal/reset — start a new rehearsal
/// generation with the deterministic fictional seed.
///
/// Designation gate: the FIRST initialization of a domain's workspace is an
/// operator/setup act (`governance:rehearsal:setup`) — a review-scoped
/// member cannot turn an arbitrary domain into a rehearsal workspace.
/// Re-resetting an already-designated workspace is an organizer act
/// (`governance:pending-publish:review`).
///
/// Reset means "start a new rehearsal generation", not "restore a clean
/// node": recorded receipts are permanent process facts, and the prior
/// run's fictional action items are retired through the normal status
/// machinery (cancelled unless already completed) so the member's active
/// queue returns to a deterministic state while history stays inspectable.
pub async fn rehearsal_reset<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
    _req: web::Json<RehearsalResetRequest>,
) -> Result<HttpResponse, ApiError> {
    let state = ctx.manager.rehearsal_state();
    let scopes = if state.is_initialized(&domain_id) {
        REVIEW_SCOPES
    } else {
        SETUP_SCOPES
    };
    let (domain, caller) = gate(&ctx, &http_req, &domain_id, scopes).await?;
    let (generation, prior_items) = state.reset(&domain.0, demo_pending_publish_rows());

    // Retire the prior run's fictional action items (skip anything already
    // completed — completion history is real and stays). Per-item failures
    // are reported, never silently swallowed.
    let mut cancelled = Vec::new();
    let mut not_cancelled = Vec::new();
    for item_id in prior_items {
        let Ok(uuid) = item_id.parse::<uuid::Uuid>() else {
            not_cancelled.push(item_id);
            continue;
        };
        let typed_id = icn_governance::ActionItemId::from_uuid(uuid);
        let already_completed = ctx
            .manager
            .get_action_item(&domain, &typed_id)
            .ok()
            .flatten()
            .map(|i| i.status == icn_governance::ActionItemStatus::Completed)
            .unwrap_or(false);
        if already_completed {
            continue;
        }
        match ctx.manager.update_action_item_status(
            &domain,
            &typed_id,
            icn_governance::ActionItemStatus::Cancelled,
            &caller,
            "governance:pending-publish:review",
        ) {
            Ok(_) => cancelled.push(item_id),
            Err(e) => {
                tracing::warn!(item = %item_id, detail = %e, "reset could not retire prior rehearsal item");
                not_cancelled.push(item_id);
            }
        }
    }

    let rows = state
        .with_workspace(&domain.0, |workspace| {
            workspace
                .rows
                .iter()
                .map(|r| listing_row_json(workspace, r))
                .collect::<Vec<_>>()
        })
        .ok_or_else(workspace_not_initialized)?;
    Ok(HttpResponse::Ok().json(json!({
        "generation": generation,
        "rows": rows,
        "cancelled_action_items": cancelled,
        "not_cancelled_action_items": not_cancelled,
        "non_claims": [
            "fictional rehearsal workspace — grants no authority",
            "reset starts a new rehearsal generation: recorded receipts are permanent process facts; the prior run's fictional action items are cancelled (not erased) unless already completed",
        ],
    })))
}

/// GET /domains/{domain_id}/rehearsal/pending-publish — current workspace rows.
pub async fn rehearsal_list<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let (domain, _caller) = read_gate(&ctx, &http_req, &domain_id).await?;
    let state = ctx.manager.rehearsal_state();
    let body = state
        .with_workspace(&domain.0, |workspace| {
            json!({
                "generation": workspace.generation,
                "rows": workspace
                    .rows
                    .iter()
                    .map(|r| listing_row_json(workspace, r))
                    .collect::<Vec<_>>(),
            })
        })
        .ok_or_else(workspace_not_initialized)?;
    Ok(HttpResponse::Ok().json(body))
}

/// GET /domains/{domain_id}/rehearsal/pending-publish/{row_id} — row detail.
pub async fn rehearsal_row_detail<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let (domain_id, row_id) = path.into_inner();
    let (domain, _caller) = read_gate(&ctx, &http_req, &domain_id).await?;
    let state = ctx.manager.rehearsal_state();
    let body = state
        .with_workspace(&domain.0, |workspace| {
            workspace.row(&row_id).map(|r| row_json(workspace, r))
        })
        .ok_or_else(workspace_not_initialized)?
        .ok_or_else(row_not_found)?;
    Ok(HttpResponse::Ok().json(body))
}

/// POST /domains/{domain_id}/rehearsal/pending-publish/{row_id}/review —
/// record a bounded review decision, backed by a real DecisionRecorded
/// receipt.
pub async fn rehearsal_review<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<RehearsalReviewRequest>,
) -> Result<HttpResponse, ApiError> {
    let (domain_id, row_id) = path.into_inner();
    let (domain, caller) = gate(&ctx, &http_req, &domain_id, REVIEW_SCOPES).await?;

    let decision = ReviewDecision::parse(&req.decision).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "Unknown review decision '{}'. Allowed: approve, reject, needs_edit, needs_more_info.",
            req.decision
        ))
    })?;
    if let Some(note) = &req.note {
        if note.len() > MAX_NOTE {
            return Err(ApiError::BadRequest(format!(
                "note is too long ({} bytes; the limit is {MAX_NOTE})",
                note.len()
            )));
        }
    }

    let state = ctx.manager.rehearsal_state();
    let body = state
        .with_workspace(&domain.0, |workspace| -> Result<Value, ApiError> {
            ensure_session(&ctx, &domain, workspace, &caller)?;
            let decision_id = workspace.next_decision_id();
            let generation = workspace.generation;
            let session_id = workspace.session_id();

            let row = workspace.row_mut(&row_id).ok_or_else(row_not_found)?;
            if row.execution.is_some() {
                return Err(ApiError::Conflict(
                    "This proposed work was already confirmed and executed; reset the rehearsal to review it again.".to_string(),
                ));
            }
            if row.pending_item_id.is_some() {
                return Err(ApiError::Conflict(
                    "An interrupted execution exists for this proposed work; retry confirm \
                     with the same preview digest to complete it, or reset the rehearsal."
                        .to_string(),
                ));
            }

            // Provisionally advance the version; roll back if the receipt
            // write fails so state and receipts never disagree.
            row.version += 1;
            let body_hash = ws::decision_body_hash_at(
                &domain.0,
                generation,
                row,
                decision,
                req.note.as_deref(),
            );
            let outcome = ctx
                .manager
                .record_decision(&domain, &session_id, &decision_id, &caller, body_hash);
            let receipt = match outcome {
                Ok(DecisionRecordOutcome::Recorded(r))
                | Ok(DecisionRecordOutcome::AlreadyRecorded(r)) => r,
                Err(e) => {
                    row.version -= 1;
                    return Err(conflict_or_internal(e));
                }
            };

            row.base.status = decision.status_after();
            row.note = req.note.clone();
            row.approve_ref = if decision == ReviewDecision::Approve {
                Some(ApproveRef {
                    decision_id: decision_id.clone(),
                    record_hash: receipt.record_hash,
                })
            } else {
                None
            };
            let row_version = row.version;
            let entry_row_id = row.base.id.clone();
            let response = json!({
                "row": to_value_or_error(&row.base),
                "version": row_version,
                "decision_receipt": {
                    "decision_id": decision_id,
                    "record_hash": hex32(&receipt.record_hash),
                },
            });
            workspace.decisions.push(DecisionLogEntry {
                seq: workspace.decision_seq,
                row_id: entry_row_id,
                row_version,
                decision,
                decision_id: decision_id.clone(),
                record_hash: receipt.record_hash,
                note_present: req.note.is_some(),
            });
            Ok(response)
        })
        .ok_or_else(workspace_not_initialized)??;
    Ok(HttpResponse::Ok().json(body))
}

/// PUT /domains/{domain_id}/rehearsal/pending-publish/{row_id} — bounded
/// edit. Only the plain summary is editable; every edit invalidates prior
/// approval and every outstanding preview.
pub async fn rehearsal_edit<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<RehearsalEditRequest>,
) -> Result<HttpResponse, ApiError> {
    let (domain_id, row_id) = path.into_inner();
    let (domain, _caller) = gate(&ctx, &http_req, &domain_id, REVIEW_SCOPES).await?;

    let summary = match &req.plain_summary {
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err(ApiError::BadRequest(
                    "plain_summary must not be empty".to_string(),
                ));
            }
            if s.len() > MAX_PLAIN_SUMMARY {
                return Err(ApiError::BadRequest(format!(
                    "plain_summary is too long ({} bytes; the limit is {MAX_PLAIN_SUMMARY})",
                    s.len()
                )));
            }
            s.clone()
        }
        None => {
            return Err(ApiError::BadRequest(
                "No editable field supplied (allowed: plain_summary)".to_string(),
            ))
        }
    };

    let state = ctx.manager.rehearsal_state();
    let body = state
        .with_workspace(&domain.0, |workspace| -> Result<Value, ApiError> {
            let row = workspace.row_mut(&row_id).ok_or_else(row_not_found)?;
            if row.execution.is_some() {
                return Err(ApiError::Conflict(
                    "This proposed work was already executed; reset the rehearsal to edit it again."
                        .to_string(),
                ));
            }
            if row.pending_item_id.is_some() {
                return Err(ApiError::Conflict(
                    "An interrupted execution exists for this proposed work; retry confirm \
                     with the same preview digest to complete it, or reset the rehearsal."
                        .to_string(),
                ));
            }
            if row.base.status == PendingPublishRowStatus::Rejected {
                return Err(ApiError::Conflict(
                    "This proposed work was rejected; record a new review decision first."
                        .to_string(),
                ));
            }
            row.base.plain_summary = summary.clone();
            row.version += 1;
            // An edit always demands fresh review: approval no longer covers
            // the changed content.
            row.base.status = PendingPublishRowStatus::PendingReview;
            row.approve_ref = None;
            Ok(json!({
                "row": to_value_or_error(&row.base),
                "version": row.version,
            }))
        })
        .ok_or_else(workspace_not_initialized)??;
    Ok(HttpResponse::Ok().json(body))
}

/// POST /domains/{domain_id}/rehearsal/pending-publish/{row_id}/assign —
/// assign by human-readable label. Unknown labels fail closed; assignment
/// invalidates prior approval and previews.
pub async fn rehearsal_assign<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<RehearsalAssignRequest>,
) -> Result<HttpResponse, ApiError> {
    let (domain_id, row_id) = path.into_inner();
    let (domain, _caller) = gate(&ctx, &http_req, &domain_id, REVIEW_SCOPES).await?;

    if let Some(label) = &req.assignee_label {
        if label.trim().is_empty() || label.len() > MAX_LABEL {
            return Err(ApiError::BadRequest(format!(
                "assignee_label must be 1..={MAX_LABEL} bytes"
            )));
        }
    }

    let state = ctx.manager.rehearsal_state();
    let result = state
        .with_workspace(&domain.0, |workspace| -> Result<Result<Value, HttpResponse>, ApiError> {
            if let Some(label) = &req.assignee_label {
                if workspace.label_bound(label).is_none() {
                    // Unknown label: fail closed without inventing identities.
                    return Ok(Err(HttpResponse::UnprocessableEntity().json(json!({
                        "error": format!(
                            "Unknown assignee label '{label}'. Labels must be registered in this rehearsal workspace (see GET .../rehearsal/bindings)."
                        ),
                    }))));
                }
            }
            let (row_value, version) = {
                let row = workspace.row_mut(&row_id).ok_or_else(row_not_found)?;
                if row.execution.is_some() {
                    return Err(ApiError::Conflict(
                        "This proposed work was already executed; reset the rehearsal to change it."
                            .to_string(),
                    ));
                }
                if row.pending_item_id.is_some() {
                    return Err(ApiError::Conflict(
                        "An interrupted execution exists for this proposed work; retry confirm \
                         with the same preview digest to complete it, or reset the rehearsal."
                            .to_string(),
                    ));
                }
                row.base.assignee_label = req.assignee_label.clone();
                row.version += 1;
                row.base.status = PendingPublishRowStatus::PendingReview;
                row.approve_ref = None;
                (to_value_or_error(&row.base), row.version)
            };
            let bound = req
                .assignee_label
                .as_deref()
                .map(|l| workspace.label_bound(l).unwrap_or(false));
            Ok(Ok(json!({
                "row": row_value,
                "version": version,
                "assignee_bound": bound,
            })))
        })
        .ok_or_else(workspace_not_initialized)??;
    match result {
        Ok(body) => Ok(HttpResponse::Ok().json(body)),
        Err(resp) => Ok(resp),
    }
}

/// POST /domains/{domain_id}/rehearsal/bindings — bind a label to a fictional
/// DID (registering the label if new). The DID is accepted on write and never
/// echoed. Rebinding invalidates outstanding previews via the digest.
pub async fn rehearsal_bind_label<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
    req: web::Json<RehearsalBindRequest>,
) -> Result<HttpResponse, ApiError> {
    // Setup capability: the organizer browser never binds identities (and
    // never handles a DID); only the internal fixture/setup credential does.
    let (domain, _caller) = gate(&ctx, &http_req, &domain_id, SETUP_SCOPES).await?;

    if req.label.trim().is_empty() || req.label.len() > MAX_LABEL {
        return Err(ApiError::BadRequest(format!(
            "label must be 1..={MAX_LABEL} bytes"
        )));
    }
    let bound_did = parse_did(&req.did, "Invalid DID in binding")?;
    // A binding target must already hold standing in this domain: binding
    // grants no authority and cannot smuggle an outside identity into the
    // rehearsal's completion loop.
    if check_domain_membership(&ctx, &domain, &bound_did)
        .await
        .is_err()
    {
        return Ok(HttpResponse::UnprocessableEntity().json(json!({
            "error": "The identity for this label does not hold membership standing in this domain; bindings must reference an existing fictional domain member.",
            "label": req.label,
        })));
    }

    let state = ctx.manager.rehearsal_state();
    let body = state
        .with_workspace(&domain.0, |workspace| {
            workspace
                .bindings
                .insert(req.label.clone(), bound_did.to_string());
            json!({ "label": req.label, "bound": true })
        })
        .ok_or_else(workspace_not_initialized)?;
    Ok(HttpResponse::Ok().json(body))
}

/// GET /domains/{domain_id}/rehearsal/bindings — labels + bound flags only.
pub async fn rehearsal_list_bindings<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let (domain, _caller) = read_gate(&ctx, &http_req, &domain_id).await?;
    let state = ctx.manager.rehearsal_state();
    let body = state
        .with_workspace(&domain.0, |workspace| {
            json!({
                "bindings": workspace
                    .bindings
                    .iter()
                    .map(|(label, did)| json!({ "label": label, "bound": !did.is_empty() }))
                    .collect::<Vec<_>>(),
            })
        })
        .ok_or_else(workspace_not_initialized)?;
    Ok(HttpResponse::Ok().json(body))
}

/// GET /domains/{domain_id}/rehearsal/pending-publish/{row_id}/preview —
/// pure read returning the exact bound mutation and its digest. Requires the
/// review capability (previewing is part of reviewing) and an approved,
/// still-current row.
pub async fn rehearsal_preview<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ApiError> {
    let (domain_id, row_id) = path.into_inner();
    let (domain, _caller) = gate(&ctx, &http_req, &domain_id, REVIEW_SCOPES).await?;

    let state = ctx.manager.rehearsal_state();
    let result = state
        .with_workspace(&domain.0, |workspace| -> Result<Result<Value, HttpResponse>, ApiError> {
            let row = workspace.row(&row_id).ok_or_else(row_not_found)?;
            let Some(plan) = plan_for_row(&domain.0, workspace, row) else {
                return Ok(Err(HttpResponse::UnprocessableEntity().json(json!({
                    "error": "This kind of proposed work is reviewable but not executable in this rehearsal slice.",
                    "row_id": row.base.id,
                }))));
            };
            if row.base.status != PendingPublishRowStatus::ApprovedForPublish
                || row.approve_ref.is_none()
            {
                return Err(ApiError::Conflict(
                    "Preview requires an approved review decision on the current version of this row.".to_string(),
                ));
            }
            let assignee_bound = row
                .base
                .assignee_label
                .as_deref()
                .map(|l| workspace.label_bound(l).unwrap_or(false));
            let confirmable = assignee_bound != Some(false);
            Ok(Ok(json!({
                "row_id": row.base.id,
                "version": row.version,
                "generation": workspace.generation,
                "action": "create_action_item",
                "domain_id": domain.0,
                "title": plan.title,
                "description": plan.description,
                "assignee_label": row.base.assignee_label,
                "assignee_bound": assignee_bound,
                "authority_basis": row.base.authority_basis,
                "risk_level": to_value_or_error(&row.base.risk_level),
                "receipt_expected": to_value_or_error(&row.base.receipt_expected),
                "reversible": false,
                "permanence_note": "Confirming creates a real action item and permanent process receipts on this rehearsal node. The rehearsal can be reset, but recorded receipts are not erased.",
                "privacy_note": "The preview and all receipts are value-withheld: member identities appear only as labels; identity bindings stay on the node.",
                "confirmable": confirmable,
                "preview_digest": plan.digest.to_hex().to_string(),
                "plan_id": format!(
                    "rehearsal-plan-{}-{}-v{}",
                    workspace.run_id, row.base.id, row.version
                ),
                "expected_receipts": [
                    "process_gate_result",
                    "activation_crossed",
                    "mutation_plan_recorded",
                    "mutation_applied",
                ],
            })))
        })
        .ok_or_else(workspace_not_initialized)??;
    match result {
        Ok(body) => Ok(HttpResponse::Ok().json(body)),
        Err(resp) => Ok(resp),
    }
}

/// POST /domains/{domain_id}/rehearsal/pending-publish/{row_id}/confirm —
/// execute the bound mutation. Fail-closed digest verification; real receipt
/// ladder; idempotent replay.
pub async fn rehearsal_confirm<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    req: web::Json<RehearsalConfirmRequest>,
) -> Result<HttpResponse, ApiError> {
    let (domain_id, row_id) = path.into_inner();
    let (domain, caller) = gate(&ctx, &http_req, &domain_id, CONFIRM_SCOPES).await?;

    let submitted = blake3::Hash::from_hex(req.preview_digest.as_bytes()).map_err(|_| {
        ApiError::BadRequest("preview_digest must be a 64-character hex digest".to_string())
    })?;

    let state = ctx.manager.rehearsal_state();
    let result = state
        .with_workspace(&domain.0, |workspace| -> Result<Result<(Value, bool), HttpResponse>, ApiError> {
            let session_id = workspace.session_id();
            let run_id = workspace.run_id.clone();
            let generation = workspace.generation;

            // Idempotent replay / conflicting-reuse check first.
            if let Some(row) = workspace.row(&row_id) {
                if let Some(exec) = &row.execution {
                    let executed_digest = blake3::Hash::from(exec.preview_digest);
                    if executed_digest == submitted {
                        return Ok(Ok((confirm_response(&row_id, &session_id, exec, true), false)));
                    }
                    return Err(ApiError::Conflict(
                        "This proposed work was already executed from a different preview; nothing was changed.".to_string(),
                    ));
                }
            } else {
                return Err(row_not_found());
            }

            // Recompute the plan from CURRENT state and verify the digest.
            let (plan, approve_ref, assignee_label) = {
                let row = workspace.row(&row_id).ok_or_else(row_not_found)?;
                if row.base.status != PendingPublishRowStatus::ApprovedForPublish
                    || row.approve_ref.is_none()
                {
                    return Err(ApiError::Conflict(
                        "Confirm requires an approved review decision on the current version of this row. Preview again after approval.".to_string(),
                    ));
                }
                let Some(plan) = plan_for_row(&domain.0, workspace, row) else {
                    return Ok(Err(HttpResponse::UnprocessableEntity().json(json!({
                        "error": "This kind of proposed work is not executable in this rehearsal slice.",
                    }))));
                };
                if let Some(label) = row.base.assignee_label.as_deref() {
                    if workspace.bound_did(label).is_none() {
                        return Err(ApiError::Conflict(format!(
                            "Assignee label '{label}' is not bound to an identity; bind it or clear the assignment, then preview again."
                        )));
                    }
                }
                let approve_ref = row.approve_ref.clone().ok_or_else(|| {
                    ApiError::Conflict(
                        "Confirm requires an approved review decision on the current version of this row.".to_string(),
                    )
                })?;
                (plan, approve_ref, row.base.assignee_label.clone())
            };
            // blake3::Hash equality is constant-time.
            if plan.digest != submitted {
                return Err(ApiError::Conflict(
                    "The preview is stale: the proposed work, its assignment, or the rehearsal state changed after the preview was issued. Request a new preview and re-check it.".to_string(),
                ));
            }

            // Real ADR-0026 ladder: gate → activation → plan → item → applied.
            ensure_session(&ctx, &domain, workspace, &caller)?;
            let row_version = workspace.row(&row_id).ok_or_else(row_not_found)?.version;
            let gate_receipt = ctx
                .manager
                .record_process_gate_result(
                    &domain,
                    &session_id,
                    ProcessGateKind::ScopeConfirmation,
                    ProcessGateResult::Pass,
                    &caller,
                )
                .map_err(conflict_or_internal)?;
            let activation_id = format!("rehearsal-activation-{run_id}-{row_id}-v{row_version}");
            let activation = match ctx
                .manager
                .record_activation_crossed(
                    &domain,
                    &session_id,
                    &activation_id,
                    &approve_ref.decision_id,
                    approve_ref.record_hash,
                    &[gate_receipt.record_hash],
                    &caller,
                )
                .map_err(conflict_or_internal)?
            {
                ActivationCrossedOutcome::Crossed(r)
                | ActivationCrossedOutcome::AlreadyCrossed(r) => r,
            };
            let plan_id = format!("rehearsal-plan-{run_id}-{row_id}-v{row_version}");
            let plan_receipt = match ctx
                .manager
                .record_mutation_plan_recorded(
                    &domain,
                    &session_id,
                    &plan_id,
                    &activation_id,
                    activation.record_hash,
                    *plan.digest.as_bytes(),
                    &caller,
                )
                .map_err(conflict_or_internal)?
            {
                MutationPlanRecordedOutcome::Recorded(r)
                | MutationPlanRecordedOutcome::AlreadyRecorded(r) => r,
            };

            let assignee_did: Option<Did> = match &plan.assignee_did {
                Some(s) => Some(parse_did(s, "Invalid bound assignee DID")?),
                None => None,
            };
            // Resume-safe creation: the pending marker is written the moment
            // the real item exists, BEFORE the mutation-applied receipt. If
            // that recording fails, an identical retried confirm resumes with
            // the same item instead of creating a second one (review
            // mutations are blocked while the marker is set, so the digest
            // still matches).
            let pending = workspace
                .row(&row_id)
                .ok_or_else(row_not_found)?
                .pending_item_id
                .clone();
            let action_item_id = match pending {
                Some(id) => id,
                None => {
                    let item = ctx
                        .manager
                        .create_action_item(
                            domain.clone(),
                            plan.title.clone(),
                            Some(plan.description.clone()),
                            caller.clone(),
                            assignee_did.clone(),
                            plan.due_date,
                            ActionItemPriority::Medium,
                            None,
                            Some(format!("rehearsal-review session {session_id}")),
                            Vec::new(),
                        )
                        .map_err(|e| {
                            tracing::warn!(detail = %e, "rehearsal action-item creation failed");
                            ApiError::Internal(
                                "Creating the rehearsal action item failed.".to_string(),
                            )
                        })?;
                    let id = item.id.to_string();
                    if let Some(row) = workspace.row_mut(&row_id) {
                        row.pending_item_id = Some(id.clone());
                    }
                    id
                }
            };

            let result_hash = ws::apply_result_hash(
                &domain.0,
                &action_item_id,
                &plan.title,
                plan.assignee_did.as_deref(),
                caller.as_str(),
            );
            let application_id = format!("rehearsal-apply-{run_id}-{row_id}-v{row_version}");
            let applied = match ctx
                .manager
                .record_mutation_applied(
                    &domain,
                    &session_id,
                    &application_id,
                    &plan_id,
                    plan_receipt.record_hash,
                    result_hash,
                    &caller,
                )
                .map_err(conflict_or_internal)?
            {
                MutationAppliedOutcome::Applied(r) | MutationAppliedOutcome::AlreadyApplied(r) => r,
            };

            let exec = ExecutionRecord {
                preview_digest: *plan.digest.as_bytes(),
                action_item_id,
                session_id: session_id.clone(),
                decision_id: approve_ref.decision_id.clone(),
                decision_record_hash: approve_ref.record_hash,
                gate_record_hash: gate_receipt.record_hash,
                activation_id,
                activation_record_hash: activation.record_hash,
                plan_id,
                plan_record_hash: plan_receipt.record_hash,
                application_id,
                application_record_hash: applied.record_hash,
                result_hash,
            };
            let _ = generation;
            let _ = assignee_label;
            let response = confirm_response(&row_id, &session_id, &exec, false);
            if let Some(row) = workspace.row_mut(&row_id) {
                row.pending_item_id = None;
                row.execution = Some(exec);
            }
            Ok(Ok((response, true)))
        })
        .ok_or_else(workspace_not_initialized)??;
    match result {
        Ok((body, created)) => {
            if created {
                Ok(HttpResponse::Created().json(body))
            } else {
                Ok(HttpResponse::Ok().json(body))
            }
        }
        Err(resp) => Ok(resp),
    }
}

fn confirm_response(
    row_id: &str,
    session_id: &str,
    exec: &ExecutionRecord,
    idempotent: bool,
) -> Value {
    json!({
        "row_id": row_id,
        "action_item_id": exec.action_item_id,
        "session_id": session_id,
        "decision_id": exec.decision_id,
        "decision_record_hash": hex32(&exec.decision_record_hash),
        "gate_record_hash": hex32(&exec.gate_record_hash),
        "activation_id": exec.activation_id,
        "activation_record_hash": hex32(&exec.activation_record_hash),
        "plan_id": exec.plan_id,
        "plan_record_hash": hex32(&exec.plan_record_hash),
        "application_id": exec.application_id,
        "application_record_hash": hex32(&exec.application_record_hash),
        "result_hash": hex32(&exec.result_hash),
        "preview_digest": hex32(&exec.preview_digest),
        "idempotent": idempotent,
        "non_claims": [
            "the receipts record process facts; they grant no authority",
            "a facilitator with a trusted narrow capability confirmed fixture work — no collective vote occurred and none is implied",
            "this is an isolated fictional rehearsal, not production governance",
        ],
    })
}

/// GET /domains/{domain_id}/rehearsal/receipts — the ladder receipts recorded
/// this workspace lifetime (ids and hashes only; no DIDs).
pub async fn rehearsal_receipts<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let (domain, _caller) = read_gate(&ctx, &http_req, &domain_id).await?;
    let state = ctx.manager.rehearsal_state();
    let body = state
        .with_workspace(&domain.0, |workspace| {
            let mut receipts = Vec::new();
            if let Some(hash) = &workspace.session_record_hash {
                receipts.push(json!({
                    "class": "process_session_opened",
                    "id": workspace.session_id(),
                    "record_hash": hex32(hash),
                }));
            }
            for d in &workspace.decisions {
                receipts.push(json!({
                    "class": "decision_recorded",
                    "id": d.decision_id,
                    "record_hash": hex32(&d.record_hash),
                    "row_id": d.row_id,
                    "decision": d.decision.as_str(),
                }));
            }
            for row in &workspace.rows {
                if let Some(exec) = &row.execution {
                    receipts.push(json!({
                        "class": "process_gate_result",
                        "id": format!("scope-confirmation:{}", exec.plan_id),
                        "record_hash": hex32(&exec.gate_record_hash),
                    }));
                    receipts.push(json!({
                        "class": "activation_crossed",
                        "id": exec.activation_id,
                        "record_hash": hex32(&exec.activation_record_hash),
                        "references_decision": exec.decision_id,
                    }));
                    receipts.push(json!({
                        "class": "mutation_plan_recorded",
                        "id": exec.plan_id,
                        "record_hash": hex32(&exec.plan_record_hash),
                        "body_hash": hex32(&exec.preview_digest),
                        "references_activation": exec.activation_id,
                    }));
                    receipts.push(json!({
                        "class": "mutation_applied",
                        "id": exec.application_id,
                        "record_hash": hex32(&exec.application_record_hash),
                        "references_plan": exec.plan_id,
                        "result_hash": hex32(&exec.result_hash),
                        "action_item_id": exec.action_item_id,
                    }));
                }
            }
            json!({
                "generation": workspace.generation,
                "receipts": receipts,
                "non_claims": ["receipts record process facts and grant no authority"],
            })
        })
        .ok_or_else(workspace_not_initialized)?;
    Ok(HttpResponse::Ok().json(body))
}

/// GET /domains/{domain_id}/rehearsal/evidence-export — value-withheld
/// evidence packet derived from actual workspace outcomes, with a verifiable
/// SHA-256 packet hash.
pub async fn rehearsal_evidence_export<E: GovernanceEventEmitter + Clone + 'static>(
    ctx: web::Data<GovernanceContext<E>>,
    http_req: HttpRequest,
    domain_id: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let (domain, _caller) = read_gate(&ctx, &http_req, &domain_id).await?;
    let state = ctx.manager.rehearsal_state();
    let body = state
        .with_workspace(&domain.0, |workspace| {
            let rows: Vec<Value> = workspace
                .rows
                .iter()
                .map(|row| {
                    let outcome = if row.execution.is_some() {
                        "executed"
                    } else if row.pending_item_id.is_some() {
                        // Item created but the applied receipt was never
                        // recorded (interrupted confirm): say so plainly.
                        "interrupted-execution"
                    } else {
                        match row.base.status {
                            PendingPublishRowStatus::Rejected => "rejected",
                            PendingPublishRowStatus::NeedsEdit => "edit-and-resubmit",
                            PendingPublishRowStatus::ApprovedForPublish => "approved-not-executed",
                            _ => "deferred",
                        }
                    };
                    let mut v = json!({
                        "id": row.base.id,
                        "kind": to_value_or_error(&row.base.kind),
                        "outcome": outcome,
                        "version": row.version,
                        "source_provenance": to_value_or_error(&row.base.source_provenance),
                        "assignee_label": row.base.assignee_label,
                    });
                    if let Some(obj) = v.as_object_mut() {
                        if let Some(exec) = &row.execution {
                            obj.insert("preview_digest".into(), json!(hex32(&exec.preview_digest)));
                            obj.insert(
                                "plan_record_hash".into(),
                                json!(hex32(&exec.plan_record_hash)),
                            );
                            obj.insert(
                                "application_record_hash".into(),
                                json!(hex32(&exec.application_record_hash)),
                            );
                            obj.insert("action_item_id".into(), json!(exec.action_item_id));
                            obj.insert("mutation_executed".into(), json!(true));
                        } else {
                            obj.insert("mutation_executed".into(), json!(false));
                        }
                    }
                    v
                })
                .collect();
            let decisions: Vec<Value> = workspace
                .decisions
                .iter()
                .map(|d| {
                    json!({
                        "seq": d.seq,
                        "row_id": d.row_id,
                        "row_version": d.row_version,
                        "decision": d.decision.as_str(),
                        "decision_id": d.decision_id,
                        "record_hash": hex32(&d.record_hash),
                        "note_present": d.note_present,
                    })
                })
                .collect();
            let bindings: Vec<Value> = workspace
                .bindings
                .iter()
                .map(|(label, did)| json!({ "label": label, "bound": !did.is_empty() }))
                .collect();

            let content = json!({
                "contract": "urn:icn:contract:rehearsal-workflow-evidence:v1",
                "origin": "rehearsal_runtime",
                "domain_id": domain.0,
                "run_id": workspace.run_id,
                "generation": workspace.generation,
                "session_id": workspace.session_id(),
                "session_record_hash": workspace.session_record_hash.as_ref().map(hex32),
                "rows": rows,
                "decisions": decisions,
                "bindings": bindings,
                "privacy_review": {
                    "dids_exported": false,
                    "credentials_exported": false,
                    "private_overlay_values_exported": false,
                    "identity_exposure": "labels-and-bound-flags-only",
                },
                "non_claims": [
                    "fictional rehearsal evidence — not production governance, not a pilot, not live federation",
                    "receipts record process facts and grant no authority; a facilitator capability confirmed fixture work — no collective vote occurred and none is implied",
                    "identity bindings stay on the rehearsal node; this packet carries labels only",
                ],
                "privacy_review_result": "pass: no DIDs, credentials, private-overlay values, paths, or topology exported",
                "hash_domain_tag": "icn:gov:rehearsal_workflow_evidence:v1",
                "generated_at": icn_time::current_timestamp_secs(),
            });
            // EVERY exported field above — including generated_at — is bound
            // by the hashes; only the two hash fields themselves are outside
            // the canonical content. BLAKE3 (domain-separated) is primary per
            // ICN receipt convention; the SHA-256 mirror lets a browser
            // steward view verify via WebCrypto.
            let canonical = match serde_json::to_vec(&content) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::error!(detail = %e, "evidence packet serialization failed");
                    return json!({ "error": "evidence packet serialization failed" });
                }
            };
            let (packet_hash, packet_hash_sha256) = ws::evidence_packet_hashes(&canonical);
            let mut packet = content;
            if let Some(obj) = packet.as_object_mut() {
                obj.insert("packet_hash".into(), json!(packet_hash));
                obj.insert("packet_hash_sha256".into(), json!(packet_hash_sha256));
            }
            packet
        })
        .ok_or_else(workspace_not_initialized)?;
    Ok(HttpResponse::Ok().json(body))
}

/// Register the rehearsal routes. Called from `configure` ONLY when the
/// build mode allows the rehearsal mutation surface.
pub fn configure_rehearsal_routes<E: GovernanceEventEmitter + Clone + 'static>(
    cfg: &mut web::ServiceConfig,
) {
    cfg.service(
        web::resource("/domains/{domain_id}/rehearsal/reset")
            .route(web::post().to(rehearsal_reset::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/pending-publish")
            .route(web::get().to(rehearsal_list::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/pending-publish/{row_id}")
            .route(web::get().to(rehearsal_row_detail::<E>))
            .route(web::put().to(rehearsal_edit::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/pending-publish/{row_id}/review")
            .route(web::post().to(rehearsal_review::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/pending-publish/{row_id}/assign")
            .route(web::post().to(rehearsal_assign::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/pending-publish/{row_id}/preview")
            .route(web::get().to(rehearsal_preview::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/pending-publish/{row_id}/confirm")
            .route(web::post().to(rehearsal_confirm::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/bindings")
            .route(web::post().to(rehearsal_bind_label::<E>))
            .route(web::get().to(rehearsal_list_bindings::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/receipts")
            .route(web::get().to(rehearsal_receipts::<E>)),
    )
    .service(
        web::resource("/domains/{domain_id}/rehearsal/evidence-export")
            .route(web::get().to(rehearsal_evidence_export::<E>)),
    );
}
