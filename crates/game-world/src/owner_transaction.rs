//! Executable owners share a sparse journal and retain their canonical runtime slots.
use crate::{GenericHost, Published, StorageAccess, Transaction};
use game_core::{Error, SimTime};
use game_ecs::{Batch, Component, ReadFootprint, ReadSet, Schema, StableId, Store};
use game_sim::Executor;
use serde::{Deserialize, Serialize};
use std::{
    any::{Any, TypeId},
    collections::{BTreeMap, BTreeSet, VecDeque},
    marker::PhantomData,
    sync::Arc,
};

/// A native owner implements its workflow without importing application state.
pub trait OwnerSystem<E>: Sized + 'static {
    type Runtime: Clone + Eq + 'static;
    type Input: 'static;
    type Output: 'static;
    fn execute(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        input: Self::Input,
    ) -> Result<Self::Output, Error>;
}

/// A typed receiving contract. Its implementation is registered by the receiver.
pub trait Request: 'static {
    type Response: 'static;
}

/// Pure structural wiring to an existing canonical runtime field.
///
/// Both functions must address the same field, perform no other mutation, and
/// never panic. Registered slots must be disjoint: in particular, an owner's
/// runtime must not contain a due queue that is also registered as a separate
/// slot. These configuration contracts cannot be inferred from function pointers.
pub struct RuntimeSlot<S, R> {
    binding: SlotBinding<S, R>,
}
enum SlotBinding<S, R> {
    Field {
        get: fn(&S) -> &R,
        get_mut: fn(&mut S) -> &mut R,
    },
    Unit,
}
impl<S, R> RuntimeSlot<S, R> {
    pub fn new(get: fn(&S) -> &R, get_mut: fn(&mut S) -> &mut R) -> Self {
        Self {
            binding: SlotBinding::Field { get, get_mut },
        }
    }
}
impl<S> RuntimeSlot<S, ()> {
    /// A fixed unit runtime has no canonical state field and installs no metadata.
    pub fn stateless() -> Self {
        Self {
            binding: SlotBinding::Unit,
        }
    }
}

/// Feature-native, serializable due work; payloads never enter a generic feature enum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DueQueue<D>(pub BTreeMap<SimTime, Vec<D>>);
impl<D> Default for DueQueue<D> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

trait Value: Any {
    fn any(&self) -> &dyn Any;
    fn any_mut(&mut self) -> &mut dyn Any;
    fn boxed_any(self: Box<Self>) -> Box<dyn Any>;
    fn copy(&self) -> Box<dyn Value>;
    fn same(&self, other: &dyn Value) -> bool;
}
impl<T: Clone + Eq + 'static> Value for T {
    fn any(&self) -> &dyn Any {
        self
    }
    fn any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn boxed_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
    fn copy(&self) -> Box<dyn Value> {
        Box::new(self.clone())
    }
    fn same(&self, other: &dyn Value) -> bool {
        other.any().downcast_ref::<T>() == Some(self)
    }
}
trait Slot<S> {
    fn copy(&self, state: &S) -> Box<dyn Value>;
    fn matches(&self, state: &S, opening: &dyn Value) -> bool;
    fn install(&self, state: &mut S, value: Box<dyn Value>);
}
impl<S, R: Clone + Eq + 'static> Slot<S> for RuntimeSlot<S, R> {
    fn copy(&self, state: &S) -> Box<dyn Value> {
        match self.binding {
            SlotBinding::Field { get, .. } => Box::new(get(state).clone()),
            SlotBinding::Unit => Box::new(()),
        }
    }
    fn matches(&self, state: &S, opening: &dyn Value) -> bool {
        match self.binding {
            SlotBinding::Field { get, .. } => opening.any().downcast_ref::<R>() == Some(get(state)),
            SlotBinding::Unit => opening.any().is::<()>(),
        }
    }
    fn install(&self, state: &mut S, value: Box<dyn Value>) {
        if let SlotBinding::Field { get_mut, .. } = self.binding {
            *get_mut(state) = *value
                .boxed_any()
                .downcast::<R>()
                .expect("registered runtime type");
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SlotKey {
    Owner(TypeId),
    Due(TypeId),
}
trait Source {
    fn copy(&self) -> Box<dyn Value>;
}
struct StateSource<'a, S> {
    slot: &'a dyn Slot<S>,
    state: &'a S,
}
impl<S> Source for StateSource<'_, S> {
    fn copy(&self) -> Box<dyn Value> {
        self.slot.copy(self.state)
    }
}
struct RuntimeChange {
    key: SlotKey,
    opening: Box<dyn Value>,
    current: Box<dyn Value>,
}

trait Edit {
    fn value(&self) -> Option<&dyn Any>;
    fn stage(&self, id: StableId, batch: &mut Batch, footprint: &mut ReadFootprint);
}
struct Row<C>(Option<C>);
impl<C: Component> Edit for Row<C> {
    fn value(&self) -> Option<&dyn Any> {
        self.0.as_ref().map(|value| value as &dyn Any)
    }
    fn stage(&self, id: StableId, batch: &mut Batch, footprint: &mut ReadFootprint) {
        match &self.0 {
            Some(value) => {
                batch.insert(id, value.clone());
            }
            None => {
                batch.remove::<C>(id);
            }
        }
        footprint.protect::<C>(id);
    }
}

