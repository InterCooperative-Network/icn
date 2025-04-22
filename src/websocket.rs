use crate::sync::{Notification, NotificationType, SyncManager};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Error as WsError, WebSocketStream};

/// WebSocket server errors
#[derive(Debug, Error)]
pub enum WebSocketError {
    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] WsError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Tokio error: {0}")]
    TokioError(String),
    
    #[error("Server error: {0}")]
    ServerError(String),
}

/// Configuration for the WebSocket server
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub host: String,
    pub port: u16,
    pub ping_interval: u64, // seconds
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9876,
            ping_interval: 30,
        }
    }
}

type WebSocketSink = futures_util::stream::SplitSink<WebSocketStream<TcpStream>, tokio_tungstenite::tungstenite::Message>;

/// Server for WebSocket connections to provide real-time updates
pub struct WebSocketServer {
    sync_manager: Arc<Mutex<SyncManager>>,
    config: WebSocketConfig,
    running: Arc<Mutex<bool>>,
    connections: Arc<Mutex<Vec<mpsc::UnboundedSender<Notification>>>>,
}

impl WebSocketServer {
    /// Create a new WebSocket server
    pub fn new(
        sync_manager: SyncManager,
        config: Option<WebSocketConfig>,
    ) -> Self {
        Self {
            sync_manager: Arc::new(Mutex::new(sync_manager)),
            config: config.unwrap_or_default(),
            running: Arc::new(Mutex::new(false)),
            connections: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Start the WebSocket server
    pub fn start(&self) -> Result<(), WebSocketError> {
        // Set running flag
        let mut running = self.running.lock().unwrap();
        if *running {
            return Ok(());
        }
        *running = true;
        drop(running);
        
        // Clone required data for async runtime
        let running = self.running.clone();
        let connections = self.connections.clone();
        let sync_manager = self.sync_manager.clone();
        let config = self.config.clone();
        
        // Start notification broadcast thread
        let notification_connections = self.connections.clone();
        let notification_running = self.running.clone();
        let notification_sync_manager = self.sync_manager.clone();
        
        thread::spawn(move || {
            while *notification_running.lock().unwrap() {
                // Check for new notifications
                let notification = {
                    let sync_manager = notification_sync_manager.lock().unwrap();
                    sync_manager.next_notification()
                };
                
                // If there's a notification, broadcast it to all clients
                if let Some(notification) = notification {
                    let connections = notification_connections.lock().unwrap();
                    let notification_json = match serde_json::to_string(&notification) {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!("Failed to serialize notification: {}", e);
                            continue;
                        }
                    };
                    
                    // Send to all connected clients
                    for conn in connections.iter() {
                        let _ = conn.send(notification.clone());
                    }
                }
                
                // Short sleep to avoid spinning
                thread::sleep(Duration::from_millis(100));
            }
        });
        
        // Start WebSocket server in a new thread with its own async runtime
        thread::spawn(move || {
            // Create a new tokio runtime
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("Failed to create tokio runtime: {}", e);
                    return;
                }
            };
            
            // Run the async WebSocket server
            runtime.block_on(async move {
                // Create the address to bind to
                let addr = format!("{}:{}", config.host, config.port);
                let socket_addr: SocketAddr = match addr.parse() {
                    Ok(addr) => addr,
                    Err(e) => {
                        eprintln!("Failed to parse WebSocket address: {}", e);
                        return;
                    }
                };
                
                // Create the TCP listener
                let listener = match TcpListener::bind(&socket_addr).await {
                    Ok(listener) => listener,
                    Err(e) => {
                        eprintln!("Failed to bind WebSocket listener: {}", e);
                        return;
                    }
                };
                
                println!("WebSocket server listening on: {}", socket_addr);
                
                // Accept incoming connections
                while *running.lock().unwrap() {
                    let accept_connections = connections.clone();
                    let accept_running = running.clone();
                    let accept_config = config.clone();
                    
                    // Accept the next connection
                    let (socket, addr) = match listener.accept().await {
                        Ok(res) => res,
                        Err(e) => {
                            eprintln!("Failed to accept connection: {}", e);
                            continue;
                        }
                    };
                    
                    println!("New WebSocket connection from: {}", addr);
                    
                    // Spawn a new task to handle this connection
                    tokio::spawn(async move {
                        Self::handle_connection(
                            socket,
                            addr,
                            accept_connections,
                            accept_running,
                            accept_config,
                        ).await;
                    });
                }
            });
        });
        
        Ok(())
    }
    
    /// Stop the WebSocket server
    pub fn stop(&self) {
        let mut running = self.running.lock().unwrap();
        *running = false;
    }
    
    /// Handle a client connection
    async fn handle_connection(
        socket: TcpStream,
        addr: SocketAddr,
        connections: Arc<Mutex<Vec<mpsc::UnboundedSender<Notification>>>>,
        running: Arc<Mutex<bool>>,
        config: WebSocketConfig,
    ) {
        // Create a channel for sending notifications to this connection
        let (tx, mut rx) = mpsc::unbounded_channel::<Notification>();
        
        // Add this connection to the list of connections
        {
            let mut connections = connections.lock().unwrap();
            connections.push(tx);
        }
        
        // Accept the WebSocket connection
        let ws_stream = match accept_async(socket).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("Failed to accept WebSocket connection: {}", e);
                return;
            }
        };
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        // Create a task for sending messages to the client
        let mut send_task = tokio::spawn(async move {
            // Setup ping interval
            let mut ping_interval = tokio::time::interval(Duration::from_secs(config.ping_interval));
            
            loop {
                tokio::select! {
                    // Handle notifications to send to client
                    Some(notification) = rx.recv() => {
                        // Serialize notification to JSON
                        match serde_json::to_string(&notification) {
                            Ok(json) => {
                                // Send the message
                                if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(json)).await {
                                    eprintln!("Failed to send message: {}", e);
                                    break;
                                }
                            },
                            Err(e) => {
                                eprintln!("Failed to serialize notification: {}", e);
                                continue;
                            }
                        }
                    },
                    
                    // Send ping to keep connection alive
                    _ = ping_interval.tick() => {
                        if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Ping(vec![])).await {
                            eprintln!("Failed to send ping: {}", e);
                            break;
                        }
                    },
                    
                    // Exit if server is no longer running
                    else => {
                        if !*running.lock().unwrap() {
                            break;
                        }
                    }
                }
            }
        });
        
        // Create a task for receiving messages from the client
        let recv_task = tokio::spawn(async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        // Handle client messages - currently just logging as this is read-only
                        println!("Received message from {}: {}", addr, text);
                    },
                    Ok(tokio_tungstenite::tungstenite::Message::Pong(_)) => {
                        // Handle pong response
                    },
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                        // Client is closing the connection
                        break;
                    },
                    Err(e) => {
                        eprintln!("Error receiving message: {}", e);
                        break;
                    },
                    _ => {}
                }
            }
        });
        
        // Wait for either task to complete
        tokio::select! {
            _ = &mut send_task => {
                println!("Send task completed for {}", addr);
                recv_task.abort();
            },
            _ = recv_task => {
                println!("Receive task completed for {}", addr);
                send_task.abort();
            }
        }
        
        println!("WebSocket connection closed: {}", addr);
        
        // Remove this connection from the list
        let mut connections = connections.lock().unwrap();
        // We don't have a direct reference to the sender, so we need to find it by filtering out errors
        connections.retain(|sender| !sender.is_closed());
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add tests for WebSocket functionality
} 