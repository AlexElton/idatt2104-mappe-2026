//! WebAssembly bindings for the RGA core.
//!
//! This module is only compiled for `wasm32`. It exposes [`WasmReplica`] as
//! `RawReplica` to JavaScript and converts all operations and data structures
//! through JSON-compatible `JsValue`s.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::{ApplyOutcome, Op, Replica};

/// Installs a panic hook so Rust panics show useful messages in the browser.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The RGA replica exposed to JavaScript as `RawReplica`.
///
/// Wraps [`Replica`] and serializes all ops to and from JSON on every boundary
/// crossing. `localInsert` returns `undefined` when `value` is not exactly one
/// character or when `pos` is out of range.
#[wasm_bindgen(js_name = RawReplica)]
pub struct WasmReplica {
    inner: Replica,
}

#[wasm_bindgen(js_class = RawReplica)]
impl WasmReplica {
    /// Creates an empty replica for JavaScript.
    #[wasm_bindgen(constructor)]
    pub fn new(replica_id: String, session_id: String) -> Self {
        Self {
            inner: Replica::new(replica_id, session_id),
        }
    }

    /// Applies a local insert and returns the operation to broadcast.
    #[wasm_bindgen(js_name = localInsert)]
    pub fn local_insert(&mut self, pos: usize, value: &str) -> JsValue {
        let mut chars = value.chars();
        let Some(ch) = chars.next() else {
            return JsValue::UNDEFINED;
        };
        if chars.next().is_some() {
            return JsValue::UNDEFINED;
        }

        self.inner
            .local_insert(pos, ch)
            .map_or(JsValue::UNDEFINED, |op| to_js(&op))
    }

    /// Applies a local delete and returns the operation to broadcast.
    #[wasm_bindgen(js_name = localDelete)]
    pub fn local_delete(&mut self, pos: usize) -> JsValue {
        self.inner
            .local_delete(pos)
            .map_or(JsValue::UNDEFINED, |op| to_js(&op))
    }

    /// Applies one remote operation and returns its outcome string.
    #[wasm_bindgen(js_name = applyRemote)]
    pub fn apply_remote(&mut self, op: JsValue) -> String {
        let Ok(op) = serde_wasm_bindgen::from_value::<Op>(op) else {
            return ApplyOutcome::Invalid.as_str().to_string();
        };

        self.inner.apply_remote(op).as_str().to_string()
    }

    /// Applies remote operations in order and returns one outcome per op.
    #[wasm_bindgen(js_name = applyRemoteBatch)]
    pub fn apply_remote_batch(&mut self, ops: JsValue) -> JsValue {
        let Ok(ops) = serde_wasm_bindgen::from_value::<Vec<Op>>(ops) else {
            return to_js(&[ApplyOutcome::Invalid.as_str()]);
        };

        let outcomes: Vec<&'static str> = self
            .inner
            .apply_remote_batch(ops)
            .into_iter()
            .map(ApplyOutcome::as_str)
            .collect();
        to_js(&outcomes)
    }

    /// Returns the current visible text.
    pub fn text(&self) -> String {
        self.inner.text()
    }

    /// Returns operations needed to hydrate another replica.
    #[wasm_bindgen(js_name = hydrationOps)]
    pub fn hydration_ops(&self) -> JsValue {
        to_js(&self.inner.hydration_ops())
    }

    /// Returns the full RGA tree snapshot for debugging UI.
    #[wasm_bindgen(js_name = rgaTree)]
    pub fn rga_tree(&self) -> JsValue {
        to_js(&self.inner.rga_tree())
    }

    /// Removes tombstones locally and returns the number removed.
    #[wasm_bindgen(js_name = clearDeletedNodes)]
    pub fn clear_deleted_nodes(&mut self) -> usize {
        self.inner.clear_deleted_nodes()
    }
}

fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::UNDEFINED)
}
