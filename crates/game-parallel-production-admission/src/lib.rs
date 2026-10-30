//! Batch reservation preparation for production.
use game_parallel_production_model::{
    Amount, Inventory, Job, JobPhase, PersonSnapshot, Recipe, Start, person_index,
};
use rayon::prelude::*;
use std::collections::HashMap;
pub struct Context<'a> {
    pub jobs: &'a [Job],
    pub recipes: &'a [Recipe],
    pub inventories: &'a [Inventory],
    pub people: &'a [PersonSnapshot],
}
pub struct Draft {
    jobs: Vec<Job>,
    needs: HashMap<(u64, u32), u128>,
}
impl Draft {
    /// Commit against the same snapshot or one with only additional stock.
    pub fn commit(self, jobs: &mut Vec<Job>, inventories: &mut [Inventory]) {
        inventories.par_iter_mut().for_each(|inventory| {
            for stock in &mut inventory.stocks {
                if let Some(amount) = self.needs.get(&(inventory.region, stock.good)) {
                    stock.quantity -= *amount as u64;
                }
            }
        });
        if jobs.is_empty() {
            *jobs = self.jobs;
        } else {
            jobs.extend(self.jobs);
        }
        if jobs.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            jobs.par_sort_unstable_by_key(|job| job.id);
        }
    }
}
impl Context<'_> {
    pub fn prepare(&self, requests: &[Start]) -> Result<Draft, String> {
        let mut ids: Vec<_> = self
            .jobs
            .par_iter()
            .map(|j| j.id)
            .chain(requests.par_iter().map(|r| r.id))
            .collect();
        if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
            ids.par_sort_unstable();
        }
        let mut busy: Vec<_> = self
            .jobs
            .par_iter()
            .filter(|j| j.phase == JobPhase::Working)
            .map(|j| j.worker)
            .chain(requests.par_iter().map(|r| r.worker))
            .collect();
        if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
            busy.par_sort_unstable();
        }
        if ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || busy.par_windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err("duplicate job or busy worker".into());
        }
        let recipes: HashMap<_, _> = self
            .recipes
            .iter()
            .map(|recipe| (recipe.id, recipe))
            .collect();
        let inventories: HashMap<_, _> = self
            .inventories
            .iter()
            .map(|inventory| (inventory.region, inventory))
            .collect();
        let jobs: Vec<Job> = requests
            .par_iter()
            .map(|request| -> Result<Job, String> {
                if request.units == 0 {
                    return Err("zero units".into());
                }
                let worker =
                    person_index(self.people, request.worker).map(|index| self.people[index]);
                if !worker.is_some_and(|(_, region, health, _)| {
                    region == Some(request.region) && health > 0
                }) {
                    return Err("worker must be living and local".into());
                }
                let recipe = recipes.get(&request.recipe).ok_or("unknown recipe")?;
                let inventory = inventories
                    .get(&request.region)
                    .ok_or("missing inventory")?;
                let convert =
                    |items: &[Amount]| -> Result<smallvec::SmallVec<[Amount; 1]>, String> {
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
                let reserved = convert(&recipe.inputs)?;
                let outputs = convert(&recipe.outputs)?;
                if outputs.iter().any(|o| {
                    !inventory
                        .stocks
                        .iter()
                        .any(|s| s.good == o.good && o.quantity <= s.capacity)
                }) {
                    return Err("output has no adequate storage".into());
                }
                Ok(Job {
                    id: request.id,
                    worker: request.worker,
                    region: request.region,
                    recipe: request.recipe,
                    remaining: u64::from(recipe.ticks) * u64::from(request.units),
                    phase: JobPhase::Working,
                    reserved,
                    outputs,
                })
            })
            .collect::<Result<_, _>>()?;
        let needs: HashMap<(u64, u32), u128> = jobs
            .par_iter()
            .fold(HashMap::new, |mut totals, job| {
                for item in &job.reserved {
                    *totals.entry((job.region, item.good)).or_default() +=
                        u128::from(item.quantity);
                }
                totals
            })
            .reduce(HashMap::new, |mut left, right| {
                for (key, amount) in right {
                    *left.entry(key).or_default() += amount;
                }
                left
            });
        for (&(region, good), &amount) in &needs {
            let stock = inventories
                .get(&region)
                .and_then(|i| i.stocks.iter().find(|s| s.good == good))
                .ok_or("missing input stock")?;
            if u128::from(stock.quantity) < amount {
                return Err("insufficient input stock".into());
            }
        }
        Ok(Draft { jobs, needs })
    }
}
