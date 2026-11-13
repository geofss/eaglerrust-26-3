//! Network transport lifecycle and packet codec exports.
//!
//! The browser adapter provides binary WebSocket transport and an Eaglercraft V5
//! application handshake. Minecraft configuration/play packet support remains
//! version-sensitive; this transport does not implement encryption or compression.

pub use crate::packet::{decode_packet_frame, encode_packet_frame, PacketError, PacketReader, PacketWriter};

/// Standard EaglerXServer does not require an HTTP WebSocket subprotocol.
/// The Eagler V5 application handshake is sent as the first binary message.
pub const DEFAULT_EAGLER_SUBPROTOCOL: &str = "";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Closing,
    Failed,
}

impl Default for ConnectionState {
    fn default() -> Self { Self::Disconnected }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use js_sys::{Array, ArrayBuffer, Uint8Array};
    use wasm_bindgen::{closure::Closure, JsCast, JsValue};
    use web_sys::{BinaryType, Event, MessageEvent, WebSocket};

    use super::ConnectionState;
    use crate::protocol::{EaglerV5Session, ProtocolEvent};

    /// Browser transport using the Eaglercraft V5 application handshake.
    pub struct BrowserProtocolConnection {
        transport: BrowserWebSocket,
        protocol: EaglerV5Session,
        pending_outbound: VecDeque<Vec<u8>>,
    }

    impl BrowserProtocolConnection {
        pub fn connect(
            url: &str,
            host: &str,
            port: u16,
            username: &str,
            uuid: [u8; 16],
        ) -> Result<Self, JsValue> {
            Self::connect_with_subprotocol(url, host, port, username, uuid, crate::network::DEFAULT_EAGLER_SUBPROTOCOL)
        }

        pub fn connect_with_subprotocol(
            url: &str,
            host: &str,
            port: u16,
            username: &str,
            uuid: [u8; 16],
            subprotocol: &str,
        ) -> Result<Self, JsValue> {
            // HTTP subprotocol negotiation is optional and independent from Eagler V5.
            // Standard EaglerXServer endpoints normally use no Sec-WebSocket-Protocol.
            let offered = if subprotocol.is_empty() { Vec::new() } else { vec![subprotocol] };
            let transport = BrowserWebSocket::connect_with_subprotocols(url, &offered)?;
            let mut protocol = EaglerV5Session::new();
            let pending_outbound = protocol
                .begin_login(host, port, username, uuid)
                .map_err(|error| JsValue::from_str(&format!("protocol setup failed: {error:?}")))?
                .into();
            Ok(Self { transport, protocol, pending_outbound })
        }

        pub fn protocol_state(&self) -> crate::protocol::ProtocolState {
            self.protocol.state()
        }

        pub fn transport_state(&self) -> ConnectionState {
            self.transport.state()
        }

        pub fn negotiated_protocol(&self) -> String {
            self.transport.negotiated_protocol()
        }

        pub fn negotiated_extensions(&self) -> String {
            self.transport.negotiated_extensions()
        }

        pub fn close(&self) -> Result<(), JsValue> {
            self.transport.close()
        }

        /// Queue one complete client play message for the audited reference protocol.
        pub fn send_play_packet(&mut self, packet_id: i32, payload: &[u8]) -> Result<(), JsValue> {
            if self.protocol.state() != crate::protocol::ProtocolState::Play {
                return Err(JsValue::from_str("Play packet requested before the connection entered Play"));
            }
            self.pending_outbound.push_back(crate::protocol::encode_ws_packet(packet_id, payload));
            self.flush_outbound()
        }

        /// Flush queued handshake/config responses and process inbound packets.
        /// Non-send protocol events are returned to the game layer.
        pub fn tick(&mut self) -> Result<Vec<ProtocolEvent>, JsValue> {
            self.flush_outbound()?;
            let mut delivered = Vec::new();
            while let Some(message) = self.transport.try_receive() {
                let events = self.protocol.process_server_message(&message)
                    .map_err(|error| JsValue::from_str(&format!("protocol packet rejected: {error:?}")))?;
                for event in events {
                    match event {
                        ProtocolEvent::Send(bytes) => self.pending_outbound.push_back(bytes),
                        other => delivered.push(other),
                    }
                }
            }
            self.flush_outbound()?;
            Ok(delivered)
        }

        fn flush_outbound(&mut self) -> Result<(), JsValue> {
            if self.transport.state() != ConnectionState::Connected {
                return Ok(());
            }
            while let Some(message) = self.pending_outbound.pop_front() {
                if let Err(error) = self.transport.send_binary(&message) {
                    self.pending_outbound.push_front(message);
                    return Err(error);
                }
            }
            Ok(())
        }
    }

    const MAX_QUEUED_MESSAGES: usize = 1024;
    const MAX_QUEUED_BYTES: usize = 16 * 1024 * 1024;

    #[derive(Default)]
    struct SharedState {
        state: ConnectionState,
        incoming: VecDeque<Vec<u8>>,
        queued_bytes: usize,
        transport_error: bool,
        negotiated_protocol: String,
        negotiated_extensions: String,
    }

    /// Browser-owned binary WebSocket connection.
    ///
    /// Messages are queued as complete binary frames. The caller is responsible
    /// for passing received bytes to the version-appropriate packet decoder.
    pub struct BrowserWebSocket {
        socket: WebSocket,
        shared: Rc<RefCell<SharedState>>,
        on_open: Closure<dyn FnMut(Event)>,
        on_message: Closure<dyn FnMut(MessageEvent)>,
        on_error: Closure<dyn FnMut(Event)>,
        on_close: Closure<dyn FnMut(Event)>,
    }

