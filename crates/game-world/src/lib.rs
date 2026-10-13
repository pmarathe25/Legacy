//! Generic canonical storage and publication. Scheduling and feature systems
//! are supplied by composition through the existing game-sim engine.
use game_core::{Error, SimTime};
use game_ecs::{Batch, ReadGuard, Schema, StableId, Store};
use game_sim::{Clock, ClockWorld};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Sum a transaction's allocations and publications before any mutation.
#[derive(Default)]
pub struct ResourceBudget {
    ids: u64,
    events: u64,
}
impl ResourceBudget {
    pub fn add_ids(&mut self, count: u64) -> Result<(), Error> {
        self.ids = self.ids.checked_add(count).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn add_events(&mut self, count: u64) -> Result<(), Error> {
        self.events = self.events.checked_add(count).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn check(&self, next_id: u64, next_event: u64) -> Result<(), Error> {
        next_id.checked_add(self.ids).ok_or(Error::Overflow)?;
        next_event.checked_add(self.events).ok_or(Error::Overflow)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Published<Event> {
    pub sequence: u64,
    pub at: SimTime,
    pub event: Event,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot<State, Event> {
    pub version: u32,
    pub time: SimTime,
    pub seed: u64,
    pub next_id: u64,
    pub next_event: u64,
    pub state: State,
    pub components: game_ecs::Snapshot,
    pub events: Vec<Published<Event>>,
}

/// An application storage wrapper retains the same single Store.
pub trait StorageAccess {
    fn store(&self) -> &Store;
    fn store_mut(&mut self) -> &mut Store;
}
impl StorageAccess for Store {
    fn store(&self) -> &Store {
        self
    }
    fn store_mut(&mut self) -> &mut Store {
        self
    }
}

/// Counters and logical clock consulted while preparing a host transaction.
#[derive(Clone, Debug)]
pub struct TransactionOpening {
    pub clock: Clock,
    pub next_id: u64,
    pub next_event: u64,
    lineage: Arc<()>,
}
impl PartialEq for TransactionOpening {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.lineage, &other.lineage)
            && self.clock == other.clock
            && self.next_id == other.next_id
            && self.next_event == other.next_event
    }
}
impl Eq for TransactionOpening {}

/// Already owner-validated combined effects. Runtime/content witnesses remain
/// private to their owners and must be checked before calling commit_transaction.
/// Bind each guard immediately after its owner's writes are staged. No owner
/// finalization may perform fallible work after successful canonical publication.
pub struct Transaction<Event> {
    pub opening: TransactionOpening,
    /// Logical closing time, admitted together with component rows and records.
    pub time: SimTime,
    pub batch: Batch,
    pub guards: Vec<ReadGuard>,
    pub ids: u64,
    pub events: Vec<Event>,
    pub revisions: u64,
}
impl<Event> Transaction<Event> {
    pub fn new(opening: TransactionOpening, batch: Batch) -> Self {
        Self {
            time: opening.clock.time,
            opening,
            batch,
            guards: Vec::new(),
            ids: 0,
            events: Vec::new(),
            revisions: 0,
        }
    }
}

#[derive(Debug)]
pub struct GenericHost<State, Event, Storage = Store> {
    pub storage: Storage,
    clock: Clock,
    seed: u64,
    next_id: u64,
    next_event: u64,
    pub state: State,
    events: Vec<Published<Event>>,
    lineage: Arc<()>,
}
impl<State: PartialEq, Event: PartialEq, Storage: PartialEq> PartialEq
    for GenericHost<State, Event, Storage>
{
    fn eq(&self, other: &Self) -> bool {
        self.storage == other.storage
            && self.clock == other.clock
            && self.seed == other.seed
            && self.next_id == other.next_id
            && self.next_event == other.next_event
            && self.state == other.state
            && self.events == other.events
    }
}
impl<State: Eq, Event: Eq, Storage: Eq> Eq for GenericHost<State, Event, Storage> {}
impl<State: Clone, Event: Clone, Storage: Clone> Clone for GenericHost<State, Event, Storage> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            clock: self.clock,
            seed: self.seed,
            next_id: self.next_id,
            next_event: self.next_event,
            state: self.state.clone(),
            events: self.events.clone(),
            lineage: Arc::new(()),
        }
    }
}

