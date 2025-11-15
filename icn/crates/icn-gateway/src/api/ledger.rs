//! Ledger API endpoints

use actix_web::{get, post, web, HttpResponse};
use std::sync::Arc;

use crate::error::Result;
use crate::ledger_mgr::LedgerManager;
use crate::models::{
    AccountDeltaResponse, BalanceResponse, CreatePaymentRequest, TransactionHistoryEntry,
};

/// GET /ledger/:coop_id/balance/:did - Get account balance
#[get("/ledger/{coop_id}/balance/{did}")]
pub async fn get_balance(
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (coop_id, did_str) = path.into_inner();

    let did = did_str.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {}", e)))?;

    let balances = ledger_mgr.get_all_balances(&coop_id, &did)?;

    let response = BalanceResponse {
        did: did_str,
        balances,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /ledger/:coop_id/payment - Create a payment
#[post("/ledger/{coop_id}/payment")]
pub async fn create_payment(
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    coop_id: web::Path<String>,
    req: web::Json<CreatePaymentRequest>,
) -> Result<HttpResponse> {
    let from = req.from.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid from DID: {}", e)))?;

    let to = req.to.parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid to DID: {}", e)))?;

    if req.amount <= 0 {
        return Err(crate::error::GatewayError::BadRequest(
            "Amount must be positive".to_string(),
        ));
    }

    let hash = ledger_mgr.create_payment(
        &coop_id,
        &from,
        &to,
        req.amount,
        req.currency.clone(),
    )?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "hash": hash,
        "from": req.from,
        "to": req.to,
        "amount": req.amount,
        "currency": req.currency,
    })))
}

/// GET /ledger/:coop_id/history?did=... - Get transaction history
#[get("/ledger/{coop_id}/history")]
pub async fn get_history(
    ledger_mgr: web::Data<Arc<LedgerManager>>,
    coop_id: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let filter_did = if let Some(did_str) = query.get("did") {
        Some(did_str.parse()
            .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {}", e)))?)
    } else {
        None
    };

    let entries = ledger_mgr.get_history(&coop_id, filter_did.as_ref())?;

    // Convert to response format
    let history: Vec<TransactionHistoryEntry> = entries
        .into_iter()
        .map(|entry| {
            let accounts: Vec<AccountDeltaResponse> = entry
                .accounts
                .into_iter()
                .map(|delta| AccountDeltaResponse {
                    account_id: delta.account_id.to_string(),
                    currency: delta.currency,
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

    Ok(HttpResponse::Ok().json(history))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use icn_identity::IdentityBundle;

    #[actix_web::test]
    async fn test_create_payment_and_get_balance() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .service(create_payment)
                .service(get_balance)
        ).await;

        // Create payment
        let req_body = CreatePaymentRequest {
            from: alice.did().to_string(),
            to: bob.did().to_string(),
            amount: 10,
            currency: "hours".to_string(),
            memo: None,
        };

        let req = test::TestRequest::post()
            .uri("/ledger/test-coop/payment")
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Get Alice's balance
        let uri = format!("/ledger/test-coop/balance/{}", alice.did());
        let req = test::TestRequest::get()
            .uri(&uri)
            .to_request();

        let resp: BalanceResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.balances.get("hours"), Some(&10));
    }

    #[actix_web::test]
    async fn test_get_history() {
        let ledger_mgr = Arc::new(LedgerManager::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        // Create payment directly
        ledger_mgr.create_payment(
            &"test-coop".to_string(),
            alice.did(),
            bob.did(),
            10,
            "hours".to_string(),
        ).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ledger_mgr.clone()))
                .service(get_history)
        ).await;

        // Get history
        let req = test::TestRequest::get()
            .uri("/ledger/test-coop/history")
            .to_request();

        let resp: Vec<TransactionHistoryEntry> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.len(), 1);
        assert_eq!(resp[0].accounts.len(), 2); // Alice and Bob

        // Get history filtered by Alice
        let uri = format!("/ledger/test-coop/history?did={}", alice.did());
        let req = test::TestRequest::get()
            .uri(&uri)
            .to_request();

        let resp: Vec<TransactionHistoryEntry> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.len(), 1);
    }
}
