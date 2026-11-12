//! CPU-side texture-atlas layout primitives for the WebGL renderer.
//!
//! This computes stable pixel/UV rectangles and deterministic development
//! pixels. The renderer uploads this placeholder atlas and samples it with
//! per-face UVs; verified target-pack assets and cutout passes remain pending.
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
    fn generated_prototype_pixels_fill_the_atlas_and_keep_rgba_alignment() {
        let layout = TextureAtlasLayout::new(PROTOTYPE_ATLAS_TILE_COUNT, PROTOTYPE_ATLAS_TILE_SIZE, 1024).unwrap();
        let pixels = prototype_atlas_pixels(layout);
        let (width, height) = layout.dimensions();
        assert_eq!(pixels.len(), width as usize * height as usize * 4);
        assert!(pixels.chunks_exact(4).all(|rgba| rgba[3] == 255));
        let white = layout.rect(8).unwrap();
        let center = (((white.y + 8) * width + white.x + 8) * 4) as usize;
        assert_eq!(&pixels[center..center + 4], &[255, 255, 255, 255]);
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


/// Number of tiles in the generated development atlas; not target-game assets.
pub const PROTOTYPE_ATLAS_TILE_COUNT: u32 = 10;
pub const PROTOTYPE_ATLAS_TILE_SIZE: u32 = 16;

/// Build deterministic RGBA pixels for the development renderer atlas.
/// Tile order: grass top, grass side, dirt, stone, water, sand, log, leaves,
/// white selection outline, and magenta unknown-block fallback.
pub fn prototype_atlas_pixels(layout: TextureAtlasLayout) -> Vec<u8> {
    let (width, height) = layout.dimensions();
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    let colors: [([u8; 3], [u8; 3]); 10] = [
        ([78, 166, 57], [91, 183, 67]),
        ([116, 83, 48], [78, 166, 57]),
        ([133, 91, 56], [145, 101, 63]),
        ([119, 123, 128], [137, 141, 146]),
        ([39, 119, 190], [49, 137, 207]),
        ([211, 190, 132], [225, 204, 145]),
        ([112, 72, 39], [132, 87, 48]),
        ([45, 130, 54], [61, 148, 67]),
        ([255, 255, 255], [255, 255, 255]),
        ([224, 45, 224], [190, 25, 190]),
    ];
    for tile in 0..PROTOTYPE_ATLAS_TILE_COUNT {
        let rect = layout.rect(tile).expect("prototype tile is inside its atlas");
        let (base, accent) = colors[tile as usize];
        for y in 0..rect.height {
            for x in 0..rect.width {
                let stripe = tile == 1 && y < 3;
                let log_line = tile == 6 && x % 5 == 0;
                let mottled = (x * 13 + y * 7 + tile * 3) % 9 == 0;
                let color = if stripe || log_line || mottled { accent } else { base };
                let offset = (((rect.y + y) * width + rect.x + x) * 4) as usize;
                pixels[offset..offset + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
            }
        }
    }
    pixels
}
