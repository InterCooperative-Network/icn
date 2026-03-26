//! Execution record query API endpoints.

use actix_web::{get, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{GatewayError, Result};
use crate::middleware::require_scope;
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus};
use icn_store::Store;

#[derive(Debug, Deserialize)]
pub struct ExecutionQuery {
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExecutionRecordResponse {
    pub decision_hash: String,
    pub proposal_id: String,
    pub status: ExecutionStatus,
    pub total_effects: usize,
    pub executed_effects: usize,
    pub not_executed_effects: usize,
    pub hard_failed_effects: usize,
    pub execution_complete: bool,
    pub retries: u32,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub error: Option<String>,
}

impl From<ExecutionRecord> for ExecutionRecordResponse {
    fn from(record: ExecutionRecord) -> Self {
        let summary = record.truth_summary_or_fallback();
        Self {
            decision_hash: record.decision_hash,
            proposal_id: record.proposal_id,
            status: record.status,
            total_effects: summary.total_effects,
            executed_effects: summary.executed_effects,
            not_executed_effects: summary.not_executed_effects,
            hard_failed_effects: summary.hard_failed_effects,
            execution_complete: summary.execution_complete,
            retries: record.retries,
            started_at: record.started_at,
            finished_at: record.finished_at,
            error: record.error,
        }
    }
}

fn key_for_decision_hash(decision_hash: &str) -> Vec<u8> {
    let mut key = b"exec:".to_vec();
    key.extend_from_slice(decision_hash.as_bytes());
    key
}

fn parse_status(raw: &str) -> Result<ExecutionStatus> {
    serde_json::from_str::<ExecutionStatus>(&format!("\"{raw}\""))
        .map_err(|_| GatewayError::BadRequest(format!("invalid execution status: {raw}")))
}

fn list_execution_records(
    store: &dyn Store,
    status: Option<ExecutionStatus>,
) -> Result<Vec<ExecutionRecordResponse>> {
    let mut records: Vec<ExecutionRecordResponse> = Vec::new();

    for (_key, value) in store.scan(b"exec:").map_err(|e| {
        GatewayError::InternalError(format!("failed to scan execution records: {e}"))
    })? {
        let record: ExecutionRecord = match serde_json::from_slice(&value) {
            Ok(record) => record,
            Err(e) => {
                tracing::warn!(error = %e, "Skipping malformed execution record");
                continue;
            }
        };

        if let Some(filter_status) = status {
            if record.status != filter_status {
                continue;
            }
        }

        records.push(record.into());
    }

    // Most recent first for operator triage.
    records.sort_by_key(|r| std::cmp::Reverse(r.started_at));

    Ok(records)
}

fn get_execution_record(
    store: &dyn Store,
    decision_hash: &str,
) -> Result<Option<ExecutionRecordResponse>> {
    let value = store
        .get(&key_for_decision_hash(decision_hash))
        .map_err(|e| {
            GatewayError::InternalError(format!("failed to read execution record: {e}"))
        })?;

    let Some(value) = value else {
        return Ok(None);
    };

    let record: ExecutionRecord = serde_json::from_slice(&value).map_err(|e| {
        GatewayError::InternalError(format!("failed to decode execution record: {e}"))
    })?;

    Ok(Some(record.into()))
}

/// GET /execution/records?status=<status>
#[get("/records")]
pub async fn list_records(
    req: HttpRequest,
    store: web::Data<Arc<dyn Store>>,
    query: web::Query<ExecutionQuery>,
) -> Result<HttpResponse> {
    require_scope(&req, "governance:read")?;

    let status = match query.status.as_deref() {
        Some(raw) => Some(parse_status(raw)?),
        None => None,
    };

    let records = list_execution_records(store.get_ref().as_ref(), status)?;
    Ok(HttpResponse::Ok().json(records))
}

/// GET /execution/records/{decision_hash}
#[get("/records/{decision_hash}")]
pub async fn get_record(
    req: HttpRequest,
    store: web::Data<Arc<dyn Store>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&req, "governance:read")?;

    let decision_hash = path.into_inner();
    match get_execution_record(store.get_ref().as_ref(), &decision_hash)? {
        Some(record) => Ok(HttpResponse::Ok().json(record)),
        None => Err(GatewayError::NotFound(format!(
            "execution record not found for decision_hash {decision_hash}"
        ))),
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_records).service(get_record);
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use icn_kernel_api::execution::ExecutionTruthSummary;

    fn temp_store() -> Arc<dyn Store> {
        Arc::new(icn_store::SledStore::temporary().expect("temp store"))
    }

    fn put_record(store: &dyn Store, record: &ExecutionRecord) -> Result<()> {
        let value = serde_json::to_vec(record)?;
        store.put(&key_for_decision_hash(&record.decision_hash), &value)?;
        Ok(())
    }

    #[test]
    fn parse_status_accepts_known_values() {
        assert!(matches!(
            parse_status("pending"),
            Ok(ExecutionStatus::Pending)
        ));
        assert!(matches!(
            parse_status("permanently_failed"),
            Ok(ExecutionStatus::PermanentlyFailed)
        ));
    }

    #[test]
    fn parse_status_rejects_unknown_values() {
        assert!(parse_status("unknown").is_err());
    }

    #[test]
    fn list_records_filters_by_status() {
        let store = temp_store();

        let pending = ExecutionRecord::new_pending("h1", "p1", "r1", vec![]);
        let mut confirmed = ExecutionRecord::new_pending("h2", "p2", "r2", vec![]);
        confirmed.mark_confirmed(vec![], vec![]);

        put_record(store.as_ref(), &pending).expect("put pending");
        put_record(store.as_ref(), &confirmed).expect("put confirmed");

        let pending_records =
            list_execution_records(store.as_ref(), Some(ExecutionStatus::Pending)).expect("list");
        assert_eq!(pending_records.len(), 1);
        assert_eq!(pending_records[0].decision_hash, "h1");

        let confirmed_records =
            list_execution_records(store.as_ref(), Some(ExecutionStatus::Confirmed))
                .expect("list confirmed");
        assert_eq!(confirmed_records.len(), 1);
        assert_eq!(confirmed_records[0].decision_hash, "h2");
    }

    #[test]
    fn get_record_returns_none_for_missing_hash() {
        let store = temp_store();
        let result = get_execution_record(store.as_ref(), "missing").expect("query");
        assert!(result.is_none());
    }

    #[test]
    fn execution_response_exposes_truth_counters() {
        let store = temp_store();
        let mut record = ExecutionRecord::new_pending("h3", "p3", "r3", vec![]);
        record.mark_confirmed(vec!["entry-1".to_string()], vec![]);
        record.truth_summary = Some(ExecutionTruthSummary {
            total_effects: 2,
            executed_effects: 1,
            not_executed_effects: 1,
            hard_failed_effects: 0,
            execution_complete: false,
        });
        put_record(store.as_ref(), &record).expect("put record");

        let got = get_execution_record(store.as_ref(), "h3")
            .expect("query")
            .expect("record exists");
        assert_eq!(got.total_effects, 2);
        assert_eq!(got.executed_effects, 1);
        assert_eq!(got.not_executed_effects, 1);
        assert!(!got.execution_complete);
    }
}
