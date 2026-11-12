use std::cell::RefCell;
use wasm_bindgen::prelude::*;

use crate::inventory::Hotbar;
use crate::world::BlockStateId;

thread_local! {
    static BROWSER_HOTBAR: RefCell<Hotbar> = RefCell::new({
        let mut hotbar = Hotbar::default();
        for state in [1_u32, 2, 3, 4, 5, 6, 7, 1, 2] {
            hotbar.add(BlockStateId(state), 64);
        }
        hotbar
    });
}

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

/// Break the block currently targeted by the player.
#[wasm_bindgen]
pub fn break_target() -> bool {
    render::break_target()
}

/// Place one item from the selected hotbar stack against the targeted face.
#[wasm_bindgen]
pub fn place_target() -> bool {
    let item = BROWSER_HOTBAR.with(|hotbar| {
        hotbar.borrow().selected().map(|stack| stack.item.0).unwrap_or(0)
    });
    if item == 0 || !render::place_target(item) {
        return false;
    }
    BROWSER_HOTBAR.with(|hotbar| hotbar.borrow_mut().consume_selected(1))
}

/// Return the prototype block ID currently targeted by the crosshair.
#[wasm_bindgen]
pub fn target_block_state() -> u32 {
    render::target_state()
}


/// Select one of the nine browser hotbar slots.
#[wasm_bindgen]
pub fn select_hotbar_slot(slot: u32) -> bool {
    BROWSER_HOTBAR.with(|hotbar| hotbar.borrow_mut().select(slot as usize))
}

/// Return the selected browser hotbar slot.
#[wasm_bindgen]
pub fn selected_hotbar_slot() -> u32 {
    BROWSER_HOTBAR.with(|hotbar| hotbar.borrow().selected_slot() as u32)
}

/// Return the prototype block-state ID in a hotbar slot (zero means empty).
#[wasm_bindgen]
pub fn hotbar_item(slot: u32) -> u32 {
    BROWSER_HOTBAR.with(|hotbar| hotbar.borrow().slot(slot as usize).map(|stack| stack.item.0).unwrap_or(0))
}

/// Return the stack count in a hotbar slot.
#[wasm_bindgen]
pub fn hotbar_count(slot: u32) -> u32 {
    BROWSER_HOTBAR.with(|hotbar| hotbar.borrow().slot(slot as usize).map(|stack| u32::from(stack.count)).unwrap_or(0))
}
