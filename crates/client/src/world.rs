//! Platform-neutral world coordinate types.
//! Version-specific chunk/block decoding will be added after the 26.1.2/26.3 audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPosition {
    pub x: i32,
    pub z: i32,
}

impl BlockPosition {
    pub const fn new(x: i32, y: i32, z: i32) -> Self { Self { x, y, z } }
}

impl ChunkPosition {
    pub const fn new(x: i32, z: i32) -> Self { Self { x, z } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positions_are_value_types() {
        assert_eq!(BlockPosition::new(1, 2, 3), BlockPosition { x: 1, y: 2, z: 3 });
    }
}
