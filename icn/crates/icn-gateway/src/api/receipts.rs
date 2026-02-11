//! Receipt API endpoints for economic chain artifacts.
//!
//! Provides REST endpoints for querying AllocationReceipt and SettlementIntent
//! by canonical hash.

use actix_web::{get, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::receipt_store::ReceiptStore;
use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};

/// Query parameters for listing by decision hash
#[derive(Debug, Deserialize)]
pub struct ByDecisionQuery {
    /// Hex-encoded decision hash
    pub decision_hash: String,
}

/// Response for allocation receipt
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EconomicChainResponse {
    /// Hex-encoded decision hash
    pub decision_hash: String,
    /// Allocation receipts
    pub allocations: Vec<AllocationReceiptResponse>,
    /// Settlement intents
    pub intents: Vec<SettlementIntentResponse>,
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

/// GET /v1/receipts/chain?decision_hash=...
#[get("/chain")]
pub async fn get_chain(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    query: web::Query<ByDecisionQuery>,
) -> HttpResponse {
    let hash = match parse_hash(&query.decision_hash) {
        Ok(h) => h,
        Err(resp) => return resp,
    };

    match receipt_store.get_chain_by_decision(&hash) {
        Ok((allocations, intents)) => {
            let response = EconomicChainResponse {
                decision_hash: query.decision_hash.clone(),
                allocations: allocations
                    .iter()
                    .map(AllocationReceiptResponse::from)
                    .collect(),
                intents: intents.iter().map(SettlementIntentResponse::from).collect(),
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        })),
    }
}

/// GET /v1/receipts/allocations?decision_hash=...
#[get("/allocations")]
pub async fn list_allocations(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    query: web::Query<ByDecisionQuery>,
) -> HttpResponse {
    let hash = match parse_hash(&query.decision_hash) {
        Ok(h) => h,
        Err(resp) => return resp,
    };

    match receipt_store.list_allocations_by_decision(&hash) {
        Ok(allocations) => {
            let responses: Vec<_> = allocations
                .iter()
                .map(AllocationReceiptResponse::from)
                .collect();
            HttpResponse::Ok().json(responses)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        })),
    }
}

/// GET /v1/receipts/intents?decision_hash=...
#[get("/intents")]
pub async fn list_intents(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    query: web::Query<ByDecisionQuery>,
) -> HttpResponse {
    let hash = match parse_hash(&query.decision_hash) {
        Ok(h) => h,
        Err(resp) => return resp,
    };

    match receipt_store.list_intents_by_decision(&hash) {
        Ok(intents) => {
            let responses: Vec<_> = intents.iter().map(SettlementIntentResponse::from).collect();
            HttpResponse::Ok().json(responses)
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        })),
    }
}

/// Configure receipt routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_allocation)
        .service(get_intent)
        .service(get_chain)
        .service(list_allocations)
        .service(list_intents);
}
