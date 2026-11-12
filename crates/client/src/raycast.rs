//! Grid traversal for selecting blocks in the prototype world.
use crate::world::{BlockPosition, BlockStateId, World, AIR};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockHit {
    pub position: BlockPosition,
    /// Normal of the face entered by the ray. Zero when starting inside a block.
    pub face: [i32; 3],
    pub distance: f64,
    pub state: BlockStateId,
}

/// Traverse voxels with a 3D DDA. Direction is normalized internally.
pub fn cast_blocks(world: &World, origin: [f64; 3], direction: [f64; 3], max_distance: f64) -> Option<BlockHit> {
    if !max_distance.is_finite() || max_distance < 0.0 || origin.iter().any(|v| !v.is_finite()) || direction.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let length = (direction[0]*direction[0] + direction[1]*direction[1] + direction[2]*direction[2]).sqrt();
    if length < 1.0e-9 { return None; }
    let dir = [direction[0]/length, direction[1]/length, direction[2]/length];
    let mut cell = [origin[0].floor() as i32, origin[1].floor() as i32, origin[2].floor() as i32];
    let initial = BlockPosition::new(cell[0], cell[1], cell[2]);
    let initial_state = world.get_block(initial);
    if initial_state != AIR {
        return Some(BlockHit { position: initial, face: [0,0,0], distance: 0.0, state: initial_state });
    }
    let step = [dir[0].signum() as i32, dir[1].signum() as i32, dir[2].signum() as i32];
    let mut t_delta = [f64::INFINITY; 3];
    let mut t_max = [f64::INFINITY; 3];
    for axis in 0..3 {
        if dir[axis].abs() > 1.0e-12 {
            t_delta[axis] = 1.0 / dir[axis].abs();
            t_max[axis] = if step[axis] > 0 {
                ((cell[axis] as f64 + 1.0) - origin[axis]) / dir[axis]
            } else {
                (origin[axis] - cell[axis] as f64) / -dir[axis]
            };
        }
    }
    loop {
        let axis = if t_max[0] <= t_max[1] && t_max[0] <= t_max[2] { 0 } else if t_max[1] <= t_max[2] { 1 } else { 2 };
        let distance = t_max[axis];
        if !distance.is_finite() || distance > max_distance { return None; }
        cell[axis] += step[axis];
        t_max[axis] += t_delta[axis];
        let position = BlockPosition::new(cell[0], cell[1], cell[2]);
        let state = world.get_block(position);
        if state != AIR {
            let mut face = [0,0,0];
            face[axis] = -step[axis];
            return Some(BlockHit { position, face, distance, state });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ray_hits_nearest_block_and_reports_entered_face() {
        let mut world = World::new();
        world.set_block(BlockPosition::new(3, 0, 0), BlockStateId(1));
        world.set_block(BlockPosition::new(5, 0, 0), BlockStateId(2));
        let hit = cast_blocks(&world, [0.5,0.5,0.5], [1.0,0.0,0.0], 8.0).unwrap();
        assert_eq!(hit.position, BlockPosition::new(3,0,0));
        assert_eq!(hit.face, [-1,0,0]);
    }
    #[test]
    fn ray_respects_range_and_zero_direction() {
        let mut world = World::new();
        world.set_block(BlockPosition::new(4,0,0), BlockStateId(1));
        assert!(cast_blocks(&world,[0.5,0.5,0.5],[1.0,0.0,0.0],2.0).is_none());
        assert!(cast_blocks(&world,[0.5,0.5,0.5],[0.0,0.0,0.0],8.0).is_none());
    }
}
