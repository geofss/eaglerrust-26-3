//! Eaglercraft V5 application handshake plus Minecraft packet state handling.
//!
//! Eagler V5 login frames use single-byte Eagler packet identifiers. After the
//! V5 exchange, Minecraft packets are carried one per binary WebSocket message
//! with a VarInt packet ID and no outer stream-length prefix. Configuration and
//! gameplay packet coverage remains version-sensitive and is not yet complete.

const REFERENCE_PROTOCOL_VERSION: i32 = 775;

/// Vanilla Java Edition 26.3 protocol baseline. This does not by itself
/// establish Eaglercraft 26.3 WebSocket-extension compatibility.
pub const JAVA_26_3_PROTOCOL_VERSION: i32 = 777;
/// Minecraft 26.3 world-storage data version. This belongs in world/NBT metadata,
/// not in the Eagler V5 transport preamble or Java network handshake.
pub const JAVA_26_3_DATA_VERSION: i32 = 5023;
const MAX_HOST_BYTES: usize = 255;
const MAX_USERNAME_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolState { Handshake, Login, Configuration, Play, Disconnected }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    InvalidState, InvalidHost, InvalidPort, InvalidUsername,
    TruncatedPacket, VarIntTooWide, InvalidKeepAliveLength, InvalidString,
    EaglerProtocolMismatch { expected: u16, actual: u16 },
    MinecraftProtocolMismatch { expected: u16, actual: u16 },
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

        let handshake = encode_handshake_message(REFERENCE_PROTOCOL_VERSION, host, port)?;

        let mut login_hello = Vec::with_capacity(32);
        login_hello.extend_from_slice(&uuid);
        write_string(&mut login_hello, username);
        write_varint(&mut login_hello, 0); // no offline-profile properties

        self.state = ProtocolState::Login;
        Ok(vec![handshake, encode_ws_packet(0x00, &login_hello)])
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
                0x04 => {
                    let (message_id, id_bytes) = read_varint(payload)?;
                    let channel_bytes = skip_utf8_string(&payload[id_bytes..])?;
                    if channel_bytes > payload.len() - id_bytes {
                        return Err(ProtocolError::InvalidString);
                    }
                    let mut response = Vec::with_capacity(6);
                    write_varint(&mut response, message_id);
                    response.push(0); // no plugin response data
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x05, &response)));
                }
                0x03 => {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                }
                _ => return Err(ProtocolError::UnexpectedPacket { state: self.state, packet_id }),
            },
            ProtocolState::Configuration => match packet_id {
                // Java protocol 777 (26.3) clientbound Configuration packet IDs.
                0x02 => {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                }
                0x03 => {
                    self.state = ProtocolState::Play;
                    events.push(ProtocolEvent::Send(client_information_packet()));
                    events.push(ProtocolEvent::Send(brand_plugin_message()));
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x03, &[])));
                    events.push(ProtocolEvent::ConfigurationFinished);
                }
                0x04 => {
                    if payload.len() != 8 { return Err(ProtocolError::InvalidKeepAliveLength); }
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x04, payload)));
                }
                0x05 => {
                    if payload.len() != 4 { return Err(ProtocolError::InvalidKeepAliveLength); }
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x05, payload)));
                }
                0x0f => {
                    events.push(ProtocolEvent::Send(known_packs_empty_packet()));
                    events.push(ProtocolEvent::ConfigurationPacket {
                        packet_id, payload: payload.to_vec(),
                    });
                }
                // Registry Data (0x07), Feature Flags (0x0d), Update Tags (0x0e),
                // and other configuration messages must be retained for the
                // registry/configuration implementation rather than discarded.
                _ => events.push(ProtocolEvent::ConfigurationPacket {
                    packet_id, payload: payload.to_vec(),
                }),
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


/// Eaglercraft V5 application-level handshake for standard EaglerXServer endpoints.
///
/// This is independent of the HTTP WebSocket subprotocol. Standard EaglerXServer
/// starts the binary stream with the V5 client-version frame (01 02 ...), then
/// carries one complete Minecraft packet per binary WebSocket message after login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EaglerLoginStage { AwaitServerVersion, AwaitAllowLogin, AwaitServerFinish, Complete }

#[derive(Debug, Clone)]
pub struct EaglerV5Session {
    state: ProtocolState,
    stage: EaglerLoginStage,
    minecraft_protocol: u16,
    requested_username: String,
    granted_username: Option<String>,
    granted_uuid: Option<[u8; 16]>,
    session_uuid: [u8; 16],
}

