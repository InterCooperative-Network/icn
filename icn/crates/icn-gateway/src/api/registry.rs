//! Decision Registry API endpoints
//!
//! Provides REST API for managing governance meetings and decisions.
//! This is a "Google Drive replacement" for co-op meetings/decisions,
//! indexing over canonical receipts without touching kernel primitives.
//!
//! ## Endpoints
//!
//! ### Meetings
//! - `POST /v1/registry/meetings` - Create a meeting
//! - `GET /v1/registry/meetings?coop_id=...` - List meetings for a coop
//!
//! ### Decisions
//! - `POST /v1/registry/decisions` - Index a decision
//! - `GET /v1/registry/decisions?coop_id=...&meeting_id=...&tag=...` - List decisions
//! - `GET /v1/registry/decisions/{id}` - Get decision details
//! - `GET /v1/registry/decisions/{id}/trace` - Get decision trace/provenance

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::api::receipts::GovernanceReceiptResponse;
use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};
use crate::receipt_store::ReceiptStore;

// ============================================================================
// Types (mirrors icn-governance-actor/src/registry/models.rs)
// ============================================================================

/// Status of a governance decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    /// Decision is pending (voting in progress).
    Pending,
    /// Decision was approved.
    Approved,
    /// Decision was rejected.
    Rejected,
}

impl Default for DecisionStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// A cooperative meeting record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    /// Unique meeting identifier, e.g., "mtg-2026-02-10-summit".
    pub meeting_id: String,
    /// Cooperative DID that owns this meeting.
    pub coop_id: String,
    /// Human-readable title, e.g., "Summit Planning 2026".
    pub title: String,
    /// Meeting start time as Unix timestamp.
    pub starts_at: u64,
    /// Optional meeting notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Creation timestamp.
    pub created_at: u64,
}

/// An index entry for a governance decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionIndexEntry {
    /// Primary key, links to canonical receipt.
    pub decision_receipt_id: String,
    /// Canonical hash (immutable anchor).
    pub decision_hash: String,
    /// Cooperative DID.
    pub coop_id: String,
    /// Optional meeting association.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_id: Option<String>,
    /// Human-readable summary.
    pub title: String,
    /// Searchable tags, e.g., ["budget", "approved"].
    pub tags: Vec<String>,
    /// Current status of the decision.
    pub status: DecisionStatus,
    /// Creation timestamp.
    pub created_at: u64,
    /// Type of proposal, e.g., "text", "treasury_spend".
    pub proposal_type: String,
    /// If this decision triggered a treasury action, the effect ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treasury_effect_id: Option<String>,
    /// Actual ledger entry hash if a ledger entry was created.
    /// Only set when we have verified the effect produced a real entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_entry_hash: Option<String>,
}

/// Decision trace showing provenance and effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionTrace {
    /// The decision hash.
    pub decision_hash: String,
    /// Parent receipts (decisions this depends on).
    pub parent_receipts: Vec<String>,
    /// Effects triggered by this decision.
    pub effects: Vec<DecisionEffect>,
    /// Actual ledger entry hash if the effect created a ledger entry.
    /// Only present when we have verified the entry exists in the ledger.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_entry_hash: Option<String>,
    /// Provenance chain (ordered list of related decisions).
    pub provenance_chain: Vec<String>,
    /// Resolution status for trace sections.
    pub resolution: TraceResolution,
}

/// Tracks what portions of a trace are resolved vs unresolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceResolution {
    /// Whether the decision was found in the registry.
    pub decision_resolved: bool,
    /// Whether the receipt is available (linked to canonical store).
    pub receipt_resolved: bool,
    /// Whether the effect result is available.
    pub effect_resolved: bool,
    /// Whether the ledger entry was verified.
    pub ledger_entry_resolved: bool,
}

/// An effect triggered by a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionEffect {
    /// Type of effect, e.g., "treasury_disbursement", "membership_change".
    #[serde(rename = "type")]
    pub effect_type: String,
    /// Details about the effect.
    pub details: serde_json::Value,
    /// Whether the effect result has been resolved from canonical stores.
    #[serde(default)]
    pub resolved: bool,
}

// ============================================================================
// Store (in-memory decision registry)
// ============================================================================

/// Filter criteria for querying decisions.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecisionFilter {
    pub coop_id: Option<String>,
    pub meeting_id: Option<String>,
    pub tag: Option<String>,
    pub after: Option<u64>,
    pub before: Option<u64>,
    pub proposal_type: Option<String>,
}

