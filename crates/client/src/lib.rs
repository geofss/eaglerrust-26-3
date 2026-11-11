use wasm_bindgen::prelude::*;

/// Browser-side Rust/WASM entry point. Gameplay systems are not ported yet.
#[wasm_bindgen]
pub fn boot() -> Result<(), JsValue> {
    web_sys::console::log_1(&JsValue::from_str("EaglerRust 26.3 WASM shell initialized"));
    Ok(())
}
