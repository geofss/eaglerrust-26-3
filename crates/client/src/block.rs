//! Prototype block definitions and namespaced registry infrastructure.
//! The built-in IDs remain project-local placeholders, not verified 26.3 IDs.
use crate::world::{BlockStateId, AIR, DIRT, GRASS_BLOCK, LEAVES, LOG, SAND, STONE, WATER};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLayer { Opaque, Cutout, Translucent, Invisible }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionShapeKind { Empty, FullCube, LowerSlab }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDefinition {
    pub id: BlockStateId,
    /// Namespaced resource key; kept separate from display text.
    pub identifier: &'static str,
    pub name: &'static str,
    pub render_layer: RenderLayer,
    pub solid: bool,
    pub replaceable: bool,
    pub hardness_tenths: u16,
    pub collision: CollisionShapeKind,
}

#[derive(Debug, Clone)]
pub struct BlockRegistry { definitions: Vec<BlockDefinition> }

/// Internal-only test/prototype state. Never serialize as a 26.3 registry ID.
pub const PROTOTYPE_LOWER_SLAB: BlockStateId = BlockStateId(1000);

/// Fast prototype collision lookup used by the physics hot path.
pub fn is_solid_state(id: BlockStateId) -> bool {
    id != AIR && id != WATER
}

/// Collision behavior for the current prototype registry.
pub fn collision_shape_for_state(id: BlockStateId) -> CollisionShapeKind {
    if id == AIR || id == WATER { CollisionShapeKind::Empty }
    else if id == PROTOTYPE_LOWER_SLAB { CollisionShapeKind::LowerSlab }
    else { CollisionShapeKind::FullCube }
}

impl BlockRegistry {
    /// Prototype registry only; these numeric IDs are not official 26.3 IDs.
    pub fn prototype() -> Self {
        let definitions = vec![
            BlockDefinition { id: AIR, identifier: "minecraft:air", name: "air", render_layer: RenderLayer::Invisible, solid: false, replaceable: true, hardness_tenths: 0, collision: CollisionShapeKind::Empty },
            BlockDefinition { id: GRASS_BLOCK, identifier: "minecraft:grass_block", name: "grass_block", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 6, collision: CollisionShapeKind::FullCube },
            BlockDefinition { id: DIRT, identifier: "minecraft:dirt", name: "dirt", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 5, collision: CollisionShapeKind::FullCube },
            BlockDefinition { id: STONE, identifier: "minecraft:stone", name: "stone", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 15, collision: CollisionShapeKind::FullCube },
            BlockDefinition { id: WATER, identifier: "minecraft:water", name: "water", render_layer: RenderLayer::Translucent, solid: false, replaceable: true, hardness_tenths: 0, collision: CollisionShapeKind::Empty },
            BlockDefinition { id: SAND, identifier: "minecraft:sand", name: "sand", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 5, collision: CollisionShapeKind::FullCube },
            BlockDefinition { id: LOG, identifier: "minecraft:oak_log", name: "oak_log", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 20, collision: CollisionShapeKind::FullCube },
            BlockDefinition { id: LEAVES, identifier: "minecraft:oak_leaves", name: "oak_leaves", render_layer: RenderLayer::Cutout, solid: true, replaceable: false, hardness_tenths: 2, collision: CollisionShapeKind::FullCube },
            BlockDefinition { id: PROTOTYPE_LOWER_SLAB, identifier: "eaglerrust:prototype_lower_slab", name: "prototype_lower_slab", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 20, collision: CollisionShapeKind::LowerSlab },
        ];
        Self { definitions }
    }

    pub fn get(&self, id: BlockStateId) -> Option<&BlockDefinition> {
        self.definitions.iter().find(|definition| definition.id == id)
    }

    pub fn get_by_identifier(&self, identifier: &str) -> Option<&BlockDefinition> {
        self.definitions.iter().find(|definition| definition.identifier == identifier)
    }

    pub fn is_solid(&self, id: BlockStateId) -> bool {
        self.get(id).map(|definition| definition.solid).unwrap_or(true)
    }

    pub fn is_replaceable(&self, id: BlockStateId) -> bool {
        self.get(id).map(|definition| definition.replaceable).unwrap_or(false)
    }

    pub fn iter(&self) -> impl Iterator<Item = &BlockDefinition> {
        self.definitions.iter()
    }
}

impl Default for BlockRegistry {
    fn default() -> Self { Self::prototype() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prototype_registry_distinguishes_air_water_and_solid_blocks() {
        let registry = BlockRegistry::prototype();
        assert!(!registry.is_solid(AIR));
        assert!(!registry.is_solid(WATER));
        assert!(registry.is_solid(STONE));
        assert!(registry.is_replaceable(WATER));
        assert_eq!(registry.get(STONE).unwrap().name, "stone");
    }

    #[test]
    fn registry_supports_namespaced_lookup_and_collision_metadata() {
        let registry = BlockRegistry::prototype();
        assert_eq!(registry.get_by_identifier("minecraft:oak_log").unwrap().id, LOG);
        assert_eq!(registry.get_by_identifier("minecraft:unknown"), None);
        assert_eq!(registry.get(PROTOTYPE_LOWER_SLAB).unwrap().collision, CollisionShapeKind::LowerSlab);
        assert_eq!(collision_shape_for_state(WATER), CollisionShapeKind::Empty);
    }
}
