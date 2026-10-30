//! Person-owned nutrient columns, staged without population-wide publication.
use game_parallel::{FoodEffect, Nutrition, NutritionPerson, World};
use game_parallel_fused_data::{Block, NutritionColumns};
use game_parallel_fused_needs::food::Allocation;
use rayon::prelude::*;
use std::collections::HashMap;

pub fn import(nutrition: Option<&Nutrition>, blocks: &mut [Block]) {
    let Some(owner) = nutrition else {
        return;
    };
    let records: HashMap<_, _> = owner
        .people
        .par_iter()
        .map(|row| (row.person, row.reserves))
        .collect();
    blocks.par_iter_mut().for_each(|block| {
        let mut columns = NutritionColumns::new(block.ids.len(), owner.rules.initial, false);
        for (i, id) in block.ids.iter().enumerate() {
            if let Some(reserves) = records.get(id) {
                columns.values[i] = *reserves;
            }
        }
        block.nutrition = Some(columns);
    });
}

pub fn ensure(nutrition: Option<&Nutrition>, blocks: &mut [Block]) {
    let Some(owner) = nutrition else {
        return;
    };
    blocks.par_iter_mut().for_each(|block| {
        if block.nutrition.is_none() {
            block.nutrition = Some(NutritionColumns::new(
                block.ids.len(),
                owner.rules.initial,
                true,
            ));
        }
    });
}

pub fn publish(block: &mut Block) {
    if let Some(columns) = &mut block.nutrition {
        std::mem::swap(&mut columns.values, &mut columns.next_values);
        columns.dirty = true;
    }
}

pub fn export_if_dirty(
    nutrition: Option<&mut Nutrition>,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
) {
    let Some(owner) = nutrition else {
        return;
    };
    if blocks.par_iter().any(|block| {
        block
            .nutrition
            .as_ref()
            .is_some_and(|columns| columns.dirty)
    }) {
        owner.people = positions
            .par_iter()
            .map(|&(person, block, slot)| NutritionPerson {
                person,
                reserves: blocks[block]
                    .nutrition
                    .as_ref()
                    .expect("nutrition columns")
                    .values[slot],
            })
            .collect();
    }
}

#[macro_export]
macro_rules! export_nutrition {
    ($world:expr, $context:expr) => {
        $crate::export_if_dirty(
            $world.nutrition.as_mut(),
            $context.blocks,
            $context.positions,
        )
    };
}

fn dense(first: u64, last: u64, len: usize) -> bool {
    last.checked_sub(first) == Some((len - 1) as u64)
}

enum Intake<'a> {
    Direct {
        rows: &'a [FoodEffect],
        first: Option<u64>,
    },
    Grouped {
        rows: Vec<(u64, [u32; 6])>,
        first: Option<u64>,
    },
}
impl Intake<'_> {
    fn new(pending: &[FoodEffect], tick: u64) -> Intake<'_> {
        if pending.par_windows(2).all(|w| w[0].person < w[1].person) {
            let first = pending
                .first()
                .zip(pending.last())
                .and_then(|(a, b)| dense(a.person, b.person, pending.len()).then_some(a.person));
            return Intake::Direct {
                rows: pending,
                first,
            };
        }
        let mut rows: Vec<_> = pending
            .par_iter()
            .filter(|row| row.at <= tick && row.nutrients != [0; 6])
            .map(|row| (row.person, row.nutrients))
            .collect();
        if rows.par_windows(2).any(|w| w[0].0 > w[1].0) {
            rows.par_sort_unstable_by_key(|row| row.0);
        }
        let rows: Vec<_> = rows
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let mut total = [0u32; 6];
                for (_, values) in group {
                    for (sum, value) in total.iter_mut().zip(values) {
                        *sum = sum.saturating_add(*value);
                    }
                }
                (group[0].0, total)
            })
            .collect();
        let first = rows
            .first()
            .zip(rows.last())
            .and_then(|(a, b)| dense(a.0, b.0, rows.len()).then_some(a.0));
        Intake::Grouped { rows, first }
    }
    fn get(&self, person: u64, tick: u64) -> [u32; 6] {
        match self {
            Intake::Direct { rows, first } => {
                let i = lookup(*first, rows.len(), person, || {
                    rows.binary_search_by_key(&person, |r| r.person).ok()
                });
                i.filter(|&i| rows[i].at <= tick)
                    .map_or([0; 6], |i| rows[i].nutrients)
            }
            Intake::Grouped { rows, first } => {
                let i = lookup(*first, rows.len(), person, || {
                    rows.binary_search_by_key(&person, |r| r.0).ok()
                });
                i.map_or([0; 6], |i| rows[i].1)
            }
        }
    }
}
fn lookup(
    first: Option<u64>,
    len: usize,
    id: u64,
    sparse: impl FnOnce() -> Option<usize>,
) -> Option<usize> {
    if let Some(first) = first {
        id.checked_sub(first)
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < len)
    } else {
        sparse()
    }
}

pub fn prepare(
    world: &World,
    blocks: &mut [Block],
    allocation: &Allocation,
    diet: Option<&game_parallel_fused_diet::Draft>,
    tick: u64,
) {
    let Some(nutrition) = &world.nutrition else {
        return;
    };
    let rules = &nutrition.rules;
    let effects = Intake::new(
        world
            .food_effects
            .as_ref()
            .map_or(&[], |owner| owner.pending.as_slice()),
        tick,
    );
    let diet_actors = diet.map_or(&[][..], |draft| draft.actors.as_slice());
    let diet_first = diet_actors
        .first()
        .zip(diet_actors.last())
        .and_then(|(&a, &b)| dense(a, b, diet_actors.len()).then_some(a));
    let personal = world
        .cargo
        .as_ref()
        .is_some_and(|cargo| cargo.personal_food_access);
    let metabolism = tick.is_multiple_of(rules.interval);
    blocks.par_iter_mut().for_each(|block| {
        let columns = block.nutrition.as_mut().expect("nutrition columns");
        let mut rank = block.prefix;
        for i in 0..block.ids.len() {
            let id = block.ids[i];
            let mut reserves = columns.values[i];
            let granted = if block.food_claim[i] {
                let granted = allocation.cutoffs[block.region]
                    .map_or(rank < allocation.meals[block.region], |cutoff| {
                        cutoff.allows(id)
                    });
                rank += 1;
                granted && (!personal || block.care_food_claim[i])
            } else {
                false
            };
            if block.health[i] > 0 {
                let diet_meal = lookup(diet_first, diet_actors.len(), id, || {
                    diet_actors.binary_search(&id).ok()
                })
                .is_some();
                let scalar = granted || (block.carried_meal[i] && !diet_meal);
                let eaten = effects.get(id, tick);
                for n in 0..6 {
                    reserves[n] = reserves[n]
                        .saturating_add(eaten[n])
                        .saturating_add(if scalar { rules.scalar_meal[n] } else { 0 })
                        .min(rules.capacity[n]);
                    if metabolism {
                        reserves[n] = reserves[n].saturating_sub(rules.metabolism[n]);
                    }
                }
                if metabolism && (0..6).any(|n| reserves[n] < rules.minimum[n]) {
                    block.next_health[i] =
                        block.next_health[i].saturating_sub(rules.deficiency_damage);
                }
            }
            columns.next_values[i] = reserves;
        }
    });
}
