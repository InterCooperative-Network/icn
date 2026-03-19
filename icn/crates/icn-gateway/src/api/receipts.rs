//! Receipt API endpoints for economic chain artifacts.
//!
//! Provides REST endpoints for querying AllocationReceipt and SettlementIntent
//! by canonical hash.

use actix_web::{get, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::receipt_store::ReceiptStore;
use icn_governance::GovernanceDecisionReceipt;
use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};

/// Query parameters for listing by decision hash.
///
/// `decision_hash` is optional — when absent, all receipts of that type are returned.
#[derive(Debug, Deserialize)]
pub struct ByDecisionQuery {
    /// Hex-encoded decision hash (optional — omit to list all)
    pub decision_hash: Option<String>,
}

/// Response for allocation receipt
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationReceiptResponse {
    /// Hex-encoded canonical hash
    pub canonical_hash: String,
    /// Hex-encoded decision hash
    pub decision_hash: String,
    /// Scope level
    pub scope: String,
    /// Creation timestamp
    pub created_at: u64,
    /// Number of intents
    pub intent_count: usize,
    /// Hex-encoded intent hashes
    pub intent_hashes: Vec<String>,
}

impl From<&AllocationReceipt> for AllocationReceiptResponse {
    fn from(r: &AllocationReceipt) -> Self {
        Self {
            canonical_hash: hex::encode(r.canonical_hash()),
            decision_hash: hex::encode(r.decision_hash),
            scope: format!("{:?}", r.scope),
            created_at: r.created_at,
            intent_count: r.intents.len(),
            intent_hashes: r.intent_hashes().iter().map(hex::encode).collect(),
        }
    }
}

/// Response for settlement intent
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementIntentResponse {
    /// Hex-encoded canonical hash
    pub canonical_hash: String,
    /// Decision receipt ID
    pub decision_receipt_id: String,
    /// Hex-encoded decision hash
    pub decision_hash: String,
    /// Asset type
    pub asset_type: String,
    /// Source account
    pub from: String,
    /// Destination account
    pub to: String,
    /// Amount
    pub amount: u64,
    /// Unit/currency
    pub unit: String,
    /// Scope level
    pub scope: String,
    /// Optional memo
    pub memo: Option<String>,
    /// Creation timestamp
    pub created_at: u64,
}

impl From<&SettlementIntent> for SettlementIntentResponse {
    fn from(i: &SettlementIntent) -> Self {
        Self {
            canonical_hash: hex::encode(i.canonical_hash()),
            decision_receipt_id: i.decision_receipt_id.clone(),
            decision_hash: hex::encode(i.decision_hash),
            asset_type: format!("{:?}", i.asset),
            from: i.from.clone(),
            to: i.to.clone(),
            amount: i.amount,
            unit: i.unit.clone(),
            scope: format!("{:?}", i.scope),
            memo: i.memo.clone(),
            created_at: i.created_at,
        }
    }
}

/// Response for economic chain query
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EconomicChainResponse {
    /// Hex-encoded decision hash
    pub decision_hash: String,
    /// Allocation receipts
    pub allocations: Vec<AllocationReceiptResponse>,
    /// Settlement intents
    pub intents: Vec<SettlementIntentResponse>,
}

/// Response for governance decision receipt
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceReceiptResponse {
    /// Hex-encoded decision hash (canonical)
    pub decision_hash: String,
    /// Proposal ID
    pub proposal_id: String,
    /// Governance domain ID
    pub domain_id: String,
    /// Final outcome
    pub outcome: String,
    /// Vote tally summary
    pub vote_tally: GovernanceVoteTallyResponse,
    /// Hex-encoded vote hash (Merkle root of sorted votes)
    pub vote_hash: String,
}

/// Serializable vote tally summary
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceVoteTallyResponse {
    /// Number of votes in favor
    pub for_votes: usize,
    /// Number of votes against
    pub against_votes: usize,
    /// Number of abstentions
    pub abstain_votes: usize,
}

impl From<&GovernanceDecisionReceipt> for GovernanceReceiptResponse {
    fn from(r: &GovernanceDecisionReceipt) -> Self {
        Self {
            decision_hash: hex::encode(r.decision_hash),
            proposal_id: r.proposal_id.clone(),
            domain_id: r.domain_id.clone(),
            outcome: format!("{:?}", r.outcome),
            vote_tally: GovernanceVoteTallyResponse {
                for_votes: r.vote_tally.for_votes,
                against_votes: r.vote_tally.against_votes,
                abstain_votes: r.vote_tally.abstain_votes,
            },
            vote_hash: hex::encode(r.vote_hash),
        }
    }
}

/// Response for the full receipt chain (governance + economic artifacts).
///
/// Returned by `GET /v1/receipts/chain/{decision_hash}`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptChainResponse {
    /// Hex-encoded decision hash
    pub decision_hash: String,
    /// Governance decision receipt (null if not yet stored)
    pub governance: Option<GovernanceReceiptResponse>,
    /// Allocation receipts for this decision
    pub allocations: Vec<AllocationReceiptResponse>,
    /// Settlement intents for this decision
    pub intents: Vec<SettlementIntentResponse>,
    /// Whether all expected chain links are present.
    ///
    /// True when at least a governance receipt exists AND every allocation has
    /// at least one intent.
    pub chain_complete: bool,
}

/// Parse hex hash or return error response
fn parse_hash(hex_str: &str) -> Result<Hash, HttpResponse> {
    let bytes = hex::decode(hex_str).map_err(|_| {
        HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid hex hash"
        }))
    })?;
    if bytes.len() != 32 {
        return Err(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Hash must be 32 bytes"
        })));
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(hash)
}