impl DecisionFilter {
    fn matches(&self, entry: &DecisionIndexEntry) -> bool {
        if let Some(ref coop_id) = self.coop_id {
            if &entry.coop_id != coop_id {
                return false;
            }
        }
        if let Some(ref meeting_id) = self.meeting_id {
            if entry.meeting_id.as_ref() != Some(meeting_id) {
                return false;
            }
        }
        if let Some(ref tag) = self.tag {
            if !entry.tags.contains(tag) {
                return false;
            }
        }
        if let Some(after) = self.after {
            if entry.created_at < after {
                return false;
            }
        }
        if let Some(before) = self.before {
            if entry.created_at > before {
                return false;
            }
        }
        if let Some(ref proposal_type) = self.proposal_type {
            if &entry.proposal_type != proposal_type {
                return false;
            }
        }
        true
    }
}

/// In-memory store for governance meetings and decisions.
#[derive(Debug, Default)]
pub struct DecisionRegistry {
    meetings: RwLock<HashMap<String, Meeting>>,
    decisions: RwLock<HashMap<String, DecisionIndexEntry>>,
}

impl DecisionRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a meeting to the registry.
    pub fn add_meeting(&self, meeting: Meeting) -> Result<()> {
        let mut meetings = self
            .meetings
            .write()
            .map_err(|_| GatewayError::InternalError("failed to acquire meetings lock".into()))?;

        if meetings.contains_key(&meeting.meeting_id) {
            return Err(GatewayError::Conflict(format!(
                "meeting already exists: {}",
                meeting.meeting_id
            )));
        }

        meetings.insert(meeting.meeting_id.clone(), meeting);
        Ok(())
    }

    /// Get a meeting by ID.
    pub fn get_meeting(&self, meeting_id: &str) -> Option<Meeting> {
        self.meetings
            .read()
            .ok()
            .and_then(|m| m.get(meeting_id).cloned())
    }

    /// List all meetings for a cooperative.
    pub fn list_meetings(&self, coop_id: &str) -> Vec<Meeting> {
        self.meetings
            .read()
            .map(|m| {
                m.values()
                    .filter(|meeting| meeting.coop_id == coop_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Index a decision entry.
    pub fn index_decision(&self, entry: DecisionIndexEntry) -> Result<()> {
        let mut decisions = self
            .decisions
            .write()
            .map_err(|_| GatewayError::InternalError("failed to acquire decisions lock".into()))?;

        decisions.insert(entry.decision_receipt_id.clone(), entry);
        Ok(())
    }

    /// Get a decision by receipt ID.
    pub fn get_decision(&self, receipt_id: &str) -> Option<DecisionIndexEntry> {
        self.decisions
            .read()
            .ok()
            .and_then(|d| d.get(receipt_id).cloned())
    }

    /// List decisions matching the given filter.
    pub fn list_decisions(&self, filter: DecisionFilter) -> Vec<DecisionIndexEntry> {
        self.decisions
            .read()
            .map(|d| d.values().filter(|e| filter.matches(e)).cloned().collect())
            .unwrap_or_default()
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create a meeting.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMeetingRequest {
    /// Cooperative DID that owns this meeting.
    pub coop_id: String,
    /// Human-readable title.
    pub title: String,
    /// Meeting start time as Unix timestamp.
    pub starts_at: u64,
    /// Optional meeting notes.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Response for a created meeting.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMeetingResponse {
    pub meeting_id: String,
    pub coop_id: String,
    pub title: String,
    pub starts_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_at: u64,
}

/// Query parameters for listing meetings.
#[derive(Debug, Deserialize)]
pub struct ListMeetingsQuery {
    pub coop_id: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Response for listing meetings.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListMeetingsResponse {
    pub data: Vec<Meeting>,
    pub pagination: PaginationInfo,
}

/// Query parameters for listing decisions.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDecisionsQuery {
    #[serde(default)]
    pub coop_id: Option<String>,
    #[serde(default)]
    pub meeting_id: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub after: Option<u64>,
    #[serde(default)]
    pub before: Option<u64>,
    #[serde(default)]
    pub proposal_type: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Response for listing decisions.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDecisionsResponse {
    pub data: Vec<DecisionIndexEntry>,
    pub pagination: PaginationInfo,
}

/// Response for getting a single decision.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDecisionResponse {
    pub decision: DecisionIndexEntry,
    /// Canonical receipt data (placeholder for future implementation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<serde_json::Value>,
}

/// Simple pagination info.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationInfo {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

/// Request to index a decision.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDecisionRequest {
    /// Cooperative DID that owns this decision.
    pub coop_id: String,
    /// Optional meeting association.
    #[serde(default)]
    pub meeting_id: Option<String>,
    /// Human-readable summary/title.
    pub title: String,
    /// Searchable tags, e.g., ["budget", "approved"].
    #[serde(default)]
    pub tags: Vec<String>,
    /// Status of the decision.
    #[serde(default)]
    pub status: DecisionStatus,
    /// Type of proposal, e.g., "text", "treasury_spend".
    #[serde(default = "default_proposal_type")]
    pub proposal_type: String,
    /// If this decision triggered a treasury action, the effect details.
    #[serde(default)]
    pub treasury_effect: Option<TreasuryEffectDetails>,
    /// Actual ledger entry hash if the effect created a verified entry.
    /// Only provided when the caller has verified the ledger entry exists.
    #[serde(default)]
    pub ledger_entry_hash: Option<String>,
}

fn default_proposal_type() -> String {
    "text".to_string()
}

/// Treasury effect details for creating decisions.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreasuryEffectDetails {
    /// Amount of credits.
    pub amount: i64,
    /// Source account (e.g., "treasury:coop-xyz").
    pub from: String,
    /// Destination account (e.g., a DID or account identifier).
    pub to: String,
    /// Currency type.
    #[serde(default = "default_currency")]
    pub currency: String,
}

fn default_currency() -> String {
    "credits".to_string()
}

/// Response for indexing a decision.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDecisionResponse {
    /// The generated receipt ID (primary key).
    pub decision_receipt_id: String,
    /// The canonical hash.
    pub decision_hash: String,
    /// Cooperative DID.
    pub coop_id: String,
    /// Meeting ID if associated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_id: Option<String>,
    /// Title of the decision.
    pub title: String,
    /// Status of the decision.
    pub status: DecisionStatus,
    /// Proposal type.
    pub proposal_type: String,
    /// Treasury effect ID if created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treasury_effect_id: Option<String>,
    /// Actual ledger entry hash if provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_entry_hash: Option<String>,
    /// Creation timestamp.
    pub created_at: u64,
}

// ============================================================================
// Handlers
// ============================================================================

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 100;

/// POST /registry/meetings - Create a new meeting
#[post("/meetings")]
pub async fn create_meeting(
    http_req: HttpRequest,
    registry: web::Data<Arc<DecisionRegistry>>,
    req: web::Json<CreateMeetingRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    // Validate title length
    if req.title.is_empty() || req.title.len() > 256 {
        return Err(GatewayError::BadRequest(
            "Title must be 1-256 characters".to_string(),
        ));
    }

    // Validate coop_id format (basic DID check)
    if !req.coop_id.starts_with("did:") {
        return Err(GatewayError::BadRequest(
            "coop_id must be a valid DID".to_string(),
        ));
    }

    // Generate meeting ID
    let now = icn_time::current_timestamp_secs();
    let meeting_id = format!(
        "mtg-{}-{}",
        chrono::Utc::now().format("%Y-%m-%d"),
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    let meeting = Meeting {
        meeting_id: meeting_id.clone(),
        coop_id: req.coop_id.clone(),
        title: req.title.clone(),
        starts_at: req.starts_at,
        notes: req.notes.clone(),
        created_at: now,
    };

    registry.add_meeting(meeting.clone())?;

    tracing::info!(
        meeting_id = %meeting_id,
        coop_id = %req.coop_id,
        creator = %claims.sub,
        "Created meeting"
    );

    Ok(HttpResponse::Created().json(CreateMeetingResponse {
        meeting_id: meeting.meeting_id,
        coop_id: meeting.coop_id,
        title: meeting.title,
        starts_at: meeting.starts_at,
        notes: meeting.notes,
        created_at: meeting.created_at,
    }))
}

/// GET /registry/meetings - List meetings for a cooperative
#[get("/meetings")]
pub async fn list_meetings(
    http_req: HttpRequest,
    registry: web::Data<Arc<DecisionRegistry>>,
    query: web::Query<ListMeetingsQuery>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:read")?;

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);

    let all_meetings = registry.list_meetings(&query.coop_id);
    let total = all_meetings.len();

    // Sort by starts_at descending (most recent first)
    let mut meetings: Vec<_> = all_meetings;
    meetings.sort_by(|a, b| b.starts_at.cmp(&a.starts_at));

    // Apply pagination
    let data: Vec<_> = meetings.into_iter().skip(offset).take(limit).collect();
    let has_more = offset + data.len() < total;

    Ok(HttpResponse::Ok().json(ListMeetingsResponse {
        data,
        pagination: PaginationInfo {
            total,
            limit,
            offset,
            has_more,
        },
    }))
}

/// POST /registry/decisions - Index a new decision
#[post("/decisions")]
pub async fn index_decision_endpoint(
    http_req: HttpRequest,
    registry: web::Data<Arc<DecisionRegistry>>,
    req: web::Json<IndexDecisionRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    // Validate title length
    if req.title.is_empty() || req.title.len() > 256 {
        return Err(GatewayError::BadRequest(
            "Title must be 1-256 characters".to_string(),
        ));
    }

    // Validate coop_id format (basic DID check)
    if !req.coop_id.starts_with("did:") {
        return Err(GatewayError::BadRequest(
            "coop_id must be a valid DID".to_string(),
        ));
    }

    // Generate IDs
    let now = icn_time::current_timestamp_secs();
    let receipt_id = format!("receipt-{}", &uuid::Uuid::new_v4().to_string()[..12]);

    // Generate 32-byte canonical hash from stable decision content.
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(req.coop_id.as_bytes());
    hasher.update([0x1f]);
    hasher.update(req.title.as_bytes());
    hasher.update([0x1f]);
    hasher.update(now.to_be_bytes());
    let decision_hash = hex::encode(hasher.finalize());

    // Handle treasury effect if present
    let treasury_effect_id = req
        .treasury_effect
        .as_ref()
        .map(|_| format!("effect-{}", &uuid::Uuid::new_v4().to_string()[..8]));

    let entry = DecisionIndexEntry {
        decision_receipt_id: receipt_id.clone(),
        decision_hash: decision_hash.clone(),
        coop_id: req.coop_id.clone(),
        meeting_id: req.meeting_id.clone(),
        title: req.title.clone(),
        tags: req.tags.clone(),
        status: req.status,
        created_at: now,
        proposal_type: req.proposal_type.clone(),
        treasury_effect_id: treasury_effect_id.clone(),
        ledger_entry_hash: req.ledger_entry_hash.clone(),
    };

    registry.index_decision(entry)?;

    tracing::info!(
        receipt_id = %receipt_id,
        coop_id = %req.coop_id,
        title = %req.title,
        proposal_type = %req.proposal_type,
        creator = %claims.sub,
        has_ledger_entry = req.ledger_entry_hash.is_some(),
        "Indexed decision"
    );

    Ok(HttpResponse::Created().json(IndexDecisionResponse {
        decision_receipt_id: receipt_id,
        decision_hash,
        coop_id: req.coop_id.clone(),
        meeting_id: req.meeting_id.clone(),
        title: req.title.clone(),
        status: req.status,
        proposal_type: req.proposal_type.clone(),
        treasury_effect_id,
        ledger_entry_hash: req.ledger_entry_hash.clone(),
        created_at: now,
    }))
}

/// GET /registry/decisions - List decisions with filtering
#[get("/decisions")]
pub async fn list_decisions(
    http_req: HttpRequest,
    registry: web::Data<Arc<DecisionRegistry>>,
    query: web::Query<ListDecisionsQuery>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:read")?;

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);

    let filter = DecisionFilter {
        coop_id: query.coop_id.clone(),
        meeting_id: query.meeting_id.clone(),
        tag: query.tag.clone(),
        after: query.after,
        before: query.before,
        proposal_type: query.proposal_type.clone(),
    };

    let all_decisions = registry.list_decisions(filter);
    let total = all_decisions.len();

    // Sort by created_at descending (most recent first)
    let mut decisions = all_decisions;
    decisions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Apply pagination
    let data: Vec<_> = decisions.into_iter().skip(offset).take(limit).collect();
    let has_more = offset + data.len() < total;

    Ok(HttpResponse::Ok().json(ListDecisionsResponse {
        data,
        pagination: PaginationInfo {
            total,
            limit,
            offset,
            has_more,
        },
    }))
}

/// GET /registry/decisions/{id} - Get a specific decision
#[get("/decisions/{id}")]
pub async fn get_decision(
    http_req: HttpRequest,
    registry: web::Data<Arc<DecisionRegistry>>,
    receipt_store: Option<web::Data<Arc<ReceiptStore>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:read")?;

    let decision_id = path.into_inner();

    let decision = registry
        .get_decision(&decision_id)
        .ok_or_else(|| GatewayError::NotFound(format!("Decision not found: {}", decision_id)))?;

    let receipt = receipt_store.as_ref().and_then(|store| {
        lookup_governance_receipt(store.get_ref().as_ref(), &decision.decision_hash).and_then(|r| {
            match serde_json::to_value(r) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        decision_id = %decision_id,
                        decision_hash = %decision.decision_hash,
                        "failed to serialize governance receipt for decision response"
                    );
                    None
                }
            }
        })
    });

    Ok(HttpResponse::Ok().json(GetDecisionResponse { decision, receipt }))
}

