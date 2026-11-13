use std::cell::RefCell;
use wasm_bindgen::prelude::*;

use crate::inventory::Hotbar;
use crate::world::BlockStateId;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static REFERENCE_CONNECTION: RefCell<Option<network::BrowserProtocolConnection>> = const { RefCell::new(None) };
}

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
pub mod atlas;
pub mod block;
pub mod chunk;
pub mod chunk_codec;
pub mod audio;
pub mod input;
pub mod interaction;
pub mod inventory;
pub mod network;
pub mod physics;
pub mod packet;
pub mod protocol;
pub mod reference_play;
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



/// Open a browser WebSocket and begin the audited 26.1.2 reference login.
/// This is a development connector; do not assume 26.3 compatibility.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn connect_reference_server(
    websocket_url: &str,
    host: &str,
    port: u16,
    username: &str,
    uuid_hex: &str,
) -> Result<(), JsValue> {
    if uuid_hex.len() != 32 || !uuid_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(JsValue::from_str("UUID must contain exactly 32 hexadecimal characters"));
    }
    let mut uuid = [0_u8; 16];
    for (index, byte) in uuid.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&uuid_hex[start..start + 2], 16)
            .map_err(|_| JsValue::from_str("Invalid UUID hex"))?;
    }
    let connection = network::BrowserProtocolConnection::connect(
        websocket_url, host, port, username, uuid,
    )?;
    REFERENCE_CONNECTION.with(|slot| *slot.borrow_mut() = Some(connection));
    Ok(())
}

/// Pump incoming WebSocket messages and deliver decoded play packets to the renderer.
/// Call this from the browser animation loop.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn tick_reference_network() -> Result<u32, JsValue> {
    let mut processed = 0_u32;
    REFERENCE_CONNECTION.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(connection) = borrowed.as_mut() else {
            return Err(JsValue::from_str("No reference server connection has been started"));
        };
        let events = connection.tick()?;
        for event in events {
            match event {
                protocol::ProtocolEvent::PlayPacket { packet_id, payload } => {
                    render::apply_reference_play_packet(packet_id, &payload)?;
                    processed = processed.saturating_add(1);
                }
                protocol::ProtocolEvent::ServerDisconnect(payload) => {
                    web_sys::console::warn_1(&JsValue::from_str(&format!(
                        "Reference server disconnected: {} bytes in reason payload", payload.len()
                    )));
                }
                protocol::ProtocolEvent::LoginSuccess(_) |
                protocol::ProtocolEvent::LoginAcknowledged |
                protocol::ProtocolEvent::ConfigurationPacket { .. } |
                protocol::ProtocolEvent::ConfigurationFinished => {}
                protocol::ProtocolEvent::Send(_) => {}
            }
        }
        Ok(())
    })?;
    Ok(processed)
}

/// Return the browser WebSocket state for the active reference connection.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn reference_connection_state() -> String {
    REFERENCE_CONNECTION.with(|slot| {
        let borrowed = slot.borrow();
        match borrowed.as_ref() {
            None => "disconnected".to_owned(),
            Some(connection) => format!("{:?}", connection.transport_state()).to_lowercase(),
        }
    })
}

/// Close and clear the active reference server connection.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn disconnect_reference_server() {
    REFERENCE_CONNECTION.with(|slot| {
        if let Some(connection) = slot.borrow_mut().take() {
            let _ = connection.close();
        }
    });
}

/// Apply a single complete 26.1.2 reference play packet to the live Rust world.
/// Packet IDs and layouts are reference-only pending 26.3 verification.
#[wasm_bindgen]
pub fn apply_reference_play_packet(packet_id: i32, payload: &[u8]) -> Result<(), JsValue> {
    render::apply_reference_play_packet(packet_id, payload)
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