impl Default for EaglerV5Session {
    fn default() -> Self {
        Self {
            state: ProtocolState::Handshake,
            stage: EaglerLoginStage::AwaitServerVersion,
            minecraft_protocol: JAVA_26_3_PROTOCOL_VERSION as u16,
            requested_username: String::new(),
            granted_username: None,
            granted_uuid: None,
            session_uuid: [0; 16],
        }
    }
}

impl EaglerV5Session {
    pub fn new() -> Self { Self::default() }
    pub fn state(&self) -> ProtocolState { self.state }

    /// Begin Eagler V5. Host and port are validated as endpoint metadata but are
    /// not sent in the V5 frame; EaglerXServer selects its default backend.
    pub fn begin_login(
        &mut self,
        host: &str,
        port: u16,
        username: &str,
        session_uuid: [u8; 16],
    ) -> Result<Vec<Vec<u8>>, ProtocolError> {
        if self.state != ProtocolState::Handshake { return Err(ProtocolError::InvalidState); }
        if host.is_empty() || host.len() > MAX_HOST_BYTES || host.bytes().any(|b| b == 0) {
            return Err(ProtocolError::InvalidHost);
        }
        if port == 0 { return Err(ProtocolError::InvalidPort); }
        if username.is_empty() || username.len() > MAX_USERNAME_BYTES
            || !username.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        { return Err(ProtocolError::InvalidUsername); }

        let mut frame = Vec::with_capacity(64);
        frame.push(0x01); // CLIENT_VERSION
        frame.push(0x02); // V2+ handshake family (V5 fields)
        frame.extend_from_slice(&1_u16.to_be_bytes());
        frame.extend_from_slice(&5_u16.to_be_bytes()); // Eagler protocol V5
        frame.extend_from_slice(&1_u16.to_be_bytes());
        frame.extend_from_slice(&self.minecraft_protocol.to_be_bytes()); // Java 26.3 / 777
        write_eagler_ascii(&mut frame, "EaglerRust")?;
        write_eagler_ascii(&mut frame, "26.3")?;
        frame.push(0); // no password offered
        write_eagler_ascii(&mut frame, username)?;

        self.requested_username = username.to_owned();
        self.session_uuid = session_uuid;
        self.state = ProtocolState::Login;
        self.stage = EaglerLoginStage::AwaitServerVersion;
        Ok(vec![frame])
    }

