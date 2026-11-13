//! Version-pinned protocol state machine based on the uploaded EaglercraftX
//! 26.1.2 reference archive (protocol 775).
//!
//! This is a Rust reimplementation of documented wire behavior, not a claim
//! that protocol 775 is identical to Eaglercraft 26.3. Each WebSocket message
//! contains one packet: VarInt packet ID followed by payload, with no stream
//! length prefix. Keep this codec distinct from stream framing.

const REFERENCE_PROTOCOL_VERSION: i32 = 775;
const MAX_HOST_BYTES: usize = 255;
const MAX_USERNAME_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolState { Handshake, Login, Configuration, Play, Disconnected }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    InvalidState, InvalidHost, InvalidPort, InvalidUsername,
    TruncatedPacket, VarIntTooWide, InvalidKeepAliveLength,
    UnexpectedPacket { state: ProtocolState, packet_id: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolEvent {
    Send(Vec<u8>),
    LoginAcknowledged,
    LoginSuccess(Vec<u8>),
    ConfigurationPacket { packet_id: i32, payload: Vec<u8> },
    ConfigurationFinished,
    PlayPacket { packet_id: i32, payload: Vec<u8> },
    ServerDisconnect(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ProtocolSession { state: ProtocolState }

impl Default for ProtocolSession {
    fn default() -> Self { Self { state: ProtocolState::Handshake } }
}

impl ProtocolSession {
    pub fn new() -> Self { Self::default() }
    pub fn state(&self) -> ProtocolState { self.state }

    /// Build the 26.1.2 reference handshake and offline Login Hello messages.
    /// UUID is supplied by the caller as 16 network-order bytes.
    pub fn begin_login(&mut self, host: &str, port: u16, username: &str, uuid: [u8; 16])
        -> Result<Vec<Vec<u8>>, ProtocolError>
    {
        if self.state != ProtocolState::Handshake { return Err(ProtocolError::InvalidState); }
        if host.is_empty() || host.len() > MAX_HOST_BYTES || host.bytes().any(|b| b == 0) {
            return Err(ProtocolError::InvalidHost);
        }
        if port == 0 { return Err(ProtocolError::InvalidPort); }
        if username.is_empty() || username.len() > MAX_USERNAME_BYTES
            || !username.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        { return Err(ProtocolError::InvalidUsername); }

        let mut handshake = Vec::with_capacity(host.len() + 16);
        write_varint(&mut handshake, REFERENCE_PROTOCOL_VERSION);
        write_string(&mut handshake, host);
        handshake.extend_from_slice(&port.to_be_bytes());
        write_varint(&mut handshake, 2); // next state: LOGIN

        let mut login_hello = Vec::with_capacity(32);
        login_hello.extend_from_slice(&uuid);
        write_string(&mut login_hello, username);
        write_varint(&mut login_hello, 0); // no offline-profile properties

        self.state = ProtocolState::Login;
        Ok(vec![encode_ws_packet(0x00, &handshake), encode_ws_packet(0x00, &login_hello)])
    }

    /// Process one complete server WebSocket message. Send events contain
    /// complete client messages ready for the browser transport.
    pub fn process_server_message(&mut self, message: &[u8])
        -> Result<Vec<ProtocolEvent>, ProtocolError>
    {
        let (packet_id, payload) = decode_ws_packet(message)?;
        let mut events = Vec::new();
        match self.state {
            ProtocolState::Handshake =>
                return Err(ProtocolError::UnexpectedPacket { state: self.state, packet_id }),
            ProtocolState::Login => match packet_id {
                0x01 => events.push(ProtocolEvent::LoginAcknowledged),
                0x02 => {
                    self.state = ProtocolState::Configuration;
                    events.push(ProtocolEvent::LoginSuccess(payload.to_vec()));
                }
                0x03 => {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                }
                _ => return Err(ProtocolError::UnexpectedPacket { state: self.state, packet_id }),
            },
            ProtocolState::Configuration => match packet_id {
                0x01 => {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                }
                0x02 => {
                    self.state = ProtocolState::Play;
                    events.push(ProtocolEvent::Send(client_information_packet()));
                    events.push(ProtocolEvent::Send(brand_plugin_message()));
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x02, &[])));
                    events.push(ProtocolEvent::ConfigurationFinished);
                }
                0x03 | 0x04 => {
                    if payload.len() != 8 { return Err(ProtocolError::InvalidKeepAliveLength); }
                    events.push(ProtocolEvent::Send(encode_ws_packet(packet_id, payload)));
                }
                0x08 => {
                    events.push(ProtocolEvent::Send(known_packs_empty_packet()));
                    events.push(ProtocolEvent::ConfigurationPacket {
                        packet_id, payload: payload.to_vec(),
                    });
                }
                0x00 | 0x06 | 0x07 => events.push(ProtocolEvent::ConfigurationPacket {
                    packet_id, payload: payload.to_vec(),
                }),
                _ => return Err(ProtocolError::UnexpectedPacket { state: self.state, packet_id }),
            },
            ProtocolState::Play => match packet_id {
                0x1a => {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                }
                0x2c => {
                    if payload.len() != 8 { return Err(ProtocolError::InvalidKeepAliveLength); }
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x11, payload)));
                }
                _ => events.push(ProtocolEvent::PlayPacket { packet_id, payload: payload.to_vec() }),
            },
            ProtocolState::Disconnected => return Err(ProtocolError::InvalidState),
        }
        Ok(events)
    }
}

fn client_information_packet() -> Vec<u8> {
    let mut payload = Vec::with_capacity(24);
    write_string(&mut payload, "en_us");
    payload.push(8);
    write_varint(&mut payload, 0);
    payload.push(1);
    payload.push(0x7f);
    write_varint(&mut payload, 1);
    payload.push(0);
    payload.push(1);
    encode_ws_packet(0x00, &payload)
}