pub struct Parts<'a, State, Event> {
    pub state: &'a State,
    pub store: &'a Store,
    pub clock: Clock,
    pub seed: u64,
    pub next_id: u64,
    pub next_event: u64,
    pub events: &'a [Published<Event>],
}
pub struct PartsMut<'a, State, Event> {
    pub state: &'a mut State,
    pub store: &'a mut Store,
    pub clock: Clock,
    pub seed: u64,
    pub next_id: u64,
    pub next_event: u64,
    pub events: &'a [Published<Event>],
}

impl<State, Event> GenericHost<State, Event> {
    pub fn new(schema: Schema, seed: u64, state: State) -> Self {
        Self::with_storage(Store::with_schema(schema), seed, state)
    }
}
impl<State, Event, Storage: StorageAccess> GenericHost<State, Event, Storage> {
    pub fn with_storage(storage: Storage, seed: u64, state: State) -> Self {
        Self {
            storage,
            state,
            clock: Clock::default(),
            seed,
            next_id: 1,
            next_event: 1,
            events: Vec::new(),
            lineage: Arc::new(()),
        }
    }
    /// Build an isolated, transient projection at an existing logical opening.
    /// Supplied storage and state are already selected by the caller. Historical
    /// records are omitted; `events()` contains only records emitted by this
    /// preview, retaining their original sequence numbers. Its snapshot is not
    /// a complete save. Independent lineage prevents preview plans from being
    /// committed to the source host without explicit source-side preparation.
    pub fn preview(
        storage: Storage,
        state: State,
        opening: &TransactionOpening,
        seed: u64,
    ) -> Self {
        Self {
            storage,
            state,
            clock: opening.clock,
            seed,
            next_id: opening.next_id,
            next_event: opening.next_event,
            events: Vec::new(),
            lineage: Arc::new(()),
        }
    }
    /// Seed only the bounded pending record suffix needed by a preview's owners.
    /// Historical sequences and timestamps remain intact; copying this tail
    /// consumes no event IDs. The preview still has independent host affinity.
    pub fn preview_with_tail(
        storage: Storage,
        state: State,
        opening: &TransactionOpening,
        seed: u64,
        tail: &[Published<Event>],
    ) -> Result<Self, Error>
    where
        Event: Clone,
    {
        if tail.iter().any(|record| {
            record.sequence == 0
                || record.sequence >= opening.next_event
                || record.at > opening.clock.time
        }) || tail
            .windows(2)
            .any(|pair| pair[0].sequence >= pair[1].sequence || pair[0].at > pair[1].at)
        {
            return Err(Error::InvalidConfiguration(
                "invalid preview event tail".into(),
            ));
        }
        let mut preview = Self::preview(storage, state, opening, seed);
        preview.events = tail.to_vec();
        Ok(preview)
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn bump_revision(&mut self, count: u64) -> Result<(), Error> {
        let next = self
            .clock
            .revision
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        self.clock.revision = next;
        Ok(())
    }
    pub fn parts(&self) -> Parts<'_, State, Event> {
        Parts {
            state: &self.state,
            store: self.storage.store(),
            clock: self.clock,
            seed: self.seed,
            next_id: self.next_id,
            next_event: self.next_event,
            events: &self.events,
        }
    }
    pub fn parts_mut(&mut self) -> PartsMut<'_, State, Event> {
        PartsMut {
            state: &mut self.state,
            store: self.storage.store_mut(),
            clock: self.clock,
            seed: self.seed,
            next_id: self.next_id,
            next_event: self.next_event,
            events: &self.events,
        }
    }
    pub fn events(&self) -> &[Published<Event>] {
        &self.events
    }
    /// Records strictly after the last consumed sequence. Sequence partitioning
    /// works both for a complete log and for a preview's retained suffix.
    pub fn events_since(&self, cursor: u64) -> &[Published<Event>] {
        let first = self
            .events
            .partition_point(|record| record.sequence <= cursor);
        &self.events[first..]
    }
    pub fn next_id(&self) -> u64 {
        self.next_id
    }
    pub fn next_event(&self) -> u64 {
        self.next_event
    }
    pub fn transaction_opening(&self) -> TransactionOpening {
        TransactionOpening {
            clock: self.clock,
            next_id: self.next_id,
            next_event: self.next_event,
            lineage: self.lineage.clone(),
        }
    }
    /// Admit one combined batch and its complete publication budget before any
    /// mutation, including monotonic logical time. All Result-returning work precedes the backend's atomic batch
    /// application. Successful application is followed only by infallible cursor
    /// assignments and record publication; failed validation preserves all state.
    pub fn commit_transaction(&mut self, transaction: Transaction<Event>) -> Result<(), Error> {
        if transaction.opening != self.transaction_opening() {
            return Err(Error::Unavailable);
        }
        if transaction.time < self.clock.time {
            return Err(Error::SequenceConflict);
        }
        let count = u64::try_from(transaction.events.len()).map_err(|_| Error::Overflow)?;
        self.preflight_records(transaction.ids, count, transaction.revisions)?;
        let next_id = self.next_id + transaction.ids;
        let next_event = self.next_event + count;
        let revision = self.clock.revision + transaction.revisions;
        let store = self.storage.store();
        if store.has_entities_in(StableId(self.next_id)..StableId(next_id))
            || transaction
                .batch
                .inserted_entities()
                .any(|id| !store.contains(id) && (id.0 < self.next_id || id.0 >= next_id))
        {
            return Err(Error::Unavailable);
        }
        for guard in &transaction.guards {
            guard.validate(store, &transaction.batch)?;
        }
        // apply_batch invokes exact registered validators and structural checks
        // before the first component change. No duplicate schema admission here.
        self.storage.store_mut().apply_batch(transaction.batch)?;
        self.next_id = next_id;
        self.clock = Clock {
            time: transaction.time,
            revision,
        };
        let first_event = self.next_event;
        for (offset, event) in transaction.events.into_iter().enumerate() {
            self.events.push(Published {
                sequence: first_event + offset as u64,
                at: transaction.time,
                event,
            });
        }
        self.next_event = next_event;
        Ok(())
    }

    /// Reserve the whole prospective interval before any feature mutation.
    /// A transaction publishing N records reports N as its revision_budget;
    /// game-sim separately reserves its final boundary publication increment.
    pub fn preflight(&self, ids: u64, publications: u64) -> Result<(), Error> {
        self.next_id.checked_add(ids).ok_or(Error::Overflow)?;
        self.next_event
            .checked_add(publications)
            .ok_or(Error::Overflow)?;
        self.clock
            .revision
            .checked_add(publications)
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    /// Allocation does not attach components; feature admission owns that step.
    pub fn allocate_id(&mut self) -> Result<StableId, Error> {
        self.allocate_ids(1)
    }

    /// Consume a preflighted contiguous ID interval for a prepared transition.
    /// No component rows are attached by this operation.
    pub fn allocate_ids(&mut self, count: u64) -> Result<StableId, Error> {
        let next = self.next_id.checked_add(count).ok_or(Error::Overflow)?;
        let first = StableId(self.next_id);
        self.next_id = next;
        Ok(first)
    }

    /// Append typed records without advancing the caller-visible revision.
    /// The enclosing command or interval publishes its own checked revision.
    pub fn append_many(&mut self, events: Vec<Event>) -> Result<(), Error> {
        let count = u64::try_from(events.len()).map_err(|_| Error::Overflow)?;
        self.next_event.checked_add(count).ok_or(Error::Overflow)?;
        for event in events {
            self.events.push(Published {
                sequence: self.next_event,
                at: self.clock.time,
                event,
            });
            self.next_event += 1;
        }
        Ok(())
    }
    pub fn publish_many(&mut self, events: Vec<Event>) -> Result<(), Error> {
        let count = u64::try_from(events.len()).map_err(|_| Error::Overflow)?;
        self.preflight(0, count)?;
        self.append_many(events)?;
        self.clock.revision += count;
        Ok(())
    }
    pub fn preflight_records(&self, ids: u64, events: u64, revisions: u64) -> Result<(), Error> {
        self.next_id.checked_add(ids).ok_or(Error::Overflow)?;
        self.next_event.checked_add(events).ok_or(Error::Overflow)?;
        self.clock
            .revision
            .checked_add(revisions)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Snapshot<State, Event>, Error>
    where
        State: Clone,
        Event: Clone,
    {
        Ok(Snapshot {
            version: 1,
            time: self.clock.time,
            seed: self.seed,
            next_id: self.next_id,
            next_event: self.next_event,
            state: self.state.clone(),
            components: self.storage.store().snapshot()?,
            events: self.events.clone(),
        })
    }

    /// Restore a fresh host through supplied schema and contextual validation.
    /// Revision is noncanonical; native query bindings are recreated by callers.
    pub fn restore(
        schema: Schema,
        snapshot: Snapshot<State, Event>,
        validate_context: impl FnOnce(&Self) -> Result<(), Error>,
    ) -> Result<Self, Error>
    where
        Storage: From<Store>,
    {
        let fail = || Error::InvalidSave("inconsistent generic host snapshot".into());
        if snapshot.version != 1
            || snapshot.next_id == 0
            || snapshot.next_event == 0
            || u64::try_from(snapshot.events.len())
                .map_err(|_| fail())?
                .checked_add(1)
                != Some(snapshot.next_event)
            || snapshot
                .events
                .windows(2)
                .any(|pair| pair[0].at > pair[1].at)
        {
            return Err(fail());
        }
        for (index, event) in snapshot.events.iter().enumerate() {
            if u64::try_from(index).map_err(|_| fail())?.checked_add(1) != Some(event.sequence)
                || event.at > snapshot.time
            {
                return Err(fail());
            }
        }
        let store = Store::restore(schema, snapshot.components)?;
        if store.ids().any(|id| id.0 >= snapshot.next_id) {
            return Err(fail());
        }
        let host = Self {
            storage: Storage::from(store),
            clock: Clock {
                time: snapshot.time,
                revision: 0,
            },
            seed: snapshot.seed,
            next_id: snapshot.next_id,
            next_event: snapshot.next_event,
            state: snapshot.state,
            events: snapshot.events,
            lineage: Arc::new(()),
        };
        validate_context(&host)?;
        Ok(host)
    }
}

impl<State, Event, Storage> ClockWorld for GenericHost<State, Event, Storage> {
    fn clock(&self) -> Clock {
        self.clock
    }
    fn publish_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }
}

