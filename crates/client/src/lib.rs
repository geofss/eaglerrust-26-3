use wasm_bindgen::prelude::*;

pub mod app;
pub mod audio;
pub mod input;
pub mod network;
pub mod render;
pub mod storage;
pub mod world;

/// Browser-side Rust/WASM entry point.
/// This initializes the shell only; it does not start a playable game yet.
#[wasm_bindgen]
pub fn boot() -> Result<(), JsValue> {
    web_sys::console::log_1(&JsValue::from_str(
        "EaglerRust 26.3: Rust/WASM client shell initialized",
    ));
    Ok(())
}
