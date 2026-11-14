//! Decoder for the compact section payload documented by the supplied
//! EaglercraftX 26.1.2 reference gateway.
//!
//! This is deliberately isolated from the 26.3 network path: the reference
//! format stores one u8 palette index per voxel (4096 bytes per section).
//! It is NOT the vanilla packed-long PalettedContainer format and must not be
//! used for a 26.3 packet until that target format is verified.
use crate::packet::{PacketError, PacketReader, PacketWriter};
use crate::world::{BlockStateId, ChunkSection, LocalBlockPosition, SectionPosition, World, SECTION_VOLUME};

const MAX_REFERENCE_CHUNK_SECTIONS: i32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteDecodeError {
    EmptyPalette,
    PaletteTooLarge,
    WrongVoxelCount { expected: usize, actual: usize },
    PaletteIndexOutOfRange { voxel: usize, index: u8, palette_len: usize },
}

/// Decode and atomically install one section into the internal world model.
///
/// This adapter remains specific to the supplied 26.1.2 reference gateway
/// layout. It is not a production 26.3 network decoder.
pub fn decode_reference_palette_section_into_world(
    world: &mut World,
    position: SectionPosition,
    palette: &[BlockStateId],
    voxel_indices: &[u8],
) -> Result<(), PaletteDecodeError> {
    let section = decode_reference_palette_section(palette, voxel_indices)?;
    world.replace_section(position, section);
    Ok(())
}

/// One already-validated section from the 26.1.2 reference chunk packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceChunkSection {
    pub section_y: i8,
    pub data: ChunkSection,
}

/// Decoded chunk-column packet from the supplied 26.1.2 reference gateway.
/// This is not the vanilla packed-long chunk format or a verified 26.3 packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceChunkPacket {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub sections: Vec<ReferenceChunkSection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceChunkPacketError {
    Packet(PacketError),
    InvalidSectionCount(i32),
    InvalidPaletteLength(i32),
    NegativeBlockStateId(i32),
    TrailingBytes(usize),
    Palette(PaletteDecodeError),
}

impl From<PacketError> for ReferenceChunkPacketError {
    fn from(error: PacketError) -> Self { Self::Packet(error) }
}

impl From<PaletteDecodeError> for ReferenceChunkPacketError {
    fn from(error: PaletteDecodeError) -> Self { Self::Palette(error) }
}

/// Decode the reference gateway's Level Chunk payload:
/// chunk X, chunk Z, section count, then section Y, palette length, palette
/// VarInts, and exactly 4096 byte-per-voxel palette indices per section.
pub fn decode_reference_chunk_packet(
    payload: &[u8],
) -> Result<ReferenceChunkPacket, ReferenceChunkPacketError> {
    let mut reader = PacketReader::new(payload);
    let chunk_x = reader.read_i32_be()?;
    let chunk_z = reader.read_i32_be()?;
    let section_count = reader.read_varint()?;
    if !(0..=MAX_REFERENCE_CHUNK_SECTIONS).contains(&section_count) {
        return Err(ReferenceChunkPacketError::InvalidSectionCount(section_count));
    }

    let mut sections = Vec::with_capacity(section_count as usize);
    for _ in 0..section_count {
        let section_y = reader.read_bytes(1)?[0] as i8;
        let palette_len = reader.read_varint()?;
        if !(1..=256).contains(&palette_len) {
            return Err(ReferenceChunkPacketError::InvalidPaletteLength(palette_len));
        }

        let mut palette = Vec::with_capacity(palette_len as usize);
        for _ in 0..palette_len {
            let block_state_id = reader.read_varint()?;
            if block_state_id < 0 {
                return Err(ReferenceChunkPacketError::NegativeBlockStateId(block_state_id));
            }
            palette.push(BlockStateId(block_state_id as u32));
        }

        let voxel_indices = reader.read_bytes(SECTION_VOLUME)?;
        let data = decode_reference_palette_section(&palette, voxel_indices)?;
        sections.push(ReferenceChunkSection { section_y, data });
    }

    if reader.remaining() != 0 {
        return Err(ReferenceChunkPacketError::TrailingBytes(reader.remaining()));
    }

    Ok(ReferenceChunkPacket { chunk_x, chunk_z, sections })
}

