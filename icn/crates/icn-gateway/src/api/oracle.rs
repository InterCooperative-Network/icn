//! Exchange Rate Oracle API endpoints
//!
//! Provides endpoints for querying and managing exchange rates between currencies.
//!
//! ## Endpoints
//!
//! | Endpoint | Method | Scope | Description |
//! |----------|--------|-------|-------------|
//! | `/v1/oracle/rate/{from}/{to}` | GET | oracle:read | Get exchange rate |
//! | `/v1/oracle/convert` | POST | oracle:read | Convert amount between currencies |
//! | `/v1/oracle/sources` | GET | oracle:read | List rate sources |
//! | `/v1/oracle/rate` | POST | oracle:write | Set manual rate |

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::error::{GatewayError, Result};
use crate::middleware::require_scope;
use icn_ledger::oracle::{CurrencyPair, ManualRateSource, OracleManager};
use icn_store::Store;

/// Shared oracle manager state
pub struct OracleState {
    /// The oracle manager
    pub oracle: Arc<OracleManager>,
    /// Manual rate source for setting rates (if available)
    pub manual_source: Option<Arc<ManualRateSource>>,
}

impl OracleState {
    /// Create a new oracle state with the given store
    pub fn new(store: Arc<dyn Store>) -> Self {
        let oracle = Arc::new(OracleManager::new(store.clone()));
        let manual_source = Arc::new(ManualRateSource::new(store));

        Self {
            oracle,
            manual_source: Some(manual_source),
        }
    }

    /// Create oracle state with an existing oracle manager
    pub fn with_oracle(oracle: Arc<OracleManager>) -> Self {
        Self {
            oracle,
            manual_source: None,
        }
    }
}

// === Request/Response Types ===

/// Exchange rate response
#[derive(Debug, Serialize)]
pub struct ExchangeRateResponse {
    /// Source currency
    pub from_currency: String,
    /// Target currency
    pub to_currency: String,
    /// Exchange rate (target per 1 source)
    pub rate: f64,
    /// Inverse rate (source per 1 target)
    pub inverse_rate: f64,
    /// Sources that contributed to this rate
    pub sources: Vec<String>,
    /// Whether the rate is stale (older than staleness threshold)
    pub is_stale: bool,
    /// When the rate was last aggregated (Unix timestamp)
    pub aggregated_at: u64,
    /// Remaining TTL in seconds
    pub remaining_ttl: u64,
}

/// Convert amount request
#[derive(Debug, Deserialize)]
pub struct ConvertAmountRequest {
    /// Amount to convert (in smallest unit)
    pub amount: i64,
    /// Source currency code
    pub from_currency: String,
    /// Target currency code
    pub to_currency: String,
}

/// Convert amount response
#[derive(Debug, Serialize)]
pub struct ConvertAmountResponse {
    /// Original amount
    pub original_amount: i64,
    /// Converted amount
    pub converted_amount: i64,
    /// Source currency
    pub from_currency: String,
    /// Target currency
    pub to_currency: String,
    /// Rate used for conversion
    pub rate_used: f64,
}

/// Rate source info response
#[derive(Debug, Serialize)]
pub struct RateSourceResponse {
    /// Source identifier
    pub source_id: String,
    /// Human-readable name
    pub name: String,
    /// Priority (lower = higher priority)
    pub priority: u8,
    /// Whether the source is healthy
    pub is_healthy: bool,
}

/// Set manual rate request
#[derive(Debug, Deserialize)]
pub struct SetManualRateRequest {
    /// Source currency code
    pub from_currency: String,
    /// Target currency code
    pub to_currency: String,
    /// Exchange rate (target per 1 source)
    pub rate: f64,
    /// Optional note/reason
    pub note: Option<String>,
}

/// Set manual rate response
#[derive(Debug, Serialize)]
pub struct SetManualRateResponse {
    /// Currency pair
    pub pair: String,
    /// Rate that was set
    pub rate: f64,
    /// Who set the rate
    pub set_by: String,
    /// When the rate was set
    pub set_at: u64,
}

