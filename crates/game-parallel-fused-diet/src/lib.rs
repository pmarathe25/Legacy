//! Physical consumption of authored food goods, staged per person.
use game_parallel::World;
mod effects;
pub use effects::{apply_due, clear_due};
use game_parallel_fused_data::Block;
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Bite {
    person: u64,
    good: u32,
    quantity: u64,
    relief: u32,
    health: i32,
    nutrients: [u32; 6],
}
pub struct Draft {
    pub actors: Vec<u64>,
    bites: Vec<Bite>,
    effect_at: Option<u64>,
}
pub struct Meals {
    pub diet: Option<Draft>,
    pub carried: Vec<u64>,
    pub debits: Option<Vec<u64>>,
}

fn dense_positions(positions: &[(u64, usize, usize)]) -> Option<u64> {
    let first = positions.first()?.0;
    let last = positions.last()?.0;
    (last.checked_sub(first) == Some((positions.len() - 1) as u64)).then_some(first)
}
fn person_position(
    positions: &[(u64, usize, usize)],
    dense: Option<u64>,
    person: u64,
) -> Option<(usize, usize)> {
    let index = dense
        .and_then(|first| person.checked_sub(first))
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|&index| index < positions.len())
        .or_else(|| {
            if dense.is_none() {
                positions.binary_search_by_key(&person, |row| row.0).ok()
            } else {
                None
            }
        })?;
    Some((positions[index].1, positions[index].2))
}

fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    busy: (&[u64], &[u64], &[u64]),
) -> Option<Draft> {
    let cargo = world
        .cargo
        .as_ref()
        .filter(|row| row.auto_eat_local || row.auto_eat_travel)?;
    let goods = world.goods_cargo.as_ref()?;
    let production = world.production.as_ref()?;
    let mut edible: Vec<_> = production
        .goods
        .par_iter()
        .filter_map(|good| {
            good.food.as_ref().map(|food| {
                (
                    good.id,
                    food,
                    game_parallel_nutrition::amounts(&food.nutrients),
                )
            })
        })
        .collect();
    if edible.is_empty() {
        return None;
    }
    if edible.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        edible.par_sort_unstable_by_key(|row| row.0);
    }
    let dense = dense_positions(positions);
    let mut bites: Vec<_> = goods
        .holdings
        .par_iter()
        .filter_map(|holding| {
            if [busy.0, busy.1, busy.2]
                .iter()
                .any(|rows| rows.binary_search(&holding.person).is_ok())
            {
                return None;
            }
            let (block, slot) = person_position(positions, dense, holding.person)?;
            let actor = &blocks[block];
            if actor.health[slot] == 0
                || actor.hunger[slot].saturating_add(world.rules.hunger_per_tick)
                    < world.rules.hunger_satisfied
                || (actor.journey.is_none() && !cargo.auto_eat_local)
                || (actor.journey.is_some() && !cargo.auto_eat_travel)
            {
                return None;
            }
            holding
                .items
                .iter()
                .filter_map(|item| {
                    let (_, food, nutrients) =
                        edible[edible.binary_search_by_key(&item.good, |row| row.0).ok()?];
                    (item.quantity >= food.portion).then_some(Bite {
                        person: holding.person,
                        good: item.good,
                        quantity: food.portion,
                        relief: food.hunger_relief,
                        health: food.health_effect,
                        nutrients: if world.nutrition.is_some() {
                            nutrients
                        } else {
                            [0; 6]
                        },
                    })
                })
                .max_by_key(|bite| (bite.relief, std::cmp::Reverse(bite.good)))
        })
        .collect();
    if bites.is_empty() {
        return None;
    }
    if bites
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        bites.par_sort_unstable_by_key(|bite| bite.person);
    }
    let actors = bites.par_iter().map(|bite| bite.person).collect();
    Some(Draft {
        actors,
        bites,
        effect_at: None,
    })
}

