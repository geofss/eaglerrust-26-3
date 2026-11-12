//! Decoder for the compact section payload documented by the supplied
//! EaglercraftX 26.1.2 reference gateway.
//!
//! This is deliberately isolated from the 26.3 network path: the reference
//! format stores one u8 palette index per voxel (4096 bytes per section).
//! It is NOT the vanilla packed-long PalettedContainer format and must not be
//! used for a 26.3 packet until that target format is verified.
use crate::world::{BlockStateId, ChunkSection, LocalBlockPosition, SectionPosition, World, SECTION_VOLUME};

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
