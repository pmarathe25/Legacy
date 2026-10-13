//! Ordered owner hooks choose and publish one atomic logical boundary.
use crate::owner_transaction::{PreparedOwnerTransaction, TransactionContext};
use crate::{GenericHost, OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, StorageAccess};
use game_core::{Error, SimDuration, SimTime};
use game_sim::{Executor, ModuleId};
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockSpan {
    pub from: SimTime,
    pub to: SimTime,
}
impl ClockSpan {
    pub fn elapsed(self) -> SimDuration {
        SimDuration(self.to.0 - self.from.0)
    }
}

/// Stable scheduling identity; registration order does not affect execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Order {
    pub priority: u16,
    pub module: ModuleId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueWork<D> {
    pub scheduled: SimTime,
    pub work: D,
}
pub type Boundary<O, E> =
    fn(&O, &mut OwnerReadTxn<'_, '_, O, E>, ClockSpan) -> Result<Option<SimTime>, Error>;
pub type Advance<O, E> = fn(&O, &mut OwnerTxn<'_, '_, O, E>, ClockSpan) -> Result<(), Error>;
pub type OnDue<O, D, E> = fn(&O, &mut OwnerTxn<'_, '_, O, E>, DueWork<D>) -> Result<(), Error>;

struct BoundaryRequest<O, E> {
    span: ClockSpan,
    callback: Boundary<O, E>,
}
impl<O: 'static, E: 'static> Request for BoundaryRequest<O, E> {
    type Response = Option<SimTime>;
}
struct AdvanceRequest<O, E> {
    span: ClockSpan,
    callback: Advance<O, E>,
}
impl<O: 'static, E: 'static> Request for AdvanceRequest<O, E> {
    type Response = ();
}
struct DueBoundary<O, D>(PhantomData<(O, D)>);
impl<O: 'static, D: 'static> Request for DueBoundary<O, D> {
    type Response = Option<SimTime>;
}
struct DueRequest<O, D, E> {
    callback: OnDue<O, D, E>,
}
impl<O: 'static, D: 'static, E: 'static> Request for DueRequest<O, D, E> {
    type Response = ();
}

fn boundary<O: OwnerSystem<E>, E: 'static>(
    owner: &O,
    tx: &mut OwnerReadTxn<'_, '_, O, E>,
    request: BoundaryRequest<O, E>,
) -> Result<Option<SimTime>, Error> {
    (request.callback)(owner, tx, request.span)
}
fn advance<O: OwnerSystem<E>, E: 'static>(
    owner: &O,
    tx: &mut OwnerTxn<'_, '_, O, E>,
    request: AdvanceRequest<O, E>,
) -> Result<(), Error> {
    (request.callback)(owner, tx, request.span)
}
fn due_boundary<O: OwnerSystem<E>, D: Clone + Eq + 'static, E: 'static>(
    _: &O,
    tx: &mut OwnerReadTxn<'_, '_, O, E>,
    _: DueBoundary<O, D>,
) -> Result<Option<SimTime>, Error> {
    Ok(tx.due::<D>()?.first().map(|(at, _)| *at))
}
fn due<O: OwnerSystem<E>, D: Clone + Eq + 'static, E: 'static>(
    owner: &O,
    tx: &mut OwnerTxn<'_, '_, O, E>,
    request: DueRequest<O, D, E>,
) -> Result<(), Error> {
    for (scheduled, work) in tx.take_due::<D>()? {
        (request.callback)(owner, tx, DueWork { scheduled, work })?;
    }
    Ok(())
}

