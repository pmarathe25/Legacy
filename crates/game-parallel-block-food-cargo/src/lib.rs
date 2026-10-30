//! Personal food custody and travel provisions for compiled blocks.
mod packing;
pub mod stores;
use game_parallel::{Holding, World};
use game_parallel_block_data::Block;
use game_parallel_cargo::Cargo;
use game_parallel_production_model::{PersonSnapshot, person_index};
pub use packing::{PackDraft, prepare_pack};
use rayon::prelude::*;
pub struct Context<'a> {
    pub world: &'a mut World,
    pub food: &'a mut [u32],
    pub blocks: &'a mut [Block],
    pub people: &'a mut [PersonSnapshot],
    pub positions: &'a [(usize, usize)],
}
impl Context<'_> {
    fn actor(&self, id: u64) -> Result<(usize, Option<usize>, u32, u32), String> {
        if self
            .world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(self.world.tick))
            == Some(id)
        {
            return Err("player control is suspended".into());
        }
        let i = person_index(self.people, id).ok_or("unknown person")?;
        let (_, region, health, hunger) = self.people[i];
        let region = region
            .map(|id| {
                self.world
                    .regions
                    .iter()
                    .position(|r| r.id == id)
                    .ok_or("unknown region")
            })
            .transpose()?;
        Ok((i, region, health, hunger))
    }
    pub fn pickup(&mut self, person: u64, amount: u32) -> Result<(), String> {
        if amount == 0 {
            return Err("amount must be positive".into());
        }
        let (_, region, health, _) = self.actor(person)?;
        let region = region.ok_or("person must be local")?;
        if health == 0 {
            return Err("dead person cannot pick up food".into());
        }
        let cargo = self
            .world
            .cargo
            .as_ref()
            .ok_or("personal cargo is disabled")?;
        cargo.validate_index(self.people)?;
        let index = cargo.holdings.iter().position(|h| h.person == person);
        let held = index.map_or(0, |i| cargo.holdings[i].food);
        let total = held.checked_add(amount).ok_or("cargo overflow")?;
        if total > cargo.capacity {
            return Err("carrying capacity exceeded".into());
        }
        if self.food[region] < amount {
            return Err("insufficient local food".into());
        }
        self.food[region] -= amount;
        let cargo = self.world.cargo.as_mut().unwrap();
        if let Some(i) = index {
            cargo.holdings[i].food = total;
        } else {
            cargo.holdings.push(Holding {
                person,
                food: total,
            });
            cargo.holdings.sort_unstable_by_key(|h| h.person);
        }
        Ok(())
    }
    pub fn eat(&mut self, person: u64) -> Result<(), String> {
        let (index, _, health, hunger) = self.actor(person)?;
        if health == 0 || hunger == 0 {
            return Err("person cannot eat now".into());
        }
        let cargo = self
            .world
            .cargo
            .as_ref()
            .ok_or("personal cargo is disabled")?;
        cargo.validate_index(self.people)?;
        let holding = cargo
            .holdings
            .iter()
            .position(|h| h.person == person)
            .ok_or("insufficient carried food")?;
        let meal = self.world.rules.food_per_meal;
        if cargo.holdings[holding].food < meal {
            return Err("insufficient carried food".into());
        }
        let new_hunger = hunger.saturating_sub(self.world.rules.hunger_satisfied);
        let (block, offset) = self.positions[index];
        self.blocks[block].hunger[offset] = new_hunger;
        self.people[index].3 = new_hunger;
        let cargo = self.world.cargo.as_mut().unwrap();
        cargo.holdings[holding].food -= meal;
        if cargo.holdings[holding].food == 0 {
            cargo.holdings.remove(holding);
        }
        Ok(())
    }
    pub fn deposit(
        &mut self,
        actor: u64,
        holder: u64,
        amount: u32,
        recover: bool,
    ) -> Result<(), String> {
        if amount == 0 || (recover && actor == holder) {
            return Err("invalid amount or holder".into());
        }
        let (_, region, health, _) = self.actor(actor)?;
        let region = region.ok_or("person must be local")?;
        if health == 0 {
            return Err("dead person cannot handle food".into());
        }
        if recover {
            let (_, other, health, _) = self.actor(holder)?;
            if other != Some(region) || health != 0 {
                return Err("holder must be deceased and local".into());
            }
        }
        let cargo = self
            .world
            .cargo
            .as_ref()
            .ok_or("personal cargo is disabled")?;
        cargo.validate_index(self.people)?;
        let index = cargo
            .holdings
            .iter()
            .position(|h| h.person == holder)
            .ok_or("holder has no cargo")?;
        if cargo.holdings[index].food < amount {
            return Err("insufficient carried food".into());
        }
        let total = self.food[region]
            .checked_add(amount)
            .filter(|&n| n <= self.world.rules.food_capacity)
            .ok_or("local storage is full")?;
        self.food[region] = total;
        let cargo = self.world.cargo.as_mut().unwrap();
        cargo.holdings[index].food -= amount;
        if cargo.holdings[index].food == 0 {
            cargo.holdings.remove(index);
        }
        Ok(())
    }
}
/// Determine automatic meals at a tick boundary without changing holdings.
pub fn prepare_meals(
    cargo: &Cargo,
    people: &[PersonSnapshot],
    rules: &game_parallel::Rules,
) -> Result<Vec<u64>, String> {
    cargo.validate_index(people)?;
    if !cargo.auto_eat_travel {
        return Ok(Vec::new());
    }
    let mut ids: Vec<_> = cargo
        .holdings
        .par_iter()
        .filter_map(|h| {
            let (_, region, health, hunger) = people[person_index(people, h.person)?];
            (region.is_none()
                && health > 0
                && hunger.saturating_add(rules.hunger_per_tick) >= rules.hunger_satisfied
                && h.food >= rules.food_per_meal)
                .then_some(h.person)
        })
        .collect();
    ids.par_sort_unstable();
    Ok(ids)
}
pub fn commit_meals(cargo: &mut Cargo, ids: &[u64], meal: u32) {
    cargo.holdings.par_iter_mut().for_each(|h| {
        if ids.binary_search(&h.person).is_ok() {
            h.food -= meal;
        }
    });
    cargo.holdings = cargo
        .holdings
        .par_iter()
        .filter(|h| h.food > 0)
        .cloned()
        .collect();
}
