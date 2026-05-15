use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Greeting {
    pub message: String,
}

pub fn build_greeting(name: &str) -> Greeting {
    Greeting {
        message: format!("Hello, {name}!"),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn greet(name: &str) -> String {
    build_greeting(name).message
}
