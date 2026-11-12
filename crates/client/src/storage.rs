//! Versioned, deterministic save snapshots for the internal prototype world.
//!
//! This is an application save format, not the Eaglercraft 26.3 chunk wire format.
//! Browser storage adapters can persist these bytes without coupling the codec to
//! localStorage, IndexedDB, or a particular host environment.

use std::collections::HashSet;

use crate::world::{BlockPosition, BlockStateId, SectionPosition, World, SECTION_VOLUME};

const MAGIC: &[u8; 4] = b"ERWS";
const FORMAT_VERSION: u32 = 1;
const SECTION_HEADER_BYTES: usize = 12;
const BLOCK_BYTES: usize = 4;
/// Defensive limit for corrupted or untrusted save data (roughly 1 GiB at v1).
const MAX_SECTIONS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageState {
    Uninitialized,
    Ready,
    ReadOnly,
    Unavailable,
}

impl Default for StorageState {
    fn default() -> Self { Self::Uninitialized }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    Truncated,
    InvalidMagic,
    UnsupportedVersion(u32),
    TooManySections,
    InvalidLength,
    DuplicateSection(SectionPosition),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(formatter, "save data is truncated"),
            Self::InvalidMagic => write!(formatter, "save data has an invalid signature"),
            Self::UnsupportedVersion(version) => write!(formatter, "unsupported save version {version}"),
            Self::TooManySections => write!(formatter, "save contains too many sections"),
            Self::InvalidLength => write!(formatter, "save data length does not match its section count"),
            Self::DuplicateSection(position) => write!(formatter, "save contains duplicate section {position:?}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// Encode loaded sections in stable coordinate order for reproducible saves.
pub fn encode_world(world: &World) -> Vec<u8> {
    let sections = world.loaded_sections();
    let mut bytes = Vec::with_capacity(12 + sections.len() * (SECTION_HEADER_BYTES + SECTION_VOLUME * BLOCK_BYTES));
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(sections.len() as u32).to_le_bytes());

    for position in sections {
        bytes.extend_from_slice(&position.x.to_le_bytes());
        bytes.extend_from_slice(&position.y.to_le_bytes());
        bytes.extend_from_slice(&position.z.to_le_bytes());
        let section = world.get_section(position).expect("loaded section list must remain consistent");
        for state in section.as_slice() {
            bytes.extend_from_slice(&state.0.to_le_bytes());
        }
    }
    bytes
}

/// Decode a complete snapshot. No partial world is returned on malformed data.
pub fn decode_world(bytes: &[u8]) -> Result<World, SaveError> {
    if bytes.len() < 12 {
        return Err(SaveError::Truncated);
    }
    if &bytes[..4] != MAGIC {
        return Err(SaveError::InvalidMagic);
    }

    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("four-byte version"));
    if version != FORMAT_VERSION {
        return Err(SaveError::UnsupportedVersion(version));
    }
    let section_count = u32::from_le_bytes(bytes[8..12].try_into().expect("four-byte count")) as usize;
    if section_count > MAX_SECTIONS {
        return Err(SaveError::TooManySections);
    }

    let record_bytes = SECTION_HEADER_BYTES + SECTION_VOLUME * BLOCK_BYTES;
    let expected_len = 12usize
        .checked_add(section_count.checked_mul(record_bytes).ok_or(SaveError::InvalidLength)?)
        .ok_or(SaveError::InvalidLength)?;
    if bytes.len() < expected_len {
        return Err(SaveError::Truncated);
    }
    if bytes.len() != expected_len {
        return Err(SaveError::InvalidLength);
    }

    let mut world = World::new();
    let mut seen = HashSet::with_capacity(section_count);
    let mut cursor = 12;
    for _ in 0..section_count {
        let x = read_i32(bytes, &mut cursor);
        let y = read_i32(bytes, &mut cursor);
        let z = read_i32(bytes, &mut cursor);
        let section_position = SectionPosition::new(x, y, z);
        if !seen.insert(section_position) {
            return Err(SaveError::DuplicateSection(section_position));
        }

        for index in 0..SECTION_VOLUME {
            let state = read_u32(bytes, &mut cursor);
            if state == 0 {
                continue;
            }
            let local = crate::world::LocalBlockPosition {
                x: (index % 16) as u8,
                y: (index / 256) as u8,
                z: ((index / 16) % 16) as u8,
            };
            let position = BlockPosition::new(
                x.saturating_mul(16).saturating_add(i32::from(local.x)),
                y.saturating_mul(16).saturating_add(i32::from(local.y)),
                z.saturating_mul(16).saturating_add(i32::from(local.z)),
            );
            world.set_block(position, BlockStateId(state));
        }
    }
    world.take_dirty_sections();
    Ok(world)
}

fn read_i32(bytes: &[u8], cursor: &mut usize) -> i32 {
    let value = i32::from_le_bytes(bytes[*cursor..*cursor + 4].try_into().expect("validated save length"));
    *cursor += 4;
    value
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> u32 {
    let value = u32::from_le_bytes(bytes[*cursor..*cursor + 4].try_into().expect("validated save length"));
    *cursor += 4;
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{AIR, BlockPosition, DIRT, STONE};

    #[test]
    fn world_snapshot_round_trips_positive_and_negative_coordinates() {
        let mut original = World::new();
        original.set_block(BlockPosition::new(-1, -17, 18), STONE);
        original.set_block(BlockPosition::new(16, 2, -16), DIRT);
        original.set_block(BlockPosition::new(3, 4, 5), BlockStateId(9001));

        let encoded = encode_world(&original);
        let restored = decode_world(&encoded).unwrap();
        for position in [
            BlockPosition::new(-1, -17, 18),
            BlockPosition::new(16, 2, -16),
            BlockPosition::new(3, 4, 5),
            BlockPosition::new(0, 0, 0),
        ] {
            assert_eq!(restored.get_block(position), original.get_block(position));
        }
        assert_eq!(restored.get_block(BlockPosition::new(0, 0, 0)), AIR);
        assert!(restored.take_dirty_sections().is_empty());
        assert_eq!(encode_world(&restored), encoded);
    }

    #[test]
    fn rejects_bad_signature_and_truncated_header() {
        assert_eq!(decode_world(b"ER"), Err(SaveError::Truncated));
        let mut bytes = vec![0; 12];
        assert_eq!(decode_world(&bytes), Err(SaveError::InvalidMagic));
        bytes[..4].copy_from_slice(MAGIC);
        assert_eq!(decode_world(&bytes), Ok(World::new()));
    }

    #[test]
    fn rejects_unsupported_versions_and_trailing_bytes() {
        let mut bytes = encode_world(&World::new());
        bytes[4..8].copy_from_slice(&99_u32.to_le_bytes());
        assert_eq!(decode_world(&bytes), Err(SaveError::UnsupportedVersion(99)));

        let mut bytes = encode_world(&World::new());
        bytes.push(0);
        assert_eq!(decode_world(&bytes), Err(SaveError::InvalidLength));
    }

    #[test]
    fn rejects_section_count_that_exceeds_safety_limit() {
        let mut bytes = encode_world(&World::new());
        bytes[8..12].copy_from_slice(&((MAX_SECTIONS as u32) + 1).to_le_bytes());
        assert_eq!(decode_world(&bytes), Err(SaveError::TooManySections));
    }
}