trait Handler<E> {
    fn owner(&self) -> TypeId;
    fn readonly(&self) -> bool {
        false
    }
    fn run(&self, journal: &mut Journal<'_, E>, input: Box<dyn Any>)
    -> Result<Box<dyn Any>, Error>;
}
struct SystemHandler<O>(Arc<O>);
impl<O: OwnerSystem<E>, E: 'static> Handler<E> for SystemHandler<O> {
    fn owner(&self) -> TypeId {
        TypeId::of::<O>()
    }
    fn run(
        &self,
        journal: &mut Journal<'_, E>,
        input: Box<dyn Any>,
    ) -> Result<Box<dyn Any>, Error> {
        let input = *input
            .downcast::<O::Input>()
            .map_err(|_| Error::Unavailable)?;
        let mut tx = OwnerTxn {
            journal,
            marker: PhantomData::<O>,
        };
        self.0
            .execute(&mut tx, input)
            .map(|value| Box::new(value) as Box<dyn Any>)
    }
}
type Receive<O, Q, E> =
    fn(&O, &mut OwnerTxn<'_, '_, O, E>, Q) -> Result<<Q as Request>::Response, Error>;
struct RequestHandler<O, Q: Request, E> {
    owner: Arc<O>,
    receive: Receive<O, Q, E>,
}
impl<O: OwnerSystem<E>, Q: Request, E: 'static> Handler<E> for RequestHandler<O, Q, E> {
    fn owner(&self) -> TypeId {
        TypeId::of::<O>()
    }
    fn run(
        &self,
        journal: &mut Journal<'_, E>,
        input: Box<dyn Any>,
    ) -> Result<Box<dyn Any>, Error> {
        let input = *input.downcast::<Q>().map_err(|_| Error::Unavailable)?;
        let mut tx = OwnerTxn {
            journal,
            marker: PhantomData::<O>,
        };
        (self.receive)(&self.owner, &mut tx, input).map(|value| Box::new(value) as Box<dyn Any>)
    }
}

type ReadReceive<O, Q, E> =
    fn(&O, &mut OwnerReadTxn<'_, '_, O, E>, Q) -> Result<<Q as Request>::Response, Error>;
struct ReadRequestHandler<O, Q: Request, E> {
    owner: Arc<O>,
    receive: ReadReceive<O, Q, E>,
}
impl<O: OwnerSystem<E>, Q: Request, E: 'static> Handler<E> for ReadRequestHandler<O, Q, E> {
    fn owner(&self) -> TypeId {
        TypeId::of::<O>()
    }
    fn readonly(&self) -> bool {
        true
    }
    fn run(
        &self,
        journal: &mut Journal<'_, E>,
        input: Box<dyn Any>,
    ) -> Result<Box<dyn Any>, Error> {
        let input = *input.downcast::<Q>().map_err(|_| Error::Unavailable)?;
        let mut tx = OwnerReadTxn {
            journal,
            marker: PhantomData::<O>,
        };
        (self.receive)(&self.owner, &mut tx, input).map(|value| Box::new(value) as Box<dyn Any>)
    }
}

type Subscriber<O, T, E> = fn(&O, &mut OwnerTxn<'_, '_, O, E>, T) -> Result<(), Error>;
struct SubscriberHandler<O, T, E> {
    owner: Arc<O>,
    receive: Subscriber<O, T, E>,
}
impl<O: OwnerSystem<E>, T: 'static, E: 'static> Handler<E> for SubscriberHandler<O, T, E> {
    fn owner(&self) -> TypeId {
        TypeId::of::<O>()
    }
    fn run(
        &self,
        journal: &mut Journal<'_, E>,
        input: Box<dyn Any>,
    ) -> Result<Box<dyn Any>, Error> {
        let input = *input.downcast::<T>().map_err(|_| Error::Unavailable)?;
        let mut tx = OwnerTxn {
            journal,
            marker: PhantomData::<O>,
        };
        (self.receive)(&self.owner, &mut tx, input).map(|()| Box::new(()) as Box<dyn Any>)
    }
}