/// GET /v1/receipts/allocations/{hash}
#[get("/allocations/{hash}")]
pub async fn get_allocation(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    path: web::Path<String>,
) -> HttpResponse {
    let hash_hex = path.into_inner();
    let hash = match parse_hash(&hash_hex) {
        Ok(h) => h,
        Err(resp) => return resp,
    };

    match receipt_store.get_allocation(&hash) {
        Ok(Some(receipt)) => HttpResponse::Ok().json(AllocationReceiptResponse::from(&receipt)),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({
            "error": format!("Allocation receipt {} not found", hash_hex)
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        })),
    }
}

/// GET /v1/receipts/intents/{hash}
#[get("/intents/{hash}")]
pub async fn get_intent(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    path: web::Path<String>,
) -> HttpResponse {
    let hash_hex = path.into_inner();
    let hash = match parse_hash(&hash_hex) {
        Ok(h) => h,
        Err(resp) => return resp,
    };

    match receipt_store.get_intent(&hash) {
        Ok(Some(intent)) => HttpResponse::Ok().json(SettlementIntentResponse::from(&intent)),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({
            "error": format!("Settlement intent {} not found", hash_hex)
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        })),
    }
}

/// GET /v1/receipts/chain?decision_hash=<hex>
///
/// `decision_hash` is required for this endpoint — it returns the economic
/// chain (allocations + intents) for a specific governance decision.
#[get("/chain")]
pub async fn get_chain(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    query: web::Query<ByDecisionQuery>,
) -> HttpResponse {
    let hex = match &query.decision_hash {
        Some(h) => h,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "decision_hash is required for /receipts/chain"
            }))
        }
    };

    let hash = match parse_hash(hex) {
        Ok(h) => h,
        Err(resp) => return resp,
    };

    match receipt_store.get_chain_by_decision(&hash) {
        Ok((allocations, intents)) => {
            let response = EconomicChainResponse {
                decision_hash: hex.clone(),
                allocations: allocations
                    .iter()
                    .map(AllocationReceiptResponse::from)
                    .collect(),
                intents: intents.iter().map(SettlementIntentResponse::from).collect(),
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

/// GET /v1/receipts/chain/{decision_hash}
///
/// Returns the full receipt chain for a decision: governance receipt,
/// allocation receipts, and settlement intents.
#[get("/chain/{decision_hash}")]
pub async fn get_full_chain(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    path: web::Path<String>,
) -> HttpResponse {
    let hash_hex = path.into_inner();
    let hash = match parse_hash(&hash_hex) {
        Ok(h) => h,
        Err(resp) => return resp,
    };

    // Fetch governance receipt
    let governance = match receipt_store.list_governance_by_decision(&hash) {
        Ok(receipts) => receipts.into_iter().next(),
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": e
            }))
        }
    };

    // Fetch economic chain
    let (allocations, intents) = match receipt_store.get_chain_by_decision(&hash) {
        Ok(chain) => chain,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": e
            }))
        }
    };

    // Chain is complete when we have a governance receipt AND every allocation
    // has at least one intent (or there are no allocations but intents exist).
    let chain_complete = governance.is_some() && allocations.iter().all(|a| !a.intents.is_empty());

    let response = ReceiptChainResponse {
        decision_hash: hash_hex,
        governance: governance.as_ref().map(GovernanceReceiptResponse::from),
        allocations: allocations
            .iter()
            .map(AllocationReceiptResponse::from)
            .collect(),
        intents: intents.iter().map(SettlementIntentResponse::from).collect(),
        chain_complete,
    };
    HttpResponse::Ok().json(response)
}

/// GET /v1/receipts/allocations[?decision_hash=<hex>]
///
/// Without `decision_hash`: returns all allocation receipts.
/// With `decision_hash`: returns only receipts linked to that decision.
#[get("/allocations")]
pub async fn list_allocations(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    query: web::Query<ByDecisionQuery>,
) -> HttpResponse {
    let result = match &query.decision_hash {
        None => receipt_store.list_all_allocations(),
        Some(hex) => match parse_hash(hex) {
            Ok(hash) => receipt_store.list_allocations_by_decision(&hash),
            Err(resp) => return resp,
        },
    };

    match result {
        Ok(allocations) => {
            let responses: Vec<_> = allocations
                .iter()
                .map(AllocationReceiptResponse::from)
                .collect();
            HttpResponse::Ok().json(responses)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

/// GET /v1/receipts/intents[?decision_hash=<hex>]
///
/// Without `decision_hash`: returns all settlement intents.
/// With `decision_hash`: returns only intents linked to that decision.
#[get("/intents")]
pub async fn list_intents(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    query: web::Query<ByDecisionQuery>,
) -> HttpResponse {
    let result = match &query.decision_hash {
        None => receipt_store.list_all_intents(),
        Some(hex) => match parse_hash(hex) {
            Ok(hash) => receipt_store.list_intents_by_decision(&hash),
            Err(resp) => return resp,
        },
    };

    match result {
        Ok(intents) => {
            let responses: Vec<_> = intents.iter().map(SettlementIntentResponse::from).collect();
            HttpResponse::Ok().json(responses)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({ "error": e })),
    }
}

/// Configure receipt routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_allocation)
        .service(get_intent)
        .service(get_full_chain)
        .service(get_chain)
        .service(list_allocations)
        .service(list_intents);
}
