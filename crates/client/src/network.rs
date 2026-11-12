//! Transport lifecycle and packet codec exports.
//! No WebSocket session or 26.3 handshake is claimed yet.
pub use crate::packet::{decode_packet_frame, encode_packet_frame, PacketError, PacketReader, PacketWriter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState { Disconnected, Connecting, Connected, Closing, Failed }

impl Default for ConnectionState {
    fn default() -> Self { Self::Disconnected }
}