/// Immutable executable registrations, canonical field lenses and write ownership.
/// Register before preparing work; changes invalidate previously prepared journals.
pub struct Registry<S, E> {
    key: Arc<()>,
    generation: u64,
    instances: BTreeMap<TypeId, Box<dyn Any>>,
    systems: BTreeMap<TypeId, Arc<dyn Handler<E>>>,
    requests: BTreeMap<TypeId, Arc<dyn Handler<E>>>,
    subscribers: BTreeMap<TypeId, Vec<Arc<dyn Handler<E>>>>,
    slots: BTreeMap<SlotKey, Box<dyn Slot<S>>>,
    writes: BTreeMap<TypeId, TypeId>,
    due_owners: BTreeMap<TypeId, TypeId>,
}
impl<S: 'static, E: 'static> Default for Registry<S, E> {
    fn default() -> Self {
        Self::new()
    }
}
impl<S: 'static, E: 'static> Registry<S, E> {
    pub fn new() -> Self {
        Self {
            key: Arc::new(()),
            generation: 0,
            instances: BTreeMap::new(),
            systems: BTreeMap::new(),
            requests: BTreeMap::new(),
            subscribers: BTreeMap::new(),
            slots: BTreeMap::new(),
            writes: BTreeMap::new(),
            due_owners: BTreeMap::new(),
        }
    }
    fn changed(&mut self) -> Result<(), Error> {
        self.generation = self.generation.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn register<O: OwnerSystem<E>>(
        &mut self,
        owner: O,
        slot: RuntimeSlot<S, O::Runtime>,
    ) -> Result<(), Error> {
        let key = TypeId::of::<O>();
        if self.systems.contains_key(&key) {
            return Err(configuration("duplicate owner system"));
        }
        self.changed()?;
        let owner = Arc::new(owner);
        self.instances.insert(key, Box::new(owner.clone()));
        self.systems.insert(key, Arc::new(SystemHandler(owner)));
        self.slots.insert(SlotKey::Owner(key), Box::new(slot));
        Ok(())
    }
    pub fn owns<O: OwnerSystem<E>, C: Component>(&mut self) -> Result<(), Error> {
        if !self.systems.contains_key(&TypeId::of::<O>())
            || self.writes.contains_key(&TypeId::of::<C>())
        {
            return Err(configuration("missing or duplicate component owner"));
        }
        self.changed()?;
        self.writes.insert(TypeId::of::<C>(), TypeId::of::<O>());
        Ok(())
    }
    pub fn register_stateless<O: OwnerSystem<E, Runtime = ()>>(
        &mut self,
        owner: O,
    ) -> Result<(), Error> {
        self.register(owner, RuntimeSlot::stateless())
    }
    pub fn register_request<O: OwnerSystem<E>, Q: Request>(
        &mut self,
        receive: Receive<O, Q, E>,
    ) -> Result<(), Error> {
        if self.requests.contains_key(&TypeId::of::<Q>()) {
            return Err(configuration("duplicate request receiver"));
        }
        let instance = self
            .instances
            .get(&TypeId::of::<O>())
            .ok_or_else(|| configuration("unregistered request receiver"))?;
        let owner = instance
            .downcast_ref::<Arc<O>>()
            .ok_or(Error::Unavailable)?
            .clone();
        self.changed()?;
        self.requests.insert(
            TypeId::of::<Q>(),
            Arc::new(RequestHandler { owner, receive }),
        );
        Ok(())
    }
    pub fn register_due<O: OwnerSystem<E>, D: Clone + Eq + 'static>(
        &mut self,
        slot: RuntimeSlot<S, DueQueue<D>>,
    ) -> Result<(), Error> {
        if !self.systems.contains_key(&TypeId::of::<O>())
            || self.due_owners.contains_key(&TypeId::of::<D>())
        {
            return Err(configuration("missing or duplicate due owner"));
        }
        self.changed()?;
        self.due_owners.insert(TypeId::of::<D>(), TypeId::of::<O>());
        self.slots
            .insert(SlotKey::Due(TypeId::of::<D>()), Box::new(slot));
        Ok(())
    }
    /// Install a fact query with an API that cannot mutate journal effects.
    /// Such handlers may consult an owner that is already executing a workflow.
    pub fn register_read_request<O: OwnerSystem<E>, Q: Request>(
        &mut self,
        receive: ReadReceive<O, Q, E>,
    ) -> Result<(), Error> {
        if self.requests.contains_key(&TypeId::of::<Q>()) {
            return Err(configuration("duplicate request receiver"));
        }
        let instance = self
            .instances
            .get(&TypeId::of::<O>())
            .ok_or_else(|| configuration("unregistered request receiver"))?;
        let owner = instance
            .downcast_ref::<Arc<O>>()
            .ok_or(Error::Unavailable)?
            .clone();
        self.changed()?;
        self.requests.insert(
            TypeId::of::<Q>(),
            Arc::new(ReadRequestHandler { owner, receive }),
        );
        Ok(())
    }
    /// Subscribe a receiving owner to an internal typed topic. Recipients run in
    /// registration order after the originating stack returns, in its journal.
    /// This does not define or publish a public gameplay record.
    pub fn register_subscriber<O: OwnerSystem<E>, T: Clone + 'static>(
        &mut self,
        receive: Subscriber<O, T, E>,
    ) -> Result<(), Error> {
        if self
            .subscribers
            .get(&TypeId::of::<T>())
            .is_some_and(|subscribers| {
                subscribers
                    .iter()
                    .any(|subscriber| subscriber.owner() == TypeId::of::<O>())
            })
        {
            return Err(configuration("duplicate typed topic recipient"));
        }
        let owner = self
            .instances
            .get(&TypeId::of::<O>())
            .ok_or_else(|| configuration("unregistered topic recipient"))?
            .downcast_ref::<Arc<O>>()
            .ok_or(Error::Unavailable)?
            .clone();
        self.changed()?;
        self.subscribers
            .entry(TypeId::of::<T>())
            .or_default()
            .push(Arc::new(SubscriberHandler { owner, receive }));
        Ok(())
    }
    pub fn prepare<T, Storage: StorageAccess>(
        &self,
        host: &GenericHost<S, E, Storage>,
        at: SimTime,
        executor: Option<&Executor>,
        execute: impl FnOnce(&mut TransactionContext<'_, '_, E>) -> Result<T, Error>,
    ) -> Result<PreparedOwnerTransaction<E, T>, Error> {
        let opening = host.transaction_opening();
        if at < opening.clock.time {
            return Err(Error::SequenceConflict);
        }
        let sources = self
            .slots
            .iter()
            .map(|(key, slot)| {
                (
                    *key,
                    Box::new(StateSource {
                        slot: &**slot,
                        state: &host.state,
                    }) as Box<dyn Source>,
                )
            })
            .collect();
        let mut journal = Journal {
            store: host.storage.store(),
            history: host.events(),
            opening: opening.clone(),
            at,
            seed: host.seed(),
            executor,
            systems: &self.systems,
            requests: &self.requests,
            subscribers: &self.subscribers,
            sources,
            writes: &self.writes,
            due_owners: &self.due_owners,
            reads: ReadSet::new(),
            witnessed: BTreeSet::new(),
            witnessed_entities: BTreeSet::new(),
            provisional_entities: BTreeMap::new(),
            columns: BTreeMap::new(),
            overlay: BTreeMap::new(),
            runtimes: BTreeMap::new(),
            active: BTreeSet::new(),
            readonly_depth: 0,
            mutable_started: false,
            facts: BTreeMap::new(),
            pending: VecDeque::new(),
            notifications: VecDeque::new(),
            notifying: false,
            calls: 0,
            failed: None,
            ids: 0,
            revisions: 0,
            events: Vec::new(),
        };
        let output = execute(&mut TransactionContext {
            journal: &mut journal,
        })?;
        journal.drain()?;
        if let Some(error) = journal.failed {
            return Err(error);
        }
        let mut batch = Batch::new();
        let mut footprint = ReadFootprint::new();
        for ((id, _), edit) in &journal.overlay {
            edit.stage(*id, &mut batch, &mut footprint);
        }
        let guard = journal.reads.bind(journal.store, &batch, &footprint)?;
        let mut transaction = Transaction::new(opening, batch);
        transaction.time = journal.at;
        transaction.ids = journal.ids;
        transaction.revisions = journal.revisions;
        transaction.events = journal.events;
        transaction.guards.push(guard);
        Ok(PreparedOwnerTransaction {
            key: self.key.clone(),
            generation: self.generation,
            schema: host.storage.store().schema().clone(),
            transaction,
            runtimes: journal.runtimes.into_values().collect(),
            output,
        })
    }
    /// Execute a registered readonly query against canonical state without
    /// publishing a transaction, records, logical time or revisions.
    pub fn inspect<Q: Request, Storage: StorageAccess>(
        &self,
        host: &GenericHost<S, E, Storage>,
        executor: Option<&Executor>,
        query: Q,
    ) -> Result<Q::Response, Error> {
        let handler = self
            .requests
            .get(&TypeId::of::<Q>())
            .ok_or_else(|| configuration("unregistered readonly owner request"))?;
        if !handler.readonly() {
            return Err(configuration("mutable owner request used for inspection"));
        }
        self.prepare(
            host,
            host.transaction_opening().clock.time,
            executor,
            |tx| tx.request(query),
        )
        .map(|prepared| prepared.output)
    }
    pub fn commit<T, Storage: StorageAccess>(
        &self,
        host: &mut GenericHost<S, E, Storage>,
        prepared: PreparedOwnerTransaction<E, T>,
    ) -> Result<T, Error> {
        if !Arc::ptr_eq(&self.key, &prepared.key) || self.generation != prepared.generation {
            return Err(Error::Unavailable);
        }
        if !prepared
            .schema
            .same_registrations(host.storage.store().schema())
        {
            return Err(Error::Unavailable);
        }
        for runtime in &prepared.runtimes {
            let slot = self.slots.get(&runtime.key).ok_or(Error::Unavailable)?;
            if !slot.matches(&host.state, &*runtime.opening) {
                return Err(Error::Unavailable);
            }
        }
        host.commit_transaction(prepared.transaction)?;
        // Only registered typed assignments follow canonical row admission.
        for runtime in prepared.runtimes {
            if !runtime.opening.same(&*runtime.current) {
                self.slots[&runtime.key].install(&mut host.state, runtime.current);
            }
        }
        Ok(prepared.output)
    }
}

