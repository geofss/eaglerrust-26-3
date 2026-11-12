//! Portable world primitives and sparse in-memory chunk-section storage.
//!
//! This layout is an internal Rust representation, not a claim about the
//! 26.3 wire format. Protocol decoding must translate into these types.

pub const SECTION_EDGE: i32 = 16;
pub const SECTION_VOLUME: usize = 16 * 16 * 16;
pub const AIR: BlockStateId = BlockStateId(0);

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

/// Sparse world storage indexed by section coordinates.
/// Missing sections read as air; this is an internal model, not the 26.3 wire format.
#[derive(Debug, Default, Clone)]
pub struct World {
    sections: std::collections::HashMap<SectionPosition, ChunkSection>,
    dirty_sections: std::collections::HashSet<SectionPosition>,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read a block in world coordinates. Unloaded sections are treated as air.
    pub fn get_block(&self, position: BlockPosition) -> BlockStateId {
        self.sections
            .get(&position.section())
            .map(|section| section.get(position.local()))
            .unwrap_or(AIR)
    }

    /// Write a block and mark every loaded section whose mesh may have changed.
    /// Returns false when the requested state was already present.
    pub fn set_block(&mut self, position: BlockPosition, state: BlockStateId) -> bool {
        let section_pos = position.section();
        let local = position.local();

        if !self.sections.contains_key(&section_pos) && state == AIR {
            return false;
        }

        let section = self.sections.entry(section_pos).or_default();
        if section.get(local) == state {
            return false;
        }
        section.set(local, state);
        self.dirty_sections.insert(section_pos);

        // A boundary edit can change the visible faces of a loaded neighbour.
        if local.x == 0 {
            self.mark_loaded_section_dirty(SectionPosition::new(section_pos.x - 1, section_pos.y, section_pos.z));
        }
        if local.x == 15 {
            self.mark_loaded_section_dirty(SectionPosition::new(section_pos.x + 1, section_pos.y, section_pos.z));
        }
        if local.y == 0 {
            self.mark_loaded_section_dirty(SectionPosition::new(section_pos.x, section_pos.y - 1, section_pos.z));
        }
        if local.y == 15 {
            self.mark_loaded_section_dirty(SectionPosition::new(section_pos.x, section_pos.y + 1, section_pos.z));
        }
        if local.z == 0 {
            self.mark_loaded_section_dirty(SectionPosition::new(section_pos.x, section_pos.y, section_pos.z - 1));
        }
        if local.z == 15 {
            self.mark_loaded_section_dirty(SectionPosition::new(section_pos.x, section_pos.y, section_pos.z + 1));
        }
        true
    }

    fn mark_loaded_section_dirty(&mut self, position: SectionPosition) {
        if self.sections.contains_key(&position) {
            self.dirty_sections.insert(position);
        }
    }

    /// Remove a loaded section and invalidate adjacent loaded section meshes.
    pub fn remove_section(&mut self, position: SectionPosition) -> bool {
        if self.sections.remove(&position).is_none() {
            return false;
        }
        self.dirty_sections.remove(&position);
        self.mark_loaded_section_dirty(SectionPosition::new(position.x - 1, position.y, position.z));
        self.mark_loaded_section_dirty(SectionPosition::new(position.x + 1, position.y, position.z));
        self.mark_loaded_section_dirty(SectionPosition::new(position.x, position.y - 1, position.z));
        self.mark_loaded_section_dirty(SectionPosition::new(position.x, position.y + 1, position.z));
        self.mark_loaded_section_dirty(SectionPosition::new(position.x, position.y, position.z - 1));
        self.mark_loaded_section_dirty(SectionPosition::new(position.x, position.y, position.z + 1));
        true
    }

