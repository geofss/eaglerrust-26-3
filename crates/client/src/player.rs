//! Basic collision-aware player controller; not yet the full 26.3 movement model.
use crate::input::InputFrame;
use crate::physics::{move_aabb,Aabb,Vec3};
use crate::world::World;
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct Player { pub position:Vec3, pub velocity:Vec3, pub yaw:f64, pub on_ground:bool }
impl Player {
 pub fn new(position:Vec3)->Self{Self{position,velocity:Vec3::default(),yaw:0.0,on_ground:false}}
 pub fn bounds(&self)->Aabb{Aabb::new(Vec3::new(self.position.x-0.3,self.position.y,self.position.z-0.3),Vec3::new(self.position.x+0.3,self.position.y+1.8,self.position.z+0.3))}
 /// Advance one fixed simulation step; a 20 Hz tick is the intended host cadence.
 pub fn tick(&mut self,world:&World,input:&InputFrame){
  let(forward,strafe)=input.normalized_movement();let forward=f64::from(forward);let strafe=f64::from(strafe);let sin=self.yaw.sin();let cos=self.yaw.cos();let speed=if input.sprint_pressed{0.13}else{0.1};
  self.velocity.x=(sin*forward+cos*strafe)*speed;self.velocity.z=(-cos*forward+sin*strafe)*speed;
  if input.jump_pressed&&self.on_ground{self.velocity.y=0.42;self.on_ground=false;}
  self.velocity.y=(self.velocity.y-0.08).max(-3.92);
  let result=move_aabb(world,self.bounds(),self.velocity);
  self.position=Vec3::new((result.bounds.min.x+result.bounds.max.x)*0.5,result.bounds.min.y,(result.bounds.min.z+result.bounds.max.z)*0.5);
  if result.applied.x!=self.velocity.x{self.velocity.x=0.0;}if result.applied.z!=self.velocity.z{self.velocity.z=0.0;}if result.applied.y!=self.velocity.y{self.velocity.y=0.0;}self.on_ground=result.on_ground;
 }
}
#[cfg(test)]mod tests{use super::*;use crate::world::{BlockPosition,BlockStateId};#[test]fn player_falls_until_supported_by_ground(){let mut world=World::new();for x in -1..=1{for z in -1..=1{world.set_block(BlockPosition::new(x,0,z),BlockStateId(1));}}let mut player=Player::new(Vec3::new(0.5,1.0,0.5));for _ in 0..30{player.tick(&world,&InputFrame::default());}assert!(player.on_ground);assert!((player.position.y-1.0).abs()<1e-8);}}