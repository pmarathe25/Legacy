use crate::World;
use game_parallel_cargo_actions::Context;
impl World {
    fn cargo_context(&mut self) -> Context<'_> {
        let suspended = self.suspended_person();
        Context {
            regions: &mut self.regions,
            travelers: &mut self.travelers,
            rules: &self.rules,
            cargo: self.cargo.as_mut(),
            suspended,
        }
    }
    pub fn pickup_food(&mut self, person: u64, amount: u32) -> Result<(), String> {
        self.cargo_context().pickup_food(person, amount)
    }
    pub fn eat_carried_food(&mut self, person: u64) -> Result<(), String> {
        game_parallel_nutrition::eat_scalar!(self, person)
    }
    pub fn deposit_food(&mut self, person: u64, amount: u32) -> Result<(), String> {
        self.cargo_context().deposit_food(person, amount)
    }
    pub fn recover_food(&mut self, actor: u64, deceased: u64, amount: u32) -> Result<(), String> {
        self.cargo_context().recover_food(actor, deceased, amount)
    }
}