    /// Process one complete binary WebSocket message. During V5 login the
    /// packet id is a single Eagler byte; after login it is a Minecraft VarInt.
    pub fn process_server_message(
        &mut self,
        message: &[u8],
    ) -> Result<Vec<ProtocolEvent>, ProtocolError> {
        if self.state == ProtocolState::Disconnected { return Err(ProtocolError::InvalidState); }
        if self.state == ProtocolState::Login {
            return self.process_eagler_login_message(message);
        }

        // Eagler extension messages (skins, voice, cookies) use 0xEE and are
        // intentionally ignored until their independent codecs are implemented.
        if self.state == ProtocolState::Play && message.first() == Some(&0xEE) {
            return Ok(Vec::new());
        }

        let (packet_id, payload) = decode_ws_packet(message)?;
        let mut events = Vec::new();
        match self.state {
            ProtocolState::Handshake => return Err(ProtocolError::InvalidState),
            ProtocolState::Login => unreachable!(),
            ProtocolState::Configuration => match packet_id {
                // Java protocol 777 (26.3) clientbound Configuration packet IDs.
                0x02 => {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                }
                0x03 => {
                    self.state = ProtocolState::Play;
                    events.push(ProtocolEvent::Send(client_information_packet()));
                    events.push(ProtocolEvent::Send(brand_plugin_message()));
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x03, &[])));
                    events.push(ProtocolEvent::ConfigurationFinished);
                }
                0x04 => {
                    if payload.len() != 8 { return Err(ProtocolError::InvalidKeepAliveLength); }
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x04, payload)));
                }
                0x05 => {
                    if payload.len() != 4 { return Err(ProtocolError::InvalidKeepAliveLength); }
                    events.push(ProtocolEvent::Send(encode_ws_packet(0x05, payload)));
                }
                0x0f => {
                    events.push(ProtocolEvent::Send(known_packs_empty_packet()));
                    events.push(ProtocolEvent::ConfigurationPacket {
                        packet_id, payload: payload.to_vec(),
                    });
                }
                // Registry Data (0x07), Feature Flags (0x0d), Update Tags (0x0e),
                // and other configuration messages must be retained for the
                // registry/configuration implementation rather than discarded.
                _ => events.push(ProtocolEvent::ConfigurationPacket {
                    packet_id, payload: payload.to_vec(),
                }),
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

    fn process_eagler_login_message(
        &mut self,
        message: &[u8],
    ) -> Result<Vec<ProtocolEvent>, ProtocolError> {
        let (&packet_id, payload) = message.split_first().ok_or(ProtocolError::TruncatedPacket)?;
        let mut events = Vec::new();
        match self.stage {
            EaglerLoginStage::AwaitServerVersion => {
                if packet_id == 0x03 || packet_id == 0x06 || packet_id == 0xFF {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                    return Ok(events);
                }
                if packet_id != 0x02 {
                    return Err(ProtocolError::UnexpectedPacket { state: ProtocolState::Login, packet_id: i32::from(packet_id) });
                }
                let mut offset = 0;
                let eagler_protocol = read_eagler_u16(payload, &mut offset)?;
                let minecraft_protocol = read_eagler_u16(payload, &mut offset)?;
                let _server_brand = read_eagler_ascii(payload, &mut offset)?;
                let _server_version = read_eagler_ascii(payload, &mut offset)?;
                let _auth_method = read_eagler_u8(payload, &mut offset)?;
                let salt_len = usize::from(read_eagler_u16(payload, &mut offset)?);
                let _salt = read_eagler_take(payload, &mut offset, salt_len)?;
                if eagler_protocol != 5 {
                    return Err(ProtocolError::EaglerProtocolMismatch { expected: 5, actual: eagler_protocol });
                }
                if minecraft_protocol != self.minecraft_protocol {
                    return Err(ProtocolError::MinecraftProtocolMismatch {
                        expected: self.minecraft_protocol,
                        actual: minecraft_protocol,
                    });
                }
                let mut request = Vec::with_capacity(32);
                request.push(0x04); // CLIENT_REQUEST_LOGIN
                write_eagler_ascii(&mut request, &self.requested_username)?;
                write_eagler_ascii(&mut request, "")?; // proxy default backend
                request.push(0); // empty password
                request.push(0); // no cookie offered
                request.push(0); // cookie length
                write_varint(&mut request, 0); // no optional capabilities requested
                request.push(0); // no extended capabilities
                events.push(ProtocolEvent::Send(request));
                self.stage = EaglerLoginStage::AwaitAllowLogin;
            }
            EaglerLoginStage::AwaitAllowLogin => {
                if packet_id == 0x06 || packet_id == 0xFF || packet_id == 0x0A {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                    return Ok(events);
                }
                if packet_id != 0x05 {
                    return Err(ProtocolError::UnexpectedPacket { state: ProtocolState::Login, packet_id: i32::from(packet_id) });
                }
                let mut offset = 0;
                let name = read_eagler_ascii(payload, &mut offset)?;
                let uuid_bytes = read_eagler_take(payload, &mut offset, 16)?;
                let mut uuid = [0_u8; 16];
                uuid.copy_from_slice(uuid_bytes);
                self.granted_username = Some(name);
                self.granted_uuid = Some(uuid);
                events.push(ProtocolEvent::Send(vec![0x08])); // CLIENT_FINISH_LOGIN
                self.stage = EaglerLoginStage::AwaitServerFinish;
            }
            EaglerLoginStage::AwaitServerFinish => {
                if packet_id == 0x06 || packet_id == 0xFF || packet_id == 0x0A {
                    self.state = ProtocolState::Disconnected;
                    events.push(ProtocolEvent::ServerDisconnect(payload.to_vec()));
                    return Ok(events);
                }
                if packet_id != 0x09 {
                    return Err(ProtocolError::UnexpectedPacket { state: ProtocolState::Login, packet_id: i32::from(packet_id) });
                }
                let name = self.granted_username.clone().ok_or(ProtocolError::InvalidState)?;
                let uuid = self.granted_uuid.ok_or(ProtocolError::InvalidState)?;
                let login_success = encode_java_login_success(&name, uuid, self.session_uuid);
                events.push(ProtocolEvent::LoginSuccess(login_success));
                // EaglerHandshake consumes this vanilla Login Acknowledged packet
                // and only then releases the backend's configuration/play packets.
                events.push(ProtocolEvent::Send(encode_ws_packet(0x03, &[])));
                self.state = if self.minecraft_protocol >= 764 {
                    ProtocolState::Configuration
                } else {
                    ProtocolState::Play
                };
                self.stage = EaglerLoginStage::Complete;
            }
            EaglerLoginStage::Complete => return Err(ProtocolError::InvalidState),
        }
        Ok(events)
    }
}

