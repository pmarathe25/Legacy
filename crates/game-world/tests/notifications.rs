use game_core::{Error, SimTime};
use game_ecs::{Schema, StableId};
use game_world::{
    DueQueue, GenericHost, OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    first: Vec<u8>,
    second: (Vec<u8>, bool),
    due: DueQueue<u8>,
}
struct Sender;
struct First;
struct Second;
#[derive(Clone)]
struct Delta {
    amount: u32,
    public_sequence: u64,
}
struct ReadFirst;
impl Request for ReadFirst {
    type Response = Vec<u8>;
}

impl OwnerSystem<u8> for Sender {
    type Runtime = ();
    type Input = u32;
    type Output = ();
    fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, u8>, amount: u32) -> Result<(), Error> {
        let before = *tx.read::<u32>(StableId(1)).unwrap();
        tx.write(StableId(1), before + amount)?;
        let public_sequence = tx.emit(7)?;
        let next = tx.next_event();
        tx.publish(Delta {
            amount,
            public_sequence,
        })?;
        assert_eq!(
            tx.next_event(),
            next,
            "internal topics consume no public sequence"
        );
        tx.revise(1)
    }
}
impl OwnerSystem<u8> for First {
    type Runtime = Vec<u8>;
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, u8>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
impl OwnerSystem<u8> for Second {
    type Runtime = (Vec<u8>, bool);
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, u8>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
fn receive_first(
    _: &First,
    tx: &mut OwnerTxn<'_, '_, First, u8>,
    delta: Delta,
) -> Result<(), Error> {
    assert_eq!(tx.next_event(), delta.public_sequence + 1);
    assert_eq!(tx.read::<u32>(StableId(1)), Some(&(10 + delta.amount)));
    tx.runtime_mut().push(delta.public_sequence as u8);
    let id = tx.reserve_ids(1)?;
    tx.write(id, u64::from(delta.amount))?;
    tx.take_due::<u8>()?;
    tx.schedule(SimTime(tx.now().0 + 1), delta.amount as u8)
}
fn read_first(
    _: &First,
    tx: &mut OwnerReadTxn<'_, '_, First, u8>,
    _: ReadFirst,
) -> Result<Vec<u8>, Error> {
    assert_eq!(tx.next_event(), 3);
    Ok(tx.runtime().clone())
}
fn receive_second(
    _: &Second,
    tx: &mut OwnerTxn<'_, '_, Second, u8>,
    delta: Delta,
) -> Result<(), Error> {
    // The second subscriber observes the first one's provisional receipt.
    assert_eq!(tx.request(ReadFirst)?, vec![delta.public_sequence as u8]);
    tx.runtime_mut().0.push(delta.public_sequence as u8);
    let id = tx.reserve_ids(1)?;
    tx.write(id, delta.amount as u16)?;
    if tx.runtime().1 {
        return Err(Error::Unavailable);
    }
    Ok(())
}
fn fixture(fail: bool) -> (GenericHost<State, u8>, Registry<State, u8>) {
    let mut schema = Schema::new();
    schema.register::<u32>("sender/count", |_| Ok(())).unwrap();
    schema.register::<u64>("first/count", |_| Ok(())).unwrap();
    schema.register::<u16>("second/count", |_| Ok(())).unwrap();
    let mut host = GenericHost::new(
        schema,
        1,
        State {
            first: vec![],
            second: (vec![], fail),
            due: DueQueue([(SimTime(2), vec![1])].into()),
        },
    );
    let id = host.allocate_id().unwrap();
    host.storage.insert_or_spawn(id, 10_u32).unwrap();
    host.append_many(vec![6]).unwrap();
    let mut registry = Registry::new();
    registry.register_stateless(Sender).unwrap();
    registry
        .register(
            First,
            RuntimeSlot::new(|state: &State| &state.first, |state| &mut state.first),
        )
        .unwrap();
    registry
        .register(
            Second,
            RuntimeSlot::new(|state: &State| &state.second, |state| &mut state.second),
        )
        .unwrap();
    registry.owns::<Sender, u32>().unwrap();
    registry.owns::<First, u64>().unwrap();
    registry.owns::<Second, u16>().unwrap();
    registry
        .register_due::<First, u8>(RuntimeSlot::new(
            |state: &State| &state.due,
            |state| &mut state.due,
        ))
        .unwrap();
    registry
        .register_read_request::<First, ReadFirst>(read_first)
        .unwrap();
    registry
        .register_subscriber::<First, Delta>(receive_first)
        .unwrap();
    registry
        .register_subscriber::<Second, Delta>(receive_second)
        .unwrap();
    (host, registry)
}

#[test]
fn two_typed_recipients_receive_in_order_before_the_next_owner_call_without_public_records() {
    let (mut host, registry) = fixture(false);
    let prepared = registry
        .prepare(&host, SimTime(5), None, |tx| {
            tx.call::<Sender>(3)?;
            // Delivery completes after Sender returns, before the next request.
            assert_eq!(tx.request(ReadFirst)?, vec![2]);
            Ok(())
        })
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(host.state.first, [2]);
    assert_eq!(host.state.second.0, [2]);
    assert_eq!(host.state.due.0, [(SimTime(6), vec![3])].into());
    assert_eq!(host.storage.get::<u32>(StableId(1)), Some(&13));
    assert_eq!(host.storage.get::<u64>(StableId(2)), Some(&3));
    assert_eq!(host.storage.get::<u16>(StableId(3)), Some(&3));
    assert_eq!(host.next_id(), 4);
    assert_eq!(host.next_event(), 3);
    assert_eq!(
        host.events()
            .iter()
            .map(|record| record.event)
            .collect::<Vec<_>>(),
        [6, 7]
    );
}

#[test]
fn late_topic_receiver_failure_preserves_source_receipts_runtime_due_ids_and_records() {
    let (host, registry) = fixture(true);
    let before = host.snapshot().unwrap();
    let result = registry.prepare(&host, SimTime(5), None, |tx| tx.call::<Sender>(3));
    assert!(matches!(result, Err(Error::Unavailable)));
    assert_eq!(host.snapshot().unwrap(), before);
}
