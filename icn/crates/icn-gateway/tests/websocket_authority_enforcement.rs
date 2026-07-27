//! Assembled WebSocket continuing-authorization regression tests.
//!
//! These use a real TCP listener, the production WebSocket handler, and the
//! production `WsSession` actor. They prove that retaining a connection after
//! initial authentication does not retain protected event or backfill access.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fmt::Debug;
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{web, App, HttpServer};
use awc::ws::{Frame, Message};
use futures::{Sink, SinkExt, Stream, StreamExt};
use icn_gateway::auth::{AuthManager, TokenClaims};
use icn_gateway::events::{EventBroadcaster, GatewayEvent};
use icn_gateway::session_authority::SessionAuthority;
use icn_identity::IdentityBundle;

const SECRET: &[u8] = b"websocket-authority-integration-secret";
const COOP_ID: &str = "test-coop";

fn authority_and_token(ttl: Duration) -> (Arc<SessionAuthority>, String, TokenClaims) {
    let auth = Arc::new(AuthManager::new(SECRET.to_vec()).with_token_ttl(ttl));
    let authority = Arc::new(SessionAuthority::evaluator(auth.clone()));
    let identity = IdentityBundle::generate().expect("test identity");
    let token = auth
        .issue_token(identity.did(), COOP_ID, vec!["coop:read".to_string()])
        .expect("test token");
    let claims = auth.verify_token(&token).expect("test claims");
    (authority, token, claims)
}

async fn start_server(
    authority: Arc<SessionAuthority>,
    broadcaster: Arc<EventBroadcaster>,
) -> (actix_web::dev::ServerHandle, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(authority.clone()))
            .app_data(web::Data::new(broadcaster.clone()))
            .service(icn_gateway::api::websocket::websocket)
    })
    .listen(listener)
    .expect("listen on test socket")
    .run();
    let handle = server.handle();
    actix_web::rt::spawn(server);
    (handle, format!("ws://{address}/ws/{COOP_ID}"))
}

async fn authenticate<S, E>(socket: &mut S, token: &str)
where
    S: Sink<Message, Error = E> + Stream<Item = Result<Frame, E>> + Unpin,
    E: Debug,
{
    socket
        .send(Message::Text(
            serde_json::json!({ "type": "Auth", "token": token })
                .to_string()
                .into(),
        ))
        .await
        .expect("send auth frame");
    let response = next_text(socket).await;
    assert!(
        response.contains("\"type\":\"AuthOk\""),
        "expected AuthOk, received {response}"
    );
}

async fn next_text<S, E>(socket: &mut S) -> String
where
    S: Stream<Item = Result<Frame, E>> + Unpin,
    E: Debug,
{
    let frame = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .expect("timed out waiting for WebSocket frame")
        .expect("WebSocket stream ended")
        .expect("WebSocket protocol error");
    match frame {
        Frame::Text(bytes) => String::from_utf8(bytes.to_vec()).expect("UTF-8 server message"),
        other => panic!("expected text frame, received {other:?}"),
    }
}

fn protected_event() -> GatewayEvent {
    GatewayEvent::MemberAdded {
        coop_id: COOP_ID.to_string(),
        did: "did:icn:protected-member".to_string(),
        role: "member".to_string(),
    }
}

#[actix_web::test]
async fn revoked_connection_receives_no_live_event() {
    let (authority, token, claims) = authority_and_token(Duration::from_secs(3600));
    let broadcaster = Arc::new(EventBroadcaster::new());
    let (server, url) = start_server(authority.clone(), broadcaster.clone()).await;
    let (_response, mut socket) = awc::Client::new()
        .ws(url)
        .connect()
        .await
        .expect("connect WebSocket");
    authenticate(&mut socket, &token).await;

    authority.revoke(&claims).expect("revoke credential");
    broadcaster.broadcast(COOP_ID, protected_event()).await;

    let response = next_text(&mut socket).await;
    assert!(
        response.contains("\"type\":\"AuthError\""),
        "revoked socket must close before event delivery: {response}"
    );
    assert!(!response.contains("\"type\":\"Event\""));
    server.stop(true).await;
}

#[actix_web::test]
async fn revoked_connection_receives_no_backfill() {
    let (authority, token, claims) = authority_and_token(Duration::from_secs(3600));
    let broadcaster = Arc::new(EventBroadcaster::new());
    broadcaster.broadcast(COOP_ID, protected_event()).await;
    let (server, url) = start_server(authority.clone(), broadcaster).await;
    let (_response, mut socket) = awc::Client::new()
        .ws(url)
        .connect()
        .await
        .expect("connect WebSocket");
    authenticate(&mut socket, &token).await;

    authority.revoke(&claims).expect("revoke credential");
    socket
        .send(Message::Text(
            serde_json::json!({ "type": "Backfill", "after_seq": 0 })
                .to_string()
                .into(),
        ))
        .await
        .expect("send backfill request");

    let response = next_text(&mut socket).await;
    assert!(
        response.contains("\"type\":\"AuthError\""),
        "revoked socket must close before backfill delivery: {response}"
    );
    assert!(!response.contains("\"type\":\"Event\""));
    server.stop(true).await;
}

#[actix_web::test]
async fn expired_connection_receives_no_live_event() {
    let (authority, token, _claims) = authority_and_token(Duration::from_secs(1));
    let broadcaster = Arc::new(EventBroadcaster::new());
    let (server, url) = start_server(authority, broadcaster.clone()).await;
    let (_response, mut socket) = awc::Client::new()
        .ws(url)
        .connect()
        .await
        .expect("connect WebSocket");
    authenticate(&mut socket, &token).await;

    tokio::time::sleep(Duration::from_secs(2)).await;
    broadcaster.broadcast(COOP_ID, protected_event()).await;

    let response = next_text(&mut socket).await;
    assert!(
        response.contains("\"type\":\"AuthError\""),
        "expired socket must close before event delivery: {response}"
    );
    assert!(!response.contains("\"type\":\"Event\""));
    server.stop(true).await;
}
