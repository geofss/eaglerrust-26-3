use std::cell::RefCell;
use wasm_bindgen::prelude::*;

use crate::inventory::Hotbar;
use crate::world::BlockStateId;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static REFERENCE_CONNECTION: RefCell<Option<network::BrowserProtocolConnection>> = const { RefCell::new(None) };
    static LAST_SENT_PLAYER_STATE: RefCell<Option<render::PlayerNetworkState>> = const { RefCell::new(None) };
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
pub mod integrated_server;
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
pub mod singleplayer;
pub mod world;

#[cfg(test)]
mod mock_server;

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

/// Set or clear the held primary break action.
#[wasm_bindgen]
pub fn set_breaking(held: bool) {
    render::set_breaking(held);
}

/// Return normalized mining progress from 0.0 to 1.0.
#[wasm_bindgen]
pub fn mining_progress() -> f32 {
    render::mining_progress()
}

/// Draw one frame using the current player camera.
#[wasm_bindgen]
pub fn draw_frame() -> Result<(), JsValue> {
    render::draw_frame()
}



#[cfg(target_arch = "wasm32")]
fn encode_reference_varint(value: i32) -> Vec<u8> {
    let mut value = value as u32;
    let mut bytes = Vec::with_capacity(5);
    while value & !0x7f != 0 {
        bytes.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}

fn reference_server_rotation(yaw: f32, pitch: f32) -> (f32, f32) {
    (yaw.to_degrees() + 180.0, -pitch.to_degrees())
}
#[cfg(target_arch = "wasm32")]
fn send_reference_movement(
    connection: &mut network::BrowserProtocolConnection,
) -> Result<(), JsValue> {
    if connection.protocol_state() != protocol::ProtocolState::Play {
        return Ok(());
    }
    let Some(current) = render::player_network_state() else { return Ok(()); };
    // Convert the local radian camera convention to reference vanilla degrees.
    let (server_yaw, server_pitch) = reference_server_rotation(current.yaw, current.pitch);
    let previous = LAST_SENT_PLAYER_STATE.with(|slot| *slot.borrow());

    let packet = match previous {
        None => {
            let mut payload = Vec::with_capacity(33);
            payload.extend_from_slice(&current.x.to_be_bytes());
            payload.extend_from_slice(&current.y.to_be_bytes());
            payload.extend_from_slice(&current.z.to_be_bytes());
            payload.extend_from_slice(&server_yaw.to_be_bytes());
            payload.extend_from_slice(&server_pitch.to_be_bytes());
            payload.push(u8::from(current.on_ground));
            Some((0x14, payload))
        }
        Some(previous) => {
            let dx = current.x - previous.x;
            let dy = current.y - previous.y;
            let dz = current.z - previous.z;
            let moved = dx * dx + dy * dy + dz * dz > 2.25e-7;
            let turned = current.yaw != previous.yaw || current.pitch != previous.pitch;
            let ground_changed = current.on_ground != previous.on_ground;
            if moved {
                let mut payload = Vec::with_capacity(33);
                payload.extend_from_slice(&current.x.to_be_bytes());
                payload.extend_from_slice(&current.y.to_be_bytes());
                payload.extend_from_slice(&current.z.to_be_bytes());
                payload.extend_from_slice(&server_yaw.to_be_bytes());
                payload.extend_from_slice(&server_pitch.to_be_bytes());
                payload.push(u8::from(current.on_ground));
                Some((0x14, payload))
            } else if turned {
                let mut payload = Vec::with_capacity(9);
                payload.extend_from_slice(&server_yaw.to_be_bytes());
                payload.extend_from_slice(&server_pitch.to_be_bytes());
                payload.push(u8::from(current.on_ground));
                Some((0x15, payload))
            } else if ground_changed {
                Some((0x16, vec![u8::from(current.on_ground)]))
            } else {
                None
            }
        }
    };

    if let Some((packet_id, payload)) = packet {
        connection.send_play_packet(packet_id, &payload)?;
        LAST_SENT_PLAYER_STATE.with(|slot| *slot.borrow_mut() = Some(current));
    }
    Ok(())
}

/// Open a standard Eaglercraft V5 connection without an HTTP WebSocket subprotocol.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn connect_reference_server(
    websocket_url: &str,
    host: &str,
    port: u16,
    username: &str,
    uuid_hex: &str,
) -> Result<(), JsValue> {
    connect_reference_server_with_protocol(
        websocket_url, host, port, username, uuid_hex, network::DEFAULT_EAGLER_SUBPROTOCOL,
    )
}