    /// Return dirty section positions in deterministic order and clear the queue.
    pub fn take_dirty_sections(&mut self) -> Vec<SectionPosition> {
        let mut dirty: Vec<_> = self.dirty_sections.drain().collect();
        dirty.sort_by_key(|position| (position.x, position.y, position.z));
        dirty
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    /// List loaded section coordinates in stable order for deterministic iteration.
    pub fn loaded_sections(&self) -> Vec<SectionPosition> {
        let mut positions: Vec<_> = self.sections.keys().copied().collect();
        positions.sort_by_key(|position| (position.x, position.y, position.z));
        positions
    }

    pub fn is_section_loaded(&self, position: SectionPosition) -> bool {
        self.sections.contains_key(&position)
    }

    pub fn get_section(&self, position: SectionPosition) -> Option<&ChunkSection> {
        self.sections.get(&position)
    }
}


/// Placeholder block IDs until the 26.3 block-state registry is mapped.
pub const GRASS_BLOCK: BlockStateId = BlockStateId(1);
pub const DIRT: BlockStateId = BlockStateId(2);
pub const STONE: BlockStateId = BlockStateId(3);
pub const WATER: BlockStateId = BlockStateId(4);
pub const SAND: BlockStateId = BlockStateId(5);
pub const LOG: BlockStateId = BlockStateId(6);
pub const LEAVES: BlockStateId = BlockStateId(7);
pub const SEA_LEVEL: i32 = 5;

/// Deterministic heightmap generator for a bounded prototype region.
#[derive(Debug, Clone, Copy)]
pub struct TerrainGenerator {
    seed: u32,
    base_height: i32,
    amplitude: i32,
}

impl TerrainGenerator {
    pub const fn new(seed: u32, base_height: i32, amplitude: i32) -> Self {
        Self { seed, base_height, amplitude }
    }

    /// Generate terrain, shorelines, shallow water, and deterministic tree features.
    pub fn generate_region(
        &self,
        world: &mut World,
        origin_x: i32,
        origin_z: i32,
        width: u32,
        depth: u32,
    ) -> usize {
        let mut written = 0;
        for dz in 0..depth {
            for dx in 0..width {
                let x = origin_x + dx as i32;
                let z = origin_z + dz as i32;
                let height = self.height_at(x, z).max(1);
                let surface = if height <= SEA_LEVEL + 1 { SAND } else { GRASS_BLOCK };

                for y in 0..=height {
                    let state = if y == height {
                        surface
                    } else if y >= height - 2 {
                        DIRT
                    } else {
                        STONE
                    };
                    if world.set_block(BlockPosition::new(x, y, z), state) {
                        written += 1;
                    }
                }

                if height < SEA_LEVEL {
                    for y in (height + 1)..=SEA_LEVEL {
                        if world.set_block(BlockPosition::new(x, y, z), WATER) {
                            written += 1;
                        }
                    }
                }
            }
        }

        // Features are placed after terrain so their bases never get overwritten.
        for dz in 2..depth.saturating_sub(2) {
            for dx in 2..width.saturating_sub(2) {
                let x = origin_x + dx as i32;
                let z = origin_z + dz as i32;
                let ground = self.height_at(x, z).max(1);
                if ground < SEA_LEVEL + 1 || self.feature_hash(x, z) % 47 != 0 {
                    continue;
                }

                for dz_leaf in -2_i32..=2 {
                    for dx_leaf in -2_i32..=2 {
                        for dy_leaf in 3_i32..=5 {
                            let radius = dx_leaf.abs() + dz_leaf.abs() + (dy_leaf - 4).abs();
                            if radius <= 4 {
                                let position = BlockPosition::new(x + dx_leaf, ground + dy_leaf, z + dz_leaf);
                                if world.set_block(position, LEAVES) {
                                    written += 1;
                                }
                            }
                        }
                    }
                }

                for dy in 1..=4 {
                    if world.set_block(BlockPosition::new(x, ground + dy, z), LOG) {
                        written += 1;
                    }
                }
            }
        }
        written
    }

    fn feature_hash(&self, x: i32, z: i32) -> u32 {
        let mut hash = self.seed
            ^ (x as u32).wrapping_mul(0x9E37_79B9)
            ^ (z as u32).wrapping_mul(0x85EB_CA6B)
            ^ 0xA511_E9B3;
        hash ^= hash >> 16;
        hash = hash.wrapping_mul(0x7FEB_352D);
        hash ^ (hash >> 15)
    }

