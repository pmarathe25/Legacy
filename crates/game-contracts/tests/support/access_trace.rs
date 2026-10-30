#![cfg(feature = "access-trace")]
use game_contracts::{Data, Resource, access_trace, mutable, read};
use serde::{Deserialize, Serialize};
use std::{any::TypeId, sync::Arc};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Row;
impl Resource for Row {
    const KEY: &'static str = "test.row";
}
#[test]
fn tracing_is_opt_in_thread_local_and_covers_missing_reads_and_mutable_access() {
    let mut data = Data::default();
    assert!(read::<Row>(&data).is_err());
    assert!(access_trace::finish().reads.is_empty());
    access_trace::begin();
    assert!(read::<Row>(&data).is_err());
    std::thread::spawn(|| {
        assert!(read::<Row>(&Data::default()).is_err());
        assert!(access_trace::finish().reads.is_empty());
    })
    .join()
    .unwrap();
    data.insert(TypeId::of::<Row>(), Arc::new(Row));
    mutable::<Row>(data.get_mut(&TypeId::of::<Row>()).unwrap());
    let accesses = access_trace::finish();
    assert_eq!(accesses.reads, [Row::KEY].into());
    assert_eq!(accesses.writes, [Row::KEY].into());
    assert!(access_trace::finish().reads.is_empty());
}
