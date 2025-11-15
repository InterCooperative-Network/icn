//! WebSocket session management

use actix::{Actor, ActorContext, AsyncContext, StreamHandler};
use actix_web_actors::ws;
use std::time::{Duration, Instant};

use crate::error::Result;
use icn_identity::Did;

/// WebSocket session for a cooperative namespace
pub struct WsSession {
    /// Cooperative ID this session is subscribed to
    coop_id: String,
    /// Authenticated DID (set after authentication)
    did: Option<Did>,
    /// Last heartbeat timestamp
    last_heartbeat: Instant,
}

impl WsSession {
    /// Create a new WebSocket session
    pub fn new(coop_id: String) -> Self {
        Self {
            coop_id,
            did: None,
            last_heartbeat: Instant::now(),
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
        // Start heartbeat
        self.heartbeat(ctx);
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
                // Handle text messages (e.g., authentication, subscriptions)
                // TODO: Implement message handling
                ctx.text(format!("Echo: {}", text));
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
        let session = WsSession::new("test-coop".to_string());
        assert_eq!(session.coop_id, "test-coop");
        assert!(session.did.is_none());
    }
}
