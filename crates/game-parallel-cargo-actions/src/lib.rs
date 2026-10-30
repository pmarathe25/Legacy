//! Checked person-local food transfers and eating.
use game_parallel_cargo::{Cargo, Holding};
use game_parallel_people::{Region, Rules};
use game_parallel_transport::Traveler;

pub struct Context<'a> {
    pub regions: &'a mut [Region],
    pub travelers: &'a mut [Traveler],
    pub rules: &'a Rules,
    pub cargo: Option<&'a mut Cargo>,
    pub suspended: Option<u64>,
}
impl Context<'_> {
    fn local_person(&self, id: u64) -> Result<(usize, u32), String> {
        let mut found = self.regions.iter().enumerate().flat_map(|(i, r)| {
            r.people
                .iter()
                .filter(move |p| p.id == id)
                .map(move |p| (i, p.health))
        });
        let person = found.next().ok_or("person is not in a region")?;
        if found.next().is_some() {
            return Err("duplicate person id".into());
        }
        Ok(person)
    }
    /// Move food from local storage to a living person's holdings atomically.
    pub fn pickup_food(&mut self, person: u64, amount: u32) -> Result<(), String> {
        if self.suspended == Some(person) {
            return Err("player control is suspended".into());
        }
        if amount == 0 {
            return Err("amount must be positive".into());
        }
        let (region, health) = self.local_person(person)?;
        if health == 0 {
            return Err("dead person cannot pick up food".into());
        }
        let cargo = self.cargo.as_ref().ok_or("personal cargo is disabled")?;
        let entries: Vec<_> = cargo
            .holdings
            .iter()
            .enumerate()
            .filter(|(_, h)| h.person == person)
            .collect();
        if entries.len() > 1 {
            return Err("duplicate cargo holder".into());
        }
        let current = entries.first().map_or(0, |(_, h)| h.food);
        let next = current.checked_add(amount).ok_or("cargo overflow")?;
        if next > cargo.capacity {
            return Err("carrying capacity exceeded".into());
        }
        if self.regions[region].food < amount {
            return Err("insufficient local food".into());
        }
        let position = entries.first().map(|(i, _)| *i);
        self.regions[region].food -= amount;
        let cargo = self.cargo.as_mut().unwrap();
        if let Some(i) = position {
            cargo.holdings[i].food = next;
        } else {
            cargo.holdings.push(Holding { person, food: next });
            cargo.holdings.sort_unstable_by_key(|h| h.person);
        }
        Ok(())
    }
    /// Consume one meal from a living person's own holdings, including in transit.
    pub fn eat_carried_food(&mut self, person: u64) -> Result<(), String> {
        if self.suspended == Some(person) {
            return Err("player control is suspended".into());
        }
        let local: Vec<_> = self
            .regions
            .iter()
            .enumerate()
            .flat_map(|(r, region)| {
                region
                    .people
                    .iter()
                    .enumerate()
                    .filter(move |(_, p)| p.id == person)
                    .map(move |(i, p)| (r, i, p.health, p.hunger))
            })
            .collect();
        let traveling: Vec<_> = self
            .travelers
            .iter()
            .enumerate()
            .filter(|(_, t)| t.person.id == person)
            .collect();
        if local.len() + traveling.len() != 1 {
            return Err("person is missing or duplicated".into());
        }
        let (health, hunger) = if let Some((_, _, health, hunger)) = local.first() {
            (*health, *hunger)
        } else {
            (traveling[0].1.person.health, traveling[0].1.person.hunger)
        };
        if health == 0 || hunger == 0 {
            return Err("person cannot eat now".into());
        }
        let cargo = self.cargo.as_ref().ok_or("personal cargo is disabled")?;
        let entries: Vec<_> = cargo
            .holdings
            .iter()
            .enumerate()
            .filter(|(_, h)| h.person == person)
            .collect();
        if entries.len() != 1 || entries[0].1.food < self.rules.food_per_meal {
            return Err("insufficient carried food".into());
        }
        let index = entries[0].0;
        let traveler_index = traveling.first().map(|(i, _)| *i);
        let local_index = local.first().map(|(r, i, _, _)| (*r, *i));
        let person = if let Some((r, i)) = local_index {
            &mut self.regions[r].people[i]
        } else {
            &mut self.travelers[traveler_index.unwrap()].person
        };
        person.hunger = person.hunger.saturating_sub(self.rules.hunger_satisfied);
        let cargo = self.cargo.as_mut().unwrap();
        cargo.holdings[index].food -= self.rules.food_per_meal;
        if cargo.holdings[index].food == 0 {
            cargo.holdings.remove(index);
        }
        Ok(())
    }
    /// Move food from a living local person's holdings to local storage.
    pub fn deposit_food(&mut self, person: u64, amount: u32) -> Result<(), String> {
        self.deposit_from(person, person, amount, false)
    }
    /// A living local person can recover food from a local deceased holder.
    pub fn recover_food(&mut self, actor: u64, deceased: u64, amount: u32) -> Result<(), String> {
        if actor == deceased {
            return Err("actor and deceased must differ".into());
        }
        self.deposit_from(actor, deceased, amount, true)
    }
    fn deposit_from(
        &mut self,
        actor: u64,
        holder: u64,
        amount: u32,
        recover: bool,
    ) -> Result<(), String> {
        if self.suspended == Some(actor) {
            return Err("player control is suspended".into());
        }
        if amount == 0 {
            return Err("amount must be positive".into());
        }
        let (region, health) = self.local_person(actor)?;
        if health == 0 {
            return Err("dead person cannot handle food".into());
        }
        if recover {
            let (other_region, other_health) = self.local_person(holder)?;
            if other_region != region || other_health != 0 {
                return Err("holder must be deceased and local".into());
            }
        }
        let cargo = self.cargo.as_ref().ok_or("personal cargo is disabled")?;
        let entries: Vec<_> = cargo
            .holdings
            .iter()
            .enumerate()
            .filter(|(_, h)| h.person == holder)
            .collect();
        if entries.len() != 1 {
            return Err("holder has no unique cargo".into());
        }
        let (index, holding) = entries[0];
        if holding.food < amount {
            return Err("insufficient carried food".into());
        }
        if self.regions[region]
            .food
            .checked_add(amount)
            .is_none_or(|n| n > self.rules.food_capacity)
        {
            return Err("local storage is full".into());
        }
        self.regions[region].food += amount;
        let cargo = self.cargo.as_mut().unwrap();
        cargo.holdings[index].food -= amount;
        if cargo.holdings[index].food == 0 {
            cargo.holdings.remove(index);
        }
        Ok(())
    }
}