trait OwnerHook<E>: Send + Sync {
    fn order(&self) -> Order;
    fn boundary(
        &self,
        tx: &mut TransactionContext<'_, '_, E>,
        span: ClockSpan,
    ) -> Result<Option<SimTime>, Error>;
    fn advance(&self, tx: &mut TransactionContext<'_, '_, E>, span: ClockSpan)
    -> Result<(), Error>;
    fn prepare(&self, tx: &mut TransactionContext<'_, '_, E>, span: ClockSpan)
    -> Result<(), Error>;
    fn close(&self, tx: &mut TransactionContext<'_, '_, E>, span: ClockSpan) -> Result<(), Error>;
}
struct Hooks<O, E> {
    order: Order,
    boundary: Option<Boundary<O, E>>,
    prepare: Option<Advance<O, E>>,
    advance: Option<Advance<O, E>>,
    close: Option<Advance<O, E>>,
}
impl<O: OwnerSystem<E>, E: 'static> Hooks<O, E> {
    fn execute(
        &self,
        tx: &mut TransactionContext<'_, '_, E>,
        span: ClockSpan,
        callback: Option<Advance<O, E>>,
    ) -> Result<(), Error> {
        match callback {
            Some(callback) => tx.request(AdvanceRequest::<O, E> { span, callback }),
            None => Ok(()),
        }
    }
}
impl<O: OwnerSystem<E>, E: 'static> OwnerHook<E> for Hooks<O, E> {
    fn order(&self) -> Order {
        self.order
    }
    fn boundary(
        &self,
        tx: &mut TransactionContext<'_, '_, E>,
        span: ClockSpan,
    ) -> Result<Option<SimTime>, Error> {
        match self.boundary {
            Some(callback) => tx.request(BoundaryRequest::<O, E> { span, callback }),
            None => Ok(None),
        }
    }
    fn advance(
        &self,
        tx: &mut TransactionContext<'_, '_, E>,
        span: ClockSpan,
    ) -> Result<(), Error> {
        self.execute(tx, span, self.advance)
    }
    fn prepare(
        &self,
        tx: &mut TransactionContext<'_, '_, E>,
        span: ClockSpan,
    ) -> Result<(), Error> {
        self.execute(tx, span, self.prepare)
    }
    fn close(&self, tx: &mut TransactionContext<'_, '_, E>, span: ClockSpan) -> Result<(), Error> {
        self.execute(tx, span, self.close)
    }
}
trait DueHook<E>: Send + Sync {
    fn order(&self) -> Order;
    fn boundary(&self, tx: &mut TransactionContext<'_, '_, E>) -> Result<Option<SimTime>, Error>;
    fn consume(&self, tx: &mut TransactionContext<'_, '_, E>) -> Result<(), Error>;
}
struct DueHooks<O, D, E> {
    order: Order,
    callback: OnDue<O, D, E>,
}
impl<O: OwnerSystem<E>, D: Clone + Eq + 'static, E: 'static> DueHook<E> for DueHooks<O, D, E> {
    fn order(&self) -> Order {
        self.order
    }
    fn boundary(&self, tx: &mut TransactionContext<'_, '_, E>) -> Result<Option<SimTime>, Error> {
        tx.request(DueBoundary::<O, D>(PhantomData))
    }
    fn consume(&self, tx: &mut TransactionContext<'_, '_, E>) -> Result<(), Error> {
        tx.request(DueRequest::<O, D, E> {
            callback: self.callback,
        })
    }
}

