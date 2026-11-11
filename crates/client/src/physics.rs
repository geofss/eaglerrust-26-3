//! Deterministic, platform-neutral geometry primitives.
//! Collision resolution and version-specific entity physics are not implemented yet.

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self { Self { x, y, z } }

    pub fn translated(self, delta: Self) -> Self {
        Self::new(self.x + delta.x, self.y + delta.y, self.z + delta.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const fn new(min: Vec3, max: Vec3) -> Self { Self { min, max } }

    /// Returns true only when the boxes overlap with positive volume.
    /// Merely touching faces/edges does not count as intersection.
    pub fn intersects(self, other: Self) -> bool {
        self.max.x > other.min.x && self.min.x < other.max.x
            && self.max.y > other.min.y && self.min.y < other.max.y
            && self.max.z > other.min.z && self.min.z < other.max.z
    }

    pub fn translated(self, delta: Vec3) -> Self {
        Self::new(self.min.translated(delta), self.max.translated(delta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box_at(x: f64) -> Aabb {
        Aabb::new(Vec3::new(x, 0.0, 0.0), Vec3::new(x + 1.0, 1.0, 1.0))
    }

    #[test]
    fn overlapping_boxes_intersect() {
        assert!(unit_box_at(0.0).intersects(unit_box_at(0.5)));
    }

    #[test]
    fn touching_faces_do_not_intersect() {
        assert!(!unit_box_at(0.0).intersects(unit_box_at(1.0)));
    }

    #[test]
    fn translated_box_preserves_dimensions() {
        let moved = unit_box_at(0.0).translated(Vec3::new(2.0, 3.0, -1.0));
        assert_eq!(moved.min, Vec3::new(2.0, 3.0, -1.0));
        assert_eq!(moved.max, Vec3::new(3.0, 4.0, 0.0));
    }
}
