//! Authoritative local singleplayer simulation.
//!
//! This is a Rust-side integrated-server core for the current prototype world.
//! It is deliberately not presented as a complete Minecraft 26.3 server: block
//! registries, entity systems, lighting, redstone, and vanilla persistence remain
//! separate porting tasks.

use crate::chunk::{ChunkManager, ChunkUpdate};
use crate::singleplayer::{SingleplayerSave, SingleplayerSaveError};
use crate::world::{BlockPosition, BlockStateId, ChunkPosition, TerrainGenerator, World, AIR};

pub const SERVER_TICK_RATE: u32 = 20;
pub const AUTOSAVE_INTERVAL_TICKS: u64 = 600;
pub const MIN_WORLD_Y: i32 = -64;
pub const MAX_WORLD_Y: i32 = 319;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameMode {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Survival,
            2 => Self::Adventure,
            3 => Self::Spectator,
            _ => Self::Creative,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Survival => 0,
            Self::Creative => 1,
            Self::Adventure => 2,
            Self::Spectator => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerPlayerState {
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl Default for ServerPlayerState {
    fn default() -> Self {
        Self { position: [0.5, 8.0, 0.5], yaw: 0.0, pitch: 0.0, on_ground: false }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerTick {
    pub tick_count: u64,
    pub world_time: u64,
    pub day_time: u64,
    pub chunk_update: ChunkUpdate,
    pub autosave_due: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockEditError {
    OutOfWorldBounds,
    UnloadedChunk,
    InvalidState,
    ReadOnlyGameMode,
}

#[derive(Debug, Clone)]
pub struct IntegratedServer {
    seed: u32,
    world: World,
    chunks: ChunkManager,
    game_mode: GameMode,
    player: ServerPlayerState,
    tick_count: u64,
    world_time: u64,
    day_time: u64,
    paused: bool,
}

impl IntegratedServer {
    pub fn new(seed: u32, view_distance: i32) -> Self {
        let generator = TerrainGenerator::new(seed, 5, 3);
        let mut world = World::new();
        let mut chunks = ChunkManager::new(view_distance.clamp(1, 8));
        chunks.update(&mut world, ChunkPosition::new(0, 0), &generator);
        let spawn_y = generator.height_at(0, 0).max(1) as f64 + 1.01;
        let player = ServerPlayerState {
            position: [0.5, spawn_y, 0.5],
            ..ServerPlayerState::default()
        };
        Self {
            seed, world, chunks, game_mode: GameMode::Creative, player,
            tick_count: 0, world_time: 0, day_time: 0, paused: false,
        }
    }

    pub fn from_save(save: SingleplayerSave, view_distance: i32) -> Self {
        let mut chunks = ChunkManager::new(view_distance.clamp(1, 8));
        chunks.adopt_loaded_world(&save.world);
        Self {
            seed: save.seed,
            world: save.world,
            chunks,
            game_mode: GameMode::from_u8(save.game_mode),
            player: ServerPlayerState {
                position: save.player_position,
                yaw: save.yaw,
                pitch: save.pitch,
                on_ground: false,
            },
            tick_count: 0,
            world_time: save.world_time,
            day_time: save.day_time % 24_000,
            paused: false,
        }
    }

    pub fn world(&self) -> &World { &self.world }
    pub fn world_mut(&mut self) -> &mut World { &mut self.world }
    pub fn chunks(&self) -> &ChunkManager { &self.chunks }
    pub fn seed(&self) -> u32 { self.seed }
    pub fn player(&self) -> ServerPlayerState { self.player }
    pub fn game_mode(&self) -> GameMode { self.game_mode }
    pub fn set_game_mode(&mut self, mode: GameMode) { self.game_mode = mode; }
    pub fn tick_count(&self) -> u64 { self.tick_count }
    pub fn world_time(&self) -> u64 { self.world_time }
    pub fn day_time(&self) -> u64 { self.day_time }
    pub fn set_paused(&mut self, paused: bool) { self.paused = paused; }

    /// Advance one fixed 20 TPS server tick and maintain the loaded chunk radius.
    pub fn tick(&mut self, player: ServerPlayerState) -> ServerTick {
        if self.paused {
            return ServerTick {
                tick_count: self.tick_count,
                world_time: self.world_time,
                day_time: self.day_time,
                ..ServerTick::default()
            };
        }
        if player.position.iter().all(|value| value.is_finite())
            && player.yaw.is_finite() && player.pitch.is_finite()
        {
            self.player = player;
        }
        self.tick_count = self.tick_count.wrapping_add(1);
        self.world_time = self.world_time.wrapping_add(1);
        self.day_time = (self.day_time + 1) % 24_000;
        let center = BlockPosition::new(
            self.player.position[0].floor() as i32,
            0,
            self.player.position[2].floor() as i32,
        ).chunk();
        let generator = TerrainGenerator::new(self.seed, 5, 3);
        let chunk_update = self.chunks.update_with_budget(
            &mut self.world, center, &generator, 1, 1,
        );
        ServerTick {
            tick_count: self.tick_count,
            world_time: self.world_time,
            day_time: self.day_time,
            chunk_update,
            autosave_due: self.tick_count % AUTOSAVE_INTERVAL_TICKS == 0,
        }
    }

    /// Apply a client-requested block edit on the authoritative local server.
    pub fn set_block(&mut self, position: BlockPosition, state: BlockStateId) -> Result<bool, BlockEditError> {
        if !(MIN_WORLD_Y..=MAX_WORLD_Y).contains(&position.y) {
            return Err(BlockEditError::OutOfWorldBounds);
        }
        if !self.chunks.is_loaded(position.chunk()) {
            return Err(BlockEditError::UnloadedChunk);
        }
        if self.game_mode == GameMode::Adventure || self.game_mode == GameMode::Spectator {
            return Err(BlockEditError::ReadOnlyGameMode);
        }
        // Current internal registry is intentionally small and project-local.
        if state.0 > 7 && state != AIR {
            return Err(BlockEditError::InvalidState);
        }
        Ok(self.world.set_block(position, state))
    }

    pub fn break_block(&mut self, position: BlockPosition) -> Result<bool, BlockEditError> {
        if self.world.get_block(position) == AIR {
            return Ok(false);
        }
        self.set_block(position, AIR)
    }

    pub fn snapshot(&self) -> SingleplayerSave {
        SingleplayerSave {
            seed: self.seed,
            player_position: self.player.position,
            yaw: self.player.yaw,
            pitch: self.player.pitch,
            game_mode: self.game_mode.as_u8(),
            world_time: self.world_time,
            day_time: self.day_time,
            world: self.world.clone(),
        }
    }

    pub fn encode_save(&self) -> Result<Vec<u8>, SingleplayerSaveError> {
        self.snapshot().encode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{BlockPosition, DIRT, STONE};

    #[test]
    fn server_owns_world_time_and_ticks_at_fixed_rate() {
        let mut server = IntegratedServer::new(77, 1);
        let player = server.player();
        let first = server.tick(player);
        assert_eq!(first.tick_count, 1);
        assert_eq!(first.world_time, 1);
        assert_eq!(first.day_time, 1);
        let mut autosave = false;
        for _ in 1..AUTOSAVE_INTERVAL_TICKS {
            autosave = server.tick(player).autosave_due;
        }
        assert_eq!(server.tick_count(), AUTOSAVE_INTERVAL_TICKS);
        assert_eq!(server.day_time(), AUTOSAVE_INTERVAL_TICKS % 24_000);
        assert!(autosave);
    }

    #[test]
    fn paused_server_does_not_advance_time_or_load_chunks() {
        let mut server = IntegratedServer::new(9, 1);
        server.set_paused(true);
        let before = server.chunks().loaded_columns();
        let result = server.tick(server.player());
        assert_eq!(result.tick_count, 0);
        assert_eq!(server.world_time(), 0);
        assert_eq!(server.chunks().loaded_columns(), before);
    }

    #[test]
    fn server_validates_block_edits_and_owns_the_authoritative_world() {
        let mut server = IntegratedServer::new(123, 1);
        let position = BlockPosition::new(1, 20, 1);
        assert_eq!(server.set_block(position, STONE), Ok(true));
        assert_eq!(server.world().get_block(position), STONE);
        assert_eq!(server.break_block(position), Ok(true));
        assert_eq!(server.world().get_block(position), AIR);
        assert_eq!(
            server.set_block(BlockPosition::new(1, MAX_WORLD_Y + 1, 1), DIRT),
            Err(BlockEditError::OutOfWorldBounds)
        );
        server.set_game_mode(GameMode::Adventure);
        assert_eq!(server.set_block(position, DIRT), Err(BlockEditError::ReadOnlyGameMode));
    }

    #[test]
    fn save_snapshot_contains_server_owned_player_and_world_state() {
        let mut server = IntegratedServer::new(456, 1);
        let mut player = server.player();
        player.position = [12.0, 15.0, -8.0];
        player.yaw = 1.25;
        server.tick(player);
        let bytes = server.encode_save().unwrap();
        let restored = SingleplayerSave::decode(&bytes).unwrap();
        assert_eq!(restored.seed, 456);
        assert_eq!(restored.player_position, player.position);
        assert_eq!(restored.yaw, player.yaw);
        assert_eq!(restored.game_mode, GameMode::Creative.as_u8());
        assert_eq!(restored.world_time, server.world_time());
        assert_eq!(restored.day_time, server.day_time());
    }
}
