//! Person- and location-owned physical lots with staged expiry and movement.
mod compose;
mod view;
pub use compose::{Composed, compose};
pub use game_lot_model::{
    Container, Creation, Lot, LotList, Movement, Outcome, OutcomeBatch, OutcomeHeader, OutcomeRef,
    Place, PlaceIndex, SimpleTransfer, Spoilage, State, Transaction, TransactionOutcome, deadline,
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
    items: Vec<(u64, Place)>,
    item_places: Vec<Place>,
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
    inner: game_lot_planning::Draft,
    additions: Vec<Container>,
    retire: Vec<Place>,
    item_updates: Vec<(u64, Option<Place>)>,
}
impl Draft {
    /// Staged item custody updates; `None` means the item was consumed.
    /// This remains available even when ordinary movement receipts are drained.
    pub fn item_updates(&self) -> &[(u64, Option<Place>)] {
        &self.item_updates
    }
    pub fn outcomes(&self) -> Vec<Outcome> {
        self.inner
            .outcomes
            .par_iter()
            .map(OutcomeRef::to_owned)
            .collect()
    }
    pub fn spoiled(&self) -> &[Spoilage] {
        &self.inner.spoiled
    }
    /// Move receipt data to a detached owner. The draft still commits the same
    /// custody changes, but its eventual commit returns no drained receipts.
    pub fn take_receipts(&mut self) -> (Vec<Outcome>, Vec<Spoilage>) {
        (
            std::mem::take(&mut self.inner.outcomes).into_owned(),
            std::mem::take(&mut self.inner.spoiled),
        )
    }
    /// Move the packed receipts without materializing individually owned lots.
    pub fn take_outcome_batch(&mut self) -> (OutcomeBatch, Vec<Spoilage>) {
        (
            std::mem::take(&mut self.inner.outcomes),
            std::mem::take(&mut self.inner.spoiled),
        )
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
        let mut items: Vec<_> = state
            .containers
            .par_iter()
            .flat_map_iter(|container| {
                container
                    .lots
                    .iter()
                    .filter_map(|lot| lot.item.map(|id| (id, container.place)))
            })
            .collect();
        items.par_sort_unstable_by_key(|row| row.0);
        if items.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("duplicate physical item".into());
        }
        let instance = instance()?;
        let item_places = item_places(&items);
        Ok(Self {
            state,
            instance,
            items,
            item_places,
        })
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
    /// Read only regional stock without materializing personal or job custody.
    pub fn stock_view(&self, boundary: u64) -> Result<View, String> {
        view::prepare_stocks(&self.state, boundary)
    }
    pub fn tick(&self) -> u64 {
        self.state.tick
    }
    /// Generation of committed custody changes.
    pub fn revision(&self) -> u64 {
        self.state.revision
    }
    /// Borrow the ordered committed custody without copying it.
    pub fn containers(&self) -> &[Container] {
        &self.state.containers
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
    /// Current physical custody of an identified item, if it exists.
    pub fn item_place(&self, id: u64) -> Option<Place> {
        let direct = self
            .items
            .first()
            .and_then(|row| id.checked_sub(row.0))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&index| self.items.get(index).is_some_and(|row| row.0 == id));
        direct
            .or_else(|| self.items.binary_search_by_key(&id, |row| row.0).ok())
            .map(|i| self.items[i].1)
    }
    /// Highest currently held unique item identity, if any.
    pub fn highest_item(&self) -> Option<u64> {
        self.items.last().map(|row| row.0)
    }
    /// Find the first unused item identity at or above `start`.
    pub fn next_unused_item(&self, start: u64) -> Option<u64> {
        if start == 0 {
            return None;
        }
        let base = self.items.partition_point(|row| row.0 < start);
        let mut low = 0usize;
        let mut high = self.items.len() - base;
        while low < high {
            let middle = low + (high - low) / 2;
            let expected = start.checked_add(u64::try_from(middle).ok()?);
            if expected == Some(self.items[base + middle].0) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        start.checked_add(u64::try_from(low).ok()?)
    }
    /// Allocate distinct unused identities in ascending order without mutating custody.
    pub fn next_unused_items(&self, start: u64, count: usize) -> Option<Vec<u64>> {
        if start == 0 {
            return None;
        }
        let base = self.items.partition_point(|row| row.0 < start);
        (0..count)
            .into_par_iter()
            .map(|rank| {
                let rank = u64::try_from(rank).ok()?;
                let mut low = 0usize;
                let mut high = self.items.len() - base;
                while low < high {
                    let middle = low + (high - low) / 2;
                    let occupied_before = u64::try_from(middle).ok()?;
                    let gaps = self.items[base + middle].0 - start - occupied_before;
                    if gaps <= rank {
                        low = middle + 1;
                    } else {
                        high = middle;
                    }
                }
                start
                    .checked_add(rank)?
                    .checked_add(u64::try_from(low).ok()?)
            })
            .collect()
    }
    fn touches_item(&self, movement: &Movement) -> bool {
        self.item_places.binary_search(&movement.from).is_ok()
            || movement
                .to
                .is_some_and(|place| self.item_places.binary_search(&place).is_ok())
    }
    fn item_draft(
        &self,
        inner: game_lot_planning::Draft,
        additions: Vec<Container>,
        check_items: bool,
    ) -> Result<Draft, String> {
        let mut item_updates = Vec::new();
        if check_items {
            let mut containers: Vec<_> = self
                .state
                .containers
                .par_iter()
                .chain(additions.par_iter())
                .collect();
            if !additions.is_empty() {
                containers.par_sort_unstable_by_key(|row| row.place);
            }
            let changes: Vec<_> = match &inner.changes {
                game_lot_planning::Changes::Empty => Vec::new(),
                game_lot_planning::Changes::Flat(rows) => rows.par_iter().collect(),
                game_lot_planning::Changes::Runs(runs) => {
                    runs.par_iter().flat_map_iter(|run| run.iter()).collect()
                }
            };
            if changes.par_iter().any(|(index, row)| {
                containers
                    .get(*index)
                    .is_none_or(|old| old.place != row.place)
            }) {
                return Err("invalid item custody change".into());
            }
            let mut removed: Vec<_> = changes
                .par_iter()
                .filter(|(index, _)| containers[*index].lots.len() < 2048)
                .flat_map_iter(|(index, _)| {
                    containers[*index].lots.iter().filter_map(|lot| lot.item)
                })
                .chain(
                    changes
                        .par_iter()
                        .filter(|(index, _)| containers[*index].lots.len() >= 2048)
                        .flat_map(|(index, _)| {
                            containers[*index]
                                .lots
                                .par_iter()
                                .filter_map(|lot| lot.item)
                        }),
                )
                .collect();
            let mut added: Vec<_> = changes
                .par_iter()
                .filter(|(_, row)| row.lots.len() < 2048)
                .flat_map_iter(|(_, row)| {
                    row.lots
                        .iter()
                        .filter_map(|lot| lot.item.map(|id| (id, row.place)))
                })
                .chain(
                    changes
                        .par_iter()
                        .filter(|(_, row)| row.lots.len() >= 2048)
                        .flat_map(|(_, row)| {
                            row.lots
                                .par_iter()
                                .filter_map(|lot| lot.item.map(|id| (id, row.place)))
                        }),
                )
                .collect();
            rayon::join(
                || removed.par_sort_unstable(),
                || added.par_sort_unstable_by_key(|row| row.0),
            );
            if added.par_windows(2).any(|pair| pair[0].0 == pair[1].0)
                || added.par_iter().any(|(id, _)| {
                    self.items.binary_search_by_key(id, |row| row.0).is_ok()
                        && removed.binary_search(id).is_err()
                })
            {
                return Err("duplicate physical item".into());
            }
            item_updates = removed
                .par_iter()
                .filter_map(|&id| {
                    let next = added
                        .binary_search_by_key(&id, |row| row.0)
                        .ok()
                        .map(|i| added[i].1);
                    let previous = self
                        .items
                        .binary_search_by_key(&id, |row| row.0)
                        .ok()
                        .map(|i| self.items[i].1);
                    (next != previous).then_some((id, next))
                })
                .chain(
                    added
                        .par_iter()
                        .filter(|(id, _)| removed.binary_search(id).is_err())
                        .map(|&(id, place)| (id, Some(place))),
                )
                .collect();
            item_updates.par_sort_unstable_by_key(|row| row.0);
        }
        Ok(Draft {
            inner,
            additions,
            retire: Vec::new(),
            item_updates,
        })
    }
    fn check_creations(&self, transactions: &[Transaction]) -> Result<bool, String> {
        let large = transactions.len() < 64
            && transactions
                .par_iter()
                .any(|row| row.creations.len() >= 2048);
        let mut ids: Vec<_> = if large {
            transactions
                .par_iter()
                .flat_map(|row| row.creations.par_iter())
                .filter_map(|creation| creation.lot.item.map(|id| (id, &creation.lot)))
                .collect()
        } else {
            transactions
                .par_iter()
                .flat_map_iter(|row| &row.creations)
                .filter_map(|creation| creation.lot.item.map(|id| (id, &creation.lot)))
                .collect()
        };
        if ids.par_iter().any(|(id, lot)| {
            *id == 0
                || lot.quantity != 1
                || lot.expires_at.is_some()
                || self.items.binary_search_by_key(id, |row| row.0).is_ok()
        }) {
            return Err("invalid physical item creation".into());
        }
        ids.par_sort_unstable_by_key(|(id, _)| *id);
        if ids.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("duplicate physical item creation".into());
        }
        Ok(!ids.is_empty()
            || transactions
                .par_iter()
                .any(|row| row.movements.iter().any(|movement| movement.item.is_some()))
            || (!self.item_places.is_empty()
                && transactions.par_iter().any(|row| {
                    row.movements
                        .iter()
                        .any(|movement| self.touches_item(movement))
                        || row
                            .creations
                            .iter()
                            .any(|creation| self.item_places.binary_search(&creation.to).is_ok())
                })))
    }
    pub fn prepare(&self, seed: u64, moves: &[Movement]) -> Result<Draft, String> {
        let inner =
            game_lot_planning::prepare(&self.state, self.instance, seed, moves, &[], None, true)?;
        let touches = !self.item_places.is_empty()
            && moves.par_iter().any(|movement| self.touches_item(movement));
        self.item_draft(inner, Vec::new(), touches)
    }
    /// Stage immediate actions without advancing simulation time or refreshing goods.
    pub fn prepare_now(&self, seed: u64, moves: &[Movement]) -> Result<Draft, String> {
        let inner =
            game_lot_planning::prepare(&self.state, self.instance, seed, moves, &[], None, false)?;
        let touches = !self.item_places.is_empty()
            && moves.par_iter().any(|movement| self.touches_item(movement));
        self.item_draft(inner, Vec::new(), touches)
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
        let check_items = self.check_creations(transactions)?;
        if check_items
            && containers.is_empty()
            && compact.is_empty()
            && let Some(inner) = game_lot_batches::prepare_local_creations(
                &self.state,
                self.instance,
                transactions,
                advance,
            )?
        {
            return self.item_draft(inner, containers, true);
        }
        if check_items
            && containers.is_empty()
            && compact.is_empty()
            && let Some(inner) = game_lot_batches::prepare_independent_items(
                &self.state,
                self.instance,
                transactions,
                advance,
            )?
        {
            return self.item_draft(inner, containers, true);
        }
        let prepare = if check_items {
            game_lot_batches::prepare_unpartitioned_validated
        } else {
            game_lot_batches::prepare_fungible_validated
        };
        let (inner, additions) = prepare(
            &self.state,
            self.instance,
            seed,
            transactions,
            advance,
            containers,
            compact,
        )?;
        self.item_draft(inner, additions, check_items)
    }
    fn prepare_batch(
        &self,
        seed: u64,
        transactions: &[Transaction],
        advance: bool,
        additions: Vec<Container>,
    ) -> Result<Draft, String> {
        let check_items = self.check_creations(transactions)?;
        if !check_items
            && !advance
            && additions.is_empty()
            && let Some(inner) = game_lot_batches::prepare_disjoint_transformations(
                &self.state,
                self.instance,
                transactions,
            )?
        {
            return self.item_draft(inner, additions, false);
        }
        if check_items
            && additions.is_empty()
            && let Some(inner) = game_lot_batches::prepare_local_creations(
                &self.state,
                self.instance,
                transactions,
                advance,
            )?
        {
            return self.item_draft(inner, additions, true);
        }
        if check_items
            && additions.is_empty()
            && let Some(inner) = game_lot_batches::prepare_independent_items(
                &self.state,
                self.instance,
                transactions,
                advance,
            )?
        {
            return self.item_draft(inner, additions, true);
        }
        let prepare = if check_items {
            game_lot_batches::prepare_unpartitioned_validated
        } else {
            game_lot_batches::prepare_fungible_validated
        };
        let (inner, additions) = prepare(
            &self.state,
            self.instance,
            seed,
            transactions,
            advance,
            additions,
            &[],
        )?;
        self.item_draft(inner, additions, check_items)
    }
    /// Attempt a disjoint flat transfer batch. A caller must use the general
    /// arbiter when this returns `None`, including for contested endpoints.
    pub fn prepare_flat_transfers(
        &self,
        transfers: &[SimpleTransfer],
        advance: bool,
    ) -> Result<Option<Draft>, String> {
        if transfers.par_iter().any(|row| row.movement.item.is_some())
            || (!self.item_places.is_empty()
                && transfers
                    .par_iter()
                    .any(|row| self.touches_item(&row.movement)))
        {
            return Ok(None);
        }
        Ok(game_lot_batches::prepare_flat_transfers_fungible(
            &self.state,
            self.instance,
            transfers,
            advance,
        )?
        .map(|inner| Draft {
            inner,
            additions: Vec::new(),
            retire: Vec::new(),
            item_updates: Vec::new(),
        }))
    }
    /// Attempt a flat batch with exclusive and contested transfer partitions.
    /// Returns `None` when the complete batch needs the general arbiter.
    pub fn prepare_partitioned_flat(
        &self,
        seed: u64,
        transfers: &[SimpleTransfer],
        advance: bool,
    ) -> Result<Option<Draft>, String> {
        if transfers.par_iter().any(|row| row.movement.item.is_some())
            || (!self.item_places.is_empty()
                && transfers
                    .par_iter()
                    .any(|row| self.touches_item(&row.movement)))
        {
            return Ok(None);
        }
        Ok(game_lot_batches::prepare_partitioned_flat_fungible(
            &self.state,
            self.instance,
            seed,
            transfers,
            advance,
        )?
        .map(|inner| Draft {
            inner,
            additions: Vec::new(),
            retire: Vec::new(),
            item_updates: Vec::new(),
        }))
    }
    /// Attempt a shared-source batch into newly staged empty custody.
    pub fn prepare_fan_out_flat_with_empty(
        &self,
        seed: u64,
        transfers: &[SimpleTransfer],
        additions: Vec<Container>,
        advance: bool,
    ) -> Result<Option<Draft>, String> {
        if transfers.par_iter().any(|row| row.movement.item.is_some())
            || (!self.item_places.is_empty()
                && transfers
                    .par_iter()
                    .any(|row| self.touches_item(&row.movement)))
        {
            return Ok(None);
        }
        Ok(game_lot_batches::prepare_fan_out_flat_with_empty_fungible(
            &self.state,
            self.instance,
            seed,
            transfers,
            additions,
            advance,
        )?
        .map(|(inner, additions)| Draft {
            inner,
            additions,
            retire: Vec::new(),
            item_updates: Vec::new(),
        }))
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
        if containers.is_empty() {
            return Ok(());
        }
        game_lot_allocation::additions::add(
            &self.state,
            &mut draft.additions,
            draft.inner.changes.flat_mut(),
            containers,
        )
    }
    /// Retire containers only if their post-transaction state will be empty.
    pub fn retire_in(&self, draft: &mut Draft, mut places: Vec<Place>) -> Result<(), String> {
        self.validate_commit(draft)?;
        if places.is_empty() && draft.retire.is_empty() {
            return Ok(());
        }
        places.extend_from_slice(&draft.retire);
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
        let changes = draft.inner.changes.flat_mut();
        if places.par_iter().any(|place| {
            let Ok(index) = containers.binary_search_by_key(place, |row| row.place) else {
                return true;
            };
            changes
                .binary_search_by_key(&index, |row| row.0)
                .map_or(containers[index].used != 0, |i| changes[i].1.used != 0)
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
            item_updates,
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
        match draft.changes {
            game_lot_planning::Changes::Empty => {}
            game_lot_planning::Changes::Flat(mut changes) => {
                publish(&mut self.state.containers, &mut changes, 0);
                changes.into_par_iter().for_each(drop);
            }
            game_lot_planning::Changes::Runs(mut runs) => {
                publish_runs(&mut self.state.containers, &mut runs, 0);
                runs.into_par_iter().flat_map_iter(|run| run).for_each(drop);
            }
        }
        if !retire.is_empty() {
            self.state.containers = std::mem::take(&mut self.state.containers)
                .into_par_iter()
                .filter(|row| retire.binary_search(&row.place).is_err())
                .collect();
        }
        self.state.tick = draft.next_tick;
        self.state.revision = revision;
        if !item_updates.is_empty() {
            self.items = merge_item_updates(&self.items, &item_updates);
            self.item_places = item_places(&self.items);
        }
        Ok((draft.outcomes.into_owned(), draft.spoiled))
    }
}

fn item_places(items: &[(u64, Place)]) -> Vec<Place> {
    let mut places: Vec<_> = items.par_iter().map(|row| row.1).collect();
    places.par_sort_unstable();
    places
        .par_chunk_by(|a, b| a == b)
        .map(|group| group[0])
        .collect()
}

fn merge_item_updates(
    items: &[(u64, Place)],
    updates: &[(u64, Option<Place>)],
) -> Vec<(u64, Place)> {
    if items.is_empty() {
        return updates
            .par_iter()
            .filter_map(|&(id, place)| place.map(|place| (id, place)))
            .collect();
    }
    const CHUNK: usize = 2048;
    items
        .par_chunks(CHUNK)
        .enumerate()
        .flat_map_iter(|(index, chunk)| {
            let start = if index == 0 {
                0
            } else {
                updates.partition_point(|row| row.0 < chunk[0].0)
            };
            let next = (index + 1) * CHUNK;
            let end = if next >= items.len() {
                updates.len()
            } else {
                updates.partition_point(|row| row.0 < items[next].0)
            };
            let mut output = Vec::with_capacity(chunk.len() + end - start);
            let mut cursor = start;
            for &(id, place) in chunk {
                while cursor < end && updates[cursor].0 < id {
                    if let Some(place) = updates[cursor].1 {
                        output.push((updates[cursor].0, place));
                    }
                    cursor += 1;
                }
                if cursor < end && updates[cursor].0 == id {
                    if let Some(place) = updates[cursor].1 {
                        output.push((id, place));
                    }
                    cursor += 1;
                } else {
                    output.push((id, place));
                }
            }
            while cursor < end {
                if let Some(place) = updates[cursor].1 {
                    output.push((updates[cursor].0, place));
                }
                cursor += 1;
            }
            output
        })
        .collect()
}

fn publish_runs(containers: &mut [Container], runs: &mut [Vec<(usize, Container)>], base: usize) {
    if runs.is_empty() {
        return;
    }
    if runs.len() == 1 {
        publish(containers, &mut runs[0], base);
        return;
    }
    let middle = runs.len() / 2;
    let split = runs[middle][0].0;
    let (left, right) = containers.split_at_mut(split - base);
    let (first, second) = runs.split_at_mut(middle);
    rayon::join(
        || publish_runs(left, first, base),
        || publish_runs(right, second, split),
    );
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
