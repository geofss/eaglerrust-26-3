use wasm_bindgen::prelude::*;

pub mod app;
pub mod audio;
pub mod input;
pub mod network;
pub mod physics;
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

/// Draw a single frame. The first milestone is a clean WebGL clear.
#[wasm_bindgen]
pub fn draw_frame() -> Result<(), JsValue> {
    render::draw_frame()
}
