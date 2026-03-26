//! Receipt API endpoints for economic chain artifacts.
//!
//! Provides REST endpoints for querying AllocationReceipt and SettlementIntent
//! by canonical hash.

use actix_web::{get, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::receipt_store::ReceiptStore;
use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus};
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};
use icn_store::Store;

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
    /// Structural chain completeness (receipt-link integrity only).
    pub structural_complete: bool,
    /// Execution completeness for milestone scope.
    pub execution_complete: bool,
    /// Overall completeness: structural + execution truth.
    pub chain_complete: bool,
    /// Canonical execution-truth counters for the decision.
    pub execution: DecisionExecutionTruthResponse,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionExecutionTruthResponse {
    pub execution_record_present: bool,
    pub status: Option<ExecutionStatus>,
    pub total_effects: usize,
    pub executed_effects: usize,
    pub not_executed_effects: usize,
    pub hard_failed_effects: usize,
    pub execution_complete: bool,
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

fn key_for_decision_hash(decision_hash: &str) -> Vec<u8> {
    let mut key = b"exec:".to_vec();
    key.extend_from_slice(decision_hash.as_bytes());
    key
}

fn load_execution_truth(
    execution_store: &dyn Store,
    decision_hash: &str,
) -> Result<DecisionExecutionTruthResponse, String> {
    let value = execution_store
        .get(&key_for_decision_hash(decision_hash))
        .map_err(|e| format!("failed to read execution record: {e}"))?;

    let Some(value) = value else {
        return Ok(DecisionExecutionTruthResponse {
            execution_record_present: false,
            status: None,
            total_effects: 0,
            executed_effects: 0,
            not_executed_effects: 0,
            hard_failed_effects: 0,
            execution_complete: false,
        });
    };

    let record: ExecutionRecord = serde_json::from_slice(&value)
        .map_err(|e| format!("failed to decode execution record: {e}"))?;
    let summary = record.truth_summary_or_fallback();
    Ok(DecisionExecutionTruthResponse {
        execution_record_present: true,
        status: Some(record.status),
        total_effects: summary.total_effects,
        executed_effects: summary.executed_effects,
        not_executed_effects: summary.not_executed_effects,
        hard_failed_effects: summary.hard_failed_effects,
        execution_complete: summary.execution_complete,
    })
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
    execution_store: web::Data<Arc<dyn Store>>,
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

    let normalized_decision_hash = hex::encode(hash);
    let execution = match load_execution_truth(
        execution_store.get_ref().as_ref(),
        &normalized_decision_hash,
    ) {
        Ok(execution) => execution,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": e
            }))
        }
    };

    let structural_complete =
        governance.is_some() && allocations.iter().all(|a| !a.intents.is_empty());
    let execution_complete = execution.execution_complete;
    let chain_complete = structural_complete && execution_complete;

    let response = ReceiptChainResponse {
        decision_hash: hash_hex,
        governance: governance.as_ref().map(|r| GovernanceReceiptResponse {
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
        }),
        allocations: allocations
            .iter()
            .map(AllocationReceiptResponse::from)
            .collect(),
        intents: intents.iter().map(SettlementIntentResponse::from).collect(),
        structural_complete,
        execution_complete,
        chain_complete,
        execution,
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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use icn_kernel_api::execution::{ExecutionRecord, ExecutionTruthSummary};
    use icn_kernel_api::receipts::AllocationReceipt;
    use icn_kernel_api::ScopeLevel;
    use icn_store::{SledStore, Store};

    #[actix_web::test]
    async fn partial_execution_chain_is_not_complete() {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .expect("temp receipt db");
        let receipt_store = Arc::new(ReceiptStore::new(db));
        let execution_store: Arc<dyn Store> =
            Arc::new(SledStore::temporary().expect("temp exec db"));

        let decision_hash = receipt_store
            .put_test_governance_receipt("prop-partial")
            .expect("governance");

        let intent = SettlementIntent::new("r1", decision_hash, "a", "b", 10, "HOURS");
        let allocation = AllocationReceipt::new(decision_hash, ScopeLevel::Org).add_intent(intent);
        receipt_store
            .put_allocation_with_intents(&allocation)
            .expect("allocation");

        let mut record =
            ExecutionRecord::new_pending(hex::encode(decision_hash), "prop-partial", "r1", vec![]);
        record.mark_confirmed(vec!["ledger-1".to_string()], vec![]);
        record.truth_summary = Some(ExecutionTruthSummary {
            total_effects: 1,
            executed_effects: 0,
            not_executed_effects: 1,
            hard_failed_effects: 0,
            execution_complete: false,
        });
        execution_store
            .put(
                format!("exec:{}", hex::encode(decision_hash)).as_bytes(),
                &serde_json::to_vec(&record).expect("serialize record"),
            )
            .expect("put execution");

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(receipt_store.clone()))
                .app_data(web::Data::new(execution_store.clone()))
                .service(web::scope("/v1/receipts").service(get_full_chain)),
        )
        .await;

        let uri = format!("/v1/receipts/chain/{}", hex::encode(decision_hash));
        let req = test::TestRequest::get().uri(&uri).to_request();
        let resp: ReceiptChainResponse = test::call_and_read_body_json(&app, req).await;

        assert!(resp.structural_complete);
        assert!(!resp.execution_complete);
        assert!(!resp.chain_complete);
        assert_eq!(resp.execution.not_executed_effects, 1);
    }

    #[actix_web::test]
    async fn full_execution_chain_is_complete() {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .expect("temp receipt db");
        let receipt_store = Arc::new(ReceiptStore::new(db));
        let execution_store: Arc<dyn Store> =
            Arc::new(SledStore::temporary().expect("temp exec db"));

        let decision_hash = receipt_store
            .put_test_governance_receipt("prop-full")
            .expect("governance");

        let intent = SettlementIntent::new("r2", decision_hash, "a", "b", 10, "HOURS");
        let allocation = AllocationReceipt::new(decision_hash, ScopeLevel::Org).add_intent(intent);
        receipt_store
            .put_allocation_with_intents(&allocation)
            .expect("allocation");

        let mut record =
            ExecutionRecord::new_pending(hex::encode(decision_hash), "prop-full", "r2", vec![]);
        record.mark_confirmed(vec!["ledger-1".to_string()], vec![]);
        record.truth_summary = Some(ExecutionTruthSummary {
            total_effects: 1,
            executed_effects: 1,
            not_executed_effects: 0,
            hard_failed_effects: 0,
            execution_complete: true,
        });
        execution_store
            .put(
                format!("exec:{}", hex::encode(decision_hash)).as_bytes(),
                &serde_json::to_vec(&record).expect("serialize record"),
            )
            .expect("put execution");

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(receipt_store.clone()))
                .app_data(web::Data::new(execution_store.clone()))
                .service(web::scope("/v1/receipts").service(get_full_chain)),
        )
        .await;

        let uri = format!("/v1/receipts/chain/{}", hex::encode(decision_hash));
        let req = test::TestRequest::get().uri(&uri).to_request();
        let resp: ReceiptChainResponse = test::call_and_read_body_json(&app, req).await;

        assert!(resp.structural_complete);
        assert!(resp.execution_complete);
        assert!(resp.chain_complete);
        assert_eq!(resp.execution.executed_effects, 1);
    }
}
