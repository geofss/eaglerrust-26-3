//! Typed gameplay packet decoding for the supplied EaglercraftX 26.1.2 reference.
//!
//! Packet IDs and layouts in this module are pinned to that archive's
//! Connection.java and ClientPacketListener.java. They are not asserted to
//! match Eaglercraft 26.3. Unknown play packets are preserved for later work.

use crate::chunk_codec::{decode_reference_chunk_packet, ReferenceChunkPacket, ReferenceChunkPacketError};
use crate::world::{
    BlockPosition, BlockStateId, ChunkSection, LocalBlockPosition, SectionPosition,
    AIR, DIRT, GRASS_BLOCK, LEAVES, LOG, SAND, STONE, World,
};

pub const REFERENCE_S2C_PLAY_BLOCK_UPDATE: i32 = 0x05;
pub const REFERENCE_S2C_PLAY_UNLOAD_CHUNK: i32 = 0x1f;
pub const REFERENCE_S2C_PLAY_LOGIN: i32 = 0x2a;
pub const REFERENCE_S2C_PLAY_CHUNK: i32 = 0x2e;
pub const REFERENCE_S2C_PLAY_PLAYER_POSITION: i32 = 0x38;

const MAX_LOGIN_STRING_BYTES: usize = 32_767;

#[derive(Debug, Clone, PartialEq)]
pub struct ReferencePlayLogin {
    pub entity_id: i32,
    pub game_mode: u8,
    pub dimension: String,
    pub difficulty: u8,
    pub hardcore: bool,
    pub level_type: String,
    pub view_distance: i32,
    pub day_time: i64,
    pub game_time: i64,
    pub spawn_x: f64,
    pub spawn_y: f64,
    pub spawn_z: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReferencePlayPacket {
    Login(ReferencePlayLogin),
    Chunk(ReferenceChunkPacket),
    BlockUpdate { position: BlockPosition, state: BlockStateId },
    UnloadChunk { chunk_x: i32, chunk_z: i32 },
    PlayerPosition {
        x: f64, y: f64, z: f64, yaw: f32, pitch: f32,
        relative_flags: u8, teleport_id: i32,
    },
    Unhandled { packet_id: i32, payload: Vec<u8> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferencePlayPacketError {
    Truncated,
    VarIntTooWide,
    InvalidString,
    InvalidBoolean(u8),
    InvalidViewDistance(i32),
    InvalidBlockStateId(i32),
    InvalidTeleportId(i32),
    TrailingBytes(usize),
    Chunk(ReferenceChunkPacketError),
}

impl From<ReferenceChunkPacketError> for ReferencePlayPacketError {
    fn from(error: ReferenceChunkPacketError) -> Self { Self::Chunk(error) }
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, cursor: 0 } }

    fn read(&mut self, length: usize) -> Result<&'a [u8], ReferencePlayPacketError> {
        let end = self.cursor.checked_add(length).ok_or(ReferencePlayPacketError::Truncated)?;
        let value = self.bytes.get(self.cursor..end).ok_or(ReferencePlayPacketError::Truncated)?;
        self.cursor = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, ReferencePlayPacketError> {
        Ok(self.read(1)?[0])
    }

    fn bool(&mut self) -> Result<bool, ReferencePlayPacketError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(ReferencePlayPacketError::InvalidBoolean(other)),
        }
    }

    fn i32(&mut self) -> Result<i32, ReferencePlayPacketError> {
        Ok(i32::from_be_bytes(self.read(4)?.try_into().map_err(|_| ReferencePlayPacketError::Truncated)?))
    }

    fn i64(&mut self) -> Result<i64, ReferencePlayPacketError> {
        Ok(i64::from_be_bytes(self.read(8)?.try_into().map_err(|_| ReferencePlayPacketError::Truncated)?))
    }

    fn f32(&mut self) -> Result<f32, ReferencePlayPacketError> {
        Ok(f32::from_bits(u32::from_be_bytes(self.read(4)?.try_into().map_err(|_| ReferencePlayPacketError::Truncated)?)))
    }

    fn f64(&mut self) -> Result<f64, ReferencePlayPacketError> {
        Ok(f64::from_bits(u64::from_be_bytes(self.read(8)?.try_into().map_err(|_| ReferencePlayPacketError::Truncated)?)))
    }

    fn varint(&mut self) -> Result<i32, ReferencePlayPacketError> {
        let mut value = 0_u32;
        for shift in [0_u32, 7, 14, 21, 28] {
            let byte = self.u8()?;
            if shift == 28 && byte & 0xf0 != 0 {
                return Err(ReferencePlayPacketError::VarIntTooWide);
            }
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 { return Ok(value as i32); }
        }
        Err(ReferencePlayPacketError::VarIntTooWide)
    }

    fn string(&mut self) -> Result<String, ReferencePlayPacketError> {
        let length = self.varint()?;
        if length < 0 || length as usize > MAX_LOGIN_STRING_BYTES {
            return Err(ReferencePlayPacketError::InvalidString);
        }
        let bytes = self.read(length as usize)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ReferencePlayPacketError::InvalidString)
    }

    fn finish(self) -> Result<(), ReferencePlayPacketError> {
        let remaining = self.bytes.len().saturating_sub(self.cursor);
        if remaining == 0 { Ok(()) } else { Err(ReferencePlayPacketError::TrailingBytes(remaining)) }
    }
}