    /// Smooth deterministic value noise sampled on an eight-block grid.
    pub fn height_at(&self, x: i32, z: i32) -> i32 {
        let cell_x = x.div_euclid(8);
        let cell_z = z.div_euclid(8);
        let local_x = x.rem_euclid(8) as f32 / 8.0;
        let local_z = z.rem_euclid(8) as f32 / 8.0;
        let smooth_x = local_x * local_x * (3.0 - 2.0 * local_x);
        let smooth_z = local_z * local_z * (3.0 - 2.0 * local_z);
        let a = self.lattice_height(cell_x, cell_z) as f32;
        let b = self.lattice_height(cell_x + 1, cell_z) as f32;
        let c = self.lattice_height(cell_x, cell_z + 1) as f32;
        let d = self.lattice_height(cell_x + 1, cell_z + 1) as f32;
        let north = a + (b - a) * smooth_x;
        let south = c + (d - c) * smooth_x;
        let broad = (north + (south - north) * smooth_z).round() as i32;
        let detail = if self.amplitude == 0 { 0 } else { (self.feature_hash(x, z) % 3) as i32 - 1 };
        broad + detail
    }

    fn lattice_height(&self, x: i32, z: i32) -> i32 {
        let mut hash = self.seed
            ^ (x as u32).wrapping_mul(0x9E37_79B9)
            ^ (z as u32).wrapping_mul(0x85EB_CA6B);
        hash ^= hash >> 16;
        hash = hash.wrapping_mul(0x7FEB_352D);
        hash ^= hash >> 15;
        let span = self.amplitude.saturating_mul(2).saturating_add(1).max(1) as u32;
        self.base_height + (hash % span) as i32 - self.amplitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_region_contains_water_and_sand_shorelines() {
        let generator = TerrainGenerator::new(0xEA61_2026, 5, 3);
        let mut world = World::new();
        generator.generate_region(&mut world, -24, -24, 48, 48);

        let mut found_water = false;
        let mut found_sand = false;
        for z in -24..24 {
            for x in -24..24 {
                for y in 0..=SEA_LEVEL {
                    let state = world.get_block(BlockPosition::new(x, y, z));
                    found_water |= state == WATER;
                    found_sand |= state == SAND;
                }
            }
        }
        assert!(found_water);
        assert!(found_sand);
    }

    #[test]
    fn generated_region_places_deterministic_trees() {
        let generator = TerrainGenerator::new(0xEA61_2026, 5, 3);
        let mut first = World::new();
        let mut second = World::new();
        generator.generate_region(&mut first, -24, -24, 48, 48);
        generator.generate_region(&mut second, -24, -24, 48, 48);

        let mut logs = 0;
        let mut leaves = 0;
        for y in 0..16 {
            for z in -24..24 {
                for x in -24..24 {
                    let position = BlockPosition::new(x, y, z);
                    let state = first.get_block(position);
                    logs += usize::from(state == LOG);
                    leaves += usize::from(state == LEAVES);
                    assert_eq!(state, second.get_block(position));
                }
            }
        }
        assert!(logs > 0);
        assert!(leaves > 0);
    }

    #[test]
    fn block_edits_mark_the_section_dirty_only_when_changed() {
        let mut world = World::new();
        let position = BlockPosition::new(3, 4, 5);
        assert!(world.set_block(position, BlockStateId(2)));
        assert!(!world.set_block(position, BlockStateId(2)));
        assert_eq!(world.take_dirty_sections(), vec![SectionPosition::new(0, 0, 0)]);
        assert!(world.take_dirty_sections().is_empty());
    }

    #[test]
    fn boundary_edits_dirty_loaded_neighbour_sections() {
        let mut world = World::new();
        world.set_block(BlockPosition::new(15, 4, 4), BlockStateId(1));
        world.set_block(BlockPosition::new(16, 4, 4), BlockStateId(1));
        let dirty = world.take_dirty_sections();
        assert_eq!(dirty, vec![SectionPosition::new(0, 0, 0), SectionPosition::new(1, 0, 0)]);

        world.set_block(BlockPosition::new(15, 4, 4), BlockStateId(2));
        assert_eq!(world.take_dirty_sections(), vec![SectionPosition::new(0, 0, 0), SectionPosition::new(1, 0, 0)]);
    }

    #[test]
    fn loaded_sections_are_returned_in_stable_order() {
        let mut world = World::new();
        world.set_block(BlockPosition::new(32, 0, 0), BlockStateId(1));
        world.set_block(BlockPosition::new(-16, 0, 0), BlockStateId(1));
        world.set_block(BlockPosition::new(0, 16, 0), BlockStateId(1));

        let expected = vec![
            SectionPosition::new(-1, 0, 0),
            SectionPosition::new(0, 1, 0),
            SectionPosition::new(2, 0, 0),
        ];
        assert_eq!(world.loaded_sections(), expected);
        assert!(world.is_section_loaded(SectionPosition::new(-1, 0, 0)));
        assert!(!world.is_section_loaded(SectionPosition::new(99, 0, 0)));
    }

    #[test]
    fn removing_section_invalidates_loaded_neighbours() {
        let mut world = World::new();
        world.set_block(BlockPosition::new(15, 1, 1), BlockStateId(1));
        world.set_block(BlockPosition::new(16, 1, 1), BlockStateId(1));
        world.take_dirty_sections();

        assert!(world.remove_section(SectionPosition::new(1, 0, 0)));
        assert_eq!(world.section_count(), 1);
        assert_eq!(world.take_dirty_sections(), vec![SectionPosition::new(0, 0, 0)]);
    }

    #[test]
    fn terrain_generation_is_deterministic_for_a_seed() {
        let first = TerrainGenerator::new(12345, 5, 3);
        let second = TerrainGenerator::new(12345, 5, 3);
        for z in -12..12 {
            for x in -12..12 {
                assert_eq!(first.height_at(x, z), second.height_at(x, z));
            }
        }
    }

    #[test]
    fn terrain_generation_writes_grass_dirt_and_stone_layers() {
        let generator = TerrainGenerator::new(7, 6, 0);
        let mut world = World::new();
        let written = generator.generate_region(&mut world, 0, 0, 2, 2);
        assert_eq!(written, 2 * 2 * 7);
        assert_eq!(world.get_block(BlockPosition::new(0, 6, 0)), GRASS_BLOCK);
        assert_eq!(world.get_block(BlockPosition::new(0, 5, 0)), DIRT);
        assert_eq!(world.get_block(BlockPosition::new(0, 1, 0)), STONE);
    }

    #[test]
    fn thirty_two_block_region_spans_four_horizontal_sections() {
        let generator = TerrainGenerator::new(2026, 5, 2);
        let mut world = World::new();
        generator.generate_region(&mut world, 0, 0, 32, 32);

        assert_eq!(world.section_count(), 4);
        assert!(world.is_section_loaded(SectionPosition::new(0, 0, 0)));
        assert!(world.is_section_loaded(SectionPosition::new(1, 0, 0)));
        assert!(world.is_section_loaded(SectionPosition::new(0, 0, 1)));
        assert!(world.is_section_loaded(SectionPosition::new(1, 0, 1)));
    }

    #[test]
    fn terrain_generation_supports_negative_world_coordinates() {
        let generator = TerrainGenerator::new(99, 9, 1);
        let mut world = World::new();
        generator.generate_region(&mut world, -2, -2, 2, 2);
        let height = generator.height_at(-1, -1).max(1);
        assert_eq!(world.get_block(BlockPosition::new(-1, height, -1)), GRASS_BLOCK);
        assert!(!world.loaded_sections().is_empty());
    }

    #[test]
    fn world_reads_missing_sections_as_air() {
        let world = World::new();
        assert_eq!(world.get_block(BlockPosition::new(40, 8, -20)), AIR);
        assert_eq!(world.section_count(), 0);
    }

    #[test]
    fn world_stores_blocks_across_section_boundaries() {
        let mut world = World::new();
        let first = BlockPosition::new(15, 15, 15);
        let second = BlockPosition::new(16, 16, 16);

        world.set_block(first, BlockStateId(3));
        world.set_block(second, BlockStateId(7));

        assert_eq!(world.get_block(first), BlockStateId(3));
        assert_eq!(world.get_block(second), BlockStateId(7));
        assert_eq!(world.section_count(), 2);
    }

    #[test]
    fn world_storage_handles_negative_world_positions() {
        let mut world = World::new();
        let position = BlockPosition::new(-1, -1, -17);
        world.set_block(position, BlockStateId(9));

        assert_eq!(world.get_block(position), BlockStateId(9));
        assert_eq!(world.section_count(), 1);
    }

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
