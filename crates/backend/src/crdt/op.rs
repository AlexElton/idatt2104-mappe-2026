use serde::{Deserialize, Serialize};

use rga_core::Op;

/// WebSocket wire envelope — wraps an Op with the originating site's ID.
/// The server stamps `site_id` before broadcasting so clients know the source.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub site_id: u64,
    pub op: Op,
}