pub struct PreparedOwnerTransaction<E, T> {
    key: Arc<()>,
    generation: u64,
    schema: Schema,
    transaction: Transaction<E>,
    runtimes: Vec<RuntimeChange>,
    output: T,
}

struct Journal<'s, E> {
    store: &'s Store,
    history: &'s [Published<E>],
    opening: crate::TransactionOpening,
    at: SimTime,
    seed: u64,
    executor: Option<&'s Executor>,
    systems: &'s BTreeMap<TypeId, Arc<dyn Handler<E>>>,
    requests: &'s BTreeMap<TypeId, Arc<dyn Handler<E>>>,
    subscribers: &'s BTreeMap<TypeId, Vec<Arc<dyn Handler<E>>>>,
    sources: BTreeMap<SlotKey, Box<dyn Source + 's>>,
    writes: &'s BTreeMap<TypeId, TypeId>,
    due_owners: &'s BTreeMap<TypeId, TypeId>,
    reads: ReadSet,
    witnessed: BTreeSet<(StableId, TypeId)>,
    witnessed_entities: BTreeSet<StableId>,
    provisional_entities: BTreeMap<StableId, usize>,
    columns: BTreeMap<TypeId, BTreeSet<StableId>>,
    overlay: BTreeMap<(StableId, TypeId), Box<dyn Edit>>,
    runtimes: BTreeMap<SlotKey, RuntimeChange>,
    active: BTreeSet<TypeId>,
    readonly_depth: u16,
    mutable_started: bool,
    facts: BTreeMap<TypeId, SharedFact>,
    pending: VecDeque<(TypeId, Box<dyn Any>)>,
    notifications: VecDeque<(Arc<dyn Handler<E>>, Box<dyn Any>)>,
    notifying: bool,
    calls: u64,
    failed: Option<Error>,
    ids: u64,
    revisions: u64,
    events: Vec<E>,
}
struct SharedFact {
    owner: TypeId,
    value: Box<dyn Any>,
}
impl<E: 'static> Journal<'_, E> {
    fn rejected<T>(&mut self, error: Error) -> Result<T, Error> {
        if self.failed.is_none() {
            self.failed = Some(error.clone());
        }
        Err(error)
    }
    fn checked<T>(&mut self, result: Result<T, Error>) -> Result<T, Error> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => self.rejected(error),
        }
    }
    fn runtime(&mut self, key: SlotKey) -> &mut dyn Value {
        self.runtimes
            .entry(key)
            .or_insert_with(|| {
                let opening = self.sources[&key].copy();
                let current = opening.copy();
                RuntimeChange {
                    key,
                    opening,
                    current,
                }
            })
            .current
            .as_mut()
    }
    fn run(
        &mut self,
        handler: Arc<dyn Handler<E>>,
        input: Box<dyn Any>,
    ) -> Result<Box<dyn Any>, Error> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        let owner = handler.owner();
        let readonly = handler.readonly();
        let already_active = self.active.contains(&owner);
        if self.calls >= 65_536
            || (readonly && self.readonly_depth >= 128)
            || (!readonly && already_active)
        {
            let error = configuration(
                "recursive mutable owner entry or request depth/count limit exceeded",
            );
            self.failed = Some(error.clone());
            return Err(error);
        }
        if !already_active {
            self.active.insert(owner);
        }
        if readonly {
            self.readonly_depth += 1;
        } else {
            self.mutable_started = true;
        }
        self.calls += 1;
        let result = handler.run(self, input);
        if readonly {
            self.readonly_depth -= 1;
        }
        if !already_active {
            self.active.remove(&owner);
        }
        if let Err(error) = &result {
            self.failed = Some(error.clone());
        }
        if result.is_ok() && self.active.is_empty() && !self.notifying {
            self.notify()?;
        }
        result
    }
    fn notify(&mut self) -> Result<(), Error> {
        if self.notifying {
            return Ok(());
        }
        self.notifying = true;
        let result = (|| {
            while let Some((handler, notification)) = self.notifications.pop_front() {
                self.run(handler, notification)?;
            }
            Ok(())
        })();
        self.notifying = false;
        result
    }
    fn call<O: OwnerSystem<E>>(&mut self, input: O::Input) -> Result<O::Output, Error> {
        let Some(handler) = self.systems.get(&TypeId::of::<O>()).cloned() else {
            return self.rejected(configuration("unregistered owner system"));
        };
        self.run(handler, Box::new(input))?
            .downcast::<O::Output>()
            .map(|value| *value)
            .map_err(|_| Error::Unavailable)
    }
    fn request<Q: Request>(&mut self, input: Q) -> Result<Q::Response, Error> {
        let Some(handler) = self.requests.get(&TypeId::of::<Q>()).cloned() else {
            return self.rejected(configuration("unregistered owner request"));
        };
        self.run(handler, Box::new(input))?
            .downcast::<Q::Response>()
            .map(|value| *value)
            .map_err(|_| Error::Unavailable)
    }
    fn read_request<Q: Request>(&mut self, input: Q) -> Result<Q::Response, Error> {
        let Some(handler) = self.requests.get(&TypeId::of::<Q>()).cloned() else {
            return self.rejected(configuration("unregistered readonly owner request"));
        };
        if !handler.readonly() {
            return self.rejected(configuration("mutable owner request from a readonly query"));
        }
        self.run(handler, Box::new(input))?
            .downcast::<Q::Response>()
            .map(|value| *value)
            .map_err(|_| Error::Unavailable)
    }
    fn drain(&mut self) -> Result<(), Error> {
        while let Some((key, input)) = self.pending.pop_front() {
            let handler = self
                .requests
                .get(&key)
                .cloned()
                .ok_or_else(|| configuration("unregistered deferred request"))?;
            self.run(handler, input)?;
        }
        Ok(())
    }
    fn read<C: Component>(&mut self, id: StableId) -> Option<&C> {
        self.witness::<C>(id);
        self.projected::<C>(id)
    }
    fn entity_exists(&mut self, id: StableId) -> bool {
        if self.witnessed_entities.insert(id) {
            self.reads.entity(self.store, id);
        }
        self.store.contains(id) || self.provisional_entities.contains_key(&id)
    }
    fn iter<C: Component>(&mut self) -> Vec<(StableId, C)> {
        let mut ids = self
            .columns
            .entry(TypeId::of::<C>())
            .or_insert_with(|| self.reads.membership::<C>(self.store, |_, _| true))
            .clone();
        ids.extend(
            self.overlay
                .keys()
                .filter_map(|(id, ty)| (*ty == TypeId::of::<C>()).then_some(*id)),
        );
        ids.into_iter()
            .filter_map(|id| self.read::<C>(id).cloned().map(|row| (id, row)))
            .collect()
    }
    fn witness<C: Component>(&mut self, id: StableId) {
        if self.witnessed.insert((id, TypeId::of::<C>())) {
            self.reads.read::<C>(self.store, id);
        }
    }
    fn projected<C: Component>(&self, id: StableId) -> Option<&C> {
        match self.overlay.get(&(id, TypeId::of::<C>())) {
            Some(edit) => edit.value().and_then(|value| value.downcast_ref::<C>()),
            None => self.store.get::<C>(id),
        }
    }
    fn write<O: OwnerSystem<E>, C: Component>(
        &mut self,
        id: StableId,
        value: Option<C>,
    ) -> Result<(), Error> {
        if self.writes.get(&TypeId::of::<C>()) != Some(&TypeId::of::<O>()) {
            let error = configuration("component write by a foreign owner");
            self.failed = Some(error.clone());
            return Err(error);
        }
        self.witness::<C>(id);
        let key = (id, TypeId::of::<C>());
        let before = self
            .overlay
            .get(&key)
            .is_some_and(|edit| edit.value().is_some());
        let after = value.is_some();
        if !before && after {
            *self.provisional_entities.entry(id).or_default() += 1;
        } else if before && !after {
            let count = self
                .provisional_entities
                .get_mut(&id)
                .expect("provisional entity row census");
            *count -= 1;
            if *count == 0 {
                self.provisional_entities.remove(&id);
            }
        }
        self.overlay.insert(key, Box::new(Row(value)));
        Ok(())
    }
}

