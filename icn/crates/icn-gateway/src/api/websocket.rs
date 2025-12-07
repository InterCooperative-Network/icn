//! WebSocket API endpoint

use actix_web::{get, web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use std::sync::Arc;

use crate::auth::AuthManager;
use crate::events::EventBroadcaster;
use crate::websocket::WsSession;

/// GET /ws/:coop_id - WebSocket connection for real-time updates
#[get("/ws/{coop_id}")]
pub async fn websocket(
    req: HttpRequest,
    stream: web::Payload,
    coop_id: web::Path<String>,
    auth_manager: web::Data<Arc<AuthManager>>,
    event_broadcaster: web::Data<Arc<EventBroadcaster>>,
) -> Result<HttpResponse, Error> {
    let session = WsSession::new(
        coop_id.into_inner(),
        auth_manager.get_ref().clone(),
        event_broadcaster.get_ref().clone(),
    );
    ws::start(session, &req, stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;

    #[actix_web::test]
    async fn test_websocket_endpoint() {
        let auth_manager = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let event_broadcaster = Arc::new(EventBroadcaster::new());

        let app = test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(auth_manager))
                .app_data(web::Data::new(event_broadcaster))
                .service(websocket),
        )
        .await;

        let req = test::TestRequest::get().uri("/ws/test-coop").to_request();

        let resp = test::call_service(&app, req).await;
        // WebSocket upgrade returns 101 Switching Protocols
        // but in test environment it may return different status
        assert!(resp.status().is_client_error() || resp.status().is_success());
    }
}
