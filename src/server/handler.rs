use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use crate::server::registry::{ClientOp, Registry, Rx};

/// Client → server: a batch of position-based ops sent on each sync interval.
#[derive(Debug, Deserialize)]
struct ClientOpsMsg {
    #[serde(rename = "type")]
    msg_type: String,
    ops: Vec<ClientOpJson>,
}

#[derive(Debug, Deserialize)]
struct ClientOpJson {
    op:  String,
    pos: usize,
    #[serde(rename = "char")]
    ch:  Option<char>,
}

/// Server → client: sent once on connect.
#[derive(Serialize)]
struct InitMsg<'a> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    site_id:  u64,
    text:     &'a str,
}

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

async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let site_id = state.assign_site_id();
    let (rx, current_text) = state.registry.connect(site_id).await;
    let (mut sink, mut stream) = socket.split();

    // Send init message immediately
    let init_json = serde_json::to_string(&InitMsg {
        msg_type: "init",
        site_id,
        text: &current_text,
    })
    .expect("serialize InitMsg");

    if sink.send(WsMessage::Text(init_json.into())).await.is_err() {
        state.registry.disconnect(site_id).await;
        return;
    }

    // Outbound task: forward JSON strings from the registry channel to this WebSocket.
    let mut send_task = tokio::spawn(async move {
        let mut rx: Rx = rx;
        while let Some(json) = rx.recv().await {
            if sink.send(WsMessage::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Inbound task: receive op batches from this client, run the CRDT sync algorithm.
    let registry = state.registry.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = stream.next().await {
            let text = match msg {
                Ok(WsMessage::Text(t)) => t,
                Ok(WsMessage::Close(_)) | Err(_) => break,
                _ => continue,
            };

            let parsed: ClientOpsMsg = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[WARN] bad message from site {}: {}", site_id, e);
                    continue;
                }
            };

            if parsed.msg_type != "ops" {
                eprintln!("[WARN] unknown msg type '{}' from site {}", parsed.msg_type, site_id);
                continue;
            }

            let ops: Vec<ClientOp> = parsed
                .ops
                .into_iter()
                .map(|o| ClientOp { op: o.op, pos: o.pos, ch: o.ch })
                .collect();

            registry.process_sync(site_id, ops).await;
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    state.registry.disconnect(site_id).await;
}
