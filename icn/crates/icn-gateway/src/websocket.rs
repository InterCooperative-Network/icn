//! WebSocket session management

use actix::{Actor, ActorContext, ActorFutureExt, AsyncContext, StreamHandler, WrapFuture};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::auth::AuthManager;
use crate::events::{EventBroadcaster, GatewayEvent};
use icn_identity::Did;
use icn_obs::metrics::gateway;

/// Global counter for active WebSocket connections
static WS_ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);

/// WebSocket client message (from client to server)
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    /// Authenticate with JWT token
    Auth { token: String },
}

/// WebSocket server message (from server to client)
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ServerMessage {
    /// Authentication successful
    AuthOk { did: String },
    /// Authentication failed
    AuthError { message: String },
    /// Event notification
    Event(GatewayEvent),
    /// Error message
    Error { message: String },
}

/// WebSocket session for a cooperative namespace
pub struct WsSession {
    /// Cooperative ID this session is subscribed to
    coop_id: String,
    /// Authenticated DID (None until authenticated)
    did: Option<Did>,
    /// Last heartbeat timestamp
    last_heartbeat: Instant,
    /// Authentication manager
    auth_manager: Arc<AuthManager>,
    /// Event broadcaster
    event_broadcaster: Arc<EventBroadcaster>,
    /// Event receiver (subscribed after authentication)
    event_rx: Option<mpsc::UnboundedReceiver<GatewayEvent>>,
}

impl WsSession {
    /// Create a new WebSocket session
    pub fn new(
        coop_id: String,
        auth_manager: Arc<AuthManager>,
        event_broadcaster: Arc<EventBroadcaster>,
    ) -> Self {
        Self {
            coop_id,
            did: None,
            last_heartbeat: Instant::now(),
            auth_manager,
            event_broadcaster,
            event_rx: None,
        }
    }

    /// Send a server message, handling serialization errors gracefully
    fn send_message(&self, msg: ServerMessage, ctx: &mut <Self as Actor>::Context) {
        match serde_json::to_string(&msg) {
            Ok(json) => ctx.text(json),
            Err(e) => {
                tracing::error!("Failed to serialize WebSocket message: {}", e);
                // Close connection on serialization error (should never happen)
                ctx.stop();
            }
        }
    }

    /// Authenticate user with JWT token
    fn authenticate(&mut self, token: &str, ctx: &mut <Self as Actor>::Context) {
        match self.auth_manager.verify_token(token) {
            Ok(claims) => {
                // Verify token is for this cooperative
                if claims.coop_id != self.coop_id {
                    let msg = ServerMessage::AuthError {
                        message: "Token coop_id mismatch".to_string(),
                    };
                    self.send_message(msg, ctx);
                    return;
                }

                // Parse DID from claims
                match claims.sub.parse::<Did>() {
                    Ok(did) => {
                        self.did = Some(did.clone());

                        // Subscribe to events
                        let event_broadcaster = self.event_broadcaster.clone();
                        let coop_id = self.coop_id.clone();

                        let fut = async move {
                            event_broadcaster.subscribe(&coop_id).await
                        }
                        .into_actor(self)
                        .map(|rx_opt, act, ctx| {
                            match rx_opt {
                                Some(rx) => {
                                    act.event_rx = Some(rx);
                                    // Start polling for events
                                    act.poll_events(ctx);

                                    // Send success message
                                    let msg = ServerMessage::AuthOk {
                                        did: act.did.as_ref().unwrap().to_string(),
                                    };
                                    act.send_message(msg, ctx);
                                }
                                None => {
                                    // Subscriber limit reached
                                    let msg = ServerMessage::AuthError {
                                        message: "Subscriber limit reached for this cooperative".to_string(),
                                    };
                                    act.send_message(msg, ctx);
                                    ctx.stop();
                                }
                            }
                        });

                        ctx.wait(fut);
                    }
                    Err(_e) => {
                        // Generic error to prevent enumeration attacks
                        let msg = ServerMessage::AuthError {
                            message: "Authentication failed".to_string(),
                        };
                        self.send_message(msg, ctx);
                    }
                }
            }
            Err(_e) => {
                // Generic error to prevent enumeration attacks
                let msg = ServerMessage::AuthError {
                    message: "Authentication failed".to_string(),
                };
                self.send_message(msg, ctx);
            }
        }
    }