    impl BrowserWebSocket {
        /// Open a WebSocket without requesting a subprotocol.
        pub fn connect(url: &str) -> Result<Self, JsValue> {
            Self::connect_with_subprotocols(url, &[])
        }

        /// Open a WebSocket and offer protocols in preference order.
        pub fn connect_with_subprotocols(url: &str, subprotocols: &[&str]) -> Result<Self, JsValue> {
            let socket = if subprotocols.is_empty() {
                WebSocket::new(url)?
            } else {
                let protocols = Array::new();
                for protocol in subprotocols {
                    if protocol.is_empty() {
                        return Err(JsValue::from_str("WebSocket subprotocol cannot be empty"));
                    }
                    if subprotocols.iter().filter(|candidate| *candidate == protocol).count() > 1 {
                        return Err(JsValue::from_str("WebSocket subprotocol list contains duplicates"));
                    }
                    protocols.push(&JsValue::from_str(protocol));
                }
                WebSocket::new_with_str_sequence(url, protocols.as_ref())?
            };
            socket.set_binary_type(BinaryType::Arraybuffer);

            let shared = Rc::new(RefCell::new(SharedState {
                state: ConnectionState::Connecting,
                ..SharedState::default()
            }));

            let open_state = Rc::clone(&shared);
            let open_socket = socket.clone();
            let on_open = Closure::wrap(Box::new(move |_event: Event| {
                let mut state = open_state.borrow_mut();
                if state.state == ConnectionState::Connecting {
                    state.state = ConnectionState::Connected;
                    // These properties are authoritative only after the open event.
                    state.negotiated_protocol = open_socket.protocol();
                    state.negotiated_extensions = open_socket.extensions();
                }
            }) as Box<dyn FnMut(Event)>);
            socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let message_state = Rc::clone(&shared);
            let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
                let data = event.data();
                let Ok(buffer) = data.dyn_into::<ArrayBuffer>() else {
                    let mut state = message_state.borrow_mut();
                    state.transport_error = true;
                    state.state = ConnectionState::Failed;
                    return;
                };

                let bytes = Uint8Array::new(&buffer).to_vec();
                let mut state = message_state.borrow_mut();
                if state.state == ConnectionState::Failed {
                    return;
                }
                if state.incoming.len() >= MAX_QUEUED_MESSAGES
                    || state.queued_bytes.saturating_add(bytes.len()) > MAX_QUEUED_BYTES
                {
                    state.transport_error = true;
                    state.state = ConnectionState::Failed;
                    state.incoming.clear();
                    state.queued_bytes = 0;
                    return;
                }
                state.queued_bytes += bytes.len();
                state.incoming.push_back(bytes);
            }) as Box<dyn FnMut(MessageEvent)>);
            socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let error_state = Rc::clone(&shared);
            let on_error = Closure::wrap(Box::new(move |_event: Event| {
                let mut state = error_state.borrow_mut();
                state.transport_error = true;
                state.state = ConnectionState::Failed;
            }) as Box<dyn FnMut(Event)>);
            socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let close_state = Rc::clone(&shared);
            let on_close = Closure::wrap(Box::new(move |_event: Event| {
                let mut state = close_state.borrow_mut();
                if state.state != ConnectionState::Failed {
                    state.state = ConnectionState::Disconnected;
                }
            }) as Box<dyn FnMut(Event)>);
            socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            Ok(Self {
                socket,
                shared,
                on_open,
                on_message,
                on_error,
                on_close,
            })
        }

        pub fn state(&self) -> ConnectionState {
            self.shared.borrow().state
        }

        pub fn has_transport_error(&self) -> bool {
            self.shared.borrow().transport_error
        }

        pub fn negotiated_protocol(&self) -> String {
            self.shared.borrow().negotiated_protocol.clone()
        }

        pub fn negotiated_extensions(&self) -> String {
            self.shared.borrow().negotiated_extensions.clone()
        }

        /// Send one complete binary message when the socket is open.
        pub fn send_binary(&self, bytes: &[u8]) -> Result<(), JsValue> {
            if self.state() != ConnectionState::Connected {
                return Err(JsValue::from_str("WebSocket is not connected"));
            }
            self.socket.send_with_u8_array(bytes)
        }

        /// Remove the oldest complete binary message from the receive queue.
        pub fn try_receive(&self) -> Option<Vec<u8>> {
            let mut state = self.shared.borrow_mut();
            let message = state.incoming.pop_front()?;
            state.queued_bytes = state.queued_bytes.saturating_sub(message.len());
            Some(message)
        }

        pub fn queued_messages(&self) -> usize {
            self.shared.borrow().incoming.len()
        }

        /// Begin a normal WebSocket close handshake.
        pub fn close(&self) -> Result<(), JsValue> {
            let mut state = self.shared.borrow_mut();
            if state.state != ConnectionState::Failed {
                state.state = ConnectionState::Closing;
            }
            drop(state);
            self.socket.close()
        }
    }

    impl Drop for BrowserWebSocket {
        fn drop(&mut self) {
            self.socket.set_onopen(None);
            self.socket.set_onmessage(None);
            self.socket.set_onerror(None);
            self.socket.set_onclose(None);
            let _ = self.socket.close();

            // Keep the callback fields observably owned until handlers are detached.
            let _ = (&self.on_open, &self.on_message, &self.on_error, &self.on_close);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::{BrowserProtocolConnection, BrowserWebSocket};
