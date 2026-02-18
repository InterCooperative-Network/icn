//! Ledger API endpoints

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;
use tracing::info;

use crate::api::budgets::BudgetStore;
use crate::error::{GatewayError, Result};
use crate::ledger_mgr::LedgerManager;
use crate::middleware::{get_claims, require_coop_access, require_scope};
use crate::models::{
    AccountDeltaResponse, BalanceResponse, CreateCrossPaymentRequest, CreatePaymentRequest,
    CrossPaymentQuoteRequest, PaginationInfo, TransactionHistoryEntry,
};
use crate::rate_limit::VelocityLimiter;
use crate::trust_mgr::TrustManager;
use crate::validation;
use icn_obs::metrics::gateway;

/// GET /ledger/:coop_id/balance/:did - Get account balance
#[get("/{coop_id}/balance/{did}")]
pub async fn get_balance(
    req: HttpRequest,
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&req, "ledger:read")?;

    let (coop_id, did_str) = path.into_inner();
    require_coop_access(&req, &coop_id)?; // CRITICAL: Prevent cross-coop privacy leaks

    let did = did_str
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let balances = ledger_mgr.get_all_balances(&coop_id, &did).await?;

    // Track balance query
    gateway::balance_queries_inc();

    // TODO: Retrieve actual credit limits from CreditPolicy when available
    // For now, return None to maintain backward compatibility
    let response = BalanceResponse {
        did: did_str,
        balances,
        credit_limits: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /ledger/:coop_id/payment - Create a payment
#[post("/{coop_id}/payment")]
pub async fn create_payment(
    http_req: HttpRequest,
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    budget_store: web::Data<BudgetStore>,
    velocity_limiter: web::Data<VelocityLimiter>,
    trust_mgr: web::Data<Arc<TrustManager>>,
    notification_service: web::Data<Arc<crate::notifications::NotificationService>>,
    event_broadcaster: web::Data<Arc<crate::events::EventBroadcaster>>,
    coop_id: web::Path<String>,
    req: web::Json<CreatePaymentRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "ledger:write")?;
    require_coop_access(&http_req, &coop_id)?; // CRITICAL: Prevent cross-coop payment creation

    // CRITICAL: Verify authenticated DID matches payment sender
    // Prevents users from creating payments from other accounts
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    if claims.sub != req.from {
        return Err(crate::error::GatewayError::AuthorizationFailed(
            format!("Cannot create payments from other accounts (authenticated as {}, attempted to send from {})",
                claims.sub, req.from)
        ));
    }

    let from = req
        .from
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid from DID: {e}")))?;

    let to = req
        .to
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid to DID: {e}")))?;

    // Prevent self-payments (spam/bloat prevention)
    if from == to {
        return Err(crate::error::GatewayError::BadRequest(
            "Self-payments not allowed (sender and recipient cannot be the same)".to_string(),
        ));
    }

    // Validate input fields
    validation::validate_payment_amount(req.amount)?;
    validation::validate_currency(&req.currency)?;
    validation::validate_memo(&req.memo)?;

    // Check budget limits before creating payment
    let can_spend = budget_store
        .check_spending(&req.from, req.amount)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;
    if !can_spend {
        return Err(crate::error::GatewayError::BadRequest(
            "Payment exceeds budget limit".to_string(),
        ));
    }

    // Check transaction velocity limit (Issue #164)
    // Look up trust score from trust graph for rate limiting
    let trust_score = trust_mgr.compute_trust_score_for_velocity(&from).await;
    velocity_limiter.check_and_record(&req.from, trust_score)?;

    let hash = ledger_mgr
        .create_payment(&coop_id, &from, &to, req.amount, req.currency.clone())
        .await?;

    // Record spending in budget tracker
    let notified_budgets = budget_store
        .record_spending(&req.from, req.amount)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;
    if !notified_budgets.is_empty() {
        info!(
            budgets = ?notified_budgets,
            "Budget threshold notifications triggered"
        );
    }

    // Track payment creation metrics
    gateway::payments_created_inc();
    gateway::payment_amount_record(&req.currency, req.amount);

    // Broadcast TransactionCompleted event to both sender and recipient
    let from_did_str = req.from.clone();
    let to_did_str = req.to.clone();
    let amount_val = req.amount;
    let currency_val = req.currency.clone();
    let hash_val = hash.clone();
    let coop_id_val = coop_id.to_string();
    let broadcaster = event_broadcaster.into_inner();

    tokio::spawn(async move {
        use crate::events::GatewayEvent;

        let event = GatewayEvent::TransactionCompleted {
            coop_id: coop_id_val.clone(),
            hash: hash_val,
            from: from_did_str.clone(),
            to: to_did_str.clone(),
            amount: amount_val,
            currency: currency_val,
        };

        // Send to cooperative channel (all members can see)
        broadcaster.broadcast(&coop_id_val, event.clone()).await;

        // Also send to personal channels for sender and recipient
        broadcaster.broadcast(&from_did_str, event.clone()).await;
        broadcaster.broadcast(&to_did_str, event).await;
    });

    // Send push notifications (async, don't block response)
    let notif_service = notification_service.into_inner();
    let from_str = req.from.clone();
    let to_str = req.to.clone();
    let amount = req.amount;
    let hash_str = hash.clone();
    let coop_str = coop_id.to_string();
    tokio::spawn(async move {
        use crate::events::GatewayEvent;
        use crate::notification_listener::handle_event_for_notifications;

        let event = GatewayEvent::PaymentCreated {
            coop_id: coop_str,
            hash: hash_str,
            from: from_str,
            to: to_str,
            amount,
            currency: "hours".to_string(),
        };

        handle_event_for_notifications(&event, &notif_service).await;
    });

    Ok(HttpResponse::Created().json(serde_json::json!({
        "hash": hash,
        "from": req.from,
        "to": req.to,
        "amount": req.amount,
        "currency": req.currency,
    })))
}

