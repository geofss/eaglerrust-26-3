//! Chunk-column streaming policy for the local prototype world.
//! The 16x16 column format is an internal representation, not the 26.3 packet format.
use std::collections::HashSet;

use crate::world::{ChunkPosition, TerrainGenerator, World};

#[derive(Debug, Clone)]
pub struct ChunkManager {
    radius: i32,
    loaded: HashSet<ChunkPosition>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChunkUpdate {
    pub loaded_columns: usize,
    pub unloaded_columns: usize,
    pub pending_loads: usize,
    pub pending_unloads: usize,
}

impl ChunkUpdate {
    pub fn has_pending_work(self) -> bool {
        self.pending_loads > 0 || self.pending_unloads > 0
    }
}

impl ChunkManager {
    pub fn new(radius: i32) -> Self {
        Self {
            radius: radius.max(0),
            loaded: HashSet::new(),
        }
    }

    pub fn radius(&self) -> i32 {
        self.radius
    }

    pub fn loaded_columns(&self) -> usize {
        self.loaded.len()
    }

    pub fn is_loaded(&self, position: ChunkPosition) -> bool {
        self.loaded.contains(&position)
    }

    /// Rebuild the loaded-column index after restoring a saved local world.
    pub fn adopt_loaded_world(&mut self, world: &World) {
        self.loaded.clear();
        for section in world.loaded_sections() {
            self.loaded.insert(ChunkPosition::new(section.x, section.z));
        }
    }

    /// Update without a work limit. Useful for initial world setup and tests.
    pub fn update(
        &mut self,
        world: &mut World,
        center: ChunkPosition,
        generator: &TerrainGenerator,
    ) -> ChunkUpdate {
        self.update_with_budget(world, center, generator, usize::MAX, usize::MAX)
    }

    /// Keep a square around the player loaded while limiting work per call.
    ///
    /// New columns are prioritized by distance, then stable Z/X order. Unloads
    /// are also stable. This keeps chunk generation from doing all travel work
    /// in one fixed simulation tick on slower devices.
    pub fn update_with_budget(
        &mut self,
        world: &mut World,
        center: ChunkPosition,
        generator: &TerrainGenerator,
        load_budget: usize,
        unload_budget: usize,
    ) -> ChunkUpdate {
        let min_x = center.x.saturating_sub(self.radius);
        let max_x = center.x.saturating_add(self.radius);
        let min_z = center.z.saturating_sub(self.radius);
        let max_z = center.z.saturating_add(self.radius);

        let mut desired = HashSet::new();
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                desired.insert(ChunkPosition::new(x, z));
            }
        }

        let mut result = ChunkUpdate::default();

        let mut obsolete: Vec<_> = self.loaded.difference(&desired).copied().collect();
        obsolete.sort_by_key(|column| (column.z, column.x));
        for column in obsolete.into_iter().take(unload_budget) {
            world.remove_column(column.x, column.z);
            self.loaded.remove(&column);
            result.unloaded_columns += 1;
        }

        let mut missing: Vec<_> = desired.difference(&self.loaded).copied().collect();
        missing.sort_by_key(|column| {
            let dx = i64::from(column.x) - i64::from(center.x);
            let dz = i64::from(column.z) - i64::from(center.z);
            (dx * dx + dz * dz, column.z, column.x)
        });
        for column in missing.into_iter().take(load_budget) {
            let origin_x = column.x.saturating_mul(16);
            let origin_z = column.z.saturating_mul(16);
            generator.generate_region(world, origin_x, origin_z, 16, 16);
            self.loaded.insert(column);
            result.loaded_columns += 1;
        }

        result.pending_loads = desired.difference(&self.loaded).count();
        result.pending_unloads = self.loaded.difference(&desired).count();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_one_loads_nine_columns_and_does_not_reload_stationary_chunks() {
        let mut world = World::new();
        let mut manager = ChunkManager::new(1);
        let generator = TerrainGenerator::new(42, 5, 2);
        let first = manager.update(&mut world, ChunkPosition::new(0, 0), &generator);
        assert_eq!(first.loaded_columns, 9);
        assert_eq!(manager.loaded_columns(), 9);
        assert!(!first.has_pending_work());

        let second = manager.update(&mut world, ChunkPosition::new(0, 0), &generator);
        assert_eq!(second.loaded_columns, 0);
        assert_eq!(second.unloaded_columns, 0);
        assert!(!second.has_pending_work());
    }