/// Decode the reference Play Login payload (S→C packet 0x2A).
///
/// The hashed seed is consumed but intentionally not exposed: terrain is
/// server-authoritative in this multiplayer client.
pub fn decode_reference_play_login(
    payload: &[u8],
) -> Result<ReferencePlayLogin, ReferencePlayPacketError> {
    let mut reader = Reader::new(payload);
    let entity_id = reader.i32()?;
    let game_mode = reader.u8()?;
    let dimension = reader.string()?;
    let _hashed_seed = reader.i64()?;
    let difficulty = reader.u8()?;
    let hardcore = reader.bool()?;
    let level_type = reader.string()?;
    let view_distance = reader.varint()?;
    if !(2..=32).contains(&view_distance) {
        return Err(ReferencePlayPacketError::InvalidViewDistance(view_distance));
    }
    let day_time = reader.i64()?;
    let game_time = reader.i64()?;
    let spawn_x = reader.f64()?;
    let spawn_y = reader.f64()?;
    let spawn_z = reader.f64()?;
    reader.finish()?;

    Ok(ReferencePlayLogin {
        entity_id, game_mode, dimension, difficulty, hardcore, level_type,
        view_distance, day_time, game_time, spawn_x, spawn_y, spawn_z,
    })
}

/// Translate the small set of 26.1.2 reference registry IDs for which
/// this prototype currently has matching render/collision definitions.
/// Unknown server states remain distinct and render with the unknown fallback.
pub fn map_reference_block_state(server_id: BlockStateId) -> BlockStateId {
    match server_id.0 {
        0 => AIR,
        1 | 4 | 33 => STONE,
        9 => GRASS_BLOCK,
        10 => DIRT,
        345 => SAND,
        351 => LOG,
        359 => LEAVES,
        other => BlockStateId(0x8000_0000 | other),
    }
}

fn map_reference_chunk_states(chunk: &mut ReferenceChunkPacket) {
    for section in &mut chunk.sections {
        let source = section.data.as_slice();
        let mut mapped = ChunkSection::new();
        for (index, state) in source.iter().copied().enumerate() {
            mapped.set(
                LocalBlockPosition {
                    x: (index % 16) as u8,
                    y: (index / 256) as u8,
                    z: ((index / 16) % 16) as u8,
                },
                map_reference_block_state(state),
            );
        }
        section.data = mapped;
    }
}

fn decode_packed_block_position(packed: i64) -> BlockPosition {
    BlockPosition::new(
        (packed >> 38) as i32,
        (packed << 52 >> 52) as i32,
        (packed << 26 >> 38) as i32,
    )
}

/// Decode the source-pinned subset of play packets. Unknown packets are kept
/// intact instead of being silently discarded.
pub fn decode_reference_play_packet(
    packet_id: i32,
    payload: &[u8],
) -> Result<ReferencePlayPacket, ReferencePlayPacketError> {
    match packet_id {
        REFERENCE_S2C_PLAY_LOGIN => Ok(ReferencePlayPacket::Login(decode_reference_play_login(payload)?)),
        REFERENCE_S2C_PLAY_CHUNK => {
            let mut chunk = decode_reference_chunk_packet(payload)?;
            map_reference_chunk_states(&mut chunk);
            Ok(ReferencePlayPacket::Chunk(chunk))
        },
        REFERENCE_S2C_PLAY_BLOCK_UPDATE => {
            let mut reader = Reader::new(payload);
            let packed = reader.i64()?;
            let block_id = reader.varint()?;
            if block_id < 0 { return Err(ReferencePlayPacketError::InvalidBlockStateId(block_id)); }
            reader.finish()?;
            Ok(ReferencePlayPacket::BlockUpdate {
                position: decode_packed_block_position(packed),
                state: map_reference_block_state(BlockStateId(block_id as u32)),
            })
        }
        REFERENCE_S2C_PLAY_UNLOAD_CHUNK => {
            let mut reader = Reader::new(payload);
            let chunk_x = reader.i32()?;
            let chunk_z = reader.i32()?;
            reader.finish()?;
            Ok(ReferencePlayPacket::UnloadChunk { chunk_x, chunk_z })
        }
        REFERENCE_S2C_PLAY_PLAYER_POSITION => {
            let mut reader = Reader::new(payload);
            let x = reader.f64()?;
            let y = reader.f64()?;
            let z = reader.f64()?;
            let yaw = reader.f32()?;
            let pitch = reader.f32()?;
            let relative_flags = reader.u8()?;
            let teleport_id = reader.varint()?;
            if teleport_id < 0 { return Err(ReferencePlayPacketError::InvalidTeleportId(teleport_id)); }
            reader.finish()?;
            Ok(ReferencePlayPacket::PlayerPosition {
                x, y, z, yaw, pitch, relative_flags, teleport_id,
            })
        }
        _ => Ok(ReferencePlayPacket::Unhandled { packet_id, payload: payload.to_vec() }),
    }
}

