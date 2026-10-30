//! Direct transactional admission into packed job blocks.
use super::{BLOCK, Block, Bundle, PackedJobs, Row};
use game_parallel_production_model::{
    Amount, Inventory, PersonSnapshot, Recipe, Start, person_index,
};
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::HashMap;

pub struct StartDraft {
    blocks: Vec<Block>,
    needs: HashMap<(u64, u32), u128>,
}
impl StartDraft {
    /// Transfer staged blocks and stock claims to another private runtime owner.
    pub fn into_parts(self) -> (Vec<Block>, HashMap<(u64, u32), u128>) {
        (self.blocks, self.needs)
    }
    pub fn commit(self, jobs: &mut PackedJobs, inventories: &mut [Inventory]) {
        apply_needs(inventories, &self.needs);
        jobs.blocks.extend(self.blocks);
    }
}
/// Debit only the resources used by already validated work reservations.
pub fn apply_needs(inventories: &mut [Inventory], needs: &HashMap<(u64, u32), u128>) {
    if needs.is_empty() {
        return;
    }
    let mut changes: Vec<_> = needs
        .par_iter()
        .map(|(&key, &quantity)| (key, quantity))
        .collect();
    changes.par_sort_unstable_by_key(|row| row.0);
    inventories.par_iter_mut().for_each(|inventory| {
        let start = changes.partition_point(|row| row.0.0 < inventory.region);
        let end = changes.partition_point(|row| row.0.0 <= inventory.region);
        let active = &changes[start..end];
        if active.is_empty() {
            return;
        }
        if active.len() < inventory.stocks.len() / 8 {
            for &((_, good), quantity) in active {
                inventory
                    .stocks
                    .iter_mut()
                    .find(|stock| stock.good == good)
                    .expect("validated input stock")
                    .quantity -= quantity as u64;
            }
        } else {
            for stock in &mut inventory.stocks {
                if let Some(quantity) = needs.get(&(inventory.region, stock.good)) {
                    stock.quantity -= *quantity as u64;
                }
            }
        }
    });
}

impl PackedJobs {
    /// Admit an ordered suffix without materializing serialized job records.
    /// Interleaved IDs require callers to repack before using this operation.
    pub fn prepare_start(
        &self,
        recipes: &[Recipe],
        inventories: &[Inventory],
        people: &[PersonSnapshot],
        requests: &[Start],
    ) -> Result<StartDraft, String> {
        self.prepare_start_internal(recipes, inventories, people, requests, false)
    }
    /// For starts produced by the validated autonomous planner at this boundary.
    pub fn prepare_planned(
        &self,
        recipes: &[Recipe],
        inventories: &[Inventory],
        people: &[PersonSnapshot],
        requests: &[Start],
    ) -> Result<StartDraft, String> {
        self.prepare_start_internal(recipes, inventories, people, requests, true)
    }
    fn prepare_start_internal(
        &self,
        recipes: &[Recipe],
        inventories: &[Inventory],
        people: &[PersonSnapshot],
        requests: &[Start],
        planned: bool,
    ) -> Result<StartDraft, String> {
        let mut ordered: Vec<_> = requests.par_iter().collect();
        if !planned && ordered.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            ordered.par_sort_unstable_by_key(|request| request.id);
        }
        if !planned && ordered.par_windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("duplicate job or busy worker".into());
        }
        if let (Some(last), Some(first)) = (
            self.blocks.iter().rev().find_map(|block| block.rows.last()),
            ordered.first(),
        ) && last.id >= first.id
        {
            return Err("interleaved job IDs require repacking".into());
        }
        if !planned {
            let mut workers: Vec<_> = self.busy_people();
            workers.par_extend(requests.par_iter().map(|request| request.worker));
            if workers.par_windows(2).any(|pair| pair[0] > pair[1]) {
                workers.par_sort_unstable();
            }
            if workers.par_windows(2).any(|pair| pair[0] == pair[1]) {
                return Err("duplicate job or busy worker".into());
            }
        }
        let recipe_index: HashMap<_, _> =
            recipes.iter().map(|recipe| (recipe.id, recipe)).collect();
        let inventory_index: HashMap<_, _> = inventories
            .iter()
            .map(|inventory| (inventory.region, inventory))
            .collect();
        let blocks: Vec<Block> = ordered
            .par_chunks(BLOCK)
            .map(|chunk| -> Result<Block, String> {
                let mut bundles = Vec::<Bundle>::new();
                let mut bundle_index = HashMap::<(u32, u32), u32>::new();
                let mut rows = Vec::with_capacity(chunk.len());
                for request in chunk {
                    if request.units == 0 {
                        return Err("zero units".into());
                    }
                    if !planned
                        && !person_index(people, request.worker)
                            .is_some_and(|i| people[i].1 == Some(request.region) && people[i].2 > 0)
                    {
                        return Err("worker must be living and local".into());
                    }
                    let recipe = recipe_index.get(&request.recipe).ok_or("unknown recipe")?;
                    let inventory = inventory_index
                        .get(&request.region)
                        .ok_or("missing inventory")?;
                    let key = (request.recipe, request.units);
                    let bundle = if let Some(&index) = bundle_index.get(&key) {
                        index
                    } else {
                        let convert = |items: &[Amount]| -> Result<SmallVec<[Amount; 1]>, String> {
                            items
                                .iter()
                                .map(|item| {
                                    Ok(Amount {
                                        good: item.good,
                                        quantity: item
                                            .quantity
                                            .checked_mul(u64::from(request.units))
                                            .ok_or("quantity overflow")?,
                                    })
                                })
                                .collect()
                        };
                        let bundle = Bundle {
                            reserved: convert(&recipe.inputs)?,
                            outputs: convert(&recipe.outputs)?,
                        };
                        let index = bundles.len() as u32;
                        bundles.push(bundle);
                        bundle_index.insert(key, index);
                        index
                    };
                    if bundles[bundle as usize].outputs.iter().any(|output| {
                        !inventory.stocks.iter().any(|stock| {
                            stock.good == output.good && output.quantity <= stock.capacity
                        })
                    }) {
                        return Err("output has no adequate storage".into());
                    }
                    rows.push(Row {
                        id: request.id,
                        worker: request.worker,
                        region: request.region,
                        recipe: request.recipe,
                        remaining: u64::from(recipe.ticks) * u64::from(request.units),
                        bundle,
                        waiting: false,
                    });
                }
                Ok(Block { rows, bundles })
            })
            .collect::<Result<_, _>>()?;
        let needs: HashMap<(u64, u32), u128> = blocks
            .par_iter()
            .map(|block| {
                let mut needs = HashMap::new();
                for row in &block.rows {
                    for item in &block.bundles[row.bundle as usize].reserved {
                        *needs.entry((row.region, item.good)).or_insert(0) +=
                            u128::from(item.quantity);
                    }
                }
                needs
            })
            .reduce(HashMap::new, |mut left, right| {
                for (key, value) in right {
                    *left.entry(key).or_insert(0) += value;
                }
                left
            });
        for (&(region, good), &amount) in &needs {
            let stock = inventory_index
                .get(&region)
                .and_then(|inventory| inventory.stocks.iter().find(|stock| stock.good == good))
                .ok_or("missing input stock")?;
            if u128::from(stock.quantity) < amount {
                return Err("insufficient input stock".into());
            }
        }
        Ok(StartDraft { blocks, needs })
    }
}
