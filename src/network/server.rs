use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use futures_util::{SinkExt, StreamExt};
use crate::crdt::Op;
use crate::network::message::Message;


struct ServerState {
    /// All ops ever applied — sent to new clients as history on join.
    op_history: Vec<Op>,
    /// Maps replica_id -> sender handle for broadcasting to that client.
    peers: HashMap<u64, broadcast::Sender<String>>,
}

impl ServerState {
    fn new() -> Self {
        Self {
            op_history: Vec::new(),
            peers: HashMap::new(),
        }
    }
}

/// Start the WebSocket server on the given address (e.g. "127.0.0.1:8080").
pub async fn run(addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    println!("Server listening on ws://{}", addr);

    let state = Arc::new(Mutex::new(ServerState::new()));

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        println!("New TCP connection from {}", peer_addr);

        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, state).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}

/// Handle one WebSocket connection for its entire lifetime.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    state: Arc<Mutex<ServerState>>,
) -> anyhow::Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Step 1: wait for the Join message to learn this client's replica_id
    let replica_id = loop {
        match ws_rx.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                match Message::from_json(&text)? {
                    Message::Join { replica_id } => break replica_id,
                    _ => continue, // ignore anything before Join
                }
            }
            Some(Ok(WsMessage::Close(_))) | None => return Ok(()),
            _ => continue,
        }
    };

    println!("Replica {} joined", replica_id);

    // Step 2: send Welcome with full op history so client can reconstruct doc
    {
        let state = state.lock().await;
        let welcome = Message::Welcome {
            history: state.op_history.clone(),
            peer_count: state.peers.len(),
        };
        ws_tx.send(WsMessage::Text(welcome.to_json()?)).await?;
    }

    // Step 3: register this peer and notify others
    let (tx, mut rx) = broadcast::channel::<String>(256);
    {
        let mut state = state.lock().await;
        state.peers.insert(replica_id, tx.clone());

        let joined = Message::PeerJoined { replica_id }.to_json()?;
        broadcast_to_others(&state, replica_id, &joined);
    }

    // Step 4: run send + receive concurrently for this connection
    loop {
        tokio::select! {
            // Inbound: message from this client
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Err(e) = handle_message(&text, replica_id, &state).await {
                            eprintln!("Error handling message from {}: {}", replica_id, e);
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Ok(WsMessage::Ping(data))) => {
                        ws_tx.send(WsMessage::Pong(data)).await?;
                    }
                    _ => {}
                }
            }

            // Outbound: op batches to forward to this client
            outbound = rx.recv() => {
                match outbound {
                    Ok(json) => {
                        ws_tx.send(WsMessage::Text(json)).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("Replica {} lagged by {} messages", replica_id, n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    // Step 5: clean up on disconnect
    {
        let mut state = state.lock().await;
        state.peers.remove(&replica_id);
        let left = Message::PeerLeft { replica_id }.to_json()?;
        broadcast_to_others(&state, replica_id, &left);
    }

    println!("Replica {} disconnected", replica_id);
    Ok(())
}

/// Process one inbound message from a client.
async fn handle_message(
    text: &str,
    from_replica: u64,
    state: &Arc<Mutex<ServerState>>,
) -> anyhow::Result<()> {
    let msg = Message::from_json(text)?;

    match msg {
        Message::Ops(batch) => {
            let mut state = state.lock().await;

            // Store ops in history so future clients get them on join
            state.op_history.extend(batch.ops.clone());

            // Broadcast to every other connected peer
            let json = Message::Ops(batch).to_json()?;
            broadcast_to_others(&state, from_replica, &json);
        }

        // UpdateDelay is client-side only; server just acknowledges
        Message::UpdateDelay { .. } => {}

        _ => {
            eprintln!("Unexpected message from replica {}: {}", from_replica, text);
        }
    }

    Ok(())
}

/// Send a JSON string to every peer except the one with `exclude_replica`.
fn broadcast_to_others(state: &ServerState, exclude_replica: u64, json: &str) {
    for (id, tx) in &state.peers {
        if *id != exclude_replica {
            let _ = tx.send(json.to_string());
        }
    }
}