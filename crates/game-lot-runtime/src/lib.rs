//! Dated physical production; callers own character decisions and observation.
mod advance;
mod identified_output;
mod receipts;
pub use advance::{AdvanceDraft, Events};
use game_lot_content::Catalog;
pub use game_lot_content::{Growth, WritingPage};
pub use game_lot_work::{Phase, Snapshot, Work};
use game_lots::{Outcome, Place, Spoilage, Store, Transaction, TransactionOutcome};
use game_production_model::{Good, JobPhase, Recipe, Start};
pub use identified_output::{IdentifiedProduction, ProducedItem};
use rayon::prelude::*;
pub use receipts::{
    ExternalReceipt, ExternalReceiptBatch, ExternalReceiptHeader, ExternalReceiptRef,
};
use std::collections::HashSet;

pub struct PhysicalOutcomes {
    pub transactions: Vec<TransactionOutcome>,
    pub movements: Vec<Outcome>,
    pub spoiled: Vec<Spoilage>,
}
pub struct Runtime {
    store: Store,
    catalog: Catalog,
    goods: Vec<Good>,
    recipes: Vec<Recipe>,
    tick_millis: u64,
    work: Vec<Work>,
    epoch: u64,
}
impl Runtime {
    pub fn compile(mut state: Snapshot) -> Result<Self, String> {
        let catalog = Catalog::from_tick_millis(&state.goods, state.tick_millis)?;
        state.work.par_sort_unstable_by_key(|row| row.job.id);
        game_lot_work::validate(&state, &catalog)?;
        let store = Store::compile(state.custody)?;
        let mut workers: Vec<_> = state
            .work
            .par_iter()
            .filter(|row| row.phase == Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        workers.par_sort_unstable();
        let recipes: HashSet<_> = state.recipes.iter().map(|row| row.id).collect();
        if recipes.len() != state.recipes.len()
            || state
                .work
                .par_windows(2)
                .any(|pair| pair[0].job.id == pair[1].job.id)
            || workers.par_windows(2).any(|pair| pair[0] == pair[1])
            || state.work.par_iter().any(|row| {
                let job = &row.job;
                job.id == 0
                    || job.worker == 0
                    || job.region == 0
                    || !recipes.contains(&job.recipe)
                    || match row.phase {
                        Phase::Working | Phase::Returning => {
                            job.phase != JobPhase::Working
                                || store.container(Place::JobInput(job.id)).is_none()
                                || store.container(Place::JobOutput(job.id)).is_none()
                        }
                        Phase::Waiting => {
                            job.phase != JobPhase::Waiting
                                || store.container(Place::JobOutput(job.id)).is_none()
                        }
                    }
            })
        {
            return Err("invalid physical production state".into());
        }
        Ok(Self {
            store,
            catalog,
            goods: state.goods,
            recipes: state.recipes,
            tick_millis: state.tick_millis,
            work: state.work,
            epoch: 0,
        })
    }
    pub fn tick(&self) -> u64 {
        self.store.tick()
    }
    /// Current custody of a unique physical item.
    pub fn item_place(&self, id: u64) -> Option<Place> {
        self.store.item_place(id)
    }
    pub fn highest_item(&self) -> Option<u64> {
        self.store.highest_item()
    }
    pub fn next_unused_item(&self, start: u64) -> Option<u64> {
        self.store.next_unused_item(start)
    }
    pub fn next_unused_items(&self, start: u64, count: usize) -> Option<Vec<u64>> {
        self.store.next_unused_items(start, count)
    }
    pub fn tick_millis(&self) -> u64 {
        self.tick_millis
    }
    pub fn goods(&self) -> &[Good] {
        &self.goods
    }
    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            custody: self.store.export(),
            goods: self.goods.clone(),
            recipes: self.recipes.clone(),
            tick_millis: self.tick_millis,
            work: self.work.clone(),
        }
    }
    /// Register real people as independent physical holders.
    pub fn add_people(&mut self, people: &[(u64, u64)]) -> Result<(), String> {
        let containers = people
            .par_iter()
            .map(|&(person, capacity)| {
                game_lots::Container::new(Place::Person(person), capacity, Vec::new())
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.store.add_empty(containers)
    }
    pub fn view(&self, boundary: u64) -> Result<game_lots::View, String> {
        self.store.view(boundary)
    }
    pub fn stock_view(&self, boundary: u64) -> Result<game_lots::View, String> {
        self.store.stock_view(boundary)
    }
    pub fn custody(&self, place: Place) -> Option<&game_lots::Container> {
        self.store.container(place)
    }
    /// Possessions remain in custody until physically removed.
    pub fn retire_person(&mut self, person: u64) -> Result<(), String> {
        self.store.remove_empty(vec![Place::Person(person)])
    }
    pub fn work(&self) -> &[Work] {
        &self.work
    }
    /// Reserve physical materials at the current time, after the caller's actor checks.
    pub fn start(&mut self, requests: &[Start], seed: u64) -> Result<Vec<(u64, bool)>, String> {
        game_lot_work::validate_starts(&self.work, requests)?;
        let plan = game_lot_work::start(requests, &self.recipes)?;
        let mut draft =
            self.store
                .prepare_with_empty(seed, &plan.transactions, plan.containers, false)?;
        let result: Vec<_> = draft
            .transactions()
            .iter()
            .map(|row| (row.id, row.accepted))
            .collect();
        let retire: Vec<_> = plan
            .jobs
            .par_iter()
            .filter(|job| {
                !result
                    .binary_search_by_key(&job.id, |row| row.0)
                    .is_ok_and(|index| result[index].1)
            })
            .flat_map_iter(|job| [Place::JobInput(job.id), Place::JobOutput(job.id)])
            .collect();
        let accepted: Vec<_> = plan
            .jobs
            .into_par_iter()
            .filter(|job| {
                result
                    .binary_search_by_key(&job.id, |row| row.0)
                    .is_ok_and(|index| result[index].1)
            })
            .map(|job| Work {
                job,
                phase: Phase::Working,
            })
            .collect();
        self.store.retire_in(&mut draft, retire)?;
        self.store.validate_commit(&draft)?;
        self.store.commit(draft)?;
        self.work.extend(accepted);
        self.work.par_sort_unstable_by_key(|row| row.job.id);
        Ok(result)
    }
    /// Apply physical exchanges, gifts or consumption now. Social decisions remain
    /// with their characters; this owner checks custody and capacity only.
    pub fn actions_now(
        &mut self,
        actions: &[Transaction],
        seed: u64,
    ) -> Result<PhysicalOutcomes, String> {
        game_lot_work::validate_actions(&self.catalog, actions)?;
        let draft = self.store.prepare_transactions_now(seed, actions)?;
        let decisions = draft.transactions().to_vec();
        self.store.validate_commit(&draft)?;
        let (outcomes, spoiled) = self.store.commit(draft)?;
        Ok(PhysicalOutcomes {
            transactions: decisions,
            movements: outcomes,
            spoiled,
        })
    }
    /// A worker may stop without requiring a successful materials return.
    pub fn abandon(&mut self, job: u64, worker: u64) -> Result<(), String> {
        let index = self
            .work
            .binary_search_by_key(&job, |row| row.job.id)
            .map_err(|_| "unknown job")?;
        let row = &mut self.work[index];
        if row.phase != Phase::Working || row.job.worker != worker {
            return Err("worker is not doing this job".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        row.phase = Phase::Returning;
        self.epoch = epoch;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