/// GET /ledger/:coop_id/history?did=...&cursor=...&limit=100 - Get transaction history
///
/// Supports both cursor-based and offset-based pagination:
/// - Cursor-based (preferred): Use `cursor` parameter for efficient large dataset navigation
/// - Offset-based (legacy): Use `offset` parameter for backward compatibility
///
/// Query parameters:
/// - `did`: Filter transactions involving this DID
/// - `cursor`: Cursor for next page (from previous response)
/// - `limit`: Maximum number of transactions to return (default: 20, max: 100)
/// - `offset`: Offset for legacy pagination (ignored if cursor provided)
#[get("/{coop_id}/history")]
pub async fn get_history(
    req: HttpRequest,
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    coop_id: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    use crate::models::{PaginationInfo, TransactionHistoryResponse};
    use crate::pagination::{Cursor, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};

    // Check authorization
    require_scope(&req, "ledger:read")?;
    require_coop_access(&req, &coop_id)?; // CRITICAL: Prevent cross-coop privacy leaks

    let filter_did = if let Some(did_str) = query.get("did") {
        Some(
            did_str
                .parse()
                .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?,
        )
    } else {
        None
    };

    // Parse pagination parameters
    let cursor = query.get("cursor").cloned();
    let limit: usize = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);

    // Decode cursor to get timestamp and hash for continuation
    let decoded_cursor = cursor.as_ref().and_then(|c| Cursor::decode(c));
    // Convert Cursor to the format expected by ledger: (timestamp, optional_hash)
    let cursor_tuple = decoded_cursor.as_ref().map(|c| {
        // Cursor stores timestamp in milliseconds, convert to seconds
        let ts = c.ts / 1000;
        let hash = if c.id.is_empty() {
            None
        } else {
            Some(c.id.clone())
        };
        (ts, hash)
    });

    // For legacy offset-based pagination, we still need to support it
    let offset: usize = query
        .get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Validate pagination parameters
    let validated_limit = validation::validate_history_limit(limit)?;

    // Track history query
    gateway::history_queries_inc();

    // Track whether we're using cursor-based pagination (for offset field)
    let using_cursor = cursor_tuple.is_some();

    // Use memory-efficient paginated query
    // This streams entries and stops after collecting enough, never loading all into memory
    let (entries, next_cursor_tuple) = if cursor_tuple.is_some() || filter_did.is_some() {
        // Use the new streaming pagination for cursor-based or filtered queries
        ledger_mgr
            .get_history_paginated(&coop_id, filter_did.as_ref(), cursor_tuple, validated_limit)
            .await?
    } else {
        // For simple offset-based queries without filters, use the offset method
        let offset = validation::validate_history_offset(offset)?;
        let entries = ledger_mgr
            .get_history(&coop_id, filter_did.as_ref(), offset, validated_limit + 1)
            .await?;
        let has_more = entries.len() > validated_limit;
        let entries: Vec<_> = entries.into_iter().take(validated_limit).collect();
        let next = if has_more {
            entries.last().map(|e| {
                let hash = e.id.as_ref().map(|h| h.to_hex()).unwrap_or_default();
                (e.timestamp, hash)
            })
        } else {
            None
        };
        (entries, next)
    };

    // Convert to response format
    let history: Vec<TransactionHistoryEntry> = entries
        .into_iter()
        .map(|entry| {
            let accounts: Vec<AccountDeltaResponse> = entry
                .accounts
                .iter()
                .map(|delta| AccountDeltaResponse {
                    account_id: delta.account_id.to_string(),
                    currency: delta.currency.clone(),
                    debit: delta.debit,
                    credit: delta.credit,
                })
                .collect();

            TransactionHistoryEntry {
                id: entry.id.map(|h| h.to_hex()).unwrap_or_default(),
                timestamp: entry.timestamp,
                author: entry.author.to_string(),
                accounts,
                decision_receipt_id: entry.decision_receipt_id.clone(),
                decision_hash: entry.decision_hash.clone(),
            }
        })
        .collect();

    // Build pagination info
    let has_more = next_cursor_tuple.is_some();
    let next_cursor = next_cursor_tuple.map(|(ts, hash)| Cursor::from_seconds(ts, &hash).encode());

    let count = history.len();
    let pagination = PaginationInfo {
        total: None, // Total not efficiently available for filtered queries
        next_cursor,
        prev_cursor: None, // Prev cursor requires storing state, not supported
        count,
        has_more,
        offset: if using_cursor { None } else { Some(offset) },
        limit: validated_limit,
    };

    let response = TransactionHistoryResponse {
        transactions: history,
        pagination,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /ledger/:coop_id/entries/by-decision - Get ledger entries by decision hash
///
/// Queries ledger entries that were authorized by a specific governance decision.
/// This enables tracing the economic effects of governance decisions.
///
/// Query parameters:
/// - `decision_hash`: The canonical decision hash (hex-encoded)
/// - `limit`: Maximum number of entries to return (default: 20, max: 100)
#[get("/{coop_id}/entries/by-decision")]
pub async fn get_entries_by_decision(
    req: HttpRequest,
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    ledger_service: Option<web::Data<Option<Arc<icn_api::LedgerService>>>>,
    coop_id: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    use crate::models::TransactionHistoryResponse;
    use crate::pagination::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};

    // Check authorization
    require_scope(&req, "ledger:read")?;
    require_coop_access(&req, &coop_id)?;

    // Get decision hash from query
    let decision_hash = query.get("decision_hash").ok_or_else(|| {
        GatewayError::BadRequest("decision_hash query parameter required".to_string())
    })?;

    // Validate hex format
    if decision_hash.len() != 64 {
        return Err(GatewayError::BadRequest(
            "decision_hash must be 64 hex characters (32 bytes)".to_string(),
        ));
    }
    hex::decode(decision_hash)
        .map_err(|_| GatewayError::BadRequest("decision_hash must be valid hex".to_string()))?;

    let limit: usize = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);

    if let Some(service) = ledger_service
        .as_ref()
        .and_then(|service| service.get_ref().clone())
    {
        let page = service
            .get_entries_by_decision(decision_hash, limit)
            .await
            .map_err(|e| GatewayError::InternalError(e.to_string()))?;
        let filtered = page
            .entries
            .into_iter()
            .map(|entry| TransactionHistoryEntry {
                id: entry.id,
                timestamp: entry.timestamp,
                author: entry.author,
                accounts: entry
                    .accounts
                    .into_iter()
                    .map(|delta| AccountDeltaResponse {
                        account_id: delta.account_id,
                        currency: delta.currency,
                        debit: delta.debit,
                        credit: delta.credit,
                    })
                    .collect(),
                decision_receipt_id: entry.decision_receipt_id,
                decision_hash: entry.decision_hash,
            })
            .collect::<Vec<_>>();

        let count = filtered.len();
        let pagination = PaginationInfo {
            total: Some(count),
            next_cursor: None,
            prev_cursor: None,
            count,
            has_more: page.has_more,
            offset: None,
            limit,
        };

        let response = TransactionHistoryResponse {
            transactions: filtered,
            pagination,
        };

        return Ok(HttpResponse::Ok().json(response));
    }

    // Get all history entries (bounded scan for pilot)
    // TODO: Add index for decision_hash in ledger for efficient queries
    let entries = ledger_mgr.get_history(&coop_id, None, 0, 1000).await?;

    // Filter entries by decision_hash
    let matching_entries: Vec<_> = entries
        .into_iter()
        .filter(|entry| entry.decision_hash.as_deref() == Some(decision_hash.as_str()))
        .collect();
    let has_more = matching_entries.len() > limit;

    let filtered: Vec<TransactionHistoryEntry> = matching_entries
        .into_iter()
        .take(limit)
        .map(|entry| {
            let accounts: Vec<AccountDeltaResponse> = entry
                .accounts
                .iter()
                .map(|delta| AccountDeltaResponse {
                    account_id: delta.account_id.to_string(),
                    currency: delta.currency.clone(),
                    debit: delta.debit,
                    credit: delta.credit,
                })
                .collect();

            TransactionHistoryEntry {
                id: entry.id.map(|h| h.to_hex()).unwrap_or_default(),
                timestamp: entry.timestamp,
                author: entry.author.to_string(),
                accounts,
                decision_receipt_id: entry.decision_receipt_id.clone(),
                decision_hash: entry.decision_hash.clone(),
            }
        })
        .collect();

    let count = filtered.len();
    let pagination = PaginationInfo {
        total: Some(count),
        next_cursor: None,
        prev_cursor: None,
        count,
        has_more,
        offset: None,
        limit,
    };

    let response = TransactionHistoryResponse {
        transactions: filtered,
        pagination,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /ledger/:coop_id/payment/convert - Create a cross-currency payment
///
/// Executes a payment where the sender pays in one currency and the recipient
/// receives in another. Uses the exchange rate oracle for conversion.
#[post("/{coop_id}/payment/convert")]
pub async fn create_cross_payment(
    http_req: HttpRequest,
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    budget_store: web::Data<BudgetStore>,
    velocity_limiter: web::Data<VelocityLimiter>,
    trust_mgr: web::Data<Arc<TrustManager>>,
    coop_id: web::Path<String>,
    req: web::Json<CreateCrossPaymentRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "ledger:write")?;
    require_coop_access(&http_req, &coop_id)?;

    // Verify authenticated DID matches payment sender
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    if claims.sub != req.from {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Cannot create payments from other accounts (authenticated as {}, attempted to send from {})",
            claims.sub, req.from
        )));
    }

    let from = req
        .from
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid from DID: {e}")))?;

    let to = req
        .to
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid to DID: {e}")))?;

    // Prevent self-payments
    if from == to {
        return Err(GatewayError::BadRequest(
            "Self-payments not allowed (sender and recipient cannot be the same)".to_string(),
        ));
    }

    // Validate cross-currency specific: currencies must differ
    if req.from_currency == req.to_currency {
        return Err(GatewayError::BadRequest(
            "For same-currency payments, use /payment endpoint instead".to_string(),
        ));
    }

    // Validate input fields
    validation::validate_payment_amount(req.amount)?;
    validation::validate_currency(&req.from_currency)?;
    validation::validate_currency(&req.to_currency)?;
    validation::validate_memo(&req.memo)?;

    // Check budget limits
    let can_spend = budget_store
        .check_spending(&req.from, req.amount)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;
    if !can_spend {
        return Err(GatewayError::BadRequest(
            "Payment exceeds budget limit".to_string(),
        ));
    }

    // Check transaction velocity limit
    let trust_score = trust_mgr.compute_trust_score_for_velocity(&from).await;
    velocity_limiter.check_and_record(&req.from, trust_score)?;

    // Execute the cross-currency payment
    let response = ledger_mgr
        .create_cross_payment(
            &coop_id,
            &from,
            &to,
            req.amount,
            &req.from_currency,
            &req.to_currency,
            req.max_target_amount,
            req.memo.clone(),
        )
        .await?;

    // Record spending in budget tracker
    let notified_budgets = budget_store
        .record_spending(&req.from, req.amount)
        .map_err(|e| GatewayError::InternalError(e.to_string()))?;
    if !notified_budgets.is_empty() {
        info!(
            budgets = ?notified_budgets,
            "Budget threshold notifications triggered"
        );
    }

    // Track metrics
    gateway::payments_created_inc();
    gateway::payment_amount_record(&req.from_currency, req.amount);

    // Track FX-specific metrics
    gateway::fx_payments_total_inc(&req.from_currency, &req.to_currency);
    gateway::fx_payment_source_amount_record(&req.from_currency, req.amount);
    gateway::fx_payment_target_amount_record(&req.to_currency, response.net_target_amount);
    gateway::fx_payment_fee_amount_record(&req.to_currency, response.fee_amount);

    Ok(HttpResponse::Created().json(response))
}

