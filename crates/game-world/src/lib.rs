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

#[derive(Debug, PartialEq, Eq)]
pub struct GenericHost<State, Event> {
    store: Store,
    clock: Clock,
    seed: u64,
    next_id: u64,
    next_event: u64,
    state: State,
    events: Vec<Published<Event>>,
}

pub struct Parts<'a, State> {
    pub state: &'a State,
    pub store: &'a Store,
    pub clock: Clock,
    pub seed: u64,
}
pub struct PartsMut<'a, State> {
    pub state: &'a mut State,
    pub store: &'a mut Store,
    pub clock: Clock,
    pub seed: u64,
}

impl<State, Event> GenericHost<State, Event> {
    pub fn new(schema: Schema, seed: u64, state: State) -> Self {
        Self {
            store: Store::with_schema(schema),
            clock: Clock::default(),
            seed,
            next_id: 1,
            next_event: 1,
            state,
            events: Vec::new(),
        }
    }

    pub fn parts(&self) -> Parts<'_, State> {
        Parts {
            state: &self.state,
            store: &self.store,
            clock: self.clock,
            seed: self.seed,
        }
    }
    pub fn parts_mut(&mut self) -> PartsMut<'_, State> {
        PartsMut {
            state: &mut self.state,
            store: &mut self.store,
            clock: self.clock,
            seed: self.seed,
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
        self.preflight(1, 0)?;
        let id = StableId(self.next_id);
        self.next_id += 1;
        Ok(id)
    }

    /// Admit a typed publication group after checking every counter. No record
    /// or publication counter changes when its complete budget is rejected.
    pub fn publish_many(&mut self, events: Vec<Event>) -> Result<(), Error> {
        let count = u64::try_from(events.len()).map_err(|_| Error::Overflow)?;
        self.preflight(0, count)?;
        for event in events {
            self.events.push(Published {
                sequence: self.next_event,
                at: self.clock.time,
                event,
            });
            self.next_event += 1;
        }
        self.clock.revision += count;
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
            components: self.store.snapshot()?,
            events: self.events.clone(),
        })
    }

    /// Restore a fresh host through supplied schema and contextual validation.
    /// Revision is noncanonical; native query bindings are recreated by callers.
    pub fn restore(
        schema: Schema,
        snapshot: Snapshot<State, Event>,
        validate_context: impl FnOnce(&Self) -> Result<(), Error>,
    ) -> Result<Self, Error> {
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
            store,
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

impl<State, Event> ClockWorld for GenericHost<State, Event> {
    fn clock(&self) -> Clock {
        self.clock
    }
    fn publish_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }
}
