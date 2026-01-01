//! Trust API endpoints

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::trust_mgr::TrustManager;
use icn_identity::Did;
use icn_trust::TrustEdge;

/// Trust score response
#[derive(Debug, Serialize, ToSchema)]
pub struct TrustScoreResponse {
    pub did: String,
    pub trust_score: f64,
    pub trust_class: String,
}

/// Create trust edge request
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTrustEdgeRequest {
    /// Target DID to attest
    pub to: String,
    /// Trust score (0.0 - 1.0)
    pub score: f64,
    /// Optional memo/reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// GET /v1/trust/{did} - Get trust score for a DID
///
/// Returns the trust score and classification for a specific DID.
#[get("/trust/{did}")]
pub async fn get_trust_score(
    path: web::Path<String>,
    from_did: web::ReqData<Did>, // Extracted from JWT
    trust_manager: web::Data<Arc<TrustManager>>,
) -> Result<HttpResponse> {
    let target_did_str = path.into_inner();
    let target_did = target_did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let from = from_did.into_inner();
    let trust_score = trust_manager
        .compute_trust_score_async(&from, &target_did)
        .await;

    let trust_class = if trust_score < 0.1 {
        "Isolated"
    } else if trust_score < 0.4 {
        "Known"
    } else if trust_score < 0.7 {
        "Partner"
    } else {
        "Federated"
    };

    Ok(HttpResponse::Ok().json(TrustScoreResponse {
        did: target_did_str,
        trust_score,
        trust_class: trust_class.to_string(),
    }))
}

/// GET /v1/trust/{did}/edges - Get trust edges from a DID
///
/// Returns all outgoing trust edges from the specified DID.
#[get("/trust/{did}/edges")]
pub async fn get_trust_edges(
    path: web::Path<String>,
    trust_manager: web::Data<Arc<TrustManager>>,
) -> Result<HttpResponse> {
    let did_str = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let edges = trust_manager.get_outgoing_edges_async(&did).await;

    let response: Vec<_> = edges
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "from": e.source.to_string(),
                "to": e.target.to_string(),
                "score": e.score,
                "created_at": e.created_at,
                "labels": if e.labels.is_empty() { None } else { Some(e.labels) },
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

/// POST /v1/trust/attest - Create a trust attestation
///
/// Creates a trust edge from the authenticated DID to another DID.
#[post("/trust/attest")]
pub async fn create_trust_attestation(
    req: web::Json<CreateTrustEdgeRequest>,
    from_did: web::ReqData<Did>,
    trust_manager: web::Data<Arc<TrustManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
) -> Result<HttpResponse> {
    let to_did = req
        .to
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid target DID: {e}")))?;

    // Validate score range
    if req.score < 0.0 || req.score > 1.0 {
        return Err(GatewayError::BadRequest(
            "Trust score must be between 0.0 and 1.0".to_string(),
        ));
    }

    let from = from_did.into_inner();
    let mut edge = TrustEdge::new(from.clone(), to_did.clone(), req.score);

    // Add memo as label if provided
    if let Some(ref memo) = req.memo {
        edge = edge.with_label(memo.clone());
    }

    trust_manager
        .add_edge_async(edge)
        .await
        .map_err(GatewayError::InternalError)?;

    // Broadcast event to target DID (they received a trust attestation)
    // Use target's DID as the "coop_id" for personal notifications
    let event = GatewayEvent::TrustAttested {
        from: from.to_string(),
        to: to_did.to_string(),
        score: req.score,
        memo: req.memo.clone(),
    };
    event_broadcaster
        .broadcast(&to_did.to_string(), event)
        .await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Trust attestation created"
    })))
}

/// GET /v1/trust/{did}/network - Get trust network for visualization
///
/// Returns the trust network (nodes and edges) around a DID for visualization.
#[get("/trust/{did}/network")]
pub async fn get_trust_network(
    path: web::Path<String>,
    query: web::Query<NetworkQuery>,
    trust_manager: web::Data<Arc<TrustManager>>,
) -> Result<HttpResponse> {
    let did_str = path.into_inner();
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let max_distance = query.depth.unwrap_or(2).min(5); // Cap at 5 hops
    let network = trust_manager.get_trust_network(&did, max_distance);

    Ok(HttpResponse::Ok().json(network))
}

/// Query parameters for trust network
#[derive(Debug, Deserialize, ToSchema)]
pub struct NetworkQuery {
    /// Maximum distance (hops) to explore
    depth: Option<u32>,
}

/// Revoke trust edge request
#[derive(Debug, Deserialize, ToSchema)]
pub struct RevokeTrustEdgeRequest {
    /// Target DID to revoke attestation for
    pub to: String,
}

/// POST /v1/trust/revoke - Revoke a trust attestation
///
/// Removes a trust edge from the authenticated DID to another DID.
#[post("/trust/revoke")]
pub async fn revoke_trust_attestation(
    req: web::Json<RevokeTrustEdgeRequest>,
    from_did: web::ReqData<Did>,
    trust_manager: web::Data<Arc<TrustManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
) -> Result<HttpResponse> {
    let to_did = req
        .to
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid target DID: {e}")))?;

    let from = from_did.into_inner();

    trust_manager
        .remove_edge_async(&from, &to_did)
        .await
        .map_err(GatewayError::InternalError)?;

    // Broadcast event to target DID (they lost a trust attestation)
    let event = GatewayEvent::TrustRevoked {
        from: from.to_string(),
        to: to_did.to_string(),
    };
    event_broadcaster
        .broadcast(&to_did.to_string(), event)
        .await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Trust attestation revoked"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust_mgr::TrustManager;
    use actix_web::{test, App};
    use icn_identity::KeyPair;

    #[actix_web::test]
    async fn test_get_trust_edges() {
        let trust_manager = Arc::new(TrustManager::new());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let edge = TrustEdge::new(alice.clone(), bob.clone(), 0.7);
        trust_manager.add_edge_async(edge).await.unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(trust_manager))
                .service(get_trust_edges),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/trust/{alice}/edges"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
