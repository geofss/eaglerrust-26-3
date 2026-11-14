//! Local singleplayer world snapshots, including world seed and player location.
//! This is the project's own save container, not an Eaglercraft wire format.

use crate::storage::{decode_world, encode_world, SaveError};
use crate::world::World;

const MAGIC: &[u8; 4] = b"ERSP";
const VERSION: u32 = 2;
const HEADER_V1_BYTES: usize = 48;
const HEADER_V2_BYTES: usize = 52;

#[derive(Debug, Clone)]
pub struct SingleplayerSave {
    pub seed: u32,
    pub player_position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub game_mode: u8,
    pub world: World,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SingleplayerSaveError {
    Truncated,
    InvalidMagic,
    UnsupportedVersion(u32),
    InvalidLength,
    InvalidPlayerState,
    InvalidGameMode(u8),
    World(SaveError),
}

impl From<SaveError> for SingleplayerSaveError {
    fn from(error: SaveError) -> Self { Self::World(error) }
}

impl std::fmt::Display for SingleplayerSaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "singleplayer save is truncated"),
            Self::InvalidMagic => write!(f, "singleplayer save signature is invalid"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported singleplayer save version {v}"),
            Self::InvalidLength => write!(f, "singleplayer save length is invalid"),
            Self::InvalidPlayerState => write!(f, "singleplayer player state is invalid"),
            Self::InvalidGameMode(mode) => write!(f, "singleplayer game mode {mode} is invalid"),
            Self::World(error) => write!(f, "world snapshot: {error}"),
        }
    }
}

impl std::error::Error for SingleplayerSaveError {}

impl SingleplayerSave {
    pub fn encode(&self) -> Result<Vec<u8>, SingleplayerSaveError> {
        if !self.player_position.iter().all(|v| v.is_finite())
            || !self.yaw.is_finite() || !self.pitch.is_finite()
        {
            return Err(SingleplayerSaveError::InvalidPlayerState);
        }
        let world = encode_world(&self.world);
        let world_len = u32::try_from(world.len()).map_err(|_| SingleplayerSaveError::InvalidLength)?;
        if self.game_mode > 3 { return Err(SingleplayerSaveError::InvalidGameMode(self.game_mode)); }
        let mut bytes = Vec::with_capacity(HEADER_V2_BYTES + world.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.seed.to_le_bytes());
        for value in self.player_position { bytes.extend_from_slice(&value.to_le_bytes()); }
        bytes.extend_from_slice(&self.yaw.to_le_bytes());
        bytes.extend_from_slice(&self.pitch.to_le_bytes());
        bytes.extend_from_slice(&(u32::from(self.game_mode)).to_le_bytes());
        bytes.extend_from_slice(&world_len.to_le_bytes());
        bytes.extend_from_slice(&world);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SingleplayerSaveError> {
        if bytes.len() < HEADER_V1_BYTES { return Err(SingleplayerSaveError::Truncated); }
        if &bytes[..4] != MAGIC { return Err(SingleplayerSaveError::InvalidMagic); }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if version != 1 && version != VERSION {
            return Err(SingleplayerSaveError::UnsupportedVersion(version));
        }
        let header_bytes = if version == 1 { HEADER_V1_BYTES } else { HEADER_V2_BYTES };
        if bytes.len() < header_bytes { return Err(SingleplayerSaveError::Truncated); }
        let seed = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let mut cursor = 12;
        let mut player_position = [0.0; 3];
        for value in &mut player_position {
            *value = f64::from_le_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;
        }
        let yaw = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        cursor += 4;
        let pitch = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        cursor += 4;
        let game_mode = if version == 1 {
            1
        } else {
            let mode = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            cursor += 4;
            u8::try_from(mode).ok().filter(|value| *value <= 3)
                .ok_or(SingleplayerSaveError::InvalidGameMode(u8::try_from(mode).unwrap_or(u8::MAX)))?
        };
        let world_len = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;
        if !player_position.iter().all(|v| v.is_finite()) || !yaw.is_finite() || !pitch.is_finite() {
            return Err(SingleplayerSaveError::InvalidPlayerState);
        }
        let end = cursor.checked_add(world_len).ok_or(SingleplayerSaveError::InvalidLength)?;
        if end != bytes.len() { return Err(SingleplayerSaveError::InvalidLength); }
        let world = decode_world(&bytes[cursor..end])?;
        Ok(Self { seed, player_position, yaw, pitch, game_mode, world })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{BlockPosition, BlockStateId};

    #[test]
    fn save_round_trips_world_seed_and_player_location() {
        let mut world = World::new();
        world.set_block(BlockPosition::new(-3, 8, 17), BlockStateId(42));
        let save = SingleplayerSave {
            seed: 0x1234_5678,
            player_position: [12.5, 81.25, -4.75],
            yaw: 1.25,
            pitch: -0.5,
            game_mode: 0,
            world,
        };
        let restored = SingleplayerSave::decode(&save.encode().unwrap()).unwrap();
        assert_eq!(restored.seed, save.seed);
        assert_eq!(restored.player_position, save.player_position);
        assert_eq!(restored.yaw, save.yaw);
        assert_eq!(restored.pitch, save.pitch);
        assert_eq!(restored.game_mode, save.game_mode);
        assert_eq!(restored.world.get_block(BlockPosition::new(-3, 8, 17)), BlockStateId(42));
    }

    #[test]
    fn reads_version_one_saves_as_creative() {
        let save = SingleplayerSave {
            seed: 99, player_position: [1.0, 2.0, 3.0], yaw: 0.5, pitch: -0.25,
            game_mode: 1, world: World::new(),
        };
        let v2 = save.encode().unwrap();
        let mut v1 = Vec::new();
        v1.extend_from_slice(&v2[..44]);
        v1.extend_from_slice(&v2[48..52]);
        v1[4..8].copy_from_slice(&1_u32.to_le_bytes());
        let restored = SingleplayerSave::decode(&v1).unwrap();
        assert_eq!(restored.seed, 99);
        assert_eq!(restored.game_mode, 1);
    }

    #[test]
    fn rejects_corrupt_save_headers_and_lengths() {
        assert!(matches!(SingleplayerSave::decode(b"ER"), Err(SingleplayerSaveError::Truncated)));
        let mut bytes = SingleplayerSave {
            seed: 1, player_position: [0.0; 3], yaw: 0.0, pitch: 0.0, game_mode: 1, world: World::new(),
        }.encode().unwrap();
        bytes[0] = b'X';
        assert!(matches!(SingleplayerSave::decode(&bytes), Err(SingleplayerSaveError::InvalidMagic)));
        let mut bytes = SingleplayerSave {
            seed: 1, player_position: [0.0; 3], yaw: 0.0, pitch: 0.0, world: World::new(),
        }.encode().unwrap();
        bytes.push(0);
        assert!(matches!(SingleplayerSave::decode(&bytes), Err(SingleplayerSaveError::InvalidLength)));
    }
}
