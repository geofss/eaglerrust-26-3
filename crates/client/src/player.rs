//! Vanilla-inspired player movement ported from the 26.1.2 reference.
//!
//! This is a deliberate compatibility baseline, not a claim of exact 26.3
//! parity. Fluid movement, crouching, step-up collision, effects, vehicles,
//! and block-specific friction are still outside this prototype.
use crate::input::InputFrame;
use crate::physics::{move_aabb, Aabb, Vec3};
use crate::world::World;

/// Reference movement constants audited against the 26.1.2 source archive.
pub const WALKING_SPEED: f64 = 0.1;
pub const BASE_JUMP_POWER: f64 = 0.42;
pub const GRAVITY: f64 = 0.08;
pub const INPUT_FRICTION: f64 = 0.98;
pub const GROUND_ACCELERATION: f64 = 0.216_000_02;
pub const DEFAULT_BLOCK_FRICTION: f64 = 0.6;
pub const AIR_ACCELERATION: f64 = 0.02;
pub const HORIZONTAL_DRAG: f64 = 0.91;
pub const VERTICAL_DRAG: f64 = 0.98;
const SPRINT_MULTIPLIER: f64 = 1.3;
const SPRINT_JUMP_BOOST: f64 = 0.2;
const JUMP_COOLDOWN_TICKS: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player {
    pub position: Vec3,
    pub velocity: Vec3,
    /// Camera yaw in radians. Zero faces toward negative Z in this renderer.
    pub yaw: f64,
    pub on_ground: bool,
    pub sprinting: bool,
    pub horizontal_collision: bool,
    pub vertical_collision: bool,
    jump_cooldown_ticks: u8,
}