/// List sources response
#[derive(Debug, Serialize)]
pub struct ListSourcesResponse {
    /// Available rate sources
    pub sources: Vec<RateSourceResponse>,
}

// === Endpoints ===

/// GET /oracle/rate/{from}/{to} - Get exchange rate
#[get("/rate/{from}/{to}")]
pub async fn get_rate(
    req: HttpRequest,
    oracle_state: web::Data<OracleState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    require_scope(&req, "oracle:read")?;

    let (from, to) = path.into_inner();
    let pair = CurrencyPair::new(&from, &to);

    let rate = oracle_state
        .oracle
        .get_rate(&pair)
        .await
        .map_err(|e| GatewayError::BadRequest(format!("Failed to get rate: {e}")))?;

    let response = ExchangeRateResponse {
        from_currency: from,
        to_currency: to,
        rate: rate.rate,
        inverse_rate: rate.inverse_rate(),
        sources: rate.observations.iter().map(|o| o.source.clone()).collect(),
        is_stale: rate.is_stale,
        aggregated_at: rate.aggregated_at,
        remaining_ttl: rate.remaining_ttl(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /oracle/convert - Convert amount between currencies
#[post("/convert")]
pub async fn convert_amount(
    req: HttpRequest,
    oracle_state: web::Data<OracleState>,
    body: web::Json<ConvertAmountRequest>,
) -> Result<HttpResponse> {
    require_scope(&req, "oracle:read")?;

    let converted = oracle_state
        .oracle
        .convert_amount(body.amount, &body.from_currency, &body.to_currency)
        .await
        .map_err(|e| GatewayError::BadRequest(format!("Failed to convert: {e}")))?;

    // Get the rate for the response
    let pair = CurrencyPair::new(&body.from_currency, &body.to_currency);
    let rate = oracle_state
        .oracle
        .get_rate(&pair)
        .await
        .map_err(|e| GatewayError::BadRequest(format!("Failed to get rate: {e}")))?;

    let response = ConvertAmountResponse {
        original_amount: body.amount,
        converted_amount: converted,
        from_currency: body.from_currency.clone(),
        to_currency: body.to_currency.clone(),
        rate_used: rate.rate,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /oracle/sources - List rate sources
#[get("/sources")]
pub async fn list_sources(
    req: HttpRequest,
    oracle_state: web::Data<OracleState>,
) -> Result<HttpResponse> {
    require_scope(&req, "oracle:read")?;

    let sources = oracle_state.oracle.list_sources().await;

    let response = ListSourcesResponse {
        sources: sources
            .into_iter()
            .map(|s| RateSourceResponse {
                source_id: s.source_id,
                name: s.name,
                priority: s.priority,
                is_healthy: s.is_healthy,
            })
            .collect(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /oracle/rate - Set manual rate
#[post("/rate")]
pub async fn set_rate(
    req: HttpRequest,
    oracle_state: web::Data<OracleState>,
    body: web::Json<SetManualRateRequest>,
) -> Result<HttpResponse> {
    require_scope(&req, "oracle:write")?;

    // Get the authenticated user's DID
    let claims = crate::middleware::get_claims(&req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let manual_source = oracle_state
        .manual_source
        .as_ref()
        .ok_or_else(|| GatewayError::BadRequest("Manual rate source not configured".to_string()))?;

    let pair = CurrencyPair::new(&body.from_currency, &body.to_currency);
    let record = manual_source
        .set_rate(&pair, body.rate, &claims.sub, body.note.clone())
        .map_err(|e| GatewayError::BadRequest(format!("Failed to set rate: {e}")))?;

    info!(
        pair = %pair,
        rate = body.rate,
        set_by = %claims.sub,
        "Manual rate set via API"
    );

    let response = SetManualRateResponse {
        pair: pair.key(),
        rate: record.rate,
        set_by: record.set_by,
        set_at: record.set_at,
    };

    Ok(HttpResponse::Created().json(response))
}

/// Configure oracle routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_rate)
        .service(convert_amount)
        .service(list_sources)
        .service(set_rate);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_currency_pair_creation() {
        let pair = CurrencyPair::new("hours", "USD");
        assert_eq!(pair.key(), "hours:USD");
    }
}
