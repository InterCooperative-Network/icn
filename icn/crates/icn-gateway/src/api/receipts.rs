//! Receipt API endpoints for economic chain artifacts.
//!
//! Provides REST endpoints for querying AllocationReceipt and SettlementIntent
//! by canonical hash.

use actix_web::{get, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::ledger_mgr::LedgerManager;
use crate::receipt_store::ReceiptStore;
use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus};
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};
use icn_ledger::types::ProvenanceRef;
use icn_store::Store;

/// Query parameters for listing by decision hash.
///
/// `decision_hash` is optional — when absent, all receipts of that type are returned.
#[derive(Debug, Deserialize)]
pub struct ByDecisionQuery {
    /// Hex-encoded decision hash (optional — omit to list all)
    pub decision_hash: Option<String>,
}

/// Query parameters for `GET /v1/receipts/chain/{decision_hash}`.
#[derive(Debug, Deserialize, Default)]
pub struct ChainQuery {
    /// Optional cooperative/domain ID for forced-accept chains where no governance
    /// receipt is stored in the backend.
    ///
    /// For normal voted decisions, `domain_id` is derived from the stored governance
    /// receipt and this parameter is ignored.
    ///
    /// For forced-accept decisions (no stored receipt), providing this parameter
    /// enables journal cross-reference. Without it, `journal_entries` is empty.
    ///
    /// The caller always knows the domain_id (it is part of the proposal record).
    /// This parameter does not imply that a governance receipt exists.
    pub domain_id: Option<String>,
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

/// Minimal journal entry provenance reference for audit cross-check.
///
/// Carries only the fields needed to verify that a ledger journal entry
/// was authorized by the originating governance decision: the receipt_id
/// and the decision_hash stored in `ProvenanceRef::Governance`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntryProvenanceResponse {
    /// Ledger-assigned entry ID (blake3 hex or empty if not yet assigned)
    pub entry_id: String,
    /// Governance receipt ID stored in the journal entry's provenance
    pub decision_receipt_id: String,
    /// Decision hash stored in the journal entry's provenance
    pub decision_hash: String,
    /// Number of account deltas in this entry
    pub account_count: usize,
    /// Entry timestamp (Unix seconds)
    pub timestamp: u64,
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
    /// Journal entries whose ProvenanceRef::Governance.decision_hash matches.
    /// Sourced from the shared kernel-ledger service (where governed treasury
    /// entries are written), falling back to the per-domain LedgerManager when
    /// the kernel service is not wired and governance.domain_id resolves a ledger.
    pub journal_entries: Vec<JournalEntryProvenanceResponse>,
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

/// GET /v1/receipts/chain/{decision_hash}[?domain_id=<coop-id>]
///
/// Returns the full receipt chain for a decision: governance receipt,
/// allocation receipts, settlement intents, and journal entry provenance refs.
///
/// Journal entries come from the shared kernel-ledger service when it is wired
/// (the authoritative store for governed treasury entries), needing no domain_id.
/// When that service is absent, the endpoint falls back to the per-domain
/// `LedgerManager`: for normal voted decisions the domain_id comes from the stored
/// governance receipt; for forced-accept decisions (no stored governance receipt)
/// the caller may supply `?domain_id=<id>` as a hint, and without it (and without
/// the kernel service) `journal_entries` remains empty.
///
/// Providing `domain_id` never implies a governance receipt is present — the
/// `governance` field reflects actual receipt-store state.
#[get("/chain/{decision_hash}")]
pub async fn get_full_chain(
    receipt_store: web::Data<Arc<ReceiptStore>>,
    execution_store: web::Data<Arc<dyn Store>>,
    ledger_mgr: Option<web::Data<Arc<LedgerManager>>>,
    ledger_service: Option<web::Data<Option<Arc<icn_api::LedgerService>>>>,
    path: web::Path<String>,
    query: web::Query<ChainQuery>,
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

    // Query journal entries whose ProvenanceRef::Governance.decision_hash matches.
    //
    // Primary source: the shared kernel-ledger service. The decision executor
    // writes governed treasury entries into the kernel ledger (the same handle
    // this service wraps), so it is authoritative for the
    // governance -> treasury -> journal chain and needs no domain_id hint.
    //
    // Fallback: the per-domain LedgerManager, used for forced-accept and test
    // contexts where the kernel ledger service is not wired. domain_id is
    // derived from the governance receipt when present; for forced-accept
    // decisions (no stored receipt) the caller may supply it as a query param.
    let mut journal_entries =
        query_journal_entries_via_decision_service(&ledger_service, &normalized_decision_hash)
            .await;
    if journal_entries.is_empty() {
        let receipt_domain_id = governance.as_ref().map(|g| g.domain_id.as_str());
        journal_entries = query_journal_entries_for_decision(
            &ledger_mgr,
            receipt_domain_id.or(query.domain_id.as_deref()),
            &normalized_decision_hash,
        )
        .await;
    }

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
        journal_entries,
        structural_complete,
        execution_complete,
        chain_complete,
        execution,
    };
    HttpResponse::Ok().json(response)
}