/// Open an Eaglercraft V5 connection, optionally requesting a separate HTTP WebSocket subprotocol.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn connect_reference_server_with_protocol(
    websocket_url: &str,
    host: &str,
    port: u16,
    username: &str,
    uuid_hex: &str,
    subprotocol: &str,
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
    let connection = network::BrowserProtocolConnection::connect_with_subprotocol(
        websocket_url, host, port, username, uuid, subprotocol,
    )?;
    REFERENCE_CONNECTION.with(|slot| *slot.borrow_mut() = Some(connection));
    LAST_SENT_PLAYER_STATE.with(|slot| *slot.borrow_mut() = None);
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
                    let decoded = reference_play::decode_reference_play_packet(packet_id, &payload)
                        .map_err(|error| JsValue::from_str(&format!("reference play packet rejected: {error:?}")))?;
                    match &decoded {
                        reference_play::ReferencePlayPacket::PlayerPosition { teleport_id, .. } => {
                            let confirm_payload = encode_reference_varint(*teleport_id);
                            connection.send_play_packet(0x00, &confirm_payload)?;
                            LAST_SENT_PLAYER_STATE.with(|slot| *slot.borrow_mut() = None);
                        }
                        reference_play::ReferencePlayPacket::Login(_) => {
                            LAST_SENT_PLAYER_STATE.with(|slot| *slot.borrow_mut() = None);
                            BROWSER_HOTBAR.with(|hotbar| hotbar.borrow_mut().clear());
                        }
                        reference_play::ReferencePlayPacket::SetContainerSlot {
                            window, slot, item_id, count,
                        } if *window == 0 && (0..9).contains(slot) => {
                            BROWSER_HOTBAR.with(|hotbar| {
                                hotbar.borrow_mut().set_slot(
                                    *slot as usize,
                                    BlockStateId(*item_id as u32),
                                    *count as u8,
                                );
                            });
                        }
                        _ => {}
                    }
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
        // Drain hold-to-mine actions after the renderer has generated them.
        // The server remains authoritative; this client never removes server blocks optimistically.
        for (status, position, face) in render::take_pending_dig_actions() {
            if let Some(payload) = render::server_dig_action_payload_at(status, position, face) {
                connection.send_play_packet(0x1c, &payload)?;
            }
        }
        send_reference_movement(connection)?;
        Ok(())
    })?;
    Ok(processed)
}

/// Return the subprotocol selected by the active WebSocket connection.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn reference_websocket_protocol() -> String {
    REFERENCE_CONNECTION.with(|slot| {
        slot.borrow().as_ref()
            .map(|connection| connection.negotiated_protocol())
            .unwrap_or_default()
    })
}

/// Return the WebSocket extensions negotiated by the browser, if any.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn reference_websocket_extensions() -> String {
    REFERENCE_CONNECTION.with(|slot| {
        slot.borrow().as_ref()
            .map(|connection| connection.negotiated_extensions())
            .unwrap_or_default()
    })
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
    LAST_SENT_PLAYER_STATE.with(|slot| *slot.borrow_mut() = None);
    render::disconnect_reference_world();
}

