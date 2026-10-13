use game_core::{Error, SimTime};
use game_ecs::{Schema, StableId};
use game_sim::{ClockWorld, ModuleId};
use game_world::scheduling::{ClockSpan, DueWork, Order, Scheduler};
use game_world::{
    DueQueue, GenericHost, OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    producer: u64,
    consumer: (u64, bool),
    producer_due: DueQueue<u8>,
    consumer_due: DueQueue<u16>,
}
struct Producer;
struct Consumer;
#[derive(Clone)]
struct ElapsedDay {
    opening_weather: u64,
    elapsed: u64,
}
struct ReadElapsedDay;
impl Request for ReadElapsedDay {
    type Response = ElapsedDay;
}
struct ReadProducerRuntime;
impl Request for ReadProducerRuntime {
    type Response = u64;
}
struct MutateProducer;
impl Request for MutateProducer {
    type Response = ();
}
struct OpeningBody {
    value: u32,
    receipts: Vec<u8>,
}
struct AppendReceipt(u8);
impl Request for AppendReceipt {
    type Response = ();
}
fn append_receipt(
    _: &Producer,
    tx: &mut OwnerTxn<'_, '_, Producer, u8>,
    receipt: AppendReceipt,
) -> Result<(), Error> {
    tx.fact_mut::<OpeningBody>()?
        .ok_or(Error::Unavailable)?
        .receipts
        .push(receipt.0);
    let before = *tx.read::<u32>(StableId(1)).unwrap();
    tx.write(StableId(1), before + u32::from(receipt.0))
}
impl OwnerSystem<u8> for Producer {
    type Runtime = u64;
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, u8>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
impl OwnerSystem<u8> for Consumer {
    type Runtime = (u64, bool);
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, u8>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
fn producer_boundary(
    _: &Producer,
    tx: &mut OwnerReadTxn<'_, '_, Producer, u8>,
    _: ClockSpan,
) -> Result<Option<SimTime>, Error> {
    // This selected runtime input must remain guarded through the closing step.
    let weather = *tx.runtime();
    Ok(Some(SimTime(weather + 3)))
}
fn producer_advance(
    _: &Producer,
    tx: &mut OwnerTxn<'_, '_, Producer, u8>,
    span: ClockSpan,
) -> Result<(), Error> {
    let opening_weather = *tx.runtime();
    let elapsed = span.elapsed().0;
    tx.share(ElapsedDay {
        opening_weather,
        elapsed,
    })?;
    *tx.runtime_mut() += elapsed;
    let before = *tx.read::<u32>(StableId(1)).unwrap();
    tx.write(StableId(1), before + 1)?;
    tx.emit(1)?;
    tx.revise(1)
}
fn read_elapsed(
    _: &Producer,
    tx: &mut OwnerReadTxn<'_, '_, Producer, u8>,
    _: ReadElapsedDay,
) -> Result<ElapsedDay, Error> {
    tx.fact::<ElapsedDay>().cloned().ok_or(Error::Unavailable)
}
fn read_runtime(
    _: &Producer,
    tx: &mut OwnerReadTxn<'_, '_, Producer, u8>,
    _: ReadProducerRuntime,
) -> Result<u64, Error> {
    Ok(*tx.runtime())
}
fn mutate_producer(
    _: &Producer,
    tx: &mut OwnerTxn<'_, '_, Producer, u8>,
    _: MutateProducer,
) -> Result<(), Error> {
    tx.emit(99).map(|_| ())
}
fn producer_prepare(
    _: &Producer,
    tx: &mut OwnerTxn<'_, '_, Producer, u8>,
    _: ClockSpan,
) -> Result<(), Error> {
    let value = *tx.read::<u32>(StableId(1)).unwrap();
    tx.share(OpeningBody {
        value,
        receipts: Vec::new(),
    })?;
    tx.emit(0)?;
    tx.revise(1)
}
fn producer_close(
    _: &Producer,
    tx: &mut OwnerTxn<'_, '_, Producer, u8>,
    _: ClockSpan,
) -> Result<(), Error> {
    let frame = tx.fact::<OpeningBody>().ok_or(Error::Unavailable)?;
    let merged = frame.value
        + 1
        + frame
            .receipts
            .iter()
            .map(|receipt| u32::from(*receipt))
            .sum::<u32>();
    assert_eq!(tx.read::<u32>(StableId(1)), Some(&merged));
    tx.write(StableId(1), merged)?;
    tx.emit(10)?;
    tx.revise(1)
}
fn producer_close_fail(
    owner: &Producer,
    tx: &mut OwnerTxn<'_, '_, Producer, u8>,
    span: ClockSpan,
) -> Result<(), Error> {
    producer_close(owner, tx, span)?;
    Err(Error::Unavailable)
}
fn consumer_advance(
    _: &Consumer,
    tx: &mut OwnerTxn<'_, '_, Consumer, u8>,
    _: ClockSpan,
) -> Result<(), Error> {
    // The native producer interprets its retained output. Composition performs
    // no environmental-output to consumer-fact mapping.
    let day = tx.request(ReadElapsedDay)?;
    let before = *tx.read::<u64>(StableId(1)).unwrap();
    tx.write(StableId(1), before + day.opening_weather)?;
    tx.runtime_mut().0 += day.elapsed;
    tx.emit(2)?;
    tx.revise(1)
}
fn producer_due(
    _: &Producer,
    tx: &mut OwnerTxn<'_, '_, Producer, u8>,
    due: DueWork<u8>,
) -> Result<(), Error> {
    assert_eq!(due.scheduled, tx.now());
    if tx.fact::<OpeningBody>().is_some() {
        tx.defer(AppendReceipt(due.work))?;
    }
    let id = tx.reserve_ids(1)?;
    tx.write(id, u32::from(due.work))?;
    tx.emit(due.work)?;
    tx.revise(1)
}
fn consumer_due(
    _: &Consumer,
    tx: &mut OwnerTxn<'_, '_, Consumer, u8>,
    due: DueWork<u16>,
) -> Result<(), Error> {
    tx.emit(due.work as u8)?;
    tx.runtime_mut().0 += 10;
    if tx.runtime().1 {
        return Err(Error::Unavailable);
    }
    tx.revise(1)
}
fn fixture(fail: bool) -> (GenericHost<State, u8>, Registry<State, u8>, Scheduler<u8>) {
    build_fixture(fail, 0)
}
fn build_fixture(
    fail: bool,
    phased: u8,
) -> (GenericHost<State, u8>, Registry<State, u8>, Scheduler<u8>) {
    let mut schema = Schema::new();
    schema
        .register::<u64>("consumer/count", |_| Ok(()))
        .unwrap();
    schema
        .register::<u32>("producer/count", |_| Ok(()))
        .unwrap();
    let mut host = GenericHost::new(
        schema,
        1,
        State {
            producer: 3,
            consumer: (0, fail),
            producer_due: DueQueue([(SimTime(4), vec![7, 8])].into()),
            consumer_due: DueQueue([(SimTime(4), vec![9])].into()),
        },
    );
    let id = host.allocate_id().unwrap();
    host.storage.insert_or_spawn(id, 10_u64).unwrap();
    host.storage.insert(id, 20_u32).unwrap();
    let mut registry = Registry::new();
    registry
        .register(
            Producer,
            RuntimeSlot::new(|state: &State| &state.producer, |state| &mut state.producer),
        )
        .unwrap();
    registry
        .register(
            Consumer,
            RuntimeSlot::new(|state: &State| &state.consumer, |state| &mut state.consumer),
        )
        .unwrap();
    registry.owns::<Producer, u32>().unwrap();
    registry.owns::<Consumer, u64>().unwrap();
    registry
        .register_read_request::<Producer, ReadElapsedDay>(read_elapsed)
        .unwrap();
    registry
        .register_read_request::<Producer, ReadProducerRuntime>(read_runtime)
        .unwrap();
    registry
        .register_request::<Producer, MutateProducer>(mutate_producer)
        .unwrap();
    registry
        .register_request::<Producer, AppendReceipt>(append_receipt)
        .unwrap();
    registry
        .register_due::<Producer, u8>(RuntimeSlot::new(
            |state: &State| &state.producer_due,
            |state| &mut state.producer_due,
        ))
        .unwrap();
    registry
        .register_due::<Consumer, u16>(RuntimeSlot::new(
            |state: &State| &state.consumer_due,
            |state| &mut state.consumer_due,
        ))
        .unwrap();
    let producer_order = Order {
        priority: 10,
        module: ModuleId(1),
    };
    let consumer_order = Order {
        priority: 20,
        module: ModuleId(2),
    };
    let mut scheduler = Scheduler::new();
    // Deliberately reverse registration; stable keys determine execution.
    scheduler
        .register_owner::<Consumer, _>(&mut registry, consumer_order, None, consumer_advance)
        .unwrap();
    if phased == 0 {
        scheduler
            .register_owner::<Producer, _>(
                &mut registry,
                producer_order,
                Some(producer_boundary),
                producer_advance,
            )
            .unwrap();
    } else {
        scheduler
            .register_owner_phased::<Producer, _>(
                &mut registry,
                producer_order,
                Some(producer_boundary),
                Some(producer_prepare),
                Some(producer_advance),
                Some(if phased == 1 {
                    producer_close
                } else {
                    producer_close_fail
                }),
            )
            .unwrap();
    }
    scheduler
        .register_due::<Consumer, u16, _>(&mut registry, consumer_order, consumer_due)
        .unwrap();
    scheduler
        .register_due::<Producer, u8, _>(&mut registry, producer_order, producer_due)
        .unwrap();
    (host, registry, scheduler)
}

#[test]
fn scheduler_picks_earliest_due_and_preserves_native_fact_and_same_time_order() {
    let (mut host, registry, scheduler) = fixture(false);
    let span = scheduler
        .step(&registry, &mut host, SimTime(10), None)
        .unwrap();
    assert_eq!(
        span,
        ClockSpan {
            from: SimTime(0),
            to: SimTime(4)
        }
    );
    assert_eq!(host.clock().time, SimTime(4));
    assert_eq!(host.clock().revision, 6);
    assert_eq!(host.storage.get::<u32>(StableId(1)), Some(&21));
    assert_eq!(host.storage.get::<u64>(StableId(1)), Some(&13));
    assert_eq!(host.state.producer, 7);
    assert_eq!(host.state.consumer, (14, false));
    assert!(host.state.producer_due.0.is_empty());
    assert!(host.state.consumer_due.0.is_empty());
    assert_eq!(host.next_id(), 4);
    assert_eq!(
        host.events()
            .iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        [1, 2, 7, 8, 9]
    );
    assert!(host.events().iter().all(|row| row.at == SimTime(4)));
}

#[test]
fn late_due_failure_and_changed_boundary_source_preserve_complete_publication() {
    let (mut host, registry, scheduler) = fixture(true);
    let before = host.snapshot().unwrap();
    assert_eq!(
        scheduler.step(&registry, &mut host, SimTime(10), None),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);

    let (mut host, registry, scheduler) = fixture(false);
    let prepared = scheduler
        .prepare_step(&registry, &host, SimTime(10), None)
        .unwrap();
    host.state.producer += 1;
    let before = host.snapshot().unwrap();
    assert_eq!(
        registry.commit(&mut host, prepared),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
}

#[test]
fn readonly_inspection_and_prepare_advance_due_close_preserve_owner_phase_order() {
    let (mut host, registry, scheduler) = build_fixture(false, 1);
    let before = host.snapshot().unwrap();
    assert_eq!(registry.inspect(&host, None, ReadProducerRuntime), Ok(3));
    assert!(matches!(
        registry.inspect(&host, None, MutateProducer),
        Err(Error::InvalidConfiguration(_))
    ));
    assert_eq!(host.snapshot().unwrap(), before);
    scheduler
        .step(&registry, &mut host, SimTime(10), None)
        .unwrap();
    assert_eq!(host.storage.get::<u32>(StableId(1)), Some(&36));
    assert_eq!(
        host.events()
            .iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        [0, 1, 2, 7, 8, 9, 10]
    );
    assert_eq!(host.clock().revision, 8);
}

#[test]
fn closing_owner_rejection_preserves_preparations_advances_due_and_receipts() {
    let (mut host, registry, scheduler) = build_fixture(false, 2);
    let before = host.snapshot().unwrap();
    assert_eq!(
        scheduler.step(&registry, &mut host, SimTime(10), None),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
}
