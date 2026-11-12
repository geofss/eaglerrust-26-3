//! CPU-side texture-atlas layout primitives for the WebGL renderer.
//!
//! This computes stable pixel/UV rectangles only. PNG loading, mip generation,
//! GPU texture upload, per-face UV assignment, and shader sampling are separate
//! renderer tasks and are not claimed to be complete here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub u_min: f32,
    pub v_min: f32,
    pub u_max: f32,
    pub v_max: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtlasError {
    EmptyAtlas,
    ZeroTileSize,
    TextureLimitTooSmall,
    TileOutOfRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureAtlasLayout {
    tile_count: u32,
    tile_size: u32,
    columns: u32,
    rows: u32,
    width: u32,
    height: u32,
}

impl TextureAtlasLayout {
    pub fn new(tile_count: u32, tile_size: u32, max_texture_size: u32) -> Result<Self, AtlasError> {
        if tile_count == 0 { return Err(AtlasError::EmptyAtlas); }
        if tile_size == 0 { return Err(AtlasError::ZeroTileSize); }
        if max_texture_size < tile_size { return Err(AtlasError::TextureLimitTooSmall); }
        let max_columns = max_texture_size / tile_size;
        let columns = ceil_sqrt(tile_count).min(max_columns).max(1);
        let rows = tile_count.div_ceil(columns);
        let width = columns.checked_mul(tile_size).ok_or(AtlasError::TextureLimitTooSmall)?;
        let height = rows.checked_mul(tile_size).ok_or(AtlasError::TextureLimitTooSmall)?;
        if width > max_texture_size || height > max_texture_size {
            return Err(AtlasError::TextureLimitTooSmall);
        }
        Ok(Self { tile_count, tile_size, columns, rows, width, height })
    }

    pub fn dimensions(self) -> (u32, u32) { (self.width, self.height) }
    pub fn grid(self) -> (u32, u32) { (self.columns, self.rows) }

    pub fn rect(self, tile: u32) -> Result<AtlasRect, AtlasError> {
        if tile >= self.tile_count { return Err(AtlasError::TileOutOfRange); }
        let x = (tile % self.columns) * self.tile_size;
        let y = (tile / self.columns) * self.tile_size;
        Ok(AtlasRect {
            x, y, width: self.tile_size, height: self.tile_size,
            u_min: x as f32 / self.width as f32,
            v_min: y as f32 / self.height as f32,
            u_max: (x + self.tile_size) as f32 / self.width as f32,
            v_max: (y + self.tile_size) as f32 / self.height as f32,
        })
    }
}

fn ceil_sqrt(value: u32) -> u32 {
    let mut root = (value as f64).sqrt() as u32;
    while root.saturating_mul(root) < value { root += 1; }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_compact_squareish_layout_and_normalized_uvs() {
        let atlas = TextureAtlasLayout::new(10, 16, 1024).unwrap();
        assert_eq!(atlas.grid(), (4, 3));
        assert_eq!(atlas.dimensions(), (64, 48));
        let rect = atlas.rect(5).unwrap();
        assert_eq!((rect.x, rect.y), (16, 16));
        assert!((rect.u_min - 0.25).abs() < 1.0e-6);
        assert!((rect.v_min - (1.0 / 3.0)).abs() < 1.0e-6);
    }

    #[test]
    fn respects_gpu_texture_dimension_limit() {
        assert_eq!(TextureAtlasLayout::new(100, 16, 64), Err(AtlasError::TextureLimitTooSmall));
    }

    #[test]
    fn rejects_invalid_atlas_inputs() {
        assert_eq!(TextureAtlasLayout::new(0, 16, 1024), Err(AtlasError::EmptyAtlas));
        assert_eq!(TextureAtlasLayout::new(1, 0, 1024), Err(AtlasError::ZeroTileSize));
        assert_eq!(TextureAtlasLayout::new(1, 16, 8), Err(AtlasError::TextureLimitTooSmall));
    }
}
