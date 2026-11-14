//! Nine-slot hotbar model. Persistence and the official item registry are pending.
use crate::world::BlockStateId;
pub const HOTBAR_SLOTS:usize=9;pub const STACK_LIMIT:u8=64;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub struct ItemStack{pub item:BlockStateId,pub count:u8}
#[derive(Debug,Clone,PartialEq,Eq)]pub struct Hotbar{slots:[Option<ItemStack>;HOTBAR_SLOTS],selected:usize}
impl Default for Hotbar{fn default()->Self{Self{slots:[None;HOTBAR_SLOTS],selected:0}}}
impl Hotbar{
 pub fn selected_slot(&self)->usize{self.selected}
 pub fn select(&mut self,slot:usize)->bool{if slot>=HOTBAR_SLOTS{return false;}self.selected=slot;true}
 pub fn slot(&self,index:usize)->Option<ItemStack>{self.slots.get(index).copied().flatten()}
 pub fn set_slot(&mut self,index:usize,item:BlockStateId,count:u8)->bool{
  let Some(slot)=self.slots.get_mut(index)else{return false;};
  *slot=if count==0{None}else{Some(ItemStack{item,count})};
  true
 }
 pub fn clear(&mut self){self.slots=[None;HOTBAR_SLOTS];self.selected=0;}
 pub fn selected(&self)->Option<ItemStack>{self.slot(self.selected)}
 pub fn add(&mut self,item:BlockStateId,mut count:u8)->u8{
  for stack in self.slots.iter_mut().flatten().filter(|s|s.item==item&&s.count<STACK_LIMIT){let moved=count.min(STACK_LIMIT-stack.count);stack.count+=moved;count-=moved;if count==0{return 0;}}
  for slot in &mut self.slots{if slot.is_none(){let moved=count.min(STACK_LIMIT);*slot=Some(ItemStack{item,count:moved});count-=moved;if count==0{return 0;}}}count
 }
 pub fn consume_selected(&mut self,count:u8)->bool{let Some(stack)=self.slots[self.selected].as_mut()else{return false;};if stack.count<count{return false;}stack.count-=count;if stack.count==0{self.slots[self.selected]=None;}true}
}
#[cfg(test)]mod tests{use super::*;#[test]fn stacks_merge_and_overflow_is_reported(){let mut bar=Hotbar::default();assert_eq!(bar.add(BlockStateId(3),70),0);assert_eq!(bar.slot(0).unwrap().count,64);assert_eq!(bar.slot(1).unwrap().count,6);}#[test]fn selected_stack_can_be_consumed(){let mut bar=Hotbar::default();bar.add(BlockStateId(2),3);assert!(bar.consume_selected(2));assert_eq!(bar.selected().unwrap().count,1);assert!(bar.consume_selected(1));assert!(bar.selected().is_none());}}