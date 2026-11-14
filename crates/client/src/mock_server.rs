//! Deterministic in-process reference server used by protocol tests.
//!
//! This is a scripted test peer, not a production server and not evidence of
//! compatibility with Eaglercraft 26.3. It exercises the 26.1.2 protocol
//! state machine without requiring a hosted WebSocket endpoint.

use crate::protocol::{
    decode_ws_packet, encode_ws_packet, ProtocolEvent, ProtocolSession, ProtocolState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockServerState {
    AwaitHandshake,
    AwaitLoginHello,
    Login,
    Configuration,
    Play,
    Disconnected,
}

struct MockReferenceServer {
    state: MockServerState,
}

impl MockReferenceServer {
    fn new() -> Self {
        Self { state: MockServerState::AwaitHandshake }
    }

    /// Validate the client's two initial WebSocket messages.
    fn accept_login_burst(&mut self, messages: &[Vec<u8>]) {
        assert_eq!(self.state, MockServerState::AwaitHandshake);
        assert_eq!(messages.len(), 2);

        let (handshake_id, handshake) = decode_ws_packet(&messages[0]).unwrap();
        assert_eq!(handshake_id, 0);
        let (protocol_version, used) = read_varint(handshake);
        assert_eq!(protocol_version, 775);
        let (host, host_used) = read_string(&handshake[used..]);
        assert_eq!(host, "mock.eagler.test");
        let port_offset = used + host_used;
        assert_eq!(
            u16::from_be_bytes([handshake[port_offset], handshake[port_offset + 1]]),
            25565
        );
        let (next_state, state_used) = read_varint(&handshake[port_offset + 2..]);
        assert_eq!(next_state, 2);
        assert_eq!(port_offset + 2 + state_used, handshake.len());

        self.state = MockServerState::AwaitLoginHello;
        let (hello_id, hello) = decode_ws_packet(&messages[1]).unwrap();
        assert_eq!(hello_id, 0);
        assert!(hello.len() >= 16);
        let (username, username_used) = read_string(&hello[16..]);
        assert_eq!(username, "RustPlayer");
        let (property_count, property_used) = read_varint(&hello[16 + username_used..]);
        assert_eq!(property_count, 0);
        assert_eq!(16 + username_used + property_used, hello.len());
        self.state = MockServerState::Login;
    }

    fn login_success(&mut self) -> Vec<u8> {
        assert_eq!(self.state, MockServerState::Login);
        self.state = MockServerState::Configuration;
        encode_ws_packet(0x02, &[0x11, 0x22])
    }

    fn known_packs(&self) -> Vec<u8> {
        assert_eq!(self.state, MockServerState::Configuration);
        encode_ws_packet(0x0f, &[0x00])
    }

    fn finish_configuration(&mut self) -> Vec<u8> {
        assert_eq!(self.state, MockServerState::Configuration);
        self.state = MockServerState::Play;
        encode_ws_packet(0x03, &[])
    }

    fn keep_alive(&self, value: i64) -> Vec<u8> {
        assert_eq!(self.state, MockServerState::Play);
        encode_ws_packet(0x2c, &value.to_be_bytes())
    }

    fn disconnect(&mut self) -> Vec<u8> {
        assert_eq!(self.state, MockServerState::Play);
        self.state = MockServerState::Disconnected;
        encode_ws_packet(0x1a, &[0])
    }
}

fn read_varint(bytes: &[u8]) -> (i32, usize) {
    let mut value = 0_u32;
    for index in 0..5 {
        let byte = *bytes.get(index).expect("truncated mock packet");
        assert!(!(index == 4 && byte & 0xf0 != 0), "mock VarInt too wide");
        value |= u32::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return (value as i32, index + 1);
        }
    }
    panic!("mock VarInt too wide");
}

fn read_string(bytes: &[u8]) -> (String, usize) {
    let (length, prefix) = read_varint(bytes);
    assert!(length >= 0);
    let end = prefix + length as usize;
    let value = std::str::from_utf8(bytes.get(prefix..end).expect("truncated mock string"))
        .expect("invalid mock UTF-8");
    (value.to_owned(), end)
}

fn sent_packets(events: Vec<ProtocolEvent>) -> Vec<Vec<u8>> {
    events.into_iter().filter_map(|event| match event {
        ProtocolEvent::Send(message) => Some(message),
        _ => None,
    }).collect()
}

#[test]
fn scripted_server_drives_client_from_handshake_to_play() {
    let mut client = ProtocolSession::new();
    let mut server = MockReferenceServer::new();

    let login_burst = client
        .begin_login("mock.eagler.test", 25565, "RustPlayer", [0x5a; 16])
        .unwrap();
    server.accept_login_burst(&login_burst);
    assert_eq!(client.state(), ProtocolState::Login);

    let events = client.process_server_message(&server.login_success()).unwrap();
    assert!(matches!(events.as_slice(), [ProtocolEvent::LoginSuccess(data)] if data == &[0x11, 0x22]));
    assert_eq!(client.state(), ProtocolState::Configuration);

    let events = client.process_server_message(&server.known_packs()).unwrap();
    let responses = sent_packets(events);
    assert_eq!(responses.len(), 1);
    assert_eq!(decode_ws_packet(&responses[0]).unwrap(), (0x07, &[0x00][..]));

    let events = client.process_server_message(&server.finish_configuration()).unwrap();
    let responses = sent_packets(events);
    assert_eq!(responses.len(), 3);
    assert_eq!(decode_ws_packet(&responses[0]).unwrap().0, 0x00); // client information
    assert_eq!(decode_ws_packet(&responses[1]).unwrap().0, 0x02); // brand
    assert_eq!(decode_ws_packet(&responses[2]).unwrap(), (0x03, &[][..]));
    assert_eq!(client.state(), ProtocolState::Play);
    assert_eq!(server.state, MockServerState::Play);

    let keep_alive = server.keep_alive(-9_876_543);
    let events = client.process_server_message(&keep_alive).unwrap();
    let responses = sent_packets(events);
    assert_eq!(responses, vec![encode_ws_packet(0x11, &(-9_876_543_i64).to_be_bytes())]);

    let events = client.process_server_message(&server.disconnect()).unwrap();
    assert!(matches!(events.as_slice(), [ProtocolEvent::ServerDisconnect(_)]));
    assert_eq!(client.state(), ProtocolState::Disconnected);
    assert_eq!(server.state, MockServerState::Disconnected);
}

#[test]
fn scripted_server_rejects_out_of_order_client_lifecycle() {
    let mut client = ProtocolSession::new();
    assert!(client.process_server_message(&encode_ws_packet(0x2c, &[0; 8])).is_err());
    assert_eq!(client.state(), ProtocolState::Handshake);
}

#[test]
fn malformed_server_message_does_not_advance_login_state() {
    let mut client = ProtocolSession::new();
    client.begin_login("mock.eagler.test", 25565, "RustPlayer", [0; 16]).unwrap();
    assert!(client.process_server_message(&[0x80]).is_err());
    assert_eq!(client.state(), ProtocolState::Login);
}