/// POST /ledger/:coop_id/payment/convert/quote - Get a cross-currency payment quote
///
/// Returns a preview of what a cross-currency payment would look like without
/// actually executing it. Useful for showing users the expected conversion.
#[post("/{coop_id}/payment/convert/quote")]
pub async fn get_cross_payment_quote(
    http_req: HttpRequest,
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    coop_id: web::Path<String>,
    req: web::Json<CrossPaymentQuoteRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "ledger:read")?;
    require_coop_access(&http_req, &coop_id)?;

    // Validate currencies must differ
    if req.from_currency == req.to_currency {
        return Err(GatewayError::BadRequest(
            "Quote requires different currencies".to_string(),
        ));
    }

    // Validate input fields
    validation::validate_payment_amount(req.amount)?;
    validation::validate_currency(&req.from_currency)?;
    validation::validate_currency(&req.to_currency)?;

    // Get the quote
    let quote = ledger_mgr
        .get_cross_payment_quote(&coop_id, req.amount, &req.from_currency, &req.to_currency)
        .await?;

    // Track FX quote metrics
    gateway::fx_quote_requests_inc(&req.from_currency, &req.to_currency);

    Ok(HttpResponse::Ok().json(quote))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use crate::events::EventBroadcaster;
    use crate::trust_mgr::TrustManager;
    use actix_web::{test, App, HttpMessage};
    use icn_identity::IdentityBundle;

    #[actix_web::test]
    async fn test_create_payment_and_get_balance() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        // Use temporary store for tests
        let store = Arc::new(crate::notifications::NotificationStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let notification_service =
            Arc::new(crate::notifications::NotificationService::new(store, None));
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .app_data(web::Data::new(notification_service.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(
                    web::scope("/ledger")
                        .service(create_payment)
                        .service(get_balance),
                ),
        )
        .await;

        // Create payment with authorization
        let req_body = CreatePaymentRequest {
            from: alice.did().to_string(),
            to: bob.did().to_string(),
            amount: 10,
            currency: "hours".to_string(),
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims.clone());

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Get Alice's balance with authorization
        let uri = format!("/ledger/test-coop/balance/{}", alice.did());
        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:read".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::get().uri(&uri).to_request();
        req.extensions_mut().insert(claims);

        let resp: BalanceResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.balances.get("hours"), Some(&10));
        assert_eq!(resp.credit_limits, None); // No credit limits configured in test
    }

    #[actix_web::test]
    async fn test_get_history() {
        use crate::models::TransactionHistoryResponse;

        let ledger_mgr = Arc::new(LedgerManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        // Create payment directly
        ledger_mgr
            .create_payment(
                &"test-coop".to_string(),
                alice.did(),
                bob.did(),
                10,
                "hours".to_string(),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .service(web::scope("/ledger").service(get_history)),
        )
        .await;

        // Get history with authorization (uses default pagination)
        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:read".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::get()
            .uri("/ledger/test-coop/history")
            .to_request();
        req.extensions_mut().insert(claims.clone());

        let resp: TransactionHistoryResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.transactions.len(), 1);
        assert_eq!(resp.transactions[0].accounts.len(), 2); // Alice and Bob
        assert_eq!(resp.pagination.count, 1);
        assert!(!resp.pagination.has_more);

        // Get history filtered by Alice with authorization
        let uri = format!("/ledger/test-coop/history?did={}", alice.did());
        let req = test::TestRequest::get().uri(&uri).to_request();
        req.extensions_mut().insert(claims.clone());

        let resp: TransactionHistoryResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.transactions.len(), 1);

        // Test pagination parameters
        let uri = "/ledger/test-coop/history?offset=0&limit=10";
        let req = test::TestRequest::get().uri(uri).to_request();
        req.extensions_mut().insert(claims.clone());

        let resp: TransactionHistoryResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.transactions.len(), 1);
        assert_eq!(resp.pagination.limit, 10);

        // Test offset beyond available entries
        let uri = "/ledger/test-coop/history?offset=100&limit=10";
        let req = test::TestRequest::get().uri(uri).to_request();
        req.extensions_mut().insert(claims.clone());

        let resp: TransactionHistoryResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.transactions.len(), 0);

        // Test cursor-based pagination (get next_cursor from first request)
        let req = test::TestRequest::get()
            .uri("/ledger/test-coop/history")
            .to_request();
        req.extensions_mut().insert(claims.clone());

        let resp: TransactionHistoryResponse = test::call_and_read_body_json(&app, req).await;
        // With only 1 item, there should be no next_cursor
        assert!(resp.pagination.next_cursor.is_none());
    }

    #[actix_web::test]
    async fn test_get_entries_by_decision_prefers_shared_service_boundary() {
        use crate::models::TransactionHistoryResponse;
        use icn_ledger::{entry::JournalEntryBuilder, Ledger};

        let ledger_mgr = Arc::new(LedgerManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();
        let decision_hash = "a".repeat(64);

        let service_store = Arc::new(icn_store::SledStore::temporary().unwrap());
        let mut service_ledger = Ledger::new(service_store).unwrap();
        let service_entry = JournalEntryBuilder::new(alice.did().clone())
            .debit(alice.did().clone(), "hours".to_string(), 10)
            .credit(bob.did().clone(), "hours".to_string(), 10)
            .with_decision_provenance("receipt-123", decision_hash.as_str())
            .build()
            .unwrap();
        service_ledger.append_entry(service_entry).await.unwrap();

        let ledger_service = Arc::new(icn_api::LedgerService::new(Arc::new(
            tokio::sync::RwLock::new(service_ledger),
        )));

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(Some(ledger_service)))
                .service(web::scope("/ledger").service(get_entries_by_decision)),
        )
        .await;

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:read".to_string()],
            exp: 9999999999,
        };

        let uri = format!(
            "/ledger/test-coop/entries/by-decision?decision_hash={}",
            decision_hash
        );
        let req = test::TestRequest::get().uri(&uri).to_request();
        req.extensions_mut().insert(claims);

        let resp: TransactionHistoryResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.transactions.len(), 1);
        assert_eq!(
            resp.transactions[0].decision_hash.as_deref(),
            Some(decision_hash.as_str())
        );
        assert_eq!(resp.pagination.count, 1);
    }

    #[actix_web::test]
    async fn test_authorization_scope_check() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        // Use temporary store for tests
        let store = Arc::new(crate::notifications::NotificationStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let notification_service =
            Arc::new(crate::notifications::NotificationService::new(store, None));
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .app_data(web::Data::new(notification_service.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(
                    web::scope("/ledger")
                        .service(create_payment)
                        .service(get_balance),
                ),
        )
        .await;

        // Try to create payment with only "ledger:read" scope (should fail)
        let req_body = CreatePaymentRequest {
            from: alice.did().to_string(),
            to: alice.did().to_string(),
            amount: 10,
            currency: "hours".to_string(),
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:read".to_string()], // Wrong scope!
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Try to read balance with "ledger:write" scope (should fail)
        let uri = format!("/ledger/test-coop/balance/{}", alice.did());
        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:write".to_string()], // Wrong scope!
            exp: 9999999999,
        };

        let req = test::TestRequest::get().uri(&uri).to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_create_payment_from_other_account_fails() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        // Use temporary store for tests
        let store = Arc::new(crate::notifications::NotificationStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let notification_service =
            Arc::new(crate::notifications::NotificationService::new(store, None));
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();
        let charlie = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .app_data(web::Data::new(notification_service.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/ledger").service(create_payment)),
        )
        .await;

        // Alice tries to create payment from Bob's account (should fail)
        let req_body = CreatePaymentRequest {
            from: bob.did().to_string(), // Bob's account
            to: charlie.did().to_string(),
            amount: 10,
            currency: "hours".to_string(),
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(), // Alice authenticated
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_cross_cooperative_ledger_privacy() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        // Use temporary store for tests
        let store = Arc::new(crate::notifications::NotificationStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let notification_service =
            Arc::new(crate::notifications::NotificationService::new(store, None));
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        // Create payment in coop-food
        let _ = ledger_mgr
            .create_payment(
                &"coop-food".to_string(),
                alice.did(),
                bob.did(),
                50,
                "hours".to_string(),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .app_data(web::Data::new(notification_service.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(
                    web::scope("/ledger")
                        .service(create_payment)
                        .service(get_balance)
                        .service(get_history),
                ),
        )
        .await;

        // Alice tries to create payment in coop-tech using coop-food token (should fail)
        let payment_req = CreatePaymentRequest {
            from: alice.did().to_string(),
            to: bob.did().to_string(),
            amount: 10,
            currency: "hours".to_string(),
            memo: None,
        };

        let claims_write = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "coop-food".to_string(), // Token for coop-food
            scopes: vec!["ledger:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/coop-tech/payment") // Trying to access coop-tech!
            .set_json(&payment_req)
            .to_request();
        req.extensions_mut().insert(claims_write);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Alice tries to read balance in coop-tech using coop-food token (should fail)
        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "coop-food".to_string(), // Token for coop-food
            scopes: vec!["ledger:read".to_string()],
            exp: 9999999999,
        };

        let uri = format!("/ledger/coop-tech/balance/{}", bob.did()); // Accessing coop-tech
        let req = test::TestRequest::get().uri(&uri).to_request();
        req.extensions_mut().insert(claims.clone());

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Also test history cross-coop access (should fail)
        let req = test::TestRequest::get()
            .uri("/ledger/coop-tech/history") // Accessing coop-tech
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_self_payment_rejected() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        // Use temporary store for tests
        let store = Arc::new(crate::notifications::NotificationStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let notification_service =
            Arc::new(crate::notifications::NotificationService::new(store, None));
        let event_broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .app_data(web::Data::new(notification_service.clone()))
                .app_data(web::Data::new(event_broadcaster.clone()))
                .service(web::scope("/ledger").service(create_payment)),
        )
        .await;

        // Alice tries to pay herself (should fail)
        let req_body = CreatePaymentRequest {
            from: alice.did().to_string(),
            to: alice.did().to_string(), // Same as from!
            amount: 10,
            currency: "hours".to_string(),
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_cross_payment_same_currency_rejected() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .service(web::scope("/ledger").service(create_cross_payment)),
        )
        .await;

        // Try cross-payment with same currency (should fail)
        let req_body = CreateCrossPaymentRequest {
            from: alice.did().to_string(),
            to: bob.did().to_string(),
            amount: 10,
            from_currency: "hours".to_string(),
            to_currency: "hours".to_string(), // Same currency!
            max_target_amount: None,
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment/convert")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_cross_payment_self_payment_rejected() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .service(web::scope("/ledger").service(create_cross_payment)),
        )
        .await;

        // Try cross-payment to self (should fail)
        let req_body = CreateCrossPaymentRequest {
            from: alice.did().to_string(),
            to: alice.did().to_string(), // Self-payment!
            amount: 10,
            from_currency: "hours".to_string(),
            to_currency: "USD".to_string(),
            max_target_amount: None,
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment/convert")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_cross_payment_authorization_check() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .service(web::scope("/ledger").service(create_cross_payment)),
        )
        .await;

        // Try cross-payment with read-only scope (should fail)
        let req_body = CreateCrossPaymentRequest {
            from: alice.did().to_string(),
            to: bob.did().to_string(),
            amount: 10,
            from_currency: "hours".to_string(),
            to_currency: "USD".to_string(),
            max_target_amount: None,
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:read".to_string()], // Wrong scope!
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment/convert")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_cross_payment_quote_same_currency_rejected() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .service(web::scope("/ledger").service(get_cross_payment_quote)),
        )
        .await;

        // Try quote with same currency (should fail)
        let req_body = CrossPaymentQuoteRequest {
            amount: 10,
            from_currency: "hours".to_string(),
            to_currency: "hours".to_string(), // Same currency!
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:read".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment/convert/quote")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_cross_payment_from_other_account_rejected() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();
        let charlie = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .service(web::scope("/ledger").service(create_cross_payment)),
        )
        .await;

        // Alice tries to create cross-payment from Bob's account (should fail)
        let req_body = CreateCrossPaymentRequest {
            from: bob.did().to_string(), // Bob's account
            to: charlie.did().to_string(),
            amount: 10,
            from_currency: "hours".to_string(),
            to_currency: "USD".to_string(),
            max_target_amount: None,
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(), // Alice authenticated
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment/convert")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    /// Test that FX errors return structured error response with code field (Issue #374)
    ///
    /// This test verifies that when an FxError is returned from LedgerManager,
    /// the HTTP response includes both 'error' and 'code' fields for programmatic handling.
    #[actix_web::test]
    async fn test_fx_error_response_includes_error_code() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
        let trust_mgr = Arc::new(TrustManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
                .app_data(web::Data::new(trust_mgr.clone()))
                .service(web::scope("/ledger").service(create_cross_payment)),
        )
        .await;

        // Cross-payment with different currencies - will reach FX code which returns
        // FX_NOT_CONFIGURED since no oracle is configured in test LedgerManager
        let req_body = CreateCrossPaymentRequest {
            from: alice.did().to_string(),
            to: bob.did().to_string(),
            amount: 100,
            from_currency: "hours".to_string(),
            to_currency: "USD".to_string(), // Different currencies -> reaches FX code
            max_target_amount: None,
            memo: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment/convert")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        // FX not configured is a server error (500)
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        // Verify the response body has both error and code fields
        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(
            json.get("error").is_some(),
            "Response should have 'error' field"
        );
        assert!(
            json.get("code").is_some(),
            "Response should have 'code' field"
        );
        assert_eq!(
            json["code"], "FX_NOT_CONFIGURED",
            "Error code should be FX_NOT_CONFIGURED"
        );
    }

    /// Test that quote endpoint also returns structured error codes (Issue #374)
    #[actix_web::test]
    async fn test_fx_quote_error_response_includes_error_code() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .service(web::scope("/ledger").service(get_cross_payment_quote)),
        )
        .await;

        // Quote with different currencies - will reach FX code which returns
        // FX_NOT_CONFIGURED since no oracle is configured in test LedgerManager
        let req_body = CrossPaymentQuoteRequest {
            amount: 100,
            from_currency: "hours".to_string(),
            to_currency: "USD".to_string(), // Different currencies -> reaches FX code
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["ledger:read".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment/convert/quote")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        // FX not configured is a server error (500)
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        // Verify the response body has both error and code fields
        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(
            json.get("error").is_some(),
            "Response should have 'error' field"
        );
        assert!(
            json.get("code").is_some(),
            "Response should have 'code' field"
        );
        assert_eq!(
            json["code"], "FX_NOT_CONFIGURED",
            "Error code should be FX_NOT_CONFIGURED"
        );
    }
}