/// Composition may invoke registered owners; it cannot mutate their rows or runtime.
pub struct TransactionContext<'a, 's, E> {
    journal: &'a mut Journal<'s, E>,
}
impl<E: 'static> TransactionContext<'_, '_, E> {
    pub fn call<O: OwnerSystem<E>>(&mut self, input: O::Input) -> Result<O::Output, Error> {
        self.journal.call::<O>(input)
    }
    pub fn request<Q: Request>(&mut self, input: Q) -> Result<Q::Response, Error> {
        self.journal.request(input)
    }
    /// Choose the closing clock after readonly boundary queries and before any
    /// mutable owner callback. All emitted records retain this one closing time.
    pub fn set_time(&mut self, at: SimTime) -> Result<(), Error> {
        if self.journal.mutable_started {
            return self.journal.rejected(configuration(
                "closing time selected after mutable owner execution",
            ));
        }
        if at < self.journal.at {
            return self.journal.rejected(Error::SequenceConflict);
        }
        self.journal.at = at;
        Ok(())
    }
    pub(crate) fn boundary_revision(&mut self) -> Result<(), Error> {
        let result = self
            .journal
            .revisions
            .checked_add(1)
            .ok_or(Error::Overflow)
            .and_then(|total| {
                self.journal
                    .opening
                    .clock
                    .revision
                    .checked_add(total)
                    .ok_or(Error::Overflow)
                    .map(|_| total)
            });
        self.journal.revisions = self.journal.checked(result)?;
        Ok(())
    }
    pub(crate) fn flush_deferred(&mut self) -> Result<(), Error> {
        self.journal.drain()
    }
}