impl Player {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::default(),
            yaw: 0.0,
            on_ground: false,
            sprinting: false,
            horizontal_collision: false,
            vertical_collision: false,
            jump_cooldown_ticks: 0,
        }
    }

    pub fn bounds(&self) -> Aabb {
        Aabb::new(
            Vec3::new(self.position.x - 0.3, self.position.y, self.position.z - 0.3),
            Vec3::new(self.position.x + 0.3, self.position.y + 1.8, self.position.z + 0.3),
        )
    }

    /// Advance one fixed 20 Hz simulation step.
    ///
    /// The acceleration, friction, gravity, jump impulse, and drag follow the
    /// audited 26.1.2 movement reference. The yaw signs are adapted to this
    /// renderer's camera convention (yaw zero looks down negative Z).
    pub fn tick(&mut self, world: &World, input: &InputFrame) {
        if input.jump_pressed {
            self.jump_cooldown_ticks = self.jump_cooldown_ticks.saturating_sub(1);
        } else {
            self.jump_cooldown_ticks = 0;
        }

        let (forward, strafe) = input.normalized_movement();
        let forward = f64::from(forward) * INPUT_FRICTION;
        let strafe = f64::from(strafe) * INPUT_FRICTION;
        self.sprinting = input.sprint_pressed && forward > 0.0;

        if input.jump_pressed && self.on_ground && self.jump_cooldown_ticks == 0 {
            self.velocity.y = self.velocity.y.max(BASE_JUMP_POWER);
            if self.sprinting {
                self.velocity.x -= self.yaw.sin() * SPRINT_JUMP_BOOST;
                self.velocity.z -= self.yaw.cos() * SPRINT_JUMP_BOOST;
            }
            self.jump_cooldown_ticks = JUMP_COOLDOWN_TICKS;
        }

        // Vanilla uses the block below the player's feet. Until per-block
        // friction is ported, ordinary prototype ground uses 0.6.
        let block_friction = if self.on_ground {
            DEFAULT_BLOCK_FRICTION
        } else {
            1.0
        };
        let acceleration = if self.on_ground {
            let movement_speed = if self.sprinting {
                WALKING_SPEED * SPRINT_MULTIPLIER
            } else {
                WALKING_SPEED
            };
            movement_speed * (GROUND_ACCELERATION / block_friction.powi(3))
        } else {
            AIR_ACCELERATION
        };

        let sin = self.yaw.sin();
        let cos = self.yaw.cos();
        self.velocity.x += (sin * forward + cos * strafe) * acceleration;
        self.velocity.z += (-cos * forward + sin * strafe) * acceleration;

        let requested = self.velocity;
        let result = move_aabb(world, self.bounds(), requested);
        self.position = Vec3::new(
            (result.bounds.min.x + result.bounds.max.x) * 0.5,
            result.bounds.min.y,
            (result.bounds.min.z + result.bounds.max.z) * 0.5,
        );

        self.horizontal_collision =
            result.applied.x != requested.x || result.applied.z != requested.z;
        self.vertical_collision = result.applied.y != requested.y;
        if result.applied.x != requested.x {
            self.velocity.x = 0.0;
        }
        if result.applied.y != requested.y {
            self.velocity.y = 0.0;
        }
        if result.applied.z != requested.z {
            self.velocity.z = 0.0;
        }
        self.on_ground = result.on_ground;

        let horizontal_drag = block_friction * HORIZONTAL_DRAG;
        self.velocity.x *= horizontal_drag;
        self.velocity.z *= horizontal_drag;
        self.velocity.y = (self.velocity.y - GRAVITY) * VERTICAL_DRAG;

        if self.velocity.x * self.velocity.x + self.velocity.z * self.velocity.z < 9.0e-6 {
            self.velocity.x = 0.0;
            self.velocity.z = 0.0;
        }
        if self.velocity.y.abs() < 0.003 {
            self.velocity.y = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{BlockPosition, BlockStateId};

    fn flat_world(radius: i32) -> World {
        let mut world = World::new();
        for x in -radius..=radius {
            for z in -radius..=radius {
                world.set_block(BlockPosition::new(x, 0, z), BlockStateId(1));
            }
        }
        world
    }

    fn grounded_player() -> Player {
        let mut player = Player::new(Vec3::new(0.5, 1.0, 0.5));
        player.on_ground = true;
        // A stable grounded tick carries the downward velocity that the
        // collision solver clips against the floor.
        player.velocity.y = -0.0784;
        player
    }

    #[test]
    fn player_falls_until_supported_by_ground() {
        let world = flat_world(2);
        let mut player = Player::new(Vec3::new(0.5, 1.0, 0.5));
        for _ in 0..30 {
            player.tick(&world, &InputFrame::default());
        }
        assert!(player.on_ground);
        assert!((player.position.y - 1.0).abs() < 1.0e-8);
    }

    #[test]
    fn walking_accelerates_and_settles_near_reference_speed() {
        let world = flat_world(32);
        let mut player = grounded_player();
        let input = InputFrame { forward: 1.0, ..InputFrame::default() };
        for _ in 0..20 {
            player.tick(&world, &input);
        }
        let travelled = 0.5 - player.position.z;
        assert!((3.8..4.3).contains(&travelled), "travelled {travelled}");
    }

    #[test]
    fn sprinting_increases_ground_acceleration() {
        let world = flat_world(32);
        let mut walking = grounded_player();
        let mut sprinting = grounded_player();
        for _ in 0..20 {
            walking.tick(
                &world,
                &InputFrame { forward: 1.0, ..InputFrame::default() },
            );
            sprinting.tick(
                &world,
                &InputFrame { forward: 1.0, sprint_pressed: true, ..InputFrame::default() },
            );
        }
        assert!(0.5 - sprinting.position.z > 0.5 - walking.position.z);
    }

    #[test]
    fn jump_uses_reference_impulse() {
        let world = flat_world(4);
        let mut player = grounded_player();
        player.tick(
            &world,
            &InputFrame { jump_pressed: true, ..InputFrame::default() },
        );
        assert!((player.position.y - 1.42).abs() < 1.0e-8);
        assert!((player.velocity.y - 0.3332).abs() < 1.0e-8);
    }
}