/// Stage carried scalar meals and distinct edible goods without double consumption.
pub fn meals(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    planned: &[u64],
    actors: &mut Vec<u64>,
    fed: &[u64],
    caregivers: &[u64],
) -> Result<Meals, &'static str> {
    let scalar = game_parallel_fused_cargo::meals(world, blocks, positions, actors);
    let mut diet = prepare(world, blocks, positions, (planned, actors, &scalar));
    if let Some(draft) = &mut diet
        && draft
            .bites
            .par_iter()
            .any(|bite| bite.health != 0 || bite.nutrients != [0; 6])
    {
        draft.effect_at = Some(
            world
                .tick
                .checked_add(2)
                .ok_or("food effect tick overflow")?,
        );
    }
    if diet.is_none() && caregivers.is_empty() && fed.is_empty() {
        return Ok(Meals {
            diet,
            carried: scalar,
            debits: None,
        });
    }
    let mut carried = scalar.clone();
    let mut debits = scalar;
    if let Some(diet) = &diet {
        carried.par_extend(diet.actors.par_iter().copied());
        actors.par_extend(diet.actors.par_iter().copied());
    }
    carried.extend_from_slice(fed);
    debits.extend_from_slice(caregivers);
    if carried.par_windows(2).any(|pair| pair[0] > pair[1]) {
        carried.par_sort_unstable();
    }
    if debits.par_windows(2).any(|pair| pair[0] > pair[1]) {
        debits.par_sort_unstable();
    }
    if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
        actors.par_sort_unstable();
    }
    Ok(Meals {
        diet,
        carried,
        debits: Some(debits),
    })
}

