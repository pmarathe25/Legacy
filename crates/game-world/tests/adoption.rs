use game_core::{Error, SimTime};
use game_ecs::{Schema, StableId, Store};
use game_sim::{Clock, ClockWorld};
use game_world::{GenericHost, StorageAccess};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Row(u64);
#[derive(Debug, PartialEq, Eq)]
struct ApplicationStore(Store);
impl StorageAccess for ApplicationStore {
    fn store(&self) -> &Store {
        &self.0
    }
    fn store_mut(&mut self) -> &mut Store {
        &mut self.0
    }
}
impl From<Store> for ApplicationStore {
    fn from(store: Store) -> Self {
        Self(store)
    }
}
fn schema() -> Schema {
    let mut schema = Schema::new();
    schema.register::<Row>("test/row", |_| Ok(())).unwrap();
    schema
}

#[test]
fn application_state_storage_and_readonly_events_borrow_disjointly() {
    let mut host = GenericHost::<Vec<u64>, u64, ApplicationStore>::with_storage(
        ApplicationStore(Store::with_schema(schema())),
        17,
        vec![5],
    );
    let first = host.allocate_ids(2).unwrap();
    assert_eq!(first, StableId(1));
    assert_eq!(host.next_id(), 3);
    host.storage
        .store_mut()
        .insert_or_spawn(first, Row(9))
        .unwrap();
    let metadata = &mut host.state;
    let store = host.storage.store_mut();
    metadata.push(store.get::<Row>(first).unwrap().0);
    host.append_many(vec![23]).unwrap();
    let parts = host.parts_mut();
    parts.state.push(parts.events[0].event);
    parts.store.get_mut::<Row>(first).unwrap().0 += parts.state[0];
    assert_eq!(host.state, vec![5, 9, 23]);
    assert_eq!(host.storage.store().get::<Row>(first), Some(&Row(14)));
    let restored = GenericHost::<Vec<u64>, u64, ApplicationStore>::restore(
        schema(),
        host.snapshot().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(host.snapshot().unwrap(), restored.snapshot().unwrap());
}

#[test]
fn appending_records_and_publishing_revisions_have_separate_budgets() {
    let mut host = GenericHost::<(), u64>::new(schema(), 2, ());
    host.publish_clock(Clock {
        time: SimTime(7),
        revision: u64::MAX,
    });
    host.append_many(vec![4, 5]).unwrap();
    assert_eq!(host.clock().revision, u64::MAX);
    assert_eq!(host.next_event(), 3);
    assert!(host.events().iter().all(|event| event.at == SimTime(7)));
    let before = host.snapshot().unwrap();
    assert_eq!(host.publish_many(vec![6]), Err(Error::Overflow));
    assert_eq!(host.bump_revision(1), Err(Error::Overflow));
    assert_eq!(host.preflight_records(0, 1, 1), Err(Error::Overflow));
    assert_eq!(host.preflight_records(0, 1, 0), Ok(()));
    assert_eq!(host.snapshot().unwrap(), before);
    host.publish_clock(Clock {
        time: SimTime(7),
        revision: 10,
    });
    host.bump_revision(1).unwrap();
    assert_eq!(host.clock().revision, 11);
    assert_eq!(host.next_event(), 3);
}

#[test]
fn overflowing_id_range_does_not_consume_any_ids_or_rows() {
    let original = GenericHost::<(), ()>::new(schema(), 1, ());
    let mut snapshot = original.snapshot().unwrap();
    snapshot.next_id = u64::MAX - 2;
    let mut host = GenericHost::<(), ()>::restore(schema(), snapshot, |_| Ok(())).unwrap();
    let before = host.snapshot().unwrap();
    assert_eq!(host.allocate_ids(3), Err(Error::Overflow));
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.allocate_ids(2), Ok(StableId(u64::MAX - 2)));
    assert_eq!(host.next_id(), u64::MAX);
}