    /// Poll for events from the event broadcaster
    fn poll_events(&mut self, ctx: &mut <Self as Actor>::Context) {
        if let Some(ref mut rx) = self.event_rx {
            // Try to receive events
            match rx.try_recv() {
                Ok(event) => {
                    // Forward event to client
                    let msg = ServerMessage::Event(event);
                    if let Ok(json) = serde_json::to_string(&msg) {
                        ctx.text(json);
                        // Track message sent
                        gateway::websocket_messages_sent_inc();
                    }
                    // Continue polling
                    ctx.run_later(Duration::from_millis(100), |act, ctx| {
                        act.poll_events(ctx);
                    });
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No events yet, poll again later
                    ctx.run_later(Duration::from_millis(100), |act, ctx| {
                        act.poll_events(ctx);
                    });
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed - event broadcaster removed this coop or stopped
                    // Close the WebSocket connection to prevent zombie connections
                    tracing::warn!(
                        "Event channel disconnected for coop {}, closing WebSocket",
                        self.coop_id
                    );
                    ctx.stop();
                }
            }
        }
    }

    /// Send heartbeat ping to client
    fn heartbeat(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Duration::from_secs(30), |act, ctx| {
            // Check if client is still responsive
            if Instant::now().duration_since(act.last_heartbeat) > Duration::from_secs(60) {
                // Client hasn't sent pong in 60 seconds, disconnect
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Check global connection limit BEFORE incrementing
        let current_active = WS_ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
        if current_active >= crate::validation::MAX_TOTAL_WEBSOCKET_CONNECTIONS {
            tracing::warn!(
                "Global WebSocket connection limit reached: {} (max {})",
                current_active,
                crate::validation::MAX_TOTAL_WEBSOCKET_CONNECTIONS
            );
            // Send error and immediately close
            let msg = ServerMessage::Error {
                message: "Server connection limit reached".to_string(),
            };
            self.send_message(msg, ctx);
            ctx.stop();
            return;
        }

        // Track connection
        let active = WS_ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed) + 1;
        gateway::websocket_connections_inc();
        gateway::websocket_connections_active_set(active);

        // Start heartbeat
        self.heartbeat(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        // Track disconnection
        let active = WS_ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed) - 1;
        gateway::websocket_disconnections_inc();
        gateway::websocket_connections_active_set(active);
    }
}

impl StreamHandler<std::result::Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: std::result::Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.last_heartbeat = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.last_heartbeat = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                // Validate message size BEFORE parsing to prevent memory exhaustion
                if text.len() > crate::validation::MAX_WEBSOCKET_MESSAGE_SIZE {
                    tracing::warn!(
                        "WebSocket message too large: {} bytes (max {})",
                        text.len(),
                        crate::validation::MAX_WEBSOCKET_MESSAGE_SIZE
                    );
                    let msg = ServerMessage::Error {
                        message: "Message too large".to_string(),
                    };
                    self.send_message(msg, ctx);
                    // Close connection on oversized message (potential attack)
                    ctx.stop();
                    return;
                }

                // Parse client message
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Auth { token }) => {
                        self.authenticate(&token, ctx);
                    }
                    Err(e) => {
                        let msg = ServerMessage::Error {
                            message: format!("Invalid message format: {e}"),
                        };
                        self.send_message(msg, ctx);
                    }
                }
            }
            Ok(ws::Message::Binary(_)) => {
                // We don't handle binary messages currently
            }
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let auth = Arc::new(AuthManager::new(b"test_secret".to_vec()));
        let broadcaster = Arc::new(EventBroadcaster::new());

        let session = WsSession::new("test-coop".to_string(), auth, broadcaster);
        assert_eq!(session.coop_id, "test-coop");
        assert!(session.did.is_none());
    }

    #[test]
    fn test_server_message_serialization() {
        let msg = ServerMessage::AuthOk {
            did: "did:icn:test".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("AuthOk"));
        assert!(json.contains("did:icn:test"));
    }
}