/// Query ledger journal entries whose `ProvenanceRef::Governance.decision_hash` matches
/// the supplied hex-encoded decision hash.
///
/// `domain_id` is the resolved cooperative domain — governance receipt domain_id
/// takes precedence over any query parameter hint; callers resolve this before calling.
/// Returns an empty vec when no domain_id is available or LedgerManager is unavailable.
async fn query_journal_entries_for_decision(
    ledger_mgr: &Option<web::Data<Arc<LedgerManager>>>,
    domain_id: Option<&str>,
    decision_hash_hex: &str,
) -> Vec<JournalEntryProvenanceResponse> {
    let domain_id = domain_id.map(str::to_owned);

    let (mgr, domain_id) = match (ledger_mgr, domain_id) {
        (Some(m), Some(d)) => (m, d),
        _ => return Vec::new(),
    };

    let entries = match mgr.get_history(&domain_id, None, 0, 100).await {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    entries
        .into_iter()
        .filter_map(|entry| match &entry.provenance {
            ProvenanceRef::Governance {
                receipt_id,
                decision_hash: dh,
            } if dh.as_str() == decision_hash_hex => Some(JournalEntryProvenanceResponse {
                entry_id: entry.id.map(|h| h.to_hex()).unwrap_or_default(),
                decision_receipt_id: receipt_id.clone(),
                decision_hash: dh.clone(),
                account_count: entry.accounts.len(),
                timestamp: entry.timestamp,
            }),
            _ => None,
        })
        .collect()
}

/// Query journal entries authorized by `decision_hash_hex` via the shared
/// kernel-ledger service.
///
/// This is the authoritative source for governed economic chains: the decision
/// executor submits treasury entries into the kernel ledger that this service
/// wraps. Matching is by `ProvenanceRef::Governance.decision_hash` (performed
/// inside the service), so no domain_id hint is required.
///
/// Returns an empty vec when the service is not wired (e.g. unit tests without a
/// daemon ledger) or when the lookup fails, letting the caller fall back to the
/// per-domain LedgerManager.
async fn query_journal_entries_via_decision_service(
    ledger_service: &Option<web::Data<Option<Arc<icn_api::LedgerService>>>>,
    decision_hash_hex: &str,
) -> Vec<JournalEntryProvenanceResponse> {
    let svc = match ledger_service.as_ref().and_then(|d| d.get_ref().as_ref()) {
        Some(svc) => svc,
        None => return Vec::new(),
    };

    // Bounded read; the chain audit only needs the entries for this decision.
    let page = match svc.get_entries_by_decision(decision_hash_hex, 100).await {
        Ok(page) => page,
        Err(_) => return Vec::new(),
    };

    page.entries
        .into_iter()
        .map(|view| JournalEntryProvenanceResponse {
            entry_id: view.id,
            decision_receipt_id: view.decision_receipt_id.unwrap_or_default(),
            decision_hash: view.decision_hash.unwrap_or_default(),
            account_count: view.accounts.len(),
            timestamp: view.timestamp,
        })
        .collect()
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

    // --- domain_id query param tests (forced-accept read path) ---

    /// Helper: build a minimal actix App with the given stores and optional LedgerManager.
    async fn build_chain_app(
        receipt_store: Arc<ReceiptStore>,
        execution_store: Arc<dyn Store>,
        ledger_mgr: Option<Arc<LedgerManager>>,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        let mut app = App::new()
            .app_data(web::Data::new(receipt_store))
            .app_data(web::Data::new(execution_store))
            .service(web::scope("/v1/receipts").service(get_full_chain));

        if let Some(mgr) = ledger_mgr {
            app = app.app_data(web::Data::new(mgr));
        }

        test::init_service(app).await
    }

    /// Forced-accept chain without domain_id hint: journal_entries stays empty.
    /// governance is None (no stored receipt), no domain_id query param.
    #[actix_web::test]
    async fn forced_accept_without_domain_id_hint_returns_empty_journal() {
        let db = sled::Config::new().temporary(true).open().expect("temp db");
        let receipt_store = Arc::new(ReceiptStore::new(db));
        let execution_store: Arc<dyn Store> =
            Arc::new(SledStore::temporary().expect("temp exec db"));

        // Use a well-formed 32-byte hash for a forced-accept with no stored receipt.
        let decision_hash_hex = "a1".repeat(32);
        let decision_hash_bytes = hex::decode(&decision_hash_hex).expect("valid hex");
        let mut hash_arr = [0u8; 32];
        hash_arr.copy_from_slice(&decision_hash_bytes);

        // Register a LedgerManager with an entry in "test-domain" carrying this hash.
        let mgr = Arc::new(LedgerManager::new());
        {
            use icn_identity::KeyPair;
            use icn_ledger::entry::JournalEntryBuilder;
            let ledger_arc = mgr
                .get_ledger(&"test-domain".to_string())
                .await
                .expect("get ledger");
            let author = KeyPair::generate().expect("keypair").did().clone();
            let entry = JournalEntryBuilder::new(author.clone())
                .debit(author.clone(), "credits".to_string(), 10)
                .credit(
                    KeyPair::generate().expect("keypair").did().clone(),
                    "credits".to_string(),
                    10,
                )
                .with_governance_provenance(
                    "gov:test-domain:forced-001:receipt",
                    &decision_hash_hex,
                )
                .build()
                .expect("entry");
            ledger_arc
                .write()
                .await
                .append_entry(entry)
                .await
                .expect("append");
        }

        // No ?domain_id= query param supplied.
        let app = build_chain_app(receipt_store, execution_store, Some(mgr)).await;
        let uri = format!("/v1/receipts/chain/{decision_hash_hex}");
        let req = test::TestRequest::get().uri(&uri).to_request();
        let resp: ReceiptChainResponse = test::call_and_read_body_json(&app, req).await;

        assert!(
            resp.governance.is_none(),
            "forced accept must not invent a governance receipt"
        );
        assert!(
            resp.journal_entries.is_empty(),
            "without domain_id hint, journal cross-reference must return empty"
        );
    }

    /// Forced-accept chain with correct domain_id hint: journal_entries populated.
    /// governance is None, but ?domain_id=test-domain resolves the ledger.
    #[actix_web::test]
    async fn forced_accept_with_domain_id_hint_finds_journal_entries() {
        let db = sled::Config::new().temporary(true).open().expect("temp db");
        let receipt_store = Arc::new(ReceiptStore::new(db));
        let execution_store: Arc<dyn Store> =
            Arc::new(SledStore::temporary().expect("temp exec db"));

        let decision_hash_hex = "b2".repeat(32);
        let mgr = Arc::new(LedgerManager::new());
        {
            use icn_identity::KeyPair;
            use icn_ledger::entry::JournalEntryBuilder;
            let ledger_arc = mgr
                .get_ledger(&"test-domain".to_string())
                .await
                .expect("get ledger");
            let author = KeyPair::generate().expect("keypair").did().clone();
            let entry = JournalEntryBuilder::new(author.clone())
                .debit(author.clone(), "credits".to_string(), 5)
                .credit(
                    KeyPair::generate().expect("keypair").did().clone(),
                    "credits".to_string(),
                    5,
                )
                .with_governance_provenance(
                    "gov:test-domain:forced-002:receipt",
                    &decision_hash_hex,
                )
                .build()
                .expect("entry");
            ledger_arc
                .write()
                .await
                .append_entry(entry)
                .await
                .expect("append");
        }

        let app = build_chain_app(receipt_store, execution_store, Some(mgr)).await;
        // Provide the correct domain_id hint.
        let uri = format!("/v1/receipts/chain/{decision_hash_hex}?domain_id=test-domain");
        let req = test::TestRequest::get().uri(&uri).to_request();
        let resp: ReceiptChainResponse = test::call_and_read_body_json(&app, req).await;

        assert!(
            resp.governance.is_none(),
            "domain_id hint must not invent a governance receipt"
        );
        assert_eq!(
            resp.journal_entries.len(),
            1,
            "correct domain_id hint must surface the journal entry"
        );
        assert_eq!(resp.journal_entries[0].decision_hash, decision_hash_hex);
    }

    /// Forced-accept chain with wrong domain_id hint: journal_entries remains empty.
    /// No match should be returned for an unrelated domain.
    #[actix_web::test]
    async fn wrong_domain_id_hint_returns_empty_journal() {
        let db = sled::Config::new().temporary(true).open().expect("temp db");
        let receipt_store = Arc::new(ReceiptStore::new(db));
        let execution_store: Arc<dyn Store> =
            Arc::new(SledStore::temporary().expect("temp exec db"));

        let decision_hash_hex = "c3".repeat(32);
        let mgr = Arc::new(LedgerManager::new());
        {
            use icn_identity::KeyPair;
            use icn_ledger::entry::JournalEntryBuilder;
            let ledger_arc = mgr
                .get_ledger(&"correct-domain".to_string())
                .await
                .expect("get ledger");
            let author = KeyPair::generate().expect("keypair").did().clone();
            let entry = JournalEntryBuilder::new(author.clone())
                .debit(author.clone(), "credits".to_string(), 3)
                .credit(
                    KeyPair::generate().expect("keypair").did().clone(),
                    "credits".to_string(),
                    3,
                )
                .with_governance_provenance(
                    "gov:correct-domain:forced-003:receipt",
                    &decision_hash_hex,
                )
                .build()
                .expect("entry");
            ledger_arc
                .write()
                .await
                .append_entry(entry)
                .await
                .expect("append");
        }

        // Provide the WRONG domain_id hint — entry lives in "correct-domain".
        let app = build_chain_app(receipt_store, execution_store, Some(mgr)).await;
        let uri = format!("/v1/receipts/chain/{decision_hash_hex}?domain_id=wrong-domain");
        let req = test::TestRequest::get().uri(&uri).to_request();
        let resp: ReceiptChainResponse = test::call_and_read_body_json(&app, req).await;

        assert!(
            resp.journal_entries.is_empty(),
            "wrong domain_id hint must not produce false matches"
        );
    }
}