/// Select the active local singleplayer game mode (0 survival, 1 creative, 2 adventure, 3 spectator).
#[wasm_bindgen]
pub fn set_singleplayer_game_mode(mode: u8) -> Result<(), JsValue> {
    let mode = match mode {
        0 => integrated_server::GameMode::Survival,
        1 => integrated_server::GameMode::Creative,
        2 => integrated_server::GameMode::Adventure,
        3 => integrated_server::GameMode::Spectator,
        _ => return Err(JsValue::from_str("Game mode must be 0, 1, 2, or 3")),
    };
    render::set_singleplayer_game_mode(mode)
}

/// Return the current local singleplayer game mode.
#[wasm_bindgen]
pub fn singleplayer_game_mode() -> Result<u8, JsValue> {
    render::singleplayer_game_mode()
}

/// Return and clear the local server's 30-second autosave request.
#[wasm_bindgen]
pub fn take_singleplayer_autosave_request() -> bool {
    render::take_singleplayer_autosave_request()
}

/// Create a new local singleplayer world from a numeric seed.
#[wasm_bindgen]
pub fn create_singleplayer_world(seed: u32) -> Result<(), JsValue> {
    render::create_singleplayer_world(seed)
}

/// Encode the current local world and player state for browser persistence.
#[wasm_bindgen]
pub fn save_singleplayer_world() -> Result<Vec<u8>, JsValue> {
    render::save_singleplayer_world()
}

/// Load and validate a local singleplayer save produced by this client.
#[wasm_bindgen]
pub fn load_singleplayer_world(bytes: &[u8]) -> Result<(), JsValue> {
    render::load_singleplayer_world(bytes)
}

/// Apply a single complete 26.1.2 reference play packet to the live Rust world.
/// Packet IDs and layouts are reference-only pending 26.3 verification.
#[wasm_bindgen]
pub fn apply_reference_play_packet(packet_id: i32, payload: &[u8]) -> Result<(), JsValue> {
    render::apply_reference_play_packet(packet_id, payload)
}


/// Send the reference start/finish dig sequence for the targeted server block.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn server_break_target() -> Result<bool, JsValue> {
    let Some(start) = render::server_dig_action_payload(0) else { return Ok(false); };
    let Some(finish) = render::server_dig_action_payload(2) else { return Ok(false); };
    REFERENCE_CONNECTION.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(connection) = borrowed.as_mut() else { return Ok(false); };
        connection.send_play_packet(0x1c, &start)?;
        connection.send_play_packet(0x1c, &finish)?;
        Ok(true)
    })
}

/// Send the reference Use Item On packet for the targeted block face.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn server_place_target() -> Result<bool, JsValue> {
    let Some(payload) = render::server_use_item_on_payload() else { return Ok(false); };
    REFERENCE_CONNECTION.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(connection) = borrowed.as_mut() else { return Ok(false); };
        connection.send_play_packet(0x2a, &payload)?;
        Ok(true)
    })
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
    let selected = BROWSER_HOTBAR.with(|hotbar| hotbar.borrow_mut().select(slot as usize));
    #[cfg(target_arch = "wasm32")]
    if selected {
        let payload = (slot as u16).to_be_bytes();
        REFERENCE_CONNECTION.with(|connection| {
            if let Some(connection) = connection.borrow_mut().as_mut() {
                if let Err(error) = connection.send_play_packet(0x22, &payload) {
                    web_sys::console::warn_1(&error);
                }
            }
        });
    }
    selected
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

#[cfg(test)]
mod movement_rotation_tests {
    use super::reference_server_rotation;

    #[test]
    fn local_camera_angles_are_encoded_as_server_degrees() {
        let (yaw, pitch) = reference_server_rotation(0.0, 0.0);
        assert!((yaw - 180.0).abs() < 1.0e-4);
        assert!(pitch.abs() < 1.0e-4);

        let (yaw, pitch) = reference_server_rotation(
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::FRAC_PI_4,
        );
        assert!((yaw - 270.0).abs() < 1.0e-4);
        assert!((pitch + 45.0).abs() < 1.0e-4);
    }
}
