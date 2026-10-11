//! Generic canonical storage and publication. Scheduling and feature systems
//! are supplied by composition through the existing game-sim engine.
use game_core::{Error, SimTime};
use game_ecs::{Schema, StableId, Store};
use game_sim::{Clock, ClockWorld};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, PartialEq, Eq)]
pub struct GenericHost<State, Event, Storage = Store> {
    pub storage: Storage,
    clock: Clock,
    seed: u64,
    next_id: u64,
    next_event: u64,
    pub state: State,
    events: Vec<Published<Event>>,
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
        }
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
    pub fn next_id(&self) -> u64 {
        self.next_id
    }
    pub fn next_event(&self) -> u64 {
        self.next_event
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
}
