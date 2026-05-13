use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use futures_util::{SinkExt, StreamExt};
use crate::crdt::{RGA, Op, OpBatch};
use crate::network::message::Message;


pub struct Client {
    pub rga: Arc<Mutex<RGA>>,
    /// Ops typed locally but not yet sent to the server.
    pending_ops: Arc<Mutex<Vec<Op>>>,
    /// How long to wait before flushing pending ops (milliseconds).
    delay_ms: Arc<Mutex<u64>>,
    pub replica_id: u64,
}

impl Client {
    pub fn new(replica_id: u64, delay_ms: u64) -> Self {
        Self {
            rga: Arc::new(Mutex::new(RGA::new(replica_id))),
            pending_ops: Arc::new(Mutex::new(Vec::new())),
            delay_ms: Arc::new(Mutex::new(delay_ms)),
            replica_id,
        }
    }

    // Local editing — called when the user types
    pub async fn insert(&self, after_index: Option<usize>, value: char) {
        let op = {
            let mut rga = self.rga.lock().await;
            rga.local_insert(after_index, value)
        };
        self.pending_ops.lock().await.push(op);
    }

    /// Applied immediately to the local RGA; queued for delayed sending.
    pub async fn delete(&self, index: usize) {
        let op = {
            let mut rga = self.rga.lock().await;
            rga.local_delete(index)
        };
        self.pending_ops.lock().await.push(op);
    }

    /// Returns the current visible document text.
    pub async fn text(&self) -> String {
        self.rga.lock().await.text()
    }

    /// Update the sync delay. Takes effect on the next flush tick.
    pub async fn set_delay(&self, delay_ms: u64) {
        *self.delay_ms.lock().await = delay_ms;
        println!("Replica {}: sync delay set to {}ms", self.replica_id, delay_ms);
    }

    // Networking — connect and run the sync loop
    pub async fn run(&self, server_url: &str) -> anyhow::Result<()> {
        let (ws_stream, _) = connect_async(server_url).await?;
        println!("Replica {} connected to {}", self.replica_id, server_url);

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        // Send Join
        let join = Message::Join { replica_id: self.replica_id }.to_json()?;
        ws_tx.send(WsMessage::Text(join)).await?;

        // Wait for Welcome and replay history
        loop {
            match ws_rx.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    match Message::from_json(&text)? {
                        Message::Welcome { history, peer_count } => {
                            println!(
                                "Replica {}: received history ({} ops, {} peers)",
                                self.replica_id,
                                history.len(),
                                peer_count
                            );
                            let mut rga = self.rga.lock().await;
                            for op in history {
                                rga.apply(&op);
                            }
                            break;
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            }
        }

        // Shared handles for the two concurrent tasks below
        let rga = Arc::clone(&self.rga);
        let pending_ops = Arc::clone(&self.pending_ops);
        let delay_ms = Arc::clone(&self.delay_ms);
        let replica_id = self.replica_id;

        let ws_tx = Arc::new(Mutex::new(ws_tx));
        let ws_tx_flush = Arc::clone(&ws_tx);

        // Task 1: receive inbound ops from server, apply to local RGA
        let receive_task = tokio::spawn(async move {
            while let Some(msg) = ws_rx.next().await {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        match Message::from_json(&text) {
                            Ok(Message::Ops(batch)) => {
                                let mut rga = rga.lock().await;
                                for op in &batch.ops {
                                    rga.apply(op);
                                }
                                println!(
                                    "Replica {}: applied {} ops from replica {}",
                                    replica_id,
                                    batch.ops.len(),
                                    batch.from_replica
                                );
                            }
                            Ok(Message::PeerJoined { replica_id: id }) => {
                                println!("Replica {}: peer {} joined", replica_id, id);
                            }
                            Ok(Message::PeerLeft { replica_id: id }) => {
                                println!("Replica {}: peer {} left", replica_id, id);
                            }
                            Ok(_) => {}
                            Err(e) => eprintln!("Replica {}: parse error: {}", replica_id, e),
                        }
                    }
                    Ok(WsMessage::Close(_)) => break,
                    _ => {}
                }
            }
        });

        // Task 2: flush pending ops to server every `delay_ms` milliseconds
        let flush_task = tokio::spawn(async move {
            // Poll frequently; actual flush only happens when delay has elapsed
            let mut ticker = interval(Duration::from_millis(50));
            let mut elapsed_ms: u64 = 0;

            loop {
                ticker.tick().await;
                elapsed_ms += 50;

                let current_delay = *delay_ms.lock().await;

                if elapsed_ms >= current_delay {
                    elapsed_ms = 0;

                    let ops: Vec<Op> = {
                        let mut pending = pending_ops.lock().await;
                        std::mem::take(&mut *pending)
                    };

                    if ops.is_empty() {
                        continue;
                    }

                    let batch = OpBatch::new(replica_id, ops);
                    let msg = match Message::Ops(batch).to_json() {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!("Replica {}: serialize error: {}", replica_id, e);
                            continue;
                        }
                    };

                    let mut tx = ws_tx_flush.lock().await;
                    if let Err(e) = tx.send(WsMessage::Text(msg)).await {
                        eprintln!("Replica {}: send error: {}", replica_id, e);
                        break;
                    }

                    println!("Replica {}: flushed ops to server", replica_id);
                }
            }
        });

        // Run both tasks; stop if either finishes
        tokio::select! {
            _ = receive_task => {}
            _ = flush_task => {}
        }

        Ok(())
    }
}