//! Portable world primitives and a canonical in-memory chunk-section layout.
//!
//! This layout is an internal Rust representation, not a claim about the
//! 26.3 wire format. Protocol decoding must translate into these types.

pub const SECTION_EDGE: i32 = 16;
pub const SECTION_VOLUME: usize = 16 * 16 * 16;

/// Stable identifier for a registered block state. Zero is reserved as air
/// in this initial internal model; the 26.3 registry mapping remains pending.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct BlockStateId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPosition {
    pub x: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SectionPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalBlockPosition {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl BlockPosition {
    pub const fn new(x: i32, y: i32, z: i32) -> Self { Self { x, y, z } }

    /// Horizontal chunk containing this block. Euclidean division is
    /// important: coordinates west/north of zero belong to negative chunks.
    pub fn chunk(self) -> ChunkPosition {
        ChunkPosition::new(
            self.x.div_euclid(SECTION_EDGE),
            self.z.div_euclid(SECTION_EDGE),
        )
    }

    /// 16x16x16 section containing this block.
    pub fn section(self) -> SectionPosition {
        SectionPosition::new(
            self.x.div_euclid(SECTION_EDGE),
            self.y.div_euclid(SECTION_EDGE),
            self.z.div_euclid(SECTION_EDGE),
        )
    }

    /// Coordinates inside the containing section, always in 0..16.
    pub fn local(self) -> LocalBlockPosition {
        LocalBlockPosition {
            x: self.x.rem_euclid(SECTION_EDGE) as u8,
            y: self.y.rem_euclid(SECTION_EDGE) as u8,
            z: self.z.rem_euclid(SECTION_EDGE) as u8,
        }
    }
}

impl ChunkPosition {
    pub const fn new(x: i32, z: i32) -> Self { Self { x, z } }
}

impl SectionPosition {
    pub const fn new(x: i32, y: i32, z: i32) -> Self { Self { x, y, z } }
}

impl LocalBlockPosition {
    /// Canonical in-memory index: y * 256 + z * 16 + x.
    pub fn index(self) -> usize {
        usize::from(self.y) * 256 + usize::from(self.z) * 16 + usize::from(self.x)
    }
}

/// Dense 16-cubed block-state storage for one section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkSection {
    blocks: Vec<BlockStateId>,
}

impl Default for ChunkSection {
    fn default() -> Self { Self::new() }
}

impl ChunkSection {
    pub fn new() -> Self {
        Self { blocks: vec![BlockStateId(0); SECTION_VOLUME] }
    }

    pub fn get(&self, local: LocalBlockPosition) -> BlockStateId {
        self.blocks[local.index()]
    }

    pub fn set(&mut self, local: LocalBlockPosition, state: BlockStateId) {
        self.blocks[local.index()] = state;
    }

    pub fn as_slice(&self) -> &[BlockStateId] { &self.blocks }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_coordinates_use_euclidean_chunk_math() {
        let position = BlockPosition::new(-1, -17, -16);
        assert_eq!(position.chunk(), ChunkPosition::new(-1, -1));
        assert_eq!(position.section(), SectionPosition::new(-1, -2, -1));
        assert_eq!(position.local(), LocalBlockPosition { x: 15, y: 15, z: 0 });
    }

    #[test]
    fn positive_section_boundaries_are_exact() {
        let position = BlockPosition::new(16, 32, 31);
        assert_eq!(position.section(), SectionPosition::new(1, 2, 1));
        assert_eq!(position.local(), LocalBlockPosition { x: 0, y: 0, z: 15 });
    }

    #[test]
    fn section_storage_round_trips_state_ids() {
        let local = LocalBlockPosition { x: 3, y: 7, z: 11 };
        let mut section = ChunkSection::new();
        assert_eq!(section.get(local), BlockStateId(0));
        section.set(local, BlockStateId(42));
        assert_eq!(section.get(local), BlockStateId(42));
        assert_eq!(section.as_slice().len(), SECTION_VOLUME);
    }

    #[test]
    fn every_local_coordinate_maps_inside_section() {
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let index = LocalBlockPosition { x, y, z }.index();
                    assert!(index < SECTION_VOLUME);
                }
            }
        }
    }
}
