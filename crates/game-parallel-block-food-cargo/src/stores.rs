//! Checked manual transfer between personal custody and a physical store.
use game_parallel::{Holding, World};

pub fn transfer(
    world: &mut World,
    person: u64,
    id: u64,
    amount: u32,
    deposit: bool,
) -> Result<(), String> {
    if amount == 0 {
        return Err("amount must be positive".into());
    }
    if world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick))
        == Some(person)
    {
        return Err("player control is suspended".into());
    }
    let region = world
        .regions
        .iter()
        .find(|region| region.people.iter().any(|p| p.id == person && p.health > 0))
        .ok_or("person must be alive and local")?;
    let cargo = world.cargo.as_ref().ok_or("personal cargo is disabled")?;
    let store = cargo
        .food_stores
        .iter()
        .find(|store| store.id == id && store.region == region.id)
        .ok_or("food store must be local")?;
    let held = cargo
        .holdings
        .iter()
        .find(|h| h.person == person)
        .map_or(0, |h| h.food);
    if deposit {
        if held < amount {
            return Err("insufficient carried food".into());
        }
        if amount > store.capacity - store.food {
            return Err("food store is full".into());
        }
    } else {
        if store.food < amount {
            return Err("insufficient stored food".into());
        }
        if amount > cargo.capacity - held {
            return Err("carrying capacity exceeded".into());
        }
    }
    let witnesses: Vec<_> = if !deposit && world.opinions.is_some() {
        game_parallel_store_witnesses::StoreWitnesses::new(cargo)
            .candidates(id, person, world.tick)
            .filter(|&claimant| {
                claimant != person
                    && region
                        .people
                        .iter()
                        .any(|p| p.id == claimant && p.health > 0)
            })
            .map(|claimant| (claimant, person))
            .collect()
    } else {
        Vec::new()
    };
    let cargo = world.cargo.as_mut().unwrap();
    let store = cargo
        .food_stores
        .iter_mut()
        .find(|store| store.id == id)
        .unwrap();
    if deposit {
        store.food += amount;
    } else {
        store.food -= amount;
    }
    let next = if deposit {
        held - amount
    } else {
        held + amount
    };
    if let Some(index) = cargo.holdings.iter().position(|h| h.person == person) {
        if next == 0 {
            cargo.holdings.remove(index);
        } else {
            cargo.holdings[index].food = next;
        }
    } else if next > 0 {
        cargo.holdings.push(Holding { person, food: next });
        cargo.holdings.sort_unstable_by_key(|h| h.person);
    }
    if let Some(opinions) = &mut world.opinions {
        opinions.witness_thefts(&witnesses, world.tick);
    }
    Ok(())
}

/// Construct a store by consuming the builder's actual carried material.
pub fn build(world: &mut World, person: u64, id: u64, blueprint_id: u32) -> Result<(), String> {
    if id == 0
        || world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(world.tick))
            == Some(person)
    {
        return Err("invalid builder or store id".into());
    }
    let region = world
        .regions
        .iter()
        .find(|region| region.people.iter().any(|p| p.id == person && p.health > 0))
        .ok_or("person must be alive and local")?
        .id;
    if world
        .production
        .as_ref()
        .is_some_and(|production| production.jobs.iter().any(|job| job.worker == person))
    {
        return Err("builder is busy".into());
    }
    let cargo = world.cargo.as_ref().ok_or("personal cargo is disabled")?;
    if cargo.food_stores.iter().any(|store| store.id == id) {
        return Err("duplicate food store id".into());
    }
    let blueprint = cargo
        .food_store_blueprints
        .iter()
        .find(|row| row.id == blueprint_id)
        .ok_or("unknown food store blueprint")?
        .clone();
    let goods = world
        .goods_cargo
        .as_mut()
        .ok_or("goods cargo is disabled")?;
    let holder = goods
        .holdings
        .iter_mut()
        .find(|row| row.person == person)
        .ok_or("missing construction material")?;
    let index = holder
        .items
        .iter()
        .position(|item| item.good == blueprint.good && item.quantity >= blueprint.quantity)
        .ok_or("missing construction material")?;
    holder.items[index].quantity -= blueprint.quantity;
    if holder.items[index].quantity == 0 {
        holder.items.remove(index);
    }
    goods.holdings.retain(|row| !row.items.is_empty());
    world
        .cargo
        .as_mut()
        .unwrap()
        .food_stores
        .push(game_parallel::FoodStore {
            id,
            region,
            food: 0,
            capacity: blueprint.capacity,
            claimant: Some(person),
            object_to_taking: false,
        });
    Ok(())
}

#[macro_export]
macro_rules! store_transfer_methods {
    () => {
        pub fn build_food_store(
            &mut self,
            person: u64,
            store: u64,
            blueprint: u32,
        ) -> Result<(), String> {
            game_parallel_fused_conflict::check(&self.template, &[person])?;
            game_parallel_fused_admission::validate_ages(
                self.owners.family.as_ref(),
                rayon::iter::once(person),
            )?;
            self.edit_world(|world| {
                game_parallel_block_food_cargo::stores::build(world, person, store, blueprint)
            })
        }
        pub fn transfer_food_store(
            &mut self,
            person: u64,
            store: u64,
            amount: u32,
            deposit: bool,
        ) -> Result<(), String> {
            game_parallel_fused_conflict::check(&self.template, &[person])?;
            self.edit_world(|world| {
                game_parallel_block_food_cargo::stores::transfer(
                    world, person, store, amount, deposit,
                )
            })
        }
    };
}