/// Decode the entire packet before mutating the world, then atomically install
/// its validated sections into the internal world representation.
pub fn apply_reference_chunk_packet(
    world: &mut World,
    payload: &[u8],
) -> Result<ReferenceChunkPacket, ReferenceChunkPacketError> {
    let packet = decode_reference_chunk_packet(payload)?;
    for section in &packet.sections {
        world.replace_section(
            SectionPosition::new(packet.chunk_x, i32::from(section.section_y), packet.chunk_z),
            section.data.clone(),
        );
    }
    Ok(packet)
}

/// Decode the reference gateway's byte-per-voxel section representation.
/// Index order is YZX: y * 256 + z * 16 + x.
pub fn decode_reference_palette_section(
    palette: &[BlockStateId],
    voxel_indices: &[u8],
) -> Result<ChunkSection, PaletteDecodeError> {
    if palette.is_empty() {
        return Err(PaletteDecodeError::EmptyPalette);
    }
    if palette.len() > 256 {
        return Err(PaletteDecodeError::PaletteTooLarge);
    }
    if voxel_indices.len() != SECTION_VOLUME {
        return Err(PaletteDecodeError::WrongVoxelCount {
            expected: SECTION_VOLUME,
            actual: voxel_indices.len(),
        });
    }

    let mut section = ChunkSection::new();
    for (index, palette_index) in voxel_indices.iter().copied().enumerate() {
        let state = *palette.get(usize::from(palette_index)).ok_or(
            PaletteDecodeError::PaletteIndexOutOfRange {
                voxel: index,
                index: palette_index,
                palette_len: palette.len(),
            },
        )?;
        let local = LocalBlockPosition {
            x: (index % 16) as u8,
            y: (index / 256) as u8,
            z: ((index / 16) % 16) as u8,
        };
        section.set(local, state);
    }
    Ok(section)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_chunk_payload(section_y: i8, indices: &[u8]) -> Vec<u8> {
        let mut writer = PacketWriter::new();
        writer.write_i32_be(-2);
        writer.write_i32_be(3);
        writer.write_varint(1);
        writer.write_bytes(&[section_y as u8]);
        writer.write_varint(2);
        writer.write_varint(0);
        writer.write_varint(77);
        writer.write_bytes(indices);
        writer.into_bytes()
    }

    #[test]
    fn decodes_reference_chunk_packet_and_installs_section() {
        let mut indices = vec![0; SECTION_VOLUME];
        indices[LocalBlockPosition { x: 15, y: 4, z: 7 }.index()] = 1;
        let payload = reference_chunk_payload(-1, &indices);
        let decoded = decode_reference_chunk_packet(&payload).unwrap();
        assert_eq!((decoded.chunk_x, decoded.chunk_z), (-2, 3));
        assert_eq!(decoded.sections.len(), 1);
        assert_eq!(decoded.sections[0].section_y, -1);

        let mut world = World::new();
        apply_reference_chunk_packet(&mut world, &payload).unwrap();
        assert_eq!(
            world.get_block(crate::world::BlockPosition::new(-17, -12, 55)),
            BlockStateId(77)
        );
    }

    #[test]
    fn rejects_invalid_reference_chunk_without_partial_world_mutation() {
        let mut indices = vec![0; SECTION_VOLUME];
        indices[400] = 2; // palette has only two entries
        let payload = reference_chunk_payload(0, &indices);
        let mut world = World::new();
        assert!(matches!(
            apply_reference_chunk_packet(&mut world, &payload),
            Err(ReferenceChunkPacketError::Palette(
                PaletteDecodeError::PaletteIndexOutOfRange { .. }
            ))
        ));
        assert_eq!(world.section_count(), 0);
    }

    #[test]
    fn rejects_trailing_bytes_and_invalid_section_counts() {
        let indices = vec![0; SECTION_VOLUME];
        let mut payload = reference_chunk_payload(0, &indices);
        payload.push(0xff);
        assert_eq!(
            decode_reference_chunk_packet(&payload),
            Err(ReferenceChunkPacketError::TrailingBytes(1))
        );

        let mut invalid = PacketWriter::new();
        invalid.write_i32_be(0);
        invalid.write_i32_be(0);
        invalid.write_varint(65);
        assert_eq!(
            decode_reference_chunk_packet(&invalid.into_bytes()),
            Err(ReferenceChunkPacketError::InvalidSectionCount(65))
        );
    }

    #[test]
    fn decodes_palette_indices_in_yzx_order() {
        let palette = [BlockStateId(0), BlockStateId(7), BlockStateId(42)];
        let mut indices = vec![0; SECTION_VOLUME];
        indices[0] = 1; // x=0, y=0, z=0
        indices[1] = 2; // x=1, y=0, z=0
        indices[16] = 1; // x=0, y=0, z=1
        indices[256] = 2; // x=0, y=1, z=0
        let section = decode_reference_palette_section(&palette, &indices).unwrap();
        assert_eq!(section.get(LocalBlockPosition { x: 0, y: 0, z: 0 }), BlockStateId(7));
        assert_eq!(section.get(LocalBlockPosition { x: 1, y: 0, z: 0 }), BlockStateId(42));
        assert_eq!(section.get(LocalBlockPosition { x: 0, y: 0, z: 1 }), BlockStateId(7));
        assert_eq!(section.get(LocalBlockPosition { x: 0, y: 1, z: 0 }), BlockStateId(42));
    }

    #[test]
    fn rejects_invalid_palette_and_payload_lengths() {
        let indices = vec![0; SECTION_VOLUME];
        assert_eq!(
            decode_reference_palette_section(&[], &indices),
            Err(PaletteDecodeError::EmptyPalette)
        );
        assert_eq!(
            decode_reference_palette_section(&[BlockStateId(0)], &indices[..10]),
            Err(PaletteDecodeError::WrongVoxelCount { expected: SECTION_VOLUME, actual: 10 })
        );
        assert_eq!(
            decode_reference_palette_section(&[BlockStateId(0)], &{
                let mut data = indices.clone();
                data[123] = 1;
                data
            }),
            Err(PaletteDecodeError::PaletteIndexOutOfRange { voxel: 123, index: 1, palette_len: 1 })
        );
    }

    #[test]
    fn decoded_reference_section_is_installed_and_marks_mesh_dirty() {
        let mut world = World::new();
        let position = SectionPosition::new(-1, 2, 3);
        let palette = [BlockStateId(0), BlockStateId(77)];
        let mut indices = vec![0; SECTION_VOLUME];
        indices[LocalBlockPosition { x: 15, y: 4, z: 7 }.index()] = 1;

        decode_reference_palette_section_into_world(
            &mut world, position, &palette, &indices,
        ).unwrap();

        let world_position = crate::world::BlockPosition::new(-1, 36, 55);
        assert_eq!(world.get_block(world_position), BlockStateId(77));
        assert_eq!(world.take_dirty_sections(), vec![position]);
    }

    #[test]
    fn invalid_reference_payload_does_not_mutate_world() {
        let mut world = World::new();
        let position = SectionPosition::new(0, 0, 0);
        let palette = [BlockStateId(0)];
        let mut indices = vec![0; SECTION_VOLUME];
        indices[5] = 1;

        assert!(decode_reference_palette_section_into_world(
            &mut world, position, &palette, &indices,
        ).is_err());
        assert!(!world.is_section_loaded(position));
        assert!(world.take_dirty_sections().is_empty());
    }

    #[test]
    fn supports_all_byte_palette_indices() {
        let palette: Vec<_> = (0..=255).map(BlockStateId).collect();
        let mut indices = vec![0; SECTION_VOLUME];
        indices[4095] = 255;
        let section = decode_reference_palette_section(&palette, &indices).unwrap();
        assert_eq!(section.get(LocalBlockPosition { x: 15, y: 15, z: 15 }), BlockStateId(255));
    }
}
