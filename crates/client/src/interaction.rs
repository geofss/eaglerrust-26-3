//! Block selection and deterministic mining progress for the prototype world.
use crate::block::BlockRegistry;
use crate::raycast::{cast_blocks, BlockHit};
use crate::world::{BlockPosition, BlockStateId, World, AIR};

pub fn target_block(world: &World, eye: [f64; 3], direction: [f64; 3], reach: f64) -> Option<BlockHit> {
    cast_blocks(world, eye, direction, reach)
}

pub fn break_block(world: &mut World, hit: BlockHit) -> bool {
    world.set_block(hit.position, AIR)
}

pub fn place_block(world: &mut World, hit: BlockHit, state: BlockStateId) -> bool {
    if hit.face == [0, 0, 0] || state == AIR { return false; }
    let target = BlockPosition::new(
        hit.position.x + hit.face[0],
        hit.position.y + hit.face[1],
        hit.position.z + hit.face[2],
    );
    if world.get_block(target) != AIR { return false; }
    world.set_block(target, state)
}

/// Result of advancing a held mining action by one simulation step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MiningUpdate {
    Idle,
    Started { position: BlockPosition },
    Progress { position: BlockPosition, progress: f32 },
    Aborted { position: BlockPosition },
    Completed { position: BlockPosition, state: BlockStateId },
}

/// Stateful hold-to-mine controller. Hardness values come from the explicitly
/// prototype-only registry; this is not yet Minecraft's complete tool/difficulty
/// calculation and must not be used as a 26.3 compatibility claim.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MiningController {
    target: Option<(BlockPosition, BlockStateId)>,
    elapsed_seconds: f32,
    started: bool,
}

impl MiningController {
    pub fn progress(&self) -> f32 {
        let Some((_, state)) = self.target else { return 0.0; };
        let hardness = BlockRegistry::prototype().get(state)
            .map(|block| block.hardness_tenths)
            .unwrap_or(20);
        if hardness == 0 { return 1.0; }
        (self.elapsed_seconds / mining_duration_seconds(hardness)).clamp(0.0, 1.0)
    }

    /// Advance mining. A target change or released input aborts the previous
    /// action; completion is emitted exactly once and resets the controller.
    pub fn tick(&mut self, hit: Option<BlockHit>, held: bool, delta_seconds: f32) -> MiningUpdate {
        let dt = if delta_seconds.is_finite() { delta_seconds.clamp(0.0, 0.1) } else { 0.0 };
        let next = if held {
            hit.map(|hit| (hit.position, hit.state))
        } else { None };

        if self.target != next {
            let previous = self.target.take();
            self.elapsed_seconds = 0.0;
            self.started = false;
            if let Some((position, _)) = previous {
                if next.is_none() { return MiningUpdate::Aborted { position }; }
            }
            self.target = next;
            if let Some((position, state)) = next {
                let hardness = BlockRegistry::prototype().get(state)
                    .map(|block| block.hardness_tenths)
                    .unwrap_or(20);
                if hardness == 0 {
                    self.target = None;
                    return MiningUpdate::Completed { position, state };
                }
                self.started = true;
                self.elapsed_seconds = dt;
                let progress = self.progress();
                if progress >= 1.0 {
                    self.target = None;
                    self.started = false;
                    self.elapsed_seconds = 0.0;
                    return MiningUpdate::Completed { position, state };
                }
                return MiningUpdate::Started { position };
            }
            return MiningUpdate::Idle;
        }

        let Some((position, state)) = self.target else { return MiningUpdate::Idle; };
        if !self.started { self.started = true; }
        self.elapsed_seconds += dt;
        let progress = self.progress();
        if progress >= 1.0 {
            self.target = None;
            self.started = false;
            self.elapsed_seconds = 0.0;
            MiningUpdate::Completed { position, state }
        } else {
            MiningUpdate::Progress { position, progress }
        }
    }

    pub fn reset(&mut self) -> Option<BlockPosition> {
        let position = self.target.map(|(position, _)| position);
        self.target = None;
        self.elapsed_seconds = 0.0;
        self.started = false;
        position
    }
}

/// A deliberately simple prototype timing model: hardness / 1.5 seconds.
/// Real Minecraft mining also depends on tool, effects, water, airborne state,
/// game mode, and server rules.
fn mining_duration_seconds(hardness_tenths: u16) -> f32 {
    (f32::from(hardness_tenths) / 10.0 / 1.5).max(0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{BlockPosition, DIRT, STONE};

    fn hit(position: BlockPosition, state: BlockStateId) -> BlockHit {
        BlockHit { position, state, face: [0, 1, 0], distance: 2.0 }
    }

    #[test]
    fn mining_requires_a_hold_and_emits_completion_once() {
        let position = BlockPosition::new(1, 2, 3);
        let mut mining = MiningController::default();
        assert!(matches!(mining.tick(Some(hit(position, DIRT)), true, 0.05), MiningUpdate::Started { .. }));
        let mut completed = false;
        for _ in 0..20 {
            if let MiningUpdate::Completed { position: p, state } = mining.tick(Some(hit(position, DIRT)), true, 0.05) {
                assert_eq!(p, position);
                assert_eq!(state, DIRT);
                completed = true;
                break;
            }
        }
        assert!(completed, "held mining should eventually complete");
        assert_eq!(mining.tick(Some(hit(position, DIRT)), true, 0.05), MiningUpdate::Started { position });
    }

    #[test]
    fn changing_target_restarts_progress() {
        let first = BlockPosition::new(0, 0, 0);
        let second = BlockPosition::new(1, 0, 0);
        let mut mining = MiningController::default();
        mining.tick(Some(hit(first, STONE)), true, 0.05);
        mining.tick(Some(hit(first, STONE)), true, 0.05);
        assert_eq!(mining.tick(Some(hit(second, STONE)), true, 0.05), MiningUpdate::Started { position: second });
        assert!(mining.progress() < 0.1);
    }

    #[test]
    fn releasing_input_aborts_the_active_target() {
        let position = BlockPosition::new(0, 0, 0);
        let mut mining = MiningController::default();
        mining.tick(Some(hit(position, STONE)), true, 0.05);
        assert_eq!(mining.tick(Some(hit(position, STONE)), false, 0.05), MiningUpdate::Aborted { position });
        assert_eq!(mining.progress(), 0.0);
    }

    #[test]
    fn zero_hardness_completes_immediately() {
        let position = BlockPosition::new(0, 0, 0);
        let mut mining = MiningController::default();
        assert_eq!(mining.tick(Some(hit(position, AIR)), true, 0.05), MiningUpdate::Completed { position, state: AIR });
    }

    #[test]
    fn invalid_delta_time_cannot_advance_mining() {
        let position = BlockPosition::new(0, 0, 0);
        let mut mining = MiningController::default();
        mining.tick(Some(hit(position, STONE)), true, f32::NAN);
        assert_eq!(mining.progress(), 0.0);
    }
}
