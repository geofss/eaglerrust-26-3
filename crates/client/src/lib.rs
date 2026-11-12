use wasm_bindgen::prelude::*;

pub mod app;
pub mod block;
pub mod audio;
pub mod input;
pub mod interaction;
pub mod inventory;
pub mod network;
pub mod physics;
pub mod player;
pub mod raycast;
pub mod render;
pub mod storage;
pub mod world;

/// Boot the browser-side client shell.
#[wasm_bindgen]
pub fn boot() -> Result<(), JsValue> {
    web_sys::console::log_1(&JsValue::from_str(
        "EaglerRust 26.3: Rust/WASM client shell initialized",
    ));
    Ok(())
}

/// Start the WebGL renderer on the canvas in the page.
#[wasm_bindgen]
pub fn start_renderer(canvas_id: &str) -> Result<(), JsValue> {
    render::start_renderer(canvas_id)
}

/// Keep the WebGL drawing buffer in sync with the displayed canvas.
#[wasm_bindgen]
pub fn resize_renderer(canvas_id: &str) -> Result<(), JsValue> {
    render::resize_renderer(canvas_id)
}

/// Set or clear a keyboard key for the Rust camera controller.
#[wasm_bindgen]
pub fn set_key(key: &str, pressed: bool) {
    render::set_key(key, pressed);
}

/// Apply pointer/touch movement to the Rust camera.
#[wasm_bindgen]
pub fn look(delta_x: f32, delta_y: f32) {
    render::look(delta_x, delta_y);
}

/// Draw one frame using the current player camera.
#[wasm_bindgen]
pub fn draw_frame() -> Result<(), JsValue> {
    render::draw_frame()
}
