//! Transport-independent connection lifecycle.
//! Do not add packet IDs or handshake assumptions until the protocol source is audited.
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