/// An owner sees the current journal, its own runtime and typed receiving contracts.
pub struct OwnerTxn<'a, 's, O, E> {
    journal: &'a mut Journal<'s, E>,
    marker: PhantomData<O>,
}
impl<O: OwnerSystem<E>, E: 'static> OwnerTxn<'_, '_, O, E> {
    pub fn now(&self) -> SimTime {
        self.journal.at
    }
    pub fn seed(&self) -> u64 {
        self.journal.seed
    }
    pub fn next_id(&self) -> u64 {
        self.journal.opening.next_id + self.journal.ids
    }
    pub fn next_event(&self) -> u64 {
        self.journal.opening.next_event + self.journal.events.len() as u64
    }
    pub fn executor(&self) -> Option<&Executor> {
        self.journal.executor
    }
    pub fn read<C: Component>(&mut self, id: StableId) -> Option<&C> {
        self.journal.read::<C>(id)
    }
    /// Witness canonical entity presence and inspect the provisional row census.
    /// Existing entities remain present after their last component is removed.
    pub fn entity_exists(&mut self, id: StableId) -> bool {
        self.journal.entity_exists(id)
    }
    /// Stable union of canonical and provisional rows, with removals excluded.
    /// Records complete column membership and the values returned to the caller.
    pub fn iter<C: Component>(&mut self) -> Vec<(StableId, C)> {
        self.journal.iter::<C>()
    }
    pub fn write<C: Component>(&mut self, id: StableId, value: C) -> Result<(), Error> {
        self.journal.write::<O, C>(id, Some(value))
    }
    pub fn remove<C: Component>(&mut self, id: StableId) -> Result<(), Error> {
        self.journal.write::<O, C>(id, None)
    }
    pub fn runtime(&mut self) -> &O::Runtime {
        self.journal
            .runtime(SlotKey::Owner(TypeId::of::<O>()))
            .any()
            .downcast_ref::<O::Runtime>()
            .expect("registered owner runtime")
    }
    pub fn runtime_mut(&mut self) -> &mut O::Runtime {
        self.journal
            .runtime(SlotKey::Owner(TypeId::of::<O>()))
            .any_mut()
            .downcast_mut::<O::Runtime>()
            .expect("registered owner runtime")
    }
    pub fn call<P: OwnerSystem<E>>(&mut self, input: P::Input) -> Result<P::Output, Error> {
        self.journal.call::<P>(input)
    }
    pub fn request<Q: Request>(&mut self, input: Q) -> Result<Q::Response, Error> {
        self.journal.request(input)
    }
    /// Publish an INTERNAL typed topic notification. This queues receiving owner
    /// callbacks and consumes no public event sequence or revision. A source
    /// publishes its authoritative public gameplay record separately with emit.
    pub fn publish<T: Clone + 'static>(&mut self, notification: T) -> Result<(), Error> {
        if let Some(subscribers) = self.journal.subscribers.get(&TypeId::of::<T>()) {
            if self
                .journal
                .notifications
                .len()
                .saturating_add(subscribers.len())
                > 65_536
            {
                return self
                    .journal
                    .rejected(configuration("typed topic notification limit exceeded"));
            }
            for subscriber in subscribers {
                self.journal
                    .notifications
                    .push_back((subscriber.clone(), Box::new(notification.clone())));
            }
        }
        Ok(())
    }
    /// Retain a native intermediate output for this journal only. Its producing
    /// owner may replace it; other owners can inspect it but cannot overwrite it.
    pub fn share<F: 'static>(&mut self, fact: F) -> Result<(), Error> {
        let key = TypeId::of::<F>();
        let owner = TypeId::of::<O>();
        if self
            .journal
            .facts
            .get(&key)
            .is_some_and(|fact| fact.owner != owner)
        {
            return self.journal.rejected(configuration(
                "intermediate fact overwritten by a foreign owner",
            ));
        }
        self.journal.facts.insert(
            key,
            SharedFact {
                owner,
                value: Box::new(fact),
            },
        );
        Ok(())
    }
    pub fn fact<F: 'static>(&self) -> Option<&F> {
        self.journal
            .facts
            .get(&TypeId::of::<F>())
            .and_then(|fact| fact.value.downcast_ref::<F>())
    }
    /// Mutate an intermediate output only from its producing owner, for example
    /// to append receiving receipts without copying the owner's whole frame.
    pub fn fact_mut<F: 'static>(&mut self) -> Result<Option<&mut F>, Error> {
        let key = TypeId::of::<F>();
        if self
            .journal
            .facts
            .get(&key)
            .is_some_and(|fact| fact.owner != TypeId::of::<O>())
        {
            return self.journal.rejected(configuration(
                "intermediate fact mutated by a foreign owner",
            ));
        }
        Ok(self
            .journal
            .facts
            .get_mut(&key)
            .and_then(|fact| fact.value.downcast_mut::<F>()))
    }
    /// Queue a notification after the initiating stack has returned. This permits
    /// deliberate owner re-entry without aliasing a live mutable runtime borrow.
    pub fn defer<Q: Request<Response = ()>>(&mut self, request: Q) -> Result<(), Error> {
        if !self.journal.requests.contains_key(&TypeId::of::<Q>()) {
            return self
                .journal
                .rejected(configuration("unregistered deferred request"));
        }
        if self.journal.pending.len() >= 65_536 {
            return self
                .journal
                .rejected(configuration("owner notification limit exceeded"));
        }
        self.journal
            .pending
            .push_back((TypeId::of::<Q>(), Box::new(request)));
        Ok(())
    }
    pub fn reserve_ids(&mut self, count: u64) -> Result<StableId, Error> {
        let result = (|| {
            let first = self
                .journal
                .opening
                .next_id
                .checked_add(self.journal.ids)
                .ok_or(Error::Overflow)?;
            let total = self.journal.ids.checked_add(count).ok_or(Error::Overflow)?;
            self.journal
                .opening
                .next_id
                .checked_add(total)
                .ok_or(Error::Overflow)?;
            Ok((first, total))
        })();
        let (first, total) = self.journal.checked(result)?;
        self.journal.ids = total;
        Ok(StableId(first))
    }
    pub fn emit(&mut self, event: E) -> Result<u64, Error> {
        let result = (|| {
            let offset = u64::try_from(self.journal.events.len()).map_err(|_| Error::Overflow)?;
            let sequence = self
                .journal
                .opening
                .next_event
                .checked_add(offset)
                .ok_or(Error::Overflow)?;
            sequence.checked_add(1).ok_or(Error::Overflow)?;
            Ok(sequence)
        })();
        let sequence = self.journal.checked(result)?;
        self.journal.events.push(event);
        Ok(sequence)
    }
    pub fn revise(&mut self, count: u64) -> Result<(), Error> {
        let result = (|| {
            let total = self
                .journal
                .revisions
                .checked_add(count)
                .ok_or(Error::Overflow)?;
            self.journal
                .opening
                .clock
                .revision
                .checked_add(total)
                .ok_or(Error::Overflow)?;
            Ok(total)
        })();
        let total = self.journal.checked(result)?;
        self.journal.revisions = total;
        Ok(())
    }
    /// Borrow historical and newly staged records without copying the event log.
    pub fn events_since(&self, cursor: u64) -> Vec<(u64, SimTime, &E)> {
        let first = self
            .journal
            .history
            .partition_point(|row| row.sequence <= cursor);
        let mut records: Vec<_> = self.journal.history[first..]
            .iter()
            .map(|row| (row.sequence, row.at, &row.event))
            .collect();
        records.extend(
            self.journal
                .events
                .iter()
                .enumerate()
                .filter_map(|(offset, event)| {
                    let sequence = self.journal.opening.next_event + offset as u64;
                    (sequence > cursor).then_some((sequence, self.journal.at, event))
                }),
        );
        records
    }
    fn due_queue<D: Clone + Eq + 'static>(&mut self) -> Result<&mut DueQueue<D>, Error> {
        if self.journal.due_owners.get(&TypeId::of::<D>()) != Some(&TypeId::of::<O>()) {
            return self
                .journal
                .rejected(configuration("due work accessed by a foreign owner"));
        }
        Ok(self
            .journal
            .runtime(SlotKey::Due(TypeId::of::<D>()))
            .any_mut()
            .downcast_mut::<DueQueue<D>>()
            .expect("registered due payload"))
    }
    pub fn schedule<D: Clone + Eq + 'static>(&mut self, at: SimTime, work: D) -> Result<(), Error> {
        if at < self.now() {
            return self.journal.rejected(Error::SequenceConflict);
        }
        self.due_queue::<D>()?.0.entry(at).or_default().push(work);
        Ok(())
    }
    pub fn take_due<D: Clone + Eq + 'static>(&mut self) -> Result<Vec<(SimTime, D)>, Error> {
        let now = self.now();
        let queue = &mut self.due_queue::<D>()?.0;
        let times: Vec<_> = queue.range(..=now).map(|(at, _)| *at).collect();
        Ok(times
            .into_iter()
            .flat_map(|at| {
                queue
                    .remove(&at)
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |work| (at, work))
            })
            .collect())
    }
    pub fn due<D: Clone + Eq + 'static>(&mut self) -> Result<Vec<(SimTime, D)>, Error> {
        Ok(self
            .due_queue::<D>()?
            .0
            .iter()
            .flat_map(|(at, work)| work.iter().cloned().map(move |work| (*at, work)))
            .collect())
    }
    pub fn cancel_due<D: Clone + Eq + 'static>(
        &mut self,
        mut predicate: impl FnMut(SimTime, &D) -> bool,
    ) -> Result<(), Error> {
        let queue = &mut self.due_queue::<D>()?.0;
        for (at, work) in queue.iter_mut() {
            work.retain(|work| !predicate(*at, work));
        }
        queue.retain(|_, work| !work.is_empty());
        Ok(())
    }
}

