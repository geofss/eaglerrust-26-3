//! Internal block definitions. IDs here are placeholders until the 26.3 registry is audited.
use crate::world::{BlockStateId, AIR, DIRT, GRASS_BLOCK, LEAVES, LOG, SAND, STONE, WATER};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLayer { Opaque, Cutout, Translucent, Invisible }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDefinition {
    pub id: BlockStateId,
    pub name: &'static str,
    pub render_layer: RenderLayer,
    pub solid: bool,
    pub replaceable: bool,
    pub hardness_tenths: u16,
}

#[derive(Debug, Clone)]
pub struct BlockRegistry { definitions: Vec<BlockDefinition> }

/// Fast prototype collision lookup used by the physics hot path.
/// Unknown IDs remain solid until the official 26.3 registry is mapped.
pub fn is_solid_state(id: BlockStateId) -> bool {
    id != AIR && id != WATER
}

impl BlockRegistry {
    /// Prototype registry only; these numeric IDs are not official 26.3 IDs.
    pub fn prototype() -> Self {
        let definitions = vec![
            BlockDefinition { id: AIR, name: "air", render_layer: RenderLayer::Invisible, solid: false, replaceable: true, hardness_tenths: 0 },
            BlockDefinition { id: GRASS_BLOCK, name: "grass_block", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 6 },
            BlockDefinition { id: DIRT, name: "dirt", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 5 },
            BlockDefinition { id: STONE, name: "stone", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 15 },
            BlockDefinition { id: WATER, name: "water", render_layer: RenderLayer::Translucent, solid: false, replaceable: true, hardness_tenths: 0 },
            BlockDefinition { id: SAND, name: "sand", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 5 },
            BlockDefinition { id: LOG, name: "log", render_layer: RenderLayer::Opaque, solid: true, replaceable: false, hardness_tenths: 20 },
            BlockDefinition { id: LEAVES, name: "leaves", render_layer: RenderLayer::Cutout, solid: true, replaceable: false, hardness_tenths: 2 },
        ];
        Self { definitions }
    }

    pub fn get(&self, id: BlockStateId) -> Option<&BlockDefinition> {
        self.definitions.iter().find(|definition| definition.id == id)
    }

    pub fn is_solid(&self, id: BlockStateId) -> bool {
        self.get(id).map(|definition| definition.solid).unwrap_or(true)
    }

    pub fn is_replaceable(&self, id: BlockStateId) -> bool {
        self.get(id).map(|definition| definition.replaceable).unwrap_or(false)
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
}
