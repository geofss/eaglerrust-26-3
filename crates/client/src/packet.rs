//! Bounded packet primitives for the audited reference protocol.
//!
//! This module does not open a WebSocket or claim 26.3 packet-ID compatibility.
//! It provides safe VarInt and length-delimited frame handling; protocol-state
//! packet schemas must be added only after the 26.3 target is verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketError {
    Truncated,
    VarIntTooWide,
    LengthTooLarge,
    LengthMismatch,
    MissingPacketId,
}

#[derive(Debug, Default, Clone)]
pub struct PacketWriter {
    bytes: Vec<u8>,
}

impl PacketWriter {
    pub fn new() -> Self { Self::default() }
    pub fn write_varint(&mut self, value: i32) {
        let mut value = value as u32;
        while value & !0x7f != 0 {
            self.bytes.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        self.bytes.push(value as u8);
    }
    pub fn write_i32_be(&mut self, value: i32) { self.bytes.extend_from_slice(&value.to_be_bytes()); }
    pub fn write_bytes(&mut self, bytes: &[u8]) { self.bytes.extend_from_slice(bytes); }
    pub fn into_bytes(self) -> Vec<u8> { self.bytes }
}

#[derive(Debug, Clone)]
pub struct PacketReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> PacketReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self { Self { bytes, cursor: 0 } }
    pub fn remaining(&self) -> usize { self.bytes.len().saturating_sub(self.cursor) }

    pub fn read_varint(&mut self) -> Result<i32, PacketError> {
        let mut value = 0_u32;
        for shift in [0_u32, 7, 14, 21, 28] {
            let byte = *self.bytes.get(self.cursor).ok_or(PacketError::Truncated)?;
            self.cursor += 1;
            if shift == 28 && byte & 0xf0 != 0 { return Err(PacketError::VarIntTooWide); }
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 { return Ok(value as i32); }
        }
        Err(PacketError::VarIntTooWide)
    }

    pub fn read_i32_be(&mut self) -> Result<i32, PacketError> {
        let end = self.cursor.checked_add(4).ok_or(PacketError::Truncated)?;
        let bytes: [u8; 4] = self.bytes.get(self.cursor..end).ok_or(PacketError::Truncated)?
            .try_into().map_err(|_| PacketError::Truncated)?;
        self.cursor = end;
        Ok(i32::from_be_bytes(bytes))
    }

    pub fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], PacketError> {
        let end = self.cursor.checked_add(length).ok_or(PacketError::Truncated)?;
        let slice = self.bytes.get(self.cursor..end).ok_or(PacketError::Truncated)?;
        self.cursor = end;
        Ok(slice)
    }
}

/// Encode [VarInt frame length][VarInt packet ID][payload].
pub fn encode_packet_frame(packet_id: i32, payload: &[u8]) -> Vec<u8> {
    let mut body = PacketWriter::new();
    body.write_varint(packet_id);
    body.write_bytes(payload);
    let body = body.into_bytes();
    let mut frame = PacketWriter::new();
    frame.write_varint(body.len() as i32);
    frame.write_bytes(&body);
    frame.into_bytes()
}

/// Decode one complete frame, rejecting trailing or truncated bytes.
pub fn decode_packet_frame(bytes: &[u8], max_frame_bytes: usize) -> Result<(i32, &[u8]), PacketError> {
    let mut reader = PacketReader::new(bytes);
    let length = reader.read_varint()?;
    if length < 0 || length as usize > max_frame_bytes { return Err(PacketError::LengthTooLarge); }
    if reader.remaining() != length as usize { return Err(PacketError::LengthMismatch); }
    let mut body = PacketReader::new(reader.read_bytes(length as usize)?);
    if body.remaining() == 0 { return Err(PacketError::MissingPacketId); }
    let packet_id = body.read_varint()?;
    let payload = body.read_bytes(body.remaining())?;
    Ok((packet_id, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_round_trips_boundaries_and_negative_values() {
        for value in [0, 1, 127, 128, 16_383, 2_097_151, i32::MAX, -1, i32::MIN] {
            let mut writer = PacketWriter::new();
            writer.write_varint(value);
            assert_eq!(PacketReader::new(&writer.into_bytes()).read_varint(), Ok(value));
        }
    }

    #[test]
    fn packet_frame_round_trips_id_and_payload() {
        let frame = encode_packet_frame(0x2e, &[1, 2, 3, 4]);
        assert_eq!(decode_packet_frame(&frame, 1024), Ok((0x2e, &[1, 2, 3, 4][..])));
    }

    #[test]
    fn rejects_truncated_and_oversized_frames() {
        assert_eq!(decode_packet_frame(&[0x80], 1024), Err(PacketError::Truncated));
        assert_eq!(decode_packet_frame(&[5, 1], 1024), Err(PacketError::LengthMismatch));
        assert_eq!(decode_packet_frame(&[5, 1, 2, 3, 4, 5], 4), Err(PacketError::LengthTooLarge));
        assert_eq!(decode_packet_frame(&[0], 1024), Err(PacketError::MissingPacketId));
    }

    #[test]
    fn rejects_overwide_varints() {
        assert_eq!(PacketReader::new(&[0xff, 0xff, 0xff, 0xff, 0x10]).read_varint(), Err(PacketError::VarIntTooWide));
    }
}
