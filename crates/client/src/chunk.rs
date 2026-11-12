//! Chunk-column streaming policy for the local prototype world.
//! The 16x16 column format is an internal representation, not the 26.3 packet format.
use std::collections::HashSet;
use crate::world::{ChunkPosition, SectionPosition, TerrainGenerator, World};

#[derive(Debug, Clone)]
pub struct ChunkManager {
    radius: i32,
    loaded: HashSet<ChunkPosition>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChunkUpdate {
    pub loaded_columns: usize,
    pub unloaded_columns: usize,
}

impl ChunkManager {
    pub fn new(radius: i32) -> Self {
        Self { radius: radius.max(0), loaded: HashSet::new() }
    }

    pub fn radius(&self) -> i32 { self.radius }
    pub fn loaded_columns(&self) -> usize { self.loaded.len() }

    /// Keep a square of columns around the player's current chunk loaded.
    /// Generation is deterministic and each new column is generated once.
    pub fn update(
        &mut self,
        world: &mut World,
        center: ChunkPosition,
        generator: &TerrainGenerator,
    ) -> ChunkUpdate {
        let mut desired = HashSet::new();
        for z in (center.z - self.radius)..=(center.z + self.radius) {
            for x in (center.x - self.radius)..=(center.x + self.radius) {
                desired.insert(ChunkPosition::new(x, z));
            }
        }

        let obsolete: Vec<_> = self.loaded.difference(&desired).copied().collect();
        let mut result = ChunkUpdate::default();
        for column in obsolete {
            let sections: Vec<_> = world.loaded_sections().into_iter()
                .filter(|section| section.x == column.x && section.z == column.z)
                .collect();
            for section in sections { world.remove_section(section); }
            self.loaded.remove(&column);
            result.unloaded_columns += 1;
        }

        for column in desired {
            if self.loaded.contains(&column) { continue; }
            generator.generate_region(world, column.x * 16, column.z * 16, 16, 16);
            self.loaded.insert(column);
            result.loaded_columns += 1;
        }
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
        let second = manager.update(&mut world, ChunkPosition::new(0, 0), &generator);
        assert_eq!(second.loaded_columns, 0);
        assert_eq!(second.unloaded_columns, 0);
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
        let sections = world.loaded_sections();
        assert!(sections.iter().any(|section| section.x == 2));
        assert!(!sections.iter().any(|section| section.x == -1));
    }
}
