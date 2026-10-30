//! Checked individual cargo commands; preserve the transactional generation boundary.
use super::Kernel;
impl Kernel {
    game_parallel_fused_food_aid::request_methods!();
    game_parallel_fused_food::food_aid_methods!();
    pub(super) fn edit_world<R>(
        &mut self,
        edit: impl FnOnce(&mut game_parallel::World) -> Result<R, String>,
    ) -> Result<R, String> {
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        let mut world = self.export();
        let value = edit(&mut world)?;
        let mut next = Self::compile(world)?;
        next.revision = revision;
        *self = next;
        Ok(value)
    }
    pub fn pickup_food(&mut self, person: u64, amount: u32) -> Result<(), String> {
        game_parallel_fused_conflict::check(&self.template, &[person])?;
        self.edit_world(|world| world.pickup_food(person, amount))
    }
    pub fn deposit_food(&mut self, person: u64, amount: u32) -> Result<(), String> {
        game_parallel_fused_conflict::check(&self.template, &[person])?;
        self.edit_world(|world| world.deposit_food(person, amount))
    }
    pub fn recover_food(&mut self, actor: u64, deceased: u64, amount: u32) -> Result<(), String> {
        game_parallel_fused_conflict::check(&self.template, &[actor])?;
        self.edit_world(|world| world.recover_food(actor, deceased, amount))
    }
    pub fn eat_carried_food(&mut self, person: u64) -> Result<(), String> {
        game_parallel_fused_conflict::check(&self.template, &[person])?;
        self.edit_world(|world| world.eat_carried_food(person))
    }
}