/// GET /registry/decisions/{id}/trace - Get decision trace/provenance
#[get("/decisions/{id}/trace")]
pub async fn get_decision_trace(
    http_req: HttpRequest,
    registry: web::Data<Arc<DecisionRegistry>>,
    receipt_store: Option<web::Data<Arc<ReceiptStore>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "governance:read")?;

    let decision_id = path.into_inner();

    let decision = registry
        .get_decision(&decision_id)
        .ok_or_else(|| GatewayError::NotFound(format!("Decision not found: {}", decision_id)))?;

    // Build trace from decision data - only include verified data
    let mut effects = Vec::new();

    // If there's a treasury effect, include it with resolution status
    // The effect is "resolved" only if we have a verified ledger_entry_hash
    let has_verified_ledger_entry = decision.ledger_entry_hash.is_some();
    if let Some(ref effect_id) = decision.treasury_effect_id {
        effects.push(DecisionEffect {
            effect_type: "treasury_disbursement".to_string(),
            details: serde_json::json!({
                "effectId": effect_id,
                // Only include ledger_entry_hash if we have it
                "ledgerEntryHash": decision.ledger_entry_hash.as_deref()
            }),
            resolved: has_verified_ledger_entry,
        });
    }

    let has_receipt = receipt_store
        .as_ref()
        .and_then(|store| {
            lookup_governance_receipt(store.get_ref().as_ref(), &decision.decision_hash)
        })
        .is_some();

    // Build honest resolution status
    let resolution = TraceResolution {
        decision_resolved: true, // We found the decision in the registry
        receipt_resolved: has_receipt,
        effect_resolved: has_verified_ledger_entry, // Only resolved if we have verified the effect
        ledger_entry_resolved: has_verified_ledger_entry, // Only true if we have the actual hash
    };

    let trace = DecisionTrace {
        decision_hash: decision.decision_hash.clone(),
        parent_receipts: vec![], // TODO: populate from canonical receipt graph when available
        effects,
        // Only include ledger_entry_hash if we have a verified one - never fabricate
        ledger_entry_hash: decision.ledger_entry_hash.clone(),
        provenance_chain: vec![decision.decision_receipt_id.clone()],
        resolution,
    };

    Ok(HttpResponse::Ok().json(trace))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure registry routes.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_meeting)
        .service(list_meetings)
        .service(index_decision_endpoint)
        .service(list_decisions)
        .service(get_decision)
        .service(get_decision_trace);
}

fn parse_decision_hash_hex(hex_str: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = hex::decode(hex_str)?;
    let hash: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("decision hash must be 32 bytes"))?;
    Ok(hash)
}

fn lookup_governance_receipt(
    store: &ReceiptStore,
    decision_hash_hex: &str,
) -> Option<GovernanceReceiptResponse> {
    let hash = match parse_decision_hash_hex(decision_hash_hex) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(
                error = %e,
                decision_hash = %decision_hash_hex,
                "failed to parse decision hash while hydrating governance receipt"
            );
            return None;
        }
    };

    match store.get_governance(&hash) {
        Ok(Some(receipt)) => Some(GovernanceReceiptResponse {
            decision_hash: hex::encode(receipt.decision_hash),
            proposal_id: receipt.proposal_id,
            domain_id: receipt.domain_id,
            outcome: format!("{:?}", receipt.outcome),
            vote_tally: crate::api::receipts::GovernanceVoteTallyResponse {
                for_votes: receipt.vote_tally.for_votes,
                against_votes: receipt.vote_tally.against_votes,
                abstain_votes: receipt.vote_tally.abstain_votes,
            },
            vote_hash: hex::encode(receipt.vote_hash),
        }),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(
                error = %e,
                decision_hash = %decision_hash_hex,
                "failed to read governance receipt from receipt store"
            );
            None
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ledger::get_entries_by_decision;
    use crate::auth::TokenClaims;
    use crate::ledger_mgr::LedgerManager;
    use crate::receipt_store::ReceiptStore;
    use actix_web::{test as actix_test, App, HttpMessage};
    use icn_identity::KeyPair;
    use icn_ledger::entry::JournalEntryBuilder;

    fn create_test_registry() -> Arc<DecisionRegistry> {
        let registry = Arc::new(DecisionRegistry::new());

        // Add sample meeting
        let meeting = Meeting {
            meeting_id: "mtg-2026-01-15-test".to_string(),
            coop_id: "did:icn:test123".to_string(),
            title: "Test Meeting".to_string(),
            starts_at: 1800000000,
            notes: Some("Test notes".to_string()),
            created_at: 1799999900,
        };
        registry.add_meeting(meeting).ok();

        // Add sample decision
        let decision = DecisionIndexEntry {
            decision_receipt_id: "receipt-001".to_string(),
            decision_hash: "hash-abc123".to_string(),
            coop_id: "did:icn:test123".to_string(),
            meeting_id: Some("mtg-2026-01-15-test".to_string()),
            title: "Approve budget".to_string(),
            tags: vec!["budget".to_string(), "approved".to_string()],
            status: DecisionStatus::Approved,
            created_at: 1800000100,
            proposal_type: "treasury_spend".to_string(),
            treasury_effect_id: Some("effect-001".to_string()),
            ledger_entry_hash: None, // No verified ledger entry yet
        };
        registry.index_decision(decision).ok();

        registry
    }

    #[test]
    fn test_registry_add_meeting() {
        let registry = DecisionRegistry::new();

        let meeting = Meeting {
            meeting_id: "mtg-001".to_string(),
            coop_id: "did:icn:coop1".to_string(),
            title: "Test".to_string(),
            starts_at: 1700000000,
            notes: None,
            created_at: 1699999900,
        };

        assert!(registry.add_meeting(meeting.clone()).is_ok());

        // Duplicate should fail
        assert!(registry.add_meeting(meeting).is_err());
    }

    #[test]
    fn test_registry_list_meetings() {
        let registry = create_test_registry();
        let meetings = registry.list_meetings("did:icn:test123");
        assert_eq!(meetings.len(), 1);
        assert_eq!(meetings[0].meeting_id, "mtg-2026-01-15-test");
    }

    #[test]
    fn test_registry_get_decision() {
        let registry = create_test_registry();
        let decision = registry.get_decision("receipt-001");
        assert!(decision.is_some());
        assert_eq!(
            decision.as_ref().map(|d| d.title.as_str()),
            Some("Approve budget")
        );
    }

    #[test]
    fn test_registry_list_decisions_with_filter() {
        let registry = create_test_registry();

        // Add another decision for filtering tests
        let decision2 = DecisionIndexEntry {
            decision_receipt_id: "receipt-002".to_string(),
            decision_hash: "hash-def456".to_string(),
            coop_id: "did:icn:other".to_string(),
            meeting_id: None,
            title: "Other decision".to_string(),
            tags: vec!["membership".to_string()],
            status: DecisionStatus::Pending,
            created_at: 1800000200,
            proposal_type: "text".to_string(),
            treasury_effect_id: None,
            ledger_entry_hash: None,
        };
        registry.index_decision(decision2).ok();

        // Filter by coop_id
        let filter = DecisionFilter {
            coop_id: Some("did:icn:test123".to_string()),
            ..Default::default()
        };
        let decisions = registry.list_decisions(filter);
        assert_eq!(decisions.len(), 1);

        // Filter by tag
        let filter = DecisionFilter {
            tag: Some("budget".to_string()),
            ..Default::default()
        };
        let decisions = registry.list_decisions(filter);
        assert_eq!(decisions.len(), 1);

        // No filter - get all
        let filter = DecisionFilter::default();
        let decisions = registry.list_decisions(filter);
        assert_eq!(decisions.len(), 2);
    }

    #[test]
    fn test_decision_status_serialization() {
        let status = DecisionStatus::Approved;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"approved\"");

        let deserialized: DecisionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, DecisionStatus::Approved);
    }

    #[test]
    fn test_decision_trace_structure() {
        // Test trace with resolved ledger entry
        let trace = DecisionTrace {
            decision_hash: "abc123".to_string(),
            parent_receipts: vec!["parent-001".to_string()],
            effects: vec![DecisionEffect {
                effect_type: "treasury_disbursement".to_string(),
                details: serde_json::json!({"amount": 1000}),
                resolved: true,
            }],
            ledger_entry_hash: Some("0x1234567890abcdef".to_string()),
            provenance_chain: vec!["abc123".to_string()],
            resolution: TraceResolution {
                decision_resolved: true,
                receipt_resolved: false,
                effect_resolved: true,
                ledger_entry_resolved: true,
            },
        };

        let json = serde_json::to_string(&trace).unwrap();
        assert!(json.contains("decisionHash"));
        assert!(json.contains("parentReceipts"));
        assert!(json.contains("ledgerEntryHash"));
        assert!(json.contains("resolution"));
        assert!(json.contains("\"resolved\":true"));
    }

    #[test]
    fn test_decision_trace_unresolved() {
        // Test trace without verified ledger entry - should NOT fabricate data
        let trace = DecisionTrace {
            decision_hash: "abc123".to_string(),
            parent_receipts: vec![],
            effects: vec![DecisionEffect {
                effect_type: "treasury_disbursement".to_string(),
                details: serde_json::json!({"effectId": "effect-001"}),
                resolved: false,
            }],
            ledger_entry_hash: None, // Honestly unresolved
            provenance_chain: vec!["abc123".to_string()],
            resolution: TraceResolution {
                decision_resolved: true,
                receipt_resolved: false,
                effect_resolved: false,
                ledger_entry_resolved: false,
            },
        };

        let json = serde_json::to_string(&trace).unwrap();
        // Should NOT contain ledgerEntryHash when None (skip_serializing_if)
        assert!(!json.contains("ledgerEntryHash"));
        // Resolution should indicate unresolved sections
        assert!(json.contains("\"ledgerEntryResolved\":false"));
        assert!(json.contains("\"effectResolved\":false"));
    }

    #[test]
    fn test_trace_with_verified_ledger_entry() {
        // Test the trace endpoint behavior with a verified ledger entry
        let registry = Arc::new(DecisionRegistry::new());

        // Add decision WITH a verified ledger entry hash
        let decision = DecisionIndexEntry {
            decision_receipt_id: "receipt-verified".to_string(),
            decision_hash: "hash-verified".to_string(),
            coop_id: "did:icn:test-coop".to_string(),
            meeting_id: None,
            title: "Treasury disbursement".to_string(),
            tags: vec!["treasury".to_string()],
            status: DecisionStatus::Approved,
            created_at: 1800000100,
            proposal_type: "treasury_spend".to_string(),
            treasury_effect_id: Some("effect-verified".to_string()),
            ledger_entry_hash: Some("0xdeadbeef12345678".to_string()), // Real verified hash
        };
        registry.index_decision(decision).ok();

        // Verify we can get the decision with the ledger entry hash
        let retrieved = registry.get_decision("receipt-verified").unwrap();
        assert_eq!(
            retrieved.ledger_entry_hash,
            Some("0xdeadbeef12345678".to_string())
        );
    }

    #[test]
    fn test_index_decision_request_deserialization() {
        let json = r#"{
            "coopId": "did:icn:test123",
            "meetingId": "mtg-2026-02-10",
            "title": "Approve venue deposit",
            "tags": ["budget", "approved"],
            "status": "approved",
            "proposalType": "treasury_spend",
            "treasuryEffect": {
                "amount": 200,
                "from": "treasury:did:icn:test123",
                "to": "did:icn:venue-account",
                "currency": "credits"
            }
        }"#;

        let req: IndexDecisionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.coop_id, "did:icn:test123");
        assert_eq!(req.title, "Approve venue deposit");
        assert_eq!(req.proposal_type, "treasury_spend");
        assert!(req.treasury_effect.is_some());

        let effect = req.treasury_effect.unwrap();
        assert_eq!(effect.amount, 200);
        assert_eq!(effect.from, "treasury:did:icn:test123");
        assert_eq!(effect.to, "did:icn:venue-account");
    }

    #[test]
    fn test_index_decision_response_serialization() {
        let resp = IndexDecisionResponse {
            decision_receipt_id: "receipt-abc123".to_string(),
            decision_hash: "hash123".to_string(),
            coop_id: "did:icn:test".to_string(),
            meeting_id: Some("mtg-001".to_string()),
            title: "Test decision".to_string(),
            status: DecisionStatus::Approved,
            proposal_type: "text".to_string(),
            treasury_effect_id: None,
            ledger_entry_hash: None,
            created_at: 1700000000,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("decisionReceiptId"));
        assert!(json.contains("decisionHash"));
        assert!(!json.contains("treasuryEffectId")); // None fields should be skipped
        assert!(!json.contains("ledgerEntryHash")); // None fields should be skipped
    }

    #[test]
    fn test_index_decision_response_with_ledger_entry() {
        let resp = IndexDecisionResponse {
            decision_receipt_id: "receipt-abc123".to_string(),
            decision_hash: "hash123".to_string(),
            coop_id: "did:icn:test".to_string(),
            meeting_id: None,
            title: "Treasury decision".to_string(),
            status: DecisionStatus::Approved,
            proposal_type: "treasury_spend".to_string(),
            treasury_effect_id: Some("effect-001".to_string()),
            ledger_entry_hash: Some("0xabcdef123456".to_string()),
            created_at: 1700000000,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ledgerEntryHash"));
        assert!(json.contains("0xabcdef123456"));
    }

    #[actix_web::test]
    async fn test_decision_endpoint_hydrates_receipt_and_matches_ledger_by_decision() {
        let registry = Arc::new(DecisionRegistry::new());
        let receipt_store = Arc::new(ReceiptStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let ledger_mgr = Arc::new(LedgerManager::new());
        let coop_id = "test-coop".to_string();
        let proposal_id = "proposal-pilot-1".to_string();
        let decision_receipt_id = "receipt-pilot-1".to_string();

        let decision_hash = receipt_store
            .put_test_governance_receipt(&proposal_id)
            .unwrap();
        let decision_hash_hex = hex::encode(decision_hash);

        registry
            .index_decision(DecisionIndexEntry {
                decision_receipt_id: decision_receipt_id.clone(),
                decision_hash: decision_hash_hex.clone(),
                coop_id: coop_id.clone(),
                meeting_id: None,
                title: "Pilot treasury spend".to_string(),
                tags: vec!["pilot".to_string(), "treasury".to_string()],
                status: DecisionStatus::Approved,
                created_at: icn_time::current_timestamp_secs(),
                proposal_type: "treasury_spend".to_string(),
                treasury_effect_id: Some("effect-pilot-1".to_string()),
                ledger_entry_hash: None,
            })
            .unwrap();

        // Append a real ledger entry carrying the same decision provenance.
        let from = KeyPair::generate().unwrap().did().clone();
        let to = KeyPair::generate().unwrap().did().clone();
        let ledger = ledger_mgr.get_ledger(&coop_id).await.unwrap();
        let entry = JournalEntryBuilder::new(from.clone())
            .debit(from, "credits".to_string(), 25)
            .credit(to, "credits".to_string(), 25)
            .with_governance_provenance(&decision_receipt_id, &decision_hash_hex)
            .build()
            .unwrap();
        ledger.write().await.append_entry(entry).await.unwrap();

        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(registry.clone()))
                .app_data(web::Data::new(receipt_store.clone()))
                .app_data(web::Data::new(ledger_mgr.clone()))
                .service(web::scope("/registry").configure(configure))
                .service(web::scope("/ledger").service(get_entries_by_decision)),
        )
        .await;

        let claims = TokenClaims {
            sub: KeyPair::generate().unwrap().did().to_string(),
            iat: 1_000_000_000,
            coop_id: coop_id.clone(),
            scopes: vec!["governance:read".to_string(), "ledger:read".to_string()],
            exp: 9_999_999_999,
        };

        let req = actix_test::TestRequest::get()
            .uri(&format!("/registry/decisions/{decision_receipt_id}"))
            .to_request();
        req.extensions_mut().insert(claims.clone());
        let decision_resp: serde_json::Value = actix_test::call_and_read_body_json(&app, req).await;

        let hydrated_receipt = decision_resp.get("receipt").cloned().unwrap_or_default();
        assert!(hydrated_receipt.is_object(), "receipt should be hydrated");
        assert_eq!(
            hydrated_receipt
                .get("decisionHash")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
            decision_hash_hex
        );
        assert_eq!(
            hydrated_receipt
                .get("proposalId")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
            proposal_id
        );

        let req = actix_test::TestRequest::get()
            .uri(&format!(
                "/ledger/{coop_id}/entries/by-decision?decision_hash={decision_hash_hex}"
            ))
            .to_request();
        req.extensions_mut().insert(claims);
        let ledger_resp: crate::models::TransactionHistoryResponse =
            actix_test::call_and_read_body_json(&app, req).await;

        assert_eq!(ledger_resp.transactions.len(), 1);
        let tx = &ledger_resp.transactions[0];
        assert_eq!(
            tx.decision_hash.as_deref().unwrap_or_default(),
            decision_hash_hex
        );
        assert_eq!(
            tx.decision_receipt_id.as_deref().unwrap_or_default(),
            decision_receipt_id
        );
    }
}