    #[test]
    fn moving_one_chunk_loads_and_unloads_only_the_outer_columns() {
        let mut world = World::new();
        let mut manager = ChunkManager::new(1);
        let generator = TerrainGenerator::new(7, 5, 1);
        manager.update(&mut world, ChunkPosition::new(0, 0), &generator);

        let update = manager.update(&mut world, ChunkPosition::new(1, 0), &generator);
        assert_eq!(update.loaded_columns, 3);
        assert_eq!(update.unloaded_columns, 3);
        assert_eq!(manager.loaded_columns(), 9);
        assert_eq!(update.pending_loads, 0);
        assert_eq!(update.pending_unloads, 0);
        assert!(manager.is_loaded(ChunkPosition::new(2, 0)));
        assert!(!manager.is_loaded(ChunkPosition::new(-1, 0)));
    }

    #[test]
    fn load_budget_spreads_generation_across_updates() {
        let mut world = World::new();
        let mut manager = ChunkManager::new(1);
        let generator = TerrainGenerator::new(19, 5, 2);
        let center = ChunkPosition::new(0, 0);

        let first = manager.update_with_budget(&mut world, center, &generator, 2, 1);
        assert_eq!(first.loaded_columns, 2);
        assert_eq!(first.pending_loads, 7);
        assert_eq!(first.pending_unloads, 0);
        assert!(first.has_pending_work());

        let mut total_loaded = first.loaded_columns;
        let mut update = first;
        for _ in 0..4 {
            update = manager.update_with_budget(&mut world, center, &generator, 2, 1);
            total_loaded += update.loaded_columns;
        }

        assert_eq!(total_loaded, 9);
        assert_eq!(manager.loaded_columns(), 9);
        assert_eq!(update.pending_loads, 0);
        assert!(!update.has_pending_work());
        let idle = manager.update_with_budget(&mut world, center, &generator, 2, 1);
        assert_eq!(idle.loaded_columns, 0);
        assert_eq!(idle.unloaded_columns, 0);
    }

    #[test]
    fn unload_budget_keeps_old_columns_until_they_are_processed() {
        let mut world = World::new();
        let mut manager = ChunkManager::new(1);
        let generator = TerrainGenerator::new(23, 5, 2);
        manager.update(&mut world, ChunkPosition::new(0, 0), &generator);

        let center = ChunkPosition::new(4, 0);
        let first = manager.update_with_budget(&mut world, center, &generator, 0, 1);
        assert_eq!(first.unloaded_columns, 1);
        assert_eq!(first.pending_unloads, 8);
        assert_eq!(first.pending_loads, 9);
        assert_eq!(manager.loaded_columns(), 8);
    }

    #[test]
    fn restored_world_columns_are_registered_without_regeneration() {
        let mut world = World::new();
        let generator = TerrainGenerator::new(31, 5, 2);
        let mut original = ChunkManager::new(0);
        original.update(&mut world, ChunkPosition::new(-2, 3), &generator);
        let expected = world.get_block(crate::world::BlockPosition::new(-32, 0, 48));

        let mut restored = ChunkManager::new(0);
        restored.adopt_loaded_world(&world);
        assert_eq!(restored.loaded_columns(), 1);
        let update = restored.update(&mut world, ChunkPosition::new(-2, 3), &generator);
        assert_eq!(update.loaded_columns, 0);
        assert_eq!(world.get_block(crate::world::BlockPosition::new(-32, 0, 48)), expected);
    }

    #[test]
    fn negative_chunk_coordinates_stream_without_aliasing_zero() {
        let mut world = World::new();
        let mut manager = ChunkManager::new(0);
        let generator = TerrainGenerator::new(29, 5, 1);

        manager.update(&mut world, ChunkPosition::new(-1, -1), &generator);
        assert!(manager.is_loaded(ChunkPosition::new(-1, -1)));
        assert!(!manager.is_loaded(ChunkPosition::new(0, 0)));
        assert!(world.section_count() > 0);
    }
}
