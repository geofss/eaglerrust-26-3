//! Deterministic AABB geometry and axis-separated voxel collision.
use crate::block::is_solid_state;
use crate::world::{BlockPosition, World};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3 { pub x: f64, pub y: f64, pub z: f64 }
impl Vec3 {
    pub const fn new(x:f64,y:f64,z:f64)->Self { Self{x,y,z} }
    pub fn translated(self, delta:Self)->Self { Self::new(self.x+delta.x,self.y+delta.y,self.z+delta.z) }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb { pub min:Vec3, pub max:Vec3 }
impl Aabb {
    pub const fn new(min:Vec3,max:Vec3)->Self { Self{min,max} }
    pub fn intersects(self, other:Self)->bool {
        self.max.x>other.min.x && self.min.x<other.max.x && self.max.y>other.min.y && self.min.y<other.max.y && self.max.z>other.min.z && self.min.z<other.max.z
    }
    pub fn translated(self, delta:Vec3)->Self { Self::new(self.min.translated(delta),self.max.translated(delta)) }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveResult { pub bounds:Aabb, pub applied:Vec3, pub on_ground:bool }

/// Resolve movement one axis at a time against full-cube solid blocks.
pub fn move_aabb(world:&World, bounds:Aabb, requested:Vec3)->MoveResult {
    let mut current=bounds;
    let dx=clip_axis(world,current,requested.x,0); current=current.translated(Vec3::new(dx,0.0,0.0));
    let dz=clip_axis(world,current,requested.z,2); current=current.translated(Vec3::new(0.0,0.0,dz));
    let dy=clip_axis(world,current,requested.y,1); current=current.translated(Vec3::new(0.0,dy,0.0));
    MoveResult { bounds:current, applied:Vec3::new(dx,dy,dz), on_ground:requested.y<0.0 && dy>requested.y }
}
fn clip_axis(world:&World,bounds:Aabb,delta:f64,axis:usize)->f64 {
    if delta.abs()<1.0e-12 { return 0.0; }
    let eps=1.0e-9;
    let mins=[bounds.min.x,bounds.min.y,bounds.min.z];
    let maxs=[bounds.max.x,bounds.max.y,bounds.max.z];
    let mut swept_min=mins; let mut swept_max=maxs;
    swept_min[axis]=(mins[axis]+delta).min(mins[axis]);
    swept_max[axis]=(maxs[axis]+delta).max(maxs[axis]);
    let ranges=[
        (swept_min[0].floor() as i32)..=((swept_max[0]-eps).floor() as i32),
        (swept_min[1].floor() as i32)..=((swept_max[1]-eps).floor() as i32),
        (swept_min[2].floor() as i32)..=((swept_max[2]-eps).floor() as i32),
    ];
    let mut allowed=delta;
    for x in ranges[0].clone(){for y in ranges[1].clone(){for z in ranges[2].clone(){
        let state=world.get_block(BlockPosition::new(x,y,z));
        if !is_solid_state(state){continue;}
        let block_min=[x as f64,y as f64,z as f64]; let block_max=[x as f64+1.0,y as f64+1.0,z as f64+1.0];
        let a=(axis+1)%3;let b=(axis+2)%3;
        if maxs[a]<=block_min[a]||mins[a]>=block_max[a]||maxs[b]<=block_min[b]||mins[b]>=block_max[b]{continue;}
        if allowed>0.0&&maxs[axis]<=block_min[axis]&&maxs[axis]+allowed>block_min[axis]{allowed=allowed.min(block_min[axis]-maxs[axis]);}
        else if allowed<0.0&&mins[axis]>=block_max[axis]&&mins[axis]+allowed<block_max[axis]{allowed=allowed.max(block_max[axis]-mins[axis]);}
    }}}
    allowed
}
#[cfg(test)]
mod tests {
 use super::*; use crate::world::BlockStateId;
 fn unit_box_at(x:f64)->Aabb{Aabb::new(Vec3::new(x,0.0,0.0),Vec3::new(x+1.0,1.0,1.0))}
 #[test]fn overlapping_boxes_intersect(){assert!(unit_box_at(0.0).intersects(unit_box_at(0.5)));}
 #[test]fn touching_faces_do_not_intersect(){assert!(!unit_box_at(0.0).intersects(unit_box_at(1.0)));}
 #[test]fn translated_box_preserves_dimensions(){let moved=unit_box_at(0.0).translated(Vec3::new(2.0,3.0,-1.0));assert_eq!(moved.min,Vec3::new(2.0,3.0,-1.0));assert_eq!(moved.max,Vec3::new(3.0,4.0,0.0));}
 #[test]fn movement_stops_at_solid_block(){let mut world=World::new();world.set_block(BlockPosition::new(2,0,0),BlockStateId(1));let bounds=Aabb::new(Vec3::new(0.5,0.0,0.1),Vec3::new(1.1,1.8,0.9));let result=move_aabb(&world,bounds,Vec3::new(2.0,0.0,0.0));assert!((result.bounds.max.x-2.0).abs()<1e-9);assert!(result.applied.x<2.0);}
}