fn encode_java_login_success(name: &str, uuid: [u8; 16], session_uuid: [u8; 16]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(64);
    payload.extend_from_slice(&uuid);
    write_string(&mut payload, name);
    write_varint(&mut payload, 0); // no profile properties in offline Eagler login
    // Java protocol 776+ carries a session UUID after profile properties.
    payload.extend_from_slice(&session_uuid);
    payload
}

fn write_eagler_ascii(out: &mut Vec<u8>, value: &str) -> Result<(), ProtocolError> {
    if value.len() > 255 || !value.is_ascii() { return Err(ProtocolError::InvalidString); }
    out.push(value.len() as u8);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_eagler_take<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], ProtocolError> {
    let end = (*offset).checked_add(length).ok_or(ProtocolError::InvalidString)?;
    let value = bytes.get(*offset..end).ok_or(ProtocolError::TruncatedPacket)?;
    *offset = end;
    Ok(value)
}

fn read_eagler_u8(bytes: &[u8], offset: &mut usize) -> Result<u8, ProtocolError> {
    Ok(read_eagler_take(bytes, offset, 1)?[0])
}

fn read_eagler_u16(bytes: &[u8], offset: &mut usize) -> Result<u16, ProtocolError> {
    let value = read_eagler_take(bytes, offset, 2)?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_eagler_ascii(bytes: &[u8], offset: &mut usize) -> Result<String, ProtocolError> {
    let length = usize::from(read_eagler_u8(bytes, offset)?);
    let value = read_eagler_take(bytes, offset, length)?;
    if !value.is_ascii() { return Err(ProtocolError::InvalidString); }
    Ok(String::from_utf8(value.to_vec()).map_err(|_| ProtocolError::InvalidString)?)
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
    // Serverbound Select Known Packs (Configuration ID 0x07), zero shared packs.
    encode_ws_packet(0x07, &[0x00])
}

fn brand_plugin_message() -> Vec<u8> {
    let mut payload = Vec::with_capacity(48);
    write_string(&mut payload, "minecraft:brand");
    write_string(&mut payload, "eaglercraft-26.1.2-web");
    // Serverbound Plugin Message is Configuration ID 0x02 in protocol 777.
    encode_ws_packet(0x02, &payload)
}

/// Encode the vanilla Java Edition 26.3 handshake packet (protocol 777).
///
/// This only constructs the vanilla handshake message. Eaglercraft-specific
/// WebSocket bootstrap, subprotocol negotiation, login packet layouts, and
/// later state transitions must be verified separately before using it live.
pub fn encode_java_26_3_handshake(host: &str, port: u16) -> Result<Vec<u8>, ProtocolError> {
    encode_handshake_message(JAVA_26_3_PROTOCOL_VERSION, host, port)
}

fn encode_handshake_message(
    protocol_version: i32,
    host: &str,
    port: u16,
) -> Result<Vec<u8>, ProtocolError> {
    if host.is_empty() || host.len() > MAX_HOST_BYTES || host.bytes().any(|b| b == 0) {
        return Err(ProtocolError::InvalidHost);
    }
    if port == 0 {
        return Err(ProtocolError::InvalidPort);
    }

    let mut payload = Vec::with_capacity(host.len() + 16);
    write_varint(&mut payload, protocol_version);
    write_string(&mut payload, host);
    payload.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut payload, 2); // next state: LOGIN
    Ok(encode_ws_packet(0x00, &payload))
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

fn skip_utf8_string(bytes: &[u8]) -> Result<usize, ProtocolError> {
    let (length, prefix_bytes) = read_varint(bytes)?;
    if length < 0 || length as usize > 131_072 {
        return Err(ProtocolError::InvalidString);
    }
    let end = prefix_bytes.checked_add(length as usize).ok_or(ProtocolError::InvalidString)?;
    let value = bytes.get(prefix_bytes..end).ok_or(ProtocolError::TruncatedPacket)?;
    std::str::from_utf8(value).map_err(|_| ProtocolError::InvalidString)?;
    Ok(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(message: &[u8]) -> i32 { decode_ws_packet(message).unwrap().0 }


    fn eagler_server_version() -> Vec<u8> {
        let mut packet = vec![0x02];
        packet.extend_from_slice(&5_u16.to_be_bytes());
        packet.extend_from_slice(&777_u16.to_be_bytes());
        packet.push(6); packet.extend_from_slice(b"ProxyX");
        packet.push(5); packet.extend_from_slice(b"1.1.1");
        packet.push(0);
        packet.extend_from_slice(&0_u16.to_be_bytes());
        packet
    }

    #[test]
    fn eagler_v5_starts_with_the_application_handshake_not_vanilla() {
        let mut session = EaglerV5Session::new();
        let frames = session.begin_login("proxy.example", 25565, "RustPlayer", [9; 16]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(&frames[0][..2], &[0x01, 0x02]);
        assert_eq!(&frames[0][2..4], &1_u16.to_be_bytes());
        assert_eq!(&frames[0][4..6], &5_u16.to_be_bytes());
        assert_eq!(&frames[0][6..8], &1_u16.to_be_bytes());
        assert_eq!(&frames[0][8..10], &777_u16.to_be_bytes());
        // Data version 5023 is storage metadata, not an initialization field.
        assert_eq!(JAVA_26_3_DATA_VERSION, 5023);
        assert!(!frames[0].windows(4).any(|bytes| bytes == JAVA_26_3_DATA_VERSION.to_be_bytes()));
    }

    #[test]
    fn eagler_v5_login_negotiates_then_acknowledges_vanilla_login() {
        let mut session = EaglerV5Session::new();
        let initial = session.begin_login("proxy.example", 25565, "RustPlayer", [9; 16]).unwrap();
        assert_eq!(&initial[0][..2], &[0x01, 0x02]);

        let request = session.process_server_message(&eagler_server_version()).unwrap();
        assert_eq!(request.len(), 1);
        let ProtocolEvent::Send(login_request) = &request[0] else { panic!("expected login request") };
        assert_eq!(login_request[0], 0x04);
        assert_eq!(&login_request[1..12], b"\x0aRustPlayer");

        let mut allow = vec![0x05, 10];
        allow.extend_from_slice(b"RustPlayer");
        allow.extend_from_slice(&[0x11; 16]);
        let finish = session.process_server_message(&allow).unwrap();
        assert_eq!(finish, vec![ProtocolEvent::Send(vec![0x08])]);

        let events = session.process_server_message(&[0x09]).unwrap();
        assert_eq!(session.state(), ProtocolState::Configuration);
        assert_eq!(events.len(), 2);
        let ProtocolEvent::LoginSuccess(login) = &events[0] else { panic!("expected synthetic Login Success") };
        assert_eq!(&login[..16], &[0x11; 16]);
        assert_eq!(events[1], ProtocolEvent::Send(encode_ws_packet(0x03, &[])));
    }

    #[test]
    fn eagler_v5_rejects_protocol_mismatch_before_login() {
        let mut session = EaglerV5Session::new();
        session.begin_login("proxy.example", 25565, "Player", [0; 16]).unwrap();
        let mut version = eagler_server_version();
        version[1..3].copy_from_slice(&4_u16.to_be_bytes());
        assert_eq!(
            session.process_server_message(&version),
            Err(ProtocolError::EaglerProtocolMismatch { expected: 5, actual: 4 })
        );
    }

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
    fn java_26_3_handshake_uses_protocol_777_without_claiming_eagler_bootstrap() {
        let message = encode_java_26_3_handshake("8b4t.org", 25565).unwrap();
        let (packet_id, payload) = decode_ws_packet(&message).unwrap();
        assert_eq!(packet_id, 0x00);

        let (version, used) = read_varint(payload).unwrap();
        assert_eq!(version, JAVA_26_3_PROTOCOL_VERSION);
        assert_eq!(version, 777);

        let host_len = skip_utf8_string(&payload[used..]).unwrap();
        let port_offset = used + host_len;
        assert_eq!(&payload[port_offset..port_offset + 2], &25565_u16.to_be_bytes());
        assert_eq!(payload[port_offset + 2], 2); // LOGIN state
    }

    #[test]
    fn java_26_3_handshake_rejects_invalid_host_and_port() {
        assert_eq!(encode_java_26_3_handshake("", 25565), Err(ProtocolError::InvalidHost));
        assert_eq!(encode_java_26_3_handshake("host", 0), Err(ProtocolError::InvalidPort));
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
    fn login_plugin_request_gets_empty_response_with_matching_id() {
        let mut s = ProtocolSession::new();
        s.begin_login("host", 25565, "Player", [0; 16]).unwrap();
        let mut payload = Vec::new();
        write_varint(&mut payload, 42);
        write_string(&mut payload, "minecraft:test");
        payload.extend_from_slice(&[1, 2, 3]);
        let events = s.process_server_message(&encode_ws_packet(0x04, &payload)).unwrap();
        let expected = encode_ws_packet(0x05, &[42, 0]);
        assert_eq!(events, vec![ProtocolEvent::Send(expected)]);
        assert_eq!(s.state(), ProtocolState::Login);
    }

    #[test]
    fn login_and_configuration_transitions_are_ordered() {
        let mut s = ProtocolSession::new();
        s.begin_login("host", 25565, "Player", [0; 16]).unwrap();
        assert_eq!(s.process_server_message(&encode_ws_packet(2, &[9, 8])),
            Ok(vec![ProtocolEvent::LoginSuccess(vec![9, 8])]));
        assert_eq!(s.state(), ProtocolState::Configuration);
        // Java 777: Select Known Packs is clientbound 0x0f;
        // the empty response is serverbound 0x07.
        assert_eq!(s.process_server_message(&encode_ws_packet(0x0f, &[0])),
            Ok(vec![
                ProtocolEvent::Send(known_packs_empty_packet()),
                ProtocolEvent::ConfigurationPacket { packet_id: 0x0f, payload: vec![0] },
            ]));
        assert_eq!(known_packs_empty_packet(), encode_ws_packet(0x07, &[0]));
        assert_eq!(brand_plugin_message()[0], 0x02);
        assert_eq!(s.process_server_message(&encode_ws_packet(0x03, &[])),
            Ok(vec![
                ProtocolEvent::Send(client_information_packet()),
                ProtocolEvent::Send(brand_plugin_message()),
                ProtocolEvent::Send(encode_ws_packet(0x03, &[])),
                ProtocolEvent::ConfigurationFinished,
            ]));
        assert_eq!(s.state(), ProtocolState::Play);
    }

    #[test]
    fn unknown_configuration_packets_are_preserved_for_diagnostics() {
        let mut session = EaglerV5Session::new();
        session.begin_login("proxy.example", 25565, "RustPlayer", [9; 16]).unwrap();
        session.process_server_message(&eagler_server_version()).unwrap();
        let mut allow = vec![0x05, 10];
        allow.extend_from_slice(b"RustPlayer");
        allow.extend_from_slice(&[0x11; 16]);
        session.process_server_message(&allow).unwrap();
        session.process_server_message(&[0x09]).unwrap();
        let payload = [0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(
            session.process_server_message(&encode_ws_packet(0x7f, &payload)).unwrap(),
            vec![ProtocolEvent::ConfigurationPacket {
                packet_id: 0x7f,
                payload: payload.to_vec(),
            }]
        );
    }

    #[test]
    fn configuration_keepalive_and_ping_are_echoed() {
        let mut s = ProtocolSession::new();
        s.begin_login("host", 25565, "Player", [0; 16]).unwrap();
        // The server must send Login Success (0x02) before Configuration.
        s.process_server_message(&encode_ws_packet(0x02, &[])).unwrap();
        // Finish Configuration (0x03) transitions the session into Play.
        s.process_server_message(&encode_ws_packet(0x03, &[])).unwrap();
        let keep_alive = 123456_i64.to_be_bytes();
        assert_eq!(
            s.process_server_message(&encode_ws_packet(0x04, &keep_alive)),
            Ok(vec![ProtocolEvent::Send(encode_ws_packet(0x04, &keep_alive))])
        );
        let ping = 123456_i32.to_be_bytes();
        assert_eq!(
            s.process_server_message(&encode_ws_packet(0x05, &ping)),
            Ok(vec![ProtocolEvent::Send(encode_ws_packet(0x05, &ping))])
        );
    }

    #[test]
    fn play_keepalive_uses_client_response_packet_id() {
        let mut s = ProtocolSession::new();
        s.begin_login("host", 25565, "Player", [0; 16]).unwrap();
        s.process_server_message(&encode_ws_packet(0x03, &[])).unwrap();
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
