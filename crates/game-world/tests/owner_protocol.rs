use game_core::{Error, SimTime};
use game_ecs::{Schema, StableId};
use game_world::{
    DueQueue, GenericHost, OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    source: u64,
    receiver: u32,
    unrelated: u8,
    due: DueQueue<u8>,
}
struct Source;
struct Receiver;
struct Add {
    fail: bool,
}
impl Request for Add {
    type Response = u32;
}
struct Notify;
impl Request for Notify {
    type Response = ();
}
struct Reenter;
impl Request for Reenter {
    type Response = ();
}
struct Missing;
impl Request for Missing {
    type Response = ();
}
struct ReadPeer {
    mutate: bool,
}
impl Request for ReadPeer {
    type Response = u64;
}
struct ReadSelf;
impl Request for ReadSelf {
    type Response = u64;
}

impl OwnerSystem<u8> for Source {
    type Runtime = u64;
    type Input = u8;
    type Output = u64;
    fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, u8>, input: u8) -> Result<u64, Error> {
        let id = StableId(1);
        let before = *tx.read::<u64>(id).unwrap();
        tx.write(id, before + 1)?;
        *tx.runtime_mut() += 1;
        match input {
            1 => {
                tx.request(Add { fail: true })?;
            }
            2 => {
                let _ = tx.request(Add { fail: true });
            }
            3 => {
                tx.request(Reenter)?;
            }
            4 => {
                let _ = tx.request(Missing);
            }
            5 => {
                let _ = tx.write(StableId(1), 99_u32);
            }
            6 => {
                assert_eq!(tx.request(ReadPeer { mutate: false })?, 12);
            }
            7 => {
                let _ = tx.request(ReadPeer { mutate: true });
            }
            8 => {
                tx.request(ReadPeer { mutate: false })?;
                // Returning from a readonly re-entry must retain the outer lock.
                tx.request(Reenter)?;
            }
            _ => {
                tx.request(Add { fail: false })?;
            }
        }
        // A later transformation of the same row sees the provisional value.
        let provisional = *tx.read::<u64>(id).unwrap();
        tx.write(id, provisional + 1)?;
        let consumed = tx.take_due::<u8>()?;
        assert!(consumed.iter().all(|(at, _)| *at <= tx.now()));
        tx.schedule(SimTime(tx.now().0 + 10), 9_u8)?;
        let new = tx.reserve_ids(1)?;
        tx.write(new, 50_u64)?;
        tx.emit(3)?;
        tx.defer(Notify)?;
        tx.revise(1)?;
        Ok(provisional + 1)
    }
}
impl OwnerSystem<u8> for Receiver {
    type Runtime = u32;
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, u8>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
fn receive_add(
    _: &Receiver,
    tx: &mut OwnerTxn<'_, '_, Receiver, u8>,
    input: Add,
) -> Result<u32, Error> {
    let current = *tx.read::<u32>(StableId(1)).unwrap();
    tx.write(StableId(1), current + 2)?;
    *tx.runtime_mut() += 1;
    tx.emit(2)?;
    if input.fail {
        return Err(Error::Unavailable);
    }
    Ok(current + 2)
}
fn notify(_: &Source, tx: &mut OwnerTxn<'_, '_, Source, u8>, _: Notify) -> Result<(), Error> {
    *tx.runtime_mut() += 10;
    tx.emit(4)?;
    Ok(())
}
fn reenter(_: &Source, _: &mut OwnerTxn<'_, '_, Source, u8>, _: Reenter) -> Result<(), Error> {
    Ok(())
}
fn read_self(
    _: &Source,
    tx: &mut OwnerReadTxn<'_, '_, Source, u8>,
    _: ReadSelf,
) -> Result<u64, Error> {
    let row = *tx.read::<u64>(StableId(1)).unwrap();
    Ok(row + *tx.runtime())
}
fn read_peer(
    _: &Receiver,
    tx: &mut OwnerReadTxn<'_, '_, Receiver, u8>,
    input: ReadPeer,
) -> Result<u64, Error> {
    if input.mutate {
        tx.request(Add { fail: false })?;
    }
    tx.request(ReadSelf)
}
fn fixture() -> (GenericHost<State, u8>, Registry<State, u8>) {
    let mut schema = Schema::new();
    schema
        .register::<u64>("test/source", |value| {
            if *value > 100 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    schema.register::<u32>("test/receiver", |_| Ok(())).unwrap();
    let state = State {
        source: 0,
        receiver: 0,
        unrelated: 0,
        due: DueQueue([(SimTime(1), vec![1])].into()),
    };
    let mut host = GenericHost::new(schema, 5, state);
    let id = host.allocate_id().unwrap();
    host.storage.insert_or_spawn(id, 10_u64).unwrap();
    host.storage.insert(id, 20_u32).unwrap();
    let mut registry = Registry::new();
    registry
        .register(
            Source,
            RuntimeSlot::new(|state: &State| &state.source, |state| &mut state.source),
        )
        .unwrap();
    registry
        .register(
            Receiver,
            RuntimeSlot::new(|state: &State| &state.receiver, |state| &mut state.receiver),
        )
        .unwrap();
    registry.owns::<Source, u64>().unwrap();
    registry.owns::<Receiver, u32>().unwrap();
    registry
        .register_request::<Receiver, Add>(receive_add)
        .unwrap();
    registry.register_request::<Source, Notify>(notify).unwrap();
    registry
        .register_request::<Source, Reenter>(reenter)
        .unwrap();
    registry
        .register_read_request::<Source, ReadSelf>(read_self)
        .unwrap();
    registry
        .register_read_request::<Receiver, ReadPeer>(read_peer)
        .unwrap();
    registry
        .register_due::<Source, u8>(RuntimeSlot::new(
            |state: &State| &state.due,
            |state| &mut state.due,
        ))
        .unwrap();
    (host, registry)
}

#[test]
fn owners_and_receivers_publish_one_shared_journal_in_order() {
    let (mut host, registry) = fixture();
    let before = host.snapshot().unwrap();
    let prepared = registry
        .prepare(&host, SimTime(1), None, |tx| tx.call::<Source>(0))
        .unwrap();
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(registry.commit(&mut host, prepared).unwrap(), 12);
    assert_eq!(host.storage.get::<u64>(StableId(1)), Some(&12));
    assert_eq!(host.storage.get::<u32>(StableId(1)), Some(&22));
    assert_eq!(host.storage.get::<u64>(StableId(2)), Some(&50));
    assert_eq!(host.state.source, 11);
    assert_eq!(host.state.receiver, 1);
    assert_eq!(host.next_id(), 3);
    assert_eq!(
        host.events()
            .iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(host.state.due.0, [(SimTime(11), vec![9])].into());
}

#[test]
fn late_receiving_failure_and_swallowed_failure_preserve_every_canonical_field() {
    for input in [1, 2] {
        let (host, registry) = fixture();
        let before = host.snapshot().unwrap();
        let result = registry.prepare(&host, SimTime(1), None, |tx| tx.call::<Source>(input));
        assert!(matches!(result, Err(Error::Unavailable)));
        assert_eq!(host.snapshot().unwrap(), before);
    }
}

#[test]
fn synchronous_owner_reentry_rejects_but_deferred_notifications_are_supported() {
    for input in [3, 4, 5] {
        let (host, registry) = fixture();
        let before = host.snapshot().unwrap();
        let result = registry.prepare(&host, SimTime(1), None, |tx| tx.call::<Source>(input));
        assert!(matches!(result, Err(Error::InvalidConfiguration(_))));
        assert_eq!(host.snapshot().unwrap(), before);
    }
}

#[test]
fn successive_owner_steps_coalesce_rows_and_due_changes() {
    let (mut host, registry) = fixture();
    let prepared = registry
        .prepare(&host, SimTime(1), None, |tx| {
            assert_eq!(tx.call::<Source>(0)?, 12);
            tx.call::<Source>(0)
        })
        .unwrap();
    assert_eq!(registry.commit(&mut host, prepared).unwrap(), 14);
    assert_eq!(host.storage.get::<u64>(StableId(1)), Some(&14));
    assert_eq!(host.storage.get::<u32>(StableId(1)), Some(&24));
    assert_eq!(host.state.source, 22);
    assert_eq!(host.next_id(), 4);
    assert_eq!(
        host.events()
            .iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        [2, 3, 2, 3, 4, 4]
    );
}

#[test]
fn stale_runtime_rows_due_or_registry_affinity_reject_before_publication() {
    for changed in 0..6 {
        let (mut host, mut registry) = fixture();
        let prepared = registry
            .prepare(&host, SimTime(1), None, |tx| tx.call::<Source>(0))
            .unwrap();
        match changed {
            0 => host.state.source += 1,
            1 => host.state.receiver += 1,
            2 => host
                .state
                .due
                .0
                .insert(SimTime(0), vec![2])
                .map(|_| ())
                .unwrap_or(()),
            3 => {
                host.storage.insert(StableId(1), 21_u32).unwrap();
            }
            5 => {
                registry.owns::<Receiver, u8>().unwrap();
            }
            _ => {}
        }
        let before = host.snapshot().unwrap();
        let result = if changed == 4 {
            fixture().1.commit(&mut host, prepared)
        } else {
            registry.commit(&mut host, prepared)
        };
        assert_eq!(result, Err(Error::Unavailable));
        assert_eq!(host.snapshot().unwrap(), before);
    }
}

#[test]
fn unrelated_runtime_field_edits_remain_valid() {
    let (mut host, registry) = fixture();
    let prepared = registry
        .prepare(&host, SimTime(1), None, |tx| tx.call::<Source>(0))
        .unwrap();
    host.state.unrelated = 9;
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(host.state.unrelated, 9);
}

#[test]
fn late_component_validator_rejection_preserves_runtime_due_and_publications() {
    let (mut host, registry) = fixture();
    host.storage.insert(StableId(1), 100_u64).unwrap();
    let prepared = registry
        .prepare(&host, SimTime(1), None, |tx| tx.call::<Source>(0))
        .unwrap();
    let before = host.snapshot().unwrap();
    assert_eq!(
        registry.commit(&mut host, prepared),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
}

#[test]
fn changed_schema_registration_rejects_identical_consulted_rows() {
    let (mut host, registry) = fixture();
    let prepared = registry
        .prepare(&host, SimTime(1), None, |tx| tx.call::<Source>(0))
        .unwrap();
    let mut schema = Schema::new();
    // The same save keys and values with independently registered validators.
    schema.register::<u64>("test/source", |_| Ok(())).unwrap();
    schema.register::<u32>("test/receiver", |_| Ok(())).unwrap();
    let components = host.storage.snapshot().unwrap();
    host.storage = game_ecs::Store::restore(schema, components).unwrap();
    let before = host.snapshot().unwrap();
    assert_eq!(
        registry.commit(&mut host, prepared),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
}

#[test]
fn unused_runtime_slots_are_never_cloned() {
    #[derive(PartialEq, Eq)]
    struct Unused;
    impl Clone for Unused {
        fn clone(&self) -> Self {
            panic!("an unused owner runtime must remain in place")
        }
    }
    struct Slots {
        active: u64,
        unused: Unused,
    }
    struct Active;
    struct Dormant;
    impl OwnerSystem<()> for Active {
        type Runtime = u64;
        type Input = ();
        type Output = ();
        fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, ()>, _: ()) -> Result<(), Error> {
            *tx.runtime_mut() += 1;
            Ok(())
        }
    }
    impl OwnerSystem<()> for Dormant {
        type Runtime = Unused;
        type Input = ();
        type Output = ();
        fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, ()>, _: ()) -> Result<(), Error> {
            unreachable!("dormant owner is not dispatched")
        }
    }
    let mut host = GenericHost::new(
        Schema::new(),
        1,
        Slots {
            active: 0,
            unused: Unused,
        },
    );
    let mut registry = Registry::new();
    registry
        .register(
            Active,
            RuntimeSlot::new(|state: &Slots| &state.active, |state| &mut state.active),
        )
        .unwrap();
    registry
        .register(
            Dormant,
            RuntimeSlot::new(|state: &Slots| &state.unused, |state| &mut state.unused),
        )
        .unwrap();
    let prepared = registry
        .prepare(&host, SimTime(1), None, |tx| tx.call::<Active>(()))
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(host.state.active, 1);
}

#[test]
fn readonly_peer_queries_reenter_active_owner_but_cannot_enter_mutable_receivers() {
    for input in [6, 7, 8] {
        let (mut host, registry) = fixture();
        let before = host.snapshot().unwrap();
        let prepared = registry.prepare(&host, SimTime(1), None, |tx| tx.call::<Source>(input));
        if input == 6 {
            registry.commit(&mut host, prepared.unwrap()).unwrap();
            assert_eq!(host.storage.get::<u64>(StableId(1)), Some(&12));
            assert_eq!(host.storage.get::<u32>(StableId(1)), Some(&20));
            assert_eq!(host.state.source, 11);
            assert_eq!(host.state.receiver, 0);
        } else {
            assert!(matches!(prepared, Err(Error::InvalidConfiguration(_))));
            assert_eq!(host.snapshot().unwrap(), before);
        }
    }
}

#[test]
fn multiple_stateless_owners_need_no_canonical_runtime_fields() {
    struct First;
    struct Second;
    struct ReadUnit;
    impl Request for ReadUnit {
        type Response = ();
    }
    impl OwnerSystem<u8> for First {
        type Runtime = ();
        type Input = ();
        type Output = ();
        fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, u8>, _: ()) -> Result<(), Error> {
            assert_eq!(tx.runtime(), &());
            *tx.runtime_mut() = ();
            tx.request(ReadUnit)?;
            tx.emit(1)?;
            Ok(())
        }
    }
    impl OwnerSystem<u8> for Second {
        type Runtime = ();
        type Input = ();
        type Output = ();
        fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, u8>, _: ()) -> Result<(), Error> {
            Ok(())
        }
    }
    fn read_unit(
        _: &Second,
        tx: &mut OwnerReadTxn<'_, '_, Second, u8>,
        _: ReadUnit,
    ) -> Result<(), Error> {
        assert_eq!(tx.runtime(), &());
        Ok(())
    }
    let mut host = GenericHost::new(Schema::new(), 1, 99_u64);
    let mut registry = Registry::new();
    registry.register_stateless(First).unwrap();
    // Existing registration helpers can still accept a RuntimeSlot argument.
    registry.register(Second, RuntimeSlot::stateless()).unwrap();
    registry
        .register_read_request::<Second, ReadUnit>(read_unit)
        .unwrap();
    let prepared = registry
        .prepare(&host, SimTime(1), None, |tx| tx.call::<First>(()))
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(host.state, 99);
    assert_eq!(
        host.events()
            .iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        [1]
    );
}
