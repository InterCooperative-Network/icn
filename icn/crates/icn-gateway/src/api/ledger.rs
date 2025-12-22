//! Ledger API endpoints

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;
use tracing::info;

use crate::api::budgets::BudgetStore;
use crate::error::{GatewayError, Result};
use crate::ledger_mgr::LedgerManager;
use crate::middleware::{get_claims, require_coop_access, require_scope};
use crate::models::{
    AccountDeltaResponse, BalanceResponse, CreatePaymentRequest, TransactionHistoryEntry,
};
use crate::rate_limit::VelocityLimiter;
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

    let balances = ledger_mgr.get_all_balances(&coop_id, &did)?;

    // Track balance query
    gateway::balance_queries_inc();

    let response = BalanceResponse {
        did: did_str,
        balances,
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
    // For now, use 0.5 as default trust score (Known+ trust level)
    // TODO: Look up actual trust score from trust graph when available
    let trust_score = 0.5;
    velocity_limiter.check_and_record(&req.from, trust_score)?;

    let hash = ledger_mgr.create_payment(&coop_id, &from, &to, req.amount, req.currency.clone())?;

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

    // Decode cursor if provided
    let decoded_cursor = cursor.as_ref().and_then(|c| Cursor::decode(c));

    // Determine starting offset for cursor-based pagination
    // For now, we fall back to loading all data and filtering in memory
    // TODO: Update icn-ledger to support native cursor-based queries
    let offset: usize = if decoded_cursor.is_some() {
        // When using cursor, we need to load enough data to find the cursor position
        0
    } else {
        query
            .get("offset")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };

    // Validate pagination parameters
    // SECURITY: Validate offset to prevent integer overflow in pagination arithmetic
    let offset = validation::validate_history_offset(offset)?;
    let validated_limit = validation::validate_history_limit(limit)?;

    // Request one extra to determine if there are more items
    let fetch_limit = if decoded_cursor.is_some() {
        validation::MAX_HISTORY_LIMIT // Need more for cursor-based
    } else {
        validated_limit + 1
    };

    let entries = ledger_mgr.get_history(&coop_id, filter_did.as_ref(), offset, fetch_limit)?;

    // Track history query
    gateway::history_queries_inc();

    // Convert to response format
    let mut history: Vec<TransactionHistoryEntry> = entries
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
            }
        })
        .collect();

    // Apply cursor-based pagination if cursor provided
    let (page_transactions, pagination) = if let Some(ref cur) = decoded_cursor {
        // Find the position after the cursor
        let start_idx = history
            .iter()
            .position(|tx| tx.timestamp < cur.ts || (tx.timestamp == cur.ts && tx.id <= cur.id))
            .unwrap_or(history.len());

        // Get page items
        let page: Vec<TransactionHistoryEntry> = history
            .iter()
            .skip(start_idx)
            .take(limit)
            .cloned()
            .collect();

        let has_more = start_idx + limit < history.len();

        // Build next cursor from last item
        let next_cursor = if has_more {
            page.last()
                .map(|tx| Cursor::from_seconds(tx.timestamp, &tx.id).encode())
        } else {
            None
        };

        // Build prev cursor
        let prev_cursor = if start_idx > 0 {
            history
                .get(start_idx.saturating_sub(1))
                .map(|tx| Cursor::from_seconds(tx.timestamp, &tx.id).encode())
        } else {
            None
        };

        let count = page.len();
        (
            page,
            PaginationInfo::cursor_based(next_cursor, prev_cursor, count, has_more, limit),
        )
    } else {
        // Offset-based pagination (legacy mode)
        let has_more = history.len() > validated_limit;
        if has_more {
            history.pop(); // Remove the extra item we fetched
        }

        // Generate cursor for next page if there are more items
        let next_cursor = if has_more {
            history
                .last()
                .map(|tx| Cursor::from_seconds(tx.timestamp, &tx.id).encode())
        } else {
            None
        };

        let count = history.len();
        let pagination = PaginationInfo {
            total: None, // Total not efficiently available
            next_cursor,
            prev_cursor: None,
            count,
            has_more,
            offset: Some(offset),
            limit: validated_limit,
        };

        (history, pagination)
    };

    let response = TransactionHistoryResponse {
        transactions: page_transactions,
        pagination,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use crate::events::EventBroadcaster;
    use actix_web::{test, App, HttpMessage};
    use icn_identity::IdentityBundle;

    #[actix_web::test]
    async fn test_create_payment_and_get_balance() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
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
    async fn test_authorization_scope_check() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let db = sled::Config::new().temporary(true).open().unwrap();
        let budget_store = BudgetStore::new(db);
        let velocity_limiter = VelocityLimiter::default();
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
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .app_data(web::Data::new(budget_store.clone()))
                .app_data(web::Data::new(velocity_limiter.clone()))
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
}