/// Apply decoded chunk data to the internal world only after the entire
/// payload has passed validation. Login metadata is returned to the caller.
pub fn apply_reference_play_packet(
    world: &mut World,
    packet_id: i32,
    payload: &[u8],
) -> Result<ReferencePlayPacket, ReferencePlayPacketError> {
    let packet = decode_reference_play_packet(packet_id, payload)?;
    match &packet {
        ReferencePlayPacket::Chunk(chunk) => {
            for section in &chunk.sections {
                world.replace_section(
                    SectionPosition::new(chunk.chunk_x, i32::from(section.section_y), chunk.chunk_z),
                    section.data.clone(),
                );
            }
        }
        ReferencePlayPacket::BlockUpdate { position, state } => {
            world.set_block(*position, *state);
        }
        ReferencePlayPacket::UnloadChunk { chunk_x, chunk_z } => {
            world.remove_column(*chunk_x, *chunk_z);
        }
        _ => {}
    }
    Ok(packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_varint(out: &mut Vec<u8>, value: i32) {
        let mut value = value as u32;
        while value & !0x7f != 0 {
            out.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    fn write_string(out: &mut Vec<u8>, value: &str) {
        write_varint(out, value.len() as i32);
        out.extend_from_slice(value.as_bytes());
    }

    fn login_payload() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&123_i32.to_be_bytes());
        out.push(1);
        write_string(&mut out, "minecraft:overworld");
        out.extend_from_slice(&(-99_i64).to_be_bytes());
        out.push(2);
        out.push(1);
        write_string(&mut out, "default");
        write_varint(&mut out, 10);
        out.extend_from_slice(&6000_i64.to_be_bytes());
        out.extend_from_slice(&12000_i64.to_be_bytes());
        out.extend_from_slice(&12.5_f64.to_bits().to_be_bytes());
        out.extend_from_slice(&64.0_f64.to_bits().to_be_bytes());
        out.extend_from_slice(&(-8.25_f64).to_bits().to_be_bytes());
        out
    }

    #[test]
    fn decodes_reference_play_login_fields_and_consumes_hashed_seed() {
        let login = decode_reference_play_login(&login_payload()).unwrap();
        assert_eq!(login.entity_id, 123);
        assert_eq!(login.game_mode, 1);
        assert_eq!(login.dimension, "minecraft:overworld");
        assert_eq!(login.difficulty, 2);
        assert!(login.hardcore);
        assert_eq!(login.level_type, "default");
        assert_eq!(login.view_distance, 10);
        assert_eq!((login.day_time, login.game_time), (6000, 12000));
        assert_eq!((login.spawn_x, login.spawn_y, login.spawn_z), (12.5, 64.0, -8.25));
    }

    #[test]
    fn rejects_malformed_login_without_accepting_trailing_bytes() {
        assert_eq!(decode_reference_play_login(&[0, 1]), Err(ReferencePlayPacketError::Truncated));
        let mut payload = login_payload();
        payload.push(0xff);
        assert_eq!(decode_reference_play_login(&payload), Err(ReferencePlayPacketError::TrailingBytes(1)));
    }

    #[test]
    fn rejects_invalid_boolean_and_view_distance() {
        let mut payload = login_payload();
        payload[4 + 1 + 1 + "minecraft:overworld".len() + 8 + 1] = 2;
        assert_eq!(decode_reference_play_login(&payload), Err(ReferencePlayPacketError::InvalidBoolean(2)));

        let mut payload = login_payload();
        let view_offset = 4 + 1 + 1 + "minecraft:overworld".len() + 8 + 1 + 1 + 1 + "default".len();
        payload[view_offset] = 1;
        assert_eq!(decode_reference_play_login(&payload), Err(ReferencePlayPacketError::InvalidViewDistance(1)));
    }

    #[test]
    fn decodes_server_block_update_and_negative_coordinates() {
        use crate::packet::PacketWriter;
        let packed = ((-2_i64 & 0x3ff_ffff) << 38)
            | ((-7_i64 & 0xfff) << 0)
            | ((3_i64 & 0x3ff_ffff) << 12);
        let mut writer = PacketWriter::new();
        writer.write_bytes(&packed.to_be_bytes());
        writer.write_varint(42);
        assert_eq!(
            decode_reference_play_packet(REFERENCE_S2C_PLAY_BLOCK_UPDATE, &writer.into_bytes()).unwrap(),
            ReferencePlayPacket::BlockUpdate {
                position: BlockPosition::new(-2, -7, 3),
                state: map_reference_block_state(BlockStateId(42)),
            }
        );
    }

    #[test]
    fn decodes_unload_and_player_teleport_packets() {
        use crate::packet::PacketWriter;
        let mut unload = PacketWriter::new();
        unload.write_i32_be(-4);
        unload.write_i32_be(9);
        assert_eq!(
            decode_reference_play_packet(REFERENCE_S2C_PLAY_UNLOAD_CHUNK, &unload.into_bytes()).unwrap(),
            ReferencePlayPacket::UnloadChunk { chunk_x: -4, chunk_z: 9 }
        );

        let mut teleport = PacketWriter::new();
        teleport.write_bytes(&1.5_f64.to_bits().to_be_bytes());
        teleport.write_bytes(&72.0_f64.to_bits().to_be_bytes());
        teleport.write_bytes(&(-3.25_f64).to_bits().to_be_bytes());
        teleport.write_bytes(&30.0_f32.to_bits().to_be_bytes());
        teleport.write_bytes(&(-10.0_f32).to_bits().to_be_bytes());
        teleport.write_bytes(&[0]);
        teleport.write_varint(17);
        assert_eq!(
            decode_reference_play_packet(REFERENCE_S2C_PLAY_PLAYER_POSITION, &teleport.into_bytes()).unwrap(),
            ReferencePlayPacket::PlayerPosition {
                x: 1.5, y: 72.0, z: -3.25, yaw: 30.0, pitch: -10.0,
                relative_flags: 0, teleport_id: 17,
            }
        );
    }

    #[test]
    fn dispatch_preserves_unknown_packets() {
        assert_eq!(
            decode_reference_play_packet(0x7f, &[1, 2, 3]).unwrap(),
            ReferencePlayPacket::Unhandled { packet_id: 0x7f, payload: vec![1, 2, 3] }
        );
    }

    #[test]
    fn chunk_dispatch_installs_validated_sections_into_world() {
        use crate::packet::PacketWriter;
        use crate::world::{BlockPosition, BlockStateId, SECTION_VOLUME};

        let mut writer = PacketWriter::new();
        writer.write_i32_be(-1);
        writer.write_i32_be(2);
        writer.write_varint(1);
        writer.write_bytes(&[0]);
        writer.write_varint(2);
        writer.write_varint(0);
        writer.write_varint(91);
        let mut voxels = vec![0; SECTION_VOLUME];
        voxels[3] = 1;
        writer.write_bytes(&voxels);

        let mut world = World::new();
        let packet = apply_reference_play_packet(&mut world, REFERENCE_S2C_PLAY_CHUNK, &writer.into_bytes()).unwrap();
        assert!(matches!(packet, ReferencePlayPacket::Chunk(_)));
        assert_eq!(world.get_block(BlockPosition::new(-13, 0, 32)), map_reference_block_state(BlockStateId(91)));
    }

    #[test]
    fn malformed_chunk_never_partially_mutates_world() {
        use crate::packet::PacketWriter;
        use crate::world::SECTION_VOLUME;

        let mut writer = PacketWriter::new();
        writer.write_i32_be(0);
        writer.write_i32_be(0);
        writer.write_varint(1);
        writer.write_bytes(&[0]);
        writer.write_varint(1);
        writer.write_varint(0);
        let mut voxels = vec![0; SECTION_VOLUME];
        voxels[0] = 1;
        writer.write_bytes(&voxels);

        let mut world = World::new();
        assert!(apply_reference_play_packet(&mut world, REFERENCE_S2C_PLAY_CHUNK, &writer.into_bytes()).is_err());
        assert_eq!(world.section_count(), 0);
    }
}
