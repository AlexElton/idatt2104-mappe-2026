use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};

use crate::{crdt::op::Message, server::registry::Registry};

/// Shared server state — cheap to clone (Arc inside Registry).
#[derive(Clone)]
pub struct AppState {
    pub registry:     Registry,
    pub next_site_id: Arc<AtomicU64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            registry:     Registry::new(),
            next_site_id: Arc::new(AtomicU64::new(1)),
        }
    }

    fn assign_site_id(&self) -> u64 {
        self.next_site_id.fetch_add(1, Ordering::Relaxed)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

/// Serve the static HTML frontend.
/// TODO: replace with tokio::fs::read_to_string("static/index.html") + axum::response::Html
async fn serve_index() -> &'static str {
    include_str!("../../static/index.html")
}

/// Upgrade the HTTP connection to a WebSocket.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// One task per connected client.
/// Reads incoming ops, broadcasts them; writes outbound ops from the registry channel.
async fn handle_socket(socket: WebSocket, state: AppState) {
    let site_id = state.assign_site_id();
    let rx = state.registry.connect(site_id).await;

    let (mut sink, mut stream) = socket.split();

    // Outbound task: forward messages from the registry channel to this WebSocket.
    let mut send_task = tokio::spawn(async move {
        todo!("loop: recv from rx, serialize to JSON, sink.send(WsMessage::Text(...))")
    });

    // Inbound task: receive ops from this client, broadcast to others.
    let registry = state.registry.clone();
    let mut recv_task = tokio::spawn(async move {
        todo!(
            "
            loop: stream.next() to get WsMessage::Text(text),
            serde_json::from_str::<Message>(&text),
            registry.broadcast(site_id, &msg).await
            "
        )
    });

    // If either task finishes (client disconnected or error), cancel the other.
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    state.registry.disconnect(site_id).await;
}
