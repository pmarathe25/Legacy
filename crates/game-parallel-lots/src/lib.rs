//! Person- and location-owned physical lots with staged expiry and movement.
mod compose;
mod view;
pub use compose::{Composed, compose};
pub use game_parallel_lot_model::{
    Container, Creation, Lot, Movement, Outcome, Place, Spoilage, State, Transaction,
    TransactionOutcome, deadline,
};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
pub use view::{Balance, ContainerView, View};

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

fn instance() -> Result<u64, String> {
    NEXT_INSTANCE
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| "lot store instance overflow".into())
}

pub struct Store {
    state: State,
    instance: u64,
}
/// Tentative new custody is removed automatically unless its boundary commits.
pub struct Stage<'a> {
    store: &'a mut Store,
    added: Vec<Place>,
    revision: u64,
    committed: bool,
}
impl Stage<'_> {
    pub fn prepare_transactions(
        &self,
        seed: u64,
        actions: &[Transaction],
    ) -> Result<Draft, String> {
        self.store.prepare_transactions(seed, actions)
    }
    pub fn prepare_transactions_now(
        &self,
        seed: u64,
        actions: &[Transaction],
    ) -> Result<Draft, String> {
        self.store.prepare_transactions_now(seed, actions)
    }
    pub fn commit(mut self, draft: Draft) -> Result<(Vec<Outcome>, Vec<Spoilage>), String> {
        let result = self.store.commit(draft)?;
        self.committed = true;
        Ok(result)
    }
}
impl Drop for Stage<'_> {
    fn drop(&mut self) {
        if self.committed || self.added.is_empty() {
            return;
        }
        self.store.state.containers = std::mem::take(&mut self.store.state.containers)
            .into_par_iter()
            .filter(|row| self.added.binary_search(&row.place).is_err())
            .collect();
        self.store.state.revision = self.revision;
    }
}
/// An unpublished snapshot; its physical changes cannot be altered by callers.
pub struct Draft {
    inner: game_parallel_lot_planning::Draft,
    additions: Vec<Container>,
    retire: Vec<Place>,
}
impl Draft {
    pub fn outcomes(&self) -> &[Outcome] {
        &self.inner.outcomes
    }
    pub fn spoiled(&self) -> &[Spoilage] {
        &self.inner.spoiled
    }
    pub fn creations(&self) -> &[(u64, bool)] {
        &self.inner.creations
    }
    pub fn transactions(&self) -> &[TransactionOutcome] {
        &self.inner.transactions
    }
}
impl Store {
    /// Validate a saved state once; subsequent mutations go through staged drafts.
    pub fn compile(state: State) -> Result<Self, String> {
        if state
            .containers
            .par_windows(2)
            .any(|pair| pair[0].place >= pair[1].place)
            || state.containers.par_iter().any(|container| {
                container.validate().is_err()
                    || container.next_expiry.is_some_and(|at| at <= state.tick)
            })
        {
            return Err("invalid lot state".into());
        }
        let instance = instance()?;
        Ok(Self { state, instance })
    }
    /// Stage new empty custody with the same commit/rollback boundary as actions.
    pub fn stage(&mut self, containers: Vec<Container>) -> Result<Stage<'_>, String> {
        let mut added: Vec<_> = containers.par_iter().map(|row| row.place).collect();
        added.par_sort_unstable();
        let revision = self.state.revision;
        // A discarded stage must never make an escaped draft valid again.
        let next_instance = if added.is_empty() {
            self.instance
        } else {
            instance()?
        };
        self.add_empty(containers)?;
        self.instance = next_instance;
        Ok(Stage {
            store: self,
            added,
            revision,
            committed: false,
        })
    }
    /// Register new, empty custody before staging actions for newborns or jobs.
    /// Existing drafts become stale. Duplicate or nonempty entries fail atomically.
    pub fn add_empty(&mut self, mut containers: Vec<Container>) -> Result<(), String> {
        if containers.is_empty() {
            return Ok(());
        }
        containers.par_sort_unstable_by_key(|row| row.place);
        if containers
            .par_windows(2)
            .any(|pair| pair[0].place == pair[1].place)
            || containers.par_iter().any(|row| {
                row.validate().is_err()
                    || row.used != 0
                    || self
                        .state
                        .containers
                        .binary_search_by_key(&row.place, |old| old.place)
                        .is_ok()
            })
        {
            return Err("invalid or existing new custody".into());
        }
        let revision = self
            .state
            .revision
            .checked_add(1)
            .ok_or("lot revision overflow")?;
        self.state.containers.append(&mut containers);
        self.state
            .containers
            .par_sort_unstable_by_key(|row| row.place);
        self.state.revision = revision;
        Ok(())
    }
    /// Retire empty custody after a job or person no longer needs it.
    pub fn remove_empty(&mut self, mut places: Vec<Place>) -> Result<(), String> {
        if places.is_empty() {
            return Ok(());
        }
        places.par_sort_unstable();
        if places.par_windows(2).any(|pair| pair[0] == pair[1])
            || places.par_iter().any(|place| {
                self.state
                    .containers
                    .binary_search_by_key(place, |row| row.place)
                    .map_or(true, |index| self.state.containers[index].used != 0)
            })
        {
            return Err("missing, duplicate, or nonempty custody".into());
        }
        let revision = self
            .state
            .revision
            .checked_add(1)
            .ok_or("lot revision overflow")?;
        self.state.containers = std::mem::take(&mut self.state.containers)
            .into_par_iter()
            .filter(|row| places.binary_search(&row.place).is_err())
            .collect();
        self.state.revision = revision;
        Ok(())
    }
    /// Build one immutable aggregate view for decision owners at this or the next boundary.
    pub fn view(&self, boundary: u64) -> Result<View, String> {
        view::prepare(&self.state, boundary)
    }
    pub fn tick(&self) -> u64 {
        self.state.tick
    }
    pub fn container(&self, place: Place) -> Option<&Container> {
        self.state
            .containers
            .binary_search_by_key(&place, |row| row.place)
            .ok()
            .map(|index| &self.state.containers[index])
    }
    pub fn export(&self) -> State {
        self.state.clone()
    }
    pub fn prepare(&self, seed: u64, moves: &[Movement]) -> Result<Draft, String> {
        game_parallel_lot_planning::prepare(
            &self.state,
            self.instance,
            seed,
            moves,
            &[],
            None,
            true,
        )
        .map(|inner| Draft {
            inner,
            additions: Vec::new(),
            retire: Vec::new(),
        })
    }
    /// Stage immediate actions without advancing simulation time or refreshing goods.
    pub fn prepare_now(&self, seed: u64, moves: &[Movement]) -> Result<Draft, String> {
        game_parallel_lot_planning::prepare(
            &self.state,
            self.instance,
            seed,
            moves,
            &[],
            None,
            false,
        )
        .map(|inner| Draft {
            inner,
            additions: Vec::new(),
            retire: Vec::new(),
        })
    }
    /// Stage all legs of each physical action together. Contention is resolved
    /// conservatively: a failed leg rejects the entire action for this boundary.
    pub fn prepare_transactions(
        &self,
        seed: u64,
        transactions: &[Transaction],
    ) -> Result<Draft, String> {
        self.prepare_batch(seed, transactions, true, Vec::new())
    }
    pub fn prepare_transactions_now(
        &self,
        seed: u64,
        transactions: &[Transaction],
    ) -> Result<Draft, String> {
        self.prepare_batch(seed, transactions, false, Vec::new())
    }
    /// Prepare new empty custody and actions without mutating the store.
    pub fn prepare_with_empty(
        &self,
        seed: u64,
        transactions: &[Transaction],
        containers: Vec<Container>,
        advance: bool,
    ) -> Result<Draft, String> {
        self.prepare_batch(seed, transactions, advance, containers)
    }
    /// Suppress lot detail for selected consuming transactions; acceptance is retained.
    pub fn prepare_with_empty_compact(
        &self,
        seed: u64,
        transactions: &[Transaction],
        containers: Vec<Container>,
        compact: &[u64],
        advance: bool,
    ) -> Result<Draft, String> {
        let (inner, additions) = game_parallel_lot_batches::prepare(
            &self.state,
            self.instance,
            seed,
            transactions,
            advance,
            containers,
            compact,
        )?;
        Ok(Draft {
            inner,
            additions,
            retire: Vec::new(),
        })
    }
    fn prepare_batch(
        &self,
        seed: u64,
        transactions: &[Transaction],
        advance: bool,
        additions: Vec<Container>,
    ) -> Result<Draft, String> {
        let (inner, additions) = game_parallel_lot_batches::prepare(
            &self.state,
            self.instance,
            seed,
            transactions,
            advance,
            additions,
            &[],
        )?;
        Ok(Draft {
            inner,
            additions,
            retire: Vec::new(),
        })
    }
    /// Check this owner before publishing any part of a composed generation.
    pub fn validate_commit(&self, draft: &Draft) -> Result<(), String> {
        let inner = &draft.inner;
        if self.instance != inner.base_instance
            || self.state.tick != inner.base_tick
            || self.state.revision != inner.base_revision
            || (inner.next_tick != self.state.tick
                && self.state.tick.checked_add(1) != Some(inner.next_tick))
        {
            return Err("stale lot draft".into());
        }
        self.state
            .revision
            .checked_add(1)
            .ok_or("lot revision overflow")?;
        Ok(())
    }
    pub fn add_empty_in(
        &self,
        draft: &mut Draft,
        containers: Vec<Container>,
    ) -> Result<(), String> {
        self.validate_commit(draft)?;
        game_parallel_lot_allocation::additions::add(
            &self.state,
            &mut draft.additions,
            &mut draft.inner.changes,
            containers,
        )
    }
    /// Retire containers only if their post-transaction state will be empty.
    pub fn retire_in(&self, draft: &mut Draft, mut places: Vec<Place>) -> Result<(), String> {
        self.validate_commit(draft)?;
        places.par_sort_unstable();
        if places.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate retired custody".into());
        }
        let mut containers: Vec<_> = self
            .state
            .containers
            .par_iter()
            .chain(draft.additions.par_iter())
            .collect();
        if !draft.additions.is_empty() {
            containers.par_sort_unstable_by_key(|row| row.place);
        }
        if places.par_iter().any(|place| {
            let Ok(index) = containers.binary_search_by_key(place, |row| row.place) else {
                return true;
            };
            draft
                .inner
                .changes
                .binary_search_by_key(&index, |row| row.0)
                .map_or(containers[index].used != 0, |i| {
                    draft.inner.changes[i].1.used != 0
                })
        }) {
            return Err("missing or nonempty retired custody".into());
        }
        draft.retire = places;
        Ok(())
    }
    pub fn commit(&mut self, draft: Draft) -> Result<(Vec<Outcome>, Vec<Spoilage>), String> {
        self.validate_commit(&draft)?;
        let Draft {
            inner: draft,
            mut additions,
            retire,
        } = draft;
        let revision = self
            .state
            .revision
            .checked_add(1)
            .ok_or("lot revision overflow")?;
        if !additions.is_empty() {
            self.state.containers.append(&mut additions);
            self.state
                .containers
                .par_sort_unstable_by_key(|row| row.place);
        }
        let mut changes = draft.changes;
        publish(&mut self.state.containers, &mut changes, 0);
        changes.into_par_iter().for_each(drop);
        if !retire.is_empty() {
            self.state.containers = std::mem::take(&mut self.state.containers)
                .into_par_iter()
                .filter(|row| retire.binary_search(&row.place).is_err())
                .collect();
        }
        self.state.tick = draft.next_tick;
        self.state.revision = revision;
        Ok((draft.outcomes, draft.spoiled))
    }
}

fn publish(containers: &mut [Container], changes: &mut [(usize, Container)], base: usize) {
    if changes.len() <= 128 {
        for (index, updated) in changes {
            std::mem::swap(&mut containers[*index - base], updated);
        }
        return;
    }
    let middle = changes.len() / 2;
    let split = changes[middle].0;
    let (left, right) = containers.split_at_mut(split - base);
    let (left_changes, right_changes) = changes.split_at_mut(middle);
    rayon::join(
        || publish(left, left_changes, base),
        || publish(right, right_changes, split),
    );
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
