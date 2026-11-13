//! Network transport lifecycle and packet codec exports.
//!
//! The browser adapter below provides binary WebSocket transport only. It does
//! not claim to implement the Eaglercraft 26.3 handshake, login state machine,
//! encryption, compression, or packet IDs.

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
                open_state.borrow_mut().state = ConnectionState::Connected;
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
            self.shared.borrow_mut().state = ConnectionState::Closing;
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
pub use browser::BrowserWebSocket;