/// Owners supply policy; the scheduler knows only time and stable hook order.
pub struct Scheduler<E> {
    owners: Vec<Box<dyn OwnerHook<E>>>,
    due: Vec<Box<dyn DueHook<E>>>,
}
impl<E: 'static> Default for Scheduler<E> {
    fn default() -> Self {
        Self::new()
    }
}
impl<E: 'static> Scheduler<E> {
    pub fn new() -> Self {
        Self {
            owners: Vec::new(),
            due: Vec::new(),
        }
    }
    pub fn register_owner<O: OwnerSystem<E>, S: 'static>(
        &mut self,
        registry: &mut Registry<S, E>,
        order: Order,
        boundary_callback: Option<Boundary<O, E>>,
        advance_callback: Advance<O, E>,
    ) -> Result<(), Error> {
        self.register_owner_phased::<O, S>(
            registry,
            order,
            boundary_callback,
            None,
            Some(advance_callback),
            None,
        )
    }
    /// Owner-native preparation captures opening inputs before any advances.
    /// Closing callbacks run after due work and can merge receiving receipts
    /// with those retained inputs in the same atomic journal.
    pub fn register_owner_phased<O: OwnerSystem<E>, S: 'static>(
        &mut self,
        registry: &mut Registry<S, E>,
        order: Order,
        boundary_callback: Option<Boundary<O, E>>,
        prepare_callback: Option<Advance<O, E>>,
        advance_callback: Option<Advance<O, E>>,
        close_callback: Option<Advance<O, E>>,
    ) -> Result<(), Error> {
        if self.owners.iter().any(|hook| hook.order() == order) {
            return Err(Error::InvalidConfiguration(
                "duplicate scheduled owner order".into(),
            ));
        }
        if boundary_callback.is_some() {
            registry.register_read_request::<O, BoundaryRequest<O, E>>(boundary::<O, E>)?;
        }
        registry.register_request::<O, AdvanceRequest<O, E>>(advance::<O, E>)?;
        self.owners.push(Box::new(Hooks::<O, E> {
            order,
            boundary: boundary_callback,
            prepare: prepare_callback,
            advance: advance_callback,
            close: close_callback,
        }));
        self.owners.sort_by_key(|hook| hook.order());
        Ok(())
    }
    /// The canonical typed queue must first be bound with Registry::register_due.
    pub fn register_due<O: OwnerSystem<E>, D: Clone + Eq + 'static, S: 'static>(
        &mut self,
        registry: &mut Registry<S, E>,
        order: Order,
        callback: OnDue<O, D, E>,
    ) -> Result<(), Error> {
        if self.due.iter().any(|hook| hook.order() == order) {
            return Err(Error::InvalidConfiguration(
                "duplicate scheduled due order".into(),
            ));
        }
        registry.register_read_request::<O, DueBoundary<O, D>>(due_boundary::<O, D, E>)?;
        registry.register_request::<O, DueRequest<O, D, E>>(due::<O, D, E>)?;
        self.due
            .push(Box::new(DueHooks::<O, D, E> { order, callback }));
        self.due.sort_by_key(|hook| hook.order());
        Ok(())
    }
    pub fn prepare_step<S: 'static, Storage: StorageAccess>(
        &self,
        registry: &Registry<S, E>,
        host: &GenericHost<S, E, Storage>,
        requested: SimTime,
        executor: Option<&Executor>,
    ) -> Result<PreparedOwnerTransaction<E, ClockSpan>, Error> {
        let from = host.transaction_opening().clock.time;
        if requested < from {
            return Err(Error::SequenceConflict);
        }
        registry.prepare(host, from, executor, |tx| {
            let horizon = ClockSpan {
                from,
                to: requested,
            };
            let mut to = requested;
            for hook in &self.owners {
                if let Some(at) = hook.boundary(tx, horizon)? {
                    if at <= from {
                        return Err(Error::InvalidConfiguration(
                            "owner boundary must follow opening time".into(),
                        ));
                    }
                    to = to.min(at);
                }
            }
            for hook in &self.due {
                if let Some(at) = hook.boundary(tx)? {
                    to = to.min(at.max(from));
                }
            }
            tx.set_time(to)?;
            tx.boundary_revision()?;
            let span = ClockSpan { from, to };
            for hook in &self.owners {
                hook.prepare(tx, span)?;
            }
            for hook in &self.owners {
                hook.advance(tx, span)?;
            }
            for hook in &self.due {
                hook.consume(tx)?;
            }
            tx.flush_deferred()?;
            for hook in &self.owners {
                hook.close(tx, span)?;
            }
            Ok(span)
        })
    }
    pub fn step<S: 'static, Storage: StorageAccess>(
        &self,
        registry: &Registry<S, E>,
        host: &mut GenericHost<S, E, Storage>,
        requested: SimTime,
        executor: Option<&Executor>,
    ) -> Result<ClockSpan, Error> {
        let prepared = self.prepare_step(registry, host, requested, executor)?;
        registry.commit(host, prepared)
    }
}
