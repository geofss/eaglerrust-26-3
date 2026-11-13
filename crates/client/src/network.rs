//! Network transport lifecycle and packet codec exports.
//!
//! The browser adapter provides binary WebSocket transport plus a version-pinned
//! 26.1.2 reference protocol driver. It does not claim 26.3 packet compatibility,
//! encryption, or compression.

pub use crate::packet::{decode_packet_frame, encode_packet_frame, PacketError, PacketReader, PacketWriter};

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

    use js_sys::{ArrayBuffer, Uint8Array};
    use wasm_bindgen::{closure::Closure, JsCast, JsValue};
    use web_sys::{BinaryType, Event, MessageEvent, WebSocket};

    use super::ConnectionState;
    use crate::protocol::{ProtocolEvent, ProtocolSession};

    /// Browser transport connected to the audited 26.1.2 reference protocol.
    /// This is a protocol driver, not a 26.3 compatibility claim.
    pub struct BrowserProtocolConnection {
        transport: BrowserWebSocket,
        protocol: ProtocolSession,
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
            let transport = BrowserWebSocket::connect(url)?;
            let mut protocol = ProtocolSession::new();
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
        /// Open a WebSocket and configure it to receive binary ArrayBuffers.
        pub fn connect(url: &str) -> Result<Self, JsValue> {
            let socket = WebSocket::new(url)?;
            socket.set_binary_type(BinaryType::Arraybuffer);

            let shared = Rc::new(RefCell::new(SharedState {
                state: ConnectionState::Connecting,
                ..SharedState::default()
            }));

            let open_state = Rc::clone(&shared);
            let on_open = Closure::wrap(Box::new(move |_event: Event| {
                let mut state = open_state.borrow_mut();
                if state.state == ConnectionState::Connecting {
                    state.state = ConnectionState::Connected;
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
