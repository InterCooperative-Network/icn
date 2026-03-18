//! Rights summary API endpoint
//!
//! Stub endpoint returning a placeholder summary of effective rights.
//! Full implementation requires trust oracle integration (future phase).

use actix_web::{get, HttpResponse};

/// GET /v1/rights/summary - Get effective rights summary for the current user
///
/// Returns a placeholder summary. Actual rights computation requires
/// trust oracle and policy oracle integration.
#[get("/summary")]
pub async fn rights_summary() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "effective_rights": [],
        "scope": "standalone",
        "note": "Rights computation requires trust oracle integration"
    }))
}