/// A receiving fact query can inspect the current provisional rows and runtime.
/// It has no write, mutable runtime, publication, allocation or scheduling API.
pub struct OwnerReadTxn<'a, 's, O, E> {
    journal: &'a mut Journal<'s, E>,
    marker: PhantomData<O>,
}
impl<O: OwnerSystem<E>, E: 'static> OwnerReadTxn<'_, '_, O, E> {
    pub fn now(&self) -> SimTime {
        self.journal.at
    }
    pub fn seed(&self) -> u64 {
        self.journal.seed
    }
    pub fn next_event(&self) -> u64 {
        self.journal.opening.next_event + self.journal.events.len() as u64
    }
    pub fn next_id(&self) -> u64 {
        self.journal.opening.next_id + self.journal.ids
    }
    pub fn read<C: Component>(&mut self, id: StableId) -> Option<&C> {
        self.journal.read::<C>(id)
    }
    pub fn entity_exists(&mut self, id: StableId) -> bool {
        self.journal.entity_exists(id)
    }
    pub fn iter<C: Component>(&mut self) -> Vec<(StableId, C)> {
        self.journal.iter::<C>()
    }
    pub fn runtime(&mut self) -> &O::Runtime {
        self.journal
            .runtime(SlotKey::Owner(TypeId::of::<O>()))
            .any()
            .downcast_ref::<O::Runtime>()
            .expect("registered owner runtime")
    }
    pub fn request<Q: Request>(&mut self, input: Q) -> Result<Q::Response, Error> {
        self.journal.read_request(input)
    }
    pub fn fact<F: 'static>(&self) -> Option<&F> {
        self.journal
            .facts
            .get(&TypeId::of::<F>())
            .and_then(|fact| fact.value.downcast_ref::<F>())
    }
    /// Inspect this owner's typed deadlines without consuming or editing them.
    pub fn due<D: Clone + Eq + 'static>(&mut self) -> Result<Vec<(SimTime, D)>, Error> {
        if self.journal.due_owners.get(&TypeId::of::<D>()) != Some(&TypeId::of::<O>()) {
            return self
                .journal
                .rejected(configuration("due work accessed by a foreign owner"));
        }
        let queue = self
            .journal
            .runtime(SlotKey::Due(TypeId::of::<D>()))
            .any()
            .downcast_ref::<DueQueue<D>>()
            .expect("registered due payload");
        Ok(queue
            .0
            .iter()
            .flat_map(|(at, work)| work.iter().cloned().map(move |work| (*at, work)))
            .collect())
    }
}
fn configuration(message: &str) -> Error {
    Error::InvalidConfiguration(message.into())
}
