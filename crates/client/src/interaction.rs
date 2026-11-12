//! Block selection and basic break/place operations for a local prototype world.
use crate::raycast::{cast_blocks,BlockHit};
use crate::world::{BlockPosition,BlockStateId,World,AIR};
pub fn target_block(world:&World,eye:[f64;3],direction:[f64;3],reach:f64)->Option<BlockHit>{cast_blocks(world,eye,direction,reach)}
pub fn break_block(world:&mut World,hit:BlockHit)->bool{world.set_block(hit.position,AIR)}
pub fn place_block(world:&mut World,hit:BlockHit,state:BlockStateId)->bool{
 if hit.face==[0,0,0]||state==AIR{return false;}
 let target=BlockPosition::new(hit.position.x+hit.face[0],hit.position.y+hit.face[1],hit.position.z+hit.face[2]);
 if world.get_block(target)!=AIR{return false;}world.set_block(target,state)
}
#[cfg(test)]mod tests{use super::*;#[test]fn blocks_can_be_broken_and_placed_on_hit_face(){let mut world=World::new();let pos=BlockPosition::new(2,0,0);world.set_block(pos,BlockStateId(1));let hit=target_block(&world,[0.5,0.5,0.5],[1.0,0.0,0.0],5.0).unwrap();assert!(place_block(&mut world,hit,BlockStateId(2)));assert_eq!(world.get_block(BlockPosition::new(1,0,0)),BlockStateId(2));assert!(break_block(&mut world,hit));assert_eq!(world.get_block(pos),AIR);}}