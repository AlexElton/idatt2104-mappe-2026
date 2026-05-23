//! Axum router and WebSocket connection handler.
//!
//! Clients connect to `/ws`. On upgrade, the handler sends a `Hydrate` message
//! with the current document state, then spawns two tasks: one that forwards
//! messages from the registry channel to the socket, and one that reads from
//! the socket and dispatches to the registry. Either task exiting causes the
//! other to be aborted and triggers a disconnect.
//!
//! `/api/health` returns `"ok"` for basic liveness checks.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use axum::{
    Router,
    extract::{
        State,
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};

use crate::server::registry::{ClientMsg, Registry, Rx};

/// Shared state cloned into each request handler by Axum.
///
/// `next_connection_id` is a monotonic counter; IDs are never reused after
/// disconnect.
#[derive(Clone)]
pub struct AppState {
    pub registry: Registry,
    pub next_connection_id: Arc<AtomicU64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            registry: Registry::new(),
            next_connection_id: Arc::new(AtomicU64::new(1)),
        }
    }

    fn assign_connection_id(&self) -> u64 {
        self.next_connection_id.fetch_add(1, Ordering::Relaxed)
    }
}

/// Builds the application router with `/api/health` and `/ws` routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let connection_id = state.assign_connection_id();
    let (rx, hydrate) = state.registry.connect(connection_id).await;
    let (mut sink, mut stream) = socket.split();

    let hydrate_json = serde_json::to_string(&hydrate).expect("serialize hydrate message");
    if sink.send(WsMessage::Text(hydrate_json)).await.is_err() {
        state.registry.disconnect(connection_id).await;
        return;
    }

    let mut send_task = tokio::spawn(async move {
        let mut rx: Rx = rx;
        while let Some(message) = rx.recv().await {
            let json = match serde_json::to_string(&message) {
                Ok(json) => json,
                Err(error) => {
                    eprintln!("[WARN] failed to serialize server message: {error}");
                    continue;
                }
            };

            if sink.send(WsMessage::Text(json)).await.is_err() {
                break;
            }
        }
    });

    let registry = state.registry.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = stream.next().await {
            let text = match msg {
                Ok(WsMessage::Text(text)) => text,
                Ok(WsMessage::Close(_)) | Err(_) => break,
                _ => continue,
            };

            let parsed: ClientMsg = match serde_json::from_str(&text) {
                Ok(message) => message,
                Err(error) => {
                    eprintln!("[WARN] bad message from connection {connection_id}: {error}");
                    continue;
                }
            };

            match parsed {
                ClientMsg::Hello {
                    replica_id,
                    session_id,
                } => {
                    registry
                        .set_identity(connection_id, replica_id, session_id)
                        .await;
                }
                ClientMsg::Ops { ops } => {
                    registry.process_ops(connection_id, ops).await;
                }
                ClientMsg::Presence { presence } => {
                    registry.update_presence(connection_id, presence).await;
                }
                ClientMsg::GarbageCollect => {
                    registry.garbage_collect(connection_id).await;
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    state.registry.disconnect(connection_id).await;
}