fn known_packs_empty_packet() -> Vec<u8> {
    encode_ws_packet(0x05, &[0x00])
}

fn brand_plugin_message() -> Vec<u8> {
    let mut payload = Vec::with_capacity(48);
    write_string(&mut payload, "minecraft:brand");
    write_string(&mut payload, "eaglercraft-26.1.2-web");
    encode_ws_packet(0x01, &payload)
}

/// Encode one 26.1.2-style WebSocket packet. No outer length prefix is added.
pub fn encode_ws_packet(packet_id: i32, payload: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(5 + payload.len());
    write_varint(&mut message, packet_id);
    message.extend_from_slice(payload);
    message
}

/// Decode the packet ID and borrow the remaining payload.
pub fn decode_ws_packet(message: &[u8]) -> Result<(i32, &[u8]), ProtocolError> {
    let (packet_id, used) = read_varint(message)?;
    Ok((packet_id, &message[used..]))
}

fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    while value & !0x7f != 0 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_varint(bytes: &[u8]) -> Result<(i32, usize), ProtocolError> {
    let mut value = 0_u32;
    for shift in [0_u32, 7, 14, 21, 28] {
        let index = (shift / 7) as usize;
        let byte = *bytes.get(index).ok_or(ProtocolError::TruncatedPacket)?;
        if shift == 28 && byte & 0xf0 != 0 { return Err(ProtocolError::VarIntTooWide); }
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 { return Ok((value as i32, index + 1)); }
    }
    Err(ProtocolError::VarIntTooWide)
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    write_varint(out, value.len() as i32);
    out.extend_from_slice(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(message: &[u8]) -> i32 { decode_ws_packet(message).unwrap().0 }

    #[test]
    fn login_burst_is_two_unprefixed_websocket_messages() {
        let mut s = ProtocolSession::new();
        let packets = s.begin_login("play.example.test", 25565, "Player_1", [7; 16]).unwrap();
        assert_eq!(s.state(), ProtocolState::Login);
        assert_eq!(packets.len(), 2);
        assert_eq!((id(&packets[0]), id(&packets[1])), (0, 0));
        let (_, handshake) = decode_ws_packet(&packets[0]).unwrap();
        assert_eq!(&handshake[..2], &[0x87, 0x06]); // protocol 775
    }

    #[test]
    fn invalid_identity_does_not_advance_state() {
        let mut s = ProtocolSession::new();
        assert_eq!(s.begin_login("host", 25565, "bad name", [0; 16]), Err(ProtocolError::InvalidUsername));
        assert_eq!(s.state(), ProtocolState::Handshake);
        s.begin_login("host", 25565, "Valid_1", [0; 16]).unwrap();
        assert_eq!(s.begin_login("host", 25565, "Valid_1", [0; 16]), Err(ProtocolError::InvalidState));
    }

    #[test]
    fn login_and_configuration_transitions_are_ordered() {
        let mut s = ProtocolSession::new();
        s.begin_login("host", 25565, "Player", [0; 16]).unwrap();
        assert_eq!(s.process_server_message(&encode_ws_packet(2, &[9, 8])),
            Ok(vec![ProtocolEvent::LoginSuccess(vec![9, 8])]));
        assert_eq!(s.state(), ProtocolState::Configuration);
        assert_eq!(s.process_server_message(&encode_ws_packet(0x08, &[0])),
            Ok(vec![
                ProtocolEvent::Send(known_packs_empty_packet()),
                ProtocolEvent::ConfigurationPacket { packet_id: 0x08, payload: vec![0] },
            ]));
        assert_eq!(s.process_server_message(&encode_ws_packet(2, &[])),
            Ok(vec![
                ProtocolEvent::Send(client_information_packet()),
                ProtocolEvent::Send(brand_plugin_message()),
                ProtocolEvent::Send(encode_ws_packet(2, &[])),
                ProtocolEvent::ConfigurationFinished,
            ]));
        assert_eq!(s.state(), ProtocolState::Play);
    }

    #[test]
    fn configuration_keepalive_and_ping_are_echoed() {
        let mut s = ProtocolSession::new();
        s.begin_login("host", 25565, "Player", [0; 16]).unwrap();
        s.process_server_message(&encode_ws_packet(2, &[])).unwrap();
        for id in [3, 4] {
            let payload = 123456_i64.to_be_bytes();
            assert_eq!(s.process_server_message(&encode_ws_packet(id, &payload)),
                Ok(vec![ProtocolEvent::Send(encode_ws_packet(id, &payload))]));
        }
    }

    #[test]
    fn play_keepalive_uses_client_response_packet_id() {
        let mut s = ProtocolSession::new();
        s.begin_login("host", 25565, "Player", [0; 16]).unwrap();
        s.process_server_message(&encode_ws_packet(2, &[])).unwrap();
        s.process_server_message(&encode_ws_packet(2, &[])).unwrap();
        let payload = (-77_i64).to_be_bytes();
        assert_eq!(s.process_server_message(&encode_ws_packet(0x2c, &payload)),
            Ok(vec![ProtocolEvent::Send(encode_ws_packet(0x11, &payload))]));
    }

    #[test]
    fn malformed_and_out_of_order_packets_are_rejected() {
        let mut s = ProtocolSession::new();
        assert_eq!(s.process_server_message(&[0x80]), Err(ProtocolError::TruncatedPacket));
        assert!(matches!(s.process_server_message(&encode_ws_packet(0, &[])),
            Err(ProtocolError::UnexpectedPacket { .. })));
    }
}