#[cfg(test)]
mod private_counter_tests {
    use super::*;

    #[test]
    fn complete_event_group_is_rejected_before_any_private_cursor_change() {
        let mut host = GenericHost::<(), u8>::new(Schema::new(), 1, ());
        host.next_event = u64::MAX - 1;
        let before = host.snapshot().unwrap();
        assert_eq!(host.append_many(vec![3, 4]), Err(Error::Overflow));
        assert_eq!(host.snapshot().unwrap(), before);
        host.append_many(vec![3]).unwrap();
        assert_eq!(host.next_event(), u64::MAX);
        let before = host.snapshot().unwrap();
        assert_eq!(host.append_many(vec![4]), Err(Error::Overflow));
        assert_eq!(host.snapshot().unwrap(), before);
    }
    #[test]
    fn transaction_budgets_fail_before_rows_ids_events_or_revision_change() {
        for exhausted in 0..3 {
            let mut schema = Schema::new();
            schema.register::<u64>("test/number", |_| Ok(())).unwrap();
            let mut host = GenericHost::<(), u8>::new(schema, 1, ());
            let id = host.allocate_id().unwrap();
            host.storage.insert_or_spawn(id, 10_u64).unwrap();
            match exhausted {
                0 => host.next_id = u64::MAX,
                1 => host.next_event = u64::MAX,
                _ => host.clock.revision = u64::MAX,
            }
            let before = host.snapshot().unwrap();
            let clock = host.clock;
            let mut batch = Batch::new();
            batch.insert(id, 11_u64);
            let mut transaction = Transaction::new(host.transaction_opening(), batch);
            transaction.ids = 1;
            transaction.events.push(1);
            transaction.revisions = 1;
            assert_eq!(host.commit_transaction(transaction), Err(Error::Overflow));
            assert_eq!(host.snapshot().unwrap(), before);
            assert_eq!(host.clock, clock);
        }
    }
}