impl Draft {
    /// Replace the generic meal reduction with the selected good's own value.
    pub fn apply_hunger(&self, blocks: &mut [Block], positions: &[(u64, usize, usize)], gain: u32) {
        if blocks.par_iter().all(|block| {
            block
                .ids
                .first()
                .zip(block.ids.last())
                .is_some_and(|(&first, &last)| {
                    last.checked_sub(first) == Some((block.ids.len() - 1) as u64)
                })
        }) {
            blocks.par_iter_mut().for_each(|block| {
                let first = block.ids[0];
                let last = *block.ids.last().expect("nonempty block");
                let start = self.bites.partition_point(|bite| bite.person < first);
                let end = self.bites.partition_point(|bite| bite.person <= last);
                for bite in &self.bites[start..end] {
                    let slot = (bite.person - first) as usize;
                    block.next_hunger[slot] = block.hunger[slot]
                        .saturating_add(gain)
                        .saturating_sub(bite.relief);
                }
            });
            return;
        }
        let dense = dense_positions(positions);
        let mut slots: Vec<_> = self
            .bites
            .par_iter()
            .map(|bite| {
                let (block, slot) =
                    person_position(positions, dense, bite.person).expect("prepared eater");
                (block, slot, bite.relief)
            })
            .collect();
        if slots
            .par_windows(2)
            .any(|pair| (pair[0].0, pair[0].1) > (pair[1].0, pair[1].1))
        {
            slots.par_sort_unstable_by_key(|row| (row.0, row.1));
        }
        blocks
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, block)| {
                let first = slots.partition_point(|row| row.0 < index);
                let end = slots.partition_point(|row| row.0 <= index);
                for &(_, slot, relief) in &slots[first..end] {
                    block.next_hunger[slot] = block.hunger[slot]
                        .saturating_add(gain)
                        .saturating_sub(relief);
                }
            });
    }
    /// Publish after disjoint trade and construction have updated held goods.
    pub fn commit(self, world: &mut World) {
        if let Some(at) = self.effect_at {
            let effects: Vec<_> = self
                .bites
                .par_iter()
                .filter(|bite| bite.health != 0 || bite.nutrients != [0; 6])
                .map(|bite| game_parallel::FoodEffect {
                    person: bite.person,
                    at,
                    health: bite.health,
                    nutrients: bite.nutrients,
                })
                .collect();
            let pending = &mut world
                .food_effects
                .get_or_insert_with(Default::default)
                .pending;
            if pending.is_empty() {
                *pending = effects;
            } else {
                pending.par_extend(effects);
            }
        }
        let cargo = world.goods_cargo.as_mut().expect("prepared edible goods");
        let dense = self.bites.first().and_then(|first| {
            self.bites.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((self.bites.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        cargo.holdings.par_iter_mut().for_each(|holding| {
            let index = dense
                .and_then(|first| holding.person.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < self.bites.len())
                .or_else(|| {
                    if dense.is_none() {
                        self.bites
                            .binary_search_by_key(&holding.person, |bite| bite.person)
                            .ok()
                    } else {
                        None
                    }
                });
            if let Some(index) = index {
                let bite = self.bites[index];
                let item = holding
                    .items
                    .iter_mut()
                    .find(|item| item.good == bite.good)
                    .expect("reserved edible good");
                item.quantity -= bite.quantity;
                holding.items.retain(|item| item.quantity > 0);
            }
        });
        cargo.holdings = std::mem::take(&mut cargo.holdings)
            .into_par_iter()
            .filter(|holding| !holding.items.is_empty())
            .collect();
    }
}

/// Consume one actual portion immediately for a checked player command.
pub fn eat(world: &mut World, person: u64, good: u32) -> Result<(), String> {
    if world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick))
        == Some(person)
    {
        return Err("player control is suspended".into());
    }
    let production = world.production.as_ref().ok_or("production is disabled")?;
    if production
        .jobs
        .iter()
        .any(|job| job.worker == person && matches!(job.phase, game_parallel::JobPhase::Working))
    {
        return Err("person is busy".into());
    }
    let food = production
        .goods
        .iter()
        .find(|row| row.id == good)
        .and_then(|row| row.food.as_ref())
        .ok_or("good is not edible")?
        .clone();
    let nutrients = if world.nutrition.is_some() {
        game_parallel_nutrition::amounts(&food.nutrients)
    } else {
        [0; 6]
    };
    let effect_at = if food.health_effect == 0 && nutrients == [0; 6] {
        None
    } else {
        Some(
            world
                .tick
                .checked_add(1)
                .ok_or("food effect tick overflow")?,
        )
    };
    let actor = world
        .regions
        .iter_mut()
        .flat_map(|region| region.people.iter_mut())
        .chain(
            world
                .travelers
                .iter_mut()
                .map(|traveler| &mut traveler.person),
        )
        .find(|actor| actor.id == person && actor.health > 0)
        .ok_or("person must be alive")?;
    if actor.hunger == 0 {
        return Err("person is not hungry".into());
    }
    let cargo = world
        .goods_cargo
        .as_mut()
        .ok_or("goods cargo is disabled")?;
    let holder = cargo
        .holdings
        .iter_mut()
        .find(|row| row.person == person)
        .ok_or("person does not carry this good")?;
    let item = holder
        .items
        .iter_mut()
        .find(|item| item.good == good && item.quantity >= food.portion)
        .ok_or("insufficient carried food")?;
    item.quantity -= food.portion;
    holder.items.retain(|item| item.quantity > 0);
    cargo.holdings.retain(|row| !row.items.is_empty());
    actor.hunger = actor.hunger.saturating_sub(food.hunger_relief);
    if let Some(at) = effect_at {
        world
            .food_effects
            .get_or_insert_with(Default::default)
            .pending
            .push(game_parallel::FoodEffect {
                person,
                at,
                health: food.health_effect,
                nutrients,
            });
    }
    Ok(())
}

#[macro_export]
macro_rules! diet_methods {
    () => {
        pub fn eat_good(&mut self, person: u64, good: u32) -> Result<(), String> {
            game_parallel_fused_conflict::check(&self.template, &[person])?;
            self.edit_world(|world| game_parallel_fused_diet::eat(world, person, good))
        }
    };
}
