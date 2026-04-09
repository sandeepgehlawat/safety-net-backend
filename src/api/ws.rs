use crate::data::models::WsMessage;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// WebSocket connection state
#[derive(Clone)]
pub struct WsState {
    /// Broadcast channel for all clients
    pub broadcast_tx: broadcast::Sender<WsMessage>,

    /// User-specific channels
    pub user_channels: Arc<RwLock<HashMap<Uuid, broadcast::Sender<WsMessage>>>>,
}

impl WsState {
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);
        Self {
            broadcast_tx,
            user_channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Broadcast message to all connected clients
    pub fn broadcast(&self, msg: WsMessage) {
        let _ = self.broadcast_tx.send(msg);
    }

    /// Send message to a specific user
    pub async fn send_to_user(&self, user_id: Uuid, msg: WsMessage) {
        let channels = self.user_channels.read().await;
        if let Some(tx) = channels.get(&user_id) {
            let _ = tx.send(msg);
        }
    }

    /// Register a user channel
    pub async fn register_user(&self, user_id: Uuid) -> broadcast::Receiver<WsMessage> {
        let mut channels = self.user_channels.write().await;
        let entry = channels.entry(user_id).or_insert_with(|| {
            let (tx, _) = broadcast::channel(256);
            tx
        });
        entry.subscribe()
    }

    /// Unregister a user channel if no more subscribers
    pub async fn unregister_user(&self, user_id: Uuid) {
        let mut channels = self.user_channels.write().await;
        if let Some(tx) = channels.get(&user_id) {
            if tx.receiver_count() == 0 {
                channels.remove(&user_id);
            }
        }
    }
}

impl Default for WsState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
}

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<WsState>>,
    State(redis): State<redis::aio::ConnectionManager>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, redis, query.token))
}

/// Handle individual WebSocket connection
pub async fn handle_socket(
    socket: WebSocket,
    state: Arc<WsState>,
    mut redis: redis::aio::ConnectionManager,
    token: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();

    // Authenticate if token provided
    let user_id = if let Some(token) = token {
        match crate::api::auth::get_session(&mut redis, &token).await {
            Ok(session) => Some(session.user_id),
            Err(e) => {
                warn!("WebSocket auth failed: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Subscribe to broadcast channel
    let mut broadcast_rx = state.broadcast_tx.subscribe();

    // Subscribe to user-specific channel if authenticated
    let _user_rx = if let Some(uid) = user_id {
        Some(state.register_user(uid).await)
    } else {
        None
    };

    info!("WebSocket connected, user: {:?}", user_id);

    // Spawn task to forward broadcast messages
    let send_state = state.clone();
    let send_user_id = user_id;
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Broadcast messages
                msg = broadcast_rx.recv() => {
                    match msg {
                        Ok(ws_msg) => {
                            let json = serde_json::to_string(&ws_msg).unwrap_or_default();
                            if sender.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("WebSocket lagged by {} messages", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }

        // Cleanup
        if let Some(uid) = send_user_id {
            send_state.unregister_user(uid).await;
        }
    });

    // Handle incoming messages (for future client -> server communication)
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Handle client messages (e.g., subscribe to specific positions)
                handle_client_message(&text, user_id, &state).await;
            }
            Ok(Message::Ping(data)) => {
                // Pong is handled automatically by axum
                let _ = data;
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Cancel send task
    send_task.abort();

    info!("WebSocket disconnected, user: {:?}", user_id);
}

/// Handle messages from client
async fn handle_client_message(text: &str, user_id: Option<Uuid>, _state: &WsState) {
    #[derive(Deserialize)]
    #[serde(tag = "type")]
    enum ClientMessage {
        #[serde(rename = "subscribe_position")]
        SubscribePosition { position_id: String },
        #[serde(rename = "unsubscribe_position")]
        UnsubscribePosition { position_id: String },
        #[serde(rename = "ping")]
        Ping,
    }

    match serde_json::from_str::<ClientMessage>(text) {
        Ok(msg) => {
            match msg {
                ClientMessage::SubscribePosition { position_id } => {
                    info!("User {:?} subscribed to position {}", user_id, position_id);
                    // In production, track subscriptions for targeted updates
                }
                ClientMessage::UnsubscribePosition { position_id } => {
                    info!("User {:?} unsubscribed from position {}", user_id, position_id);
                }
                ClientMessage::Ping => {
                    // Client ping, could send pong
                }
            }
        }
        Err(e) => {
            warn!("Invalid client message: {}", e);
        }
    }
}

/// Broadcast helpers for different message types
impl WsState {
    pub fn broadcast_position_update(
        &self,
        position_id: Uuid,
        position_type: &str,
        health_factor: Option<f64>,
        in_range: Option<bool>,
        block_number: u64,
    ) {
        self.broadcast(WsMessage::PositionUpdate {
            position_id: position_id.to_string(),
            position_type: position_type.to_string(),
            health_factor,
            in_range,
            block_number,
        });
    }

    pub fn broadcast_block_processed(
        &self,
        block_number: u64,
        latency_ms: u64,
        positions_checked: u32,
    ) {
        self.broadcast(WsMessage::BlockProcessed {
            block_number,
            latency_ms,
            positions_checked,
        });
    }

    pub fn broadcast_token_update(
        &self,
        symbol: &str,
        price_usd: f64,
        change_pct: f64,
    ) {
        let status = if change_pct >= 0.0 {
            "ok"
        } else if change_pct > -10.0 {
            "ok"
        } else if change_pct > -20.0 {
            "warn"
        } else {
            "bad"
        };

        self.broadcast(WsMessage::TokenUpdate {
            symbol: symbol.to_string(),
            price_usd,
            change_pct,
            status: status.to_string(),
        });
    }

    pub fn broadcast_ticker_event(&self, event_type: &str, message: &str) {
        self.broadcast(WsMessage::TickerEvent {
            event_type: event_type.to_string(),
            message: message.to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        });
    }

    pub async fn send_alert_to_user(
        &self,
        user_id: Uuid,
        alert_id: Uuid,
        position_id: Uuid,
        alert_type: &str,
        current_value: f64,
        threshold: f64,
        suggested_action: Option<crate::data::models::SuggestedAction>,
    ) {
        self.send_to_user(user_id, WsMessage::AlertFired {
            alert_id: alert_id.to_string(),
            position_id: position_id.to_string(),
            alert_type: alert_type.to_string(),
            current_value,
            threshold,
            suggested_action,
        }).await;
    }

    pub async fn send_tx_status_to_user(
        &self,
        user_id: Uuid,
        tx_id: Uuid,
        status: &str,
        tx_hash: Option<String>,
        confirmed_at: Option<u64>,
    ) {
        self.send_to_user(user_id, WsMessage::TxStatus {
            tx_id: tx_id.to_string(),
            status: status.to_string(),
            tx_hash,
            confirmed_at,
        }).await;
    }

    /// Broadcast alert notification to a specific user via WebSocket
    pub fn broadcast_alert(&self, user_id: Uuid, title: &str) {
        // Send via broadcast channel - clients filter by user_id
        self.broadcast(WsMessage::TickerEvent {
            event_type: "alert".to_string(),
            message: format!("{}:{}", user_id, title),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        });
    }
}
