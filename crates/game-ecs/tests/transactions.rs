use game_core::Error;
use game_ecs::{Batch, ReadFootprint, ReadSet, Schema, StableId, Store};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Row {
    scope: u64,
    value: u64,
}
fn fixture() -> Store {
    let mut schema = Schema::new();
    schema.register::<Row>("test/row", |_| Ok(())).unwrap();
    schema.register::<u64>("test/number", |_| Ok(())).unwrap();
    let mut store = Store::with_schema(schema);
    store
        .insert_or_spawn(
            StableId(1),
            Row {
                scope: 7,
                value: 10,
            },
        )
        .unwrap();
    store
        .insert_or_spawn(
            StableId(2),
            Row {
                scope: 8,
                value: 20,
            },
        )
        .unwrap();
    store
}

#[test]
fn opening_witnesses_are_sparse_typed_and_include_absence() {
    let mut store = fixture();
    let mut reads = ReadSet::new();
    assert_eq!(reads.read::<Row>(&store, StableId(1)).unwrap().value, 10);
    assert!(reads.read::<u64>(&store, StableId(1)).is_none());
    store
        .insert(
            StableId(2),
            Row {
                scope: 9,
                value: 21,
            },
        )
        .unwrap();
    store.insert_or_spawn(StableId(3), 31_u64).unwrap();
    assert_eq!(reads.validate(&store), Ok(()));
    store.insert(StableId(1), 11_u64).unwrap();
    assert_eq!(reads.validate(&store), Err(Error::Unavailable));
    store.remove::<u64>(StableId(1)).unwrap();
    assert_eq!(reads.validate(&store), Ok(()));
    store.get_mut::<Row>(StableId(1)).unwrap().value += 1;
    assert_eq!(reads.validate(&store), Err(Error::Unavailable));
}

#[test]
fn binding_requires_explicit_admission_and_protects_exact_projection_and_affinity() {
    let store = fixture();
    let mut reads = ReadSet::new();
    reads.read::<Row>(&store, StableId(1));
    let mut batch = Batch::new();
    batch.insert(
        StableId(1),
        Row {
            scope: 7,
            value: 11,
        },
    );
    assert!(
        reads
            .clone()
            .bind(&store, &batch, &ReadFootprint::new())
            .is_err()
    );
    let mut writes = ReadFootprint::new();
    writes.protect::<Row>(StableId(1));
    let guard = reads.bind(&store, &batch, &writes).unwrap();
    batch.insert(StableId(2), 200_u64);
    guard.validate(&store, &batch).unwrap();
    assert!(guard.validate(&store, &Batch::new()).is_err());
    let before = store.snapshot().unwrap();
    batch.insert(
        StableId(1),
        Row {
            scope: 7,
            value: 12,
        },
    );
    assert!(guard.validate(&store, &batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn guard_only_rows_reject_even_equal_rewrites_and_declared_writes_are_always_retained() {
    let store = fixture();
    let mut reads = ReadSet::new();
    reads.read::<Row>(&store, StableId(1));
    let mut batch = Batch::new();
    let guard = reads.bind(&store, &batch, &ReadFootprint::new()).unwrap();
    batch.insert(StableId(1), store.get::<Row>(StableId(1)).unwrap().clone());
    assert!(guard.validate(&store, &batch).is_err());
    let mut writes = ReadFootprint::new();
    writes.protect::<u64>(StableId(2));
    let mut batch = Batch::new();
    batch.insert(StableId(2), 22_u64);
    let guard = ReadSet::new().bind(&store, &batch, &writes).unwrap();
    guard.validate(&store, &batch).unwrap();
    batch.remove::<u64>(StableId(2));
    assert!(guard.validate(&store, &batch).is_err());
}

#[test]
fn scoped_membership_accepts_owner_projection_and_rejects_phantoms_and_stale_sources() {
    let mut store = fixture();
    let mut reads = ReadSet::new();
    assert_eq!(
        reads.membership::<Row>(&store, |_, row| row.scope == 7),
        [StableId(1)].into()
    );
    let mut writes = ReadFootprint::new();
    writes.protect::<Row>(StableId(1));
    writes.protect::<Row>(StableId(3));
    let mut batch = Batch::new();
    batch.remove::<Row>(StableId(1));
    batch.insert(
        StableId(3),
        Row {
            scope: 7,
            value: 30,
        },
    );
    let guard = reads.bind(&store, &batch, &writes).unwrap();
    batch.insert(
        StableId(2),
        Row {
            scope: 8,
            value: 21,
        },
    );
    guard.validate(&store, &batch).unwrap();
    store
        .insert_or_spawn(
            StableId(8),
            Row {
                scope: 8,
                value: 80,
            },
        )
        .unwrap();
    guard.validate(&store, &batch).unwrap();
    let before = store.snapshot().unwrap();
    batch.insert(
        StableId(4),
        Row {
            scope: 7,
            value: 40,
        },
    );
    assert!(guard.validate(&store, &batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut reads = ReadSet::new();
    reads.membership::<Row>(&store, |_, row| row.scope == 7);
    let batch = Batch::new();
    let guard = reads.bind(&store, &batch, &ReadFootprint::new()).unwrap();
    store.get_mut::<Row>(StableId(2)).unwrap().scope = 7;
    assert!(guard.validate(&store, &batch).is_err());
}

#[test]
fn shared_entity_creation_keeps_each_owners_rows_guarded() {
    let store = fixture();
    let mut reads = ReadSet::new();
    assert!(!reads.entity(&store, StableId(3)));
    let mut writes = ReadFootprint::new();
    writes.protect::<Row>(StableId(3));
    let mut batch = Batch::new();
    batch.insert(
        StableId(3),
        Row {
            scope: 7,
            value: 30,
        },
    );
    let guard = reads.bind(&store, &batch, &writes).unwrap();
    guard.validate(&store, &batch).unwrap();
    let mut second_reads = ReadSet::new();
    assert!(!second_reads.entity(&store, StableId(3)));
    second_reads.read::<u64>(&store, StableId(3));
    batch.insert(StableId(3), 31_u64);
    let mut second_writes = ReadFootprint::new();
    second_writes.protect::<u64>(StableId(3));
    let second_guard = second_reads.bind(&store, &batch, &second_writes).unwrap();
    guard.validate(&store, &batch).unwrap();
    second_guard.validate(&store, &batch).unwrap();
    batch.insert(StableId(3), 32_u64);
    assert!(second_guard.validate(&store, &batch).is_err());
}

#[test]
fn pure_entity_absence_guard_rejects_foreign_creation_before_and_after_binding() {
    let store = fixture();
    let mut reads = ReadSet::new();
    assert!(!reads.entity(&store, StableId(3)));
    let mut batch = Batch::new();
    let guard = reads
        .clone()
        .bind(&store, &batch, &ReadFootprint::new())
        .unwrap();
    batch.insert(StableId(3), 31_u64);
    assert!(guard.validate(&store, &batch).is_err());
    assert!(reads.bind(&store, &batch, &ReadFootprint::new()).is_err());
}

#[test]
fn projected_reads_distinguish_absence_removal_and_duplicate_edits() {
    let store = fixture();
    let mut batch = Batch::new();
    assert_eq!(
        batch.projected::<Row>(&store, StableId(1)).unwrap(),
        store.get::<Row>(StableId(1))
    );
    batch.remove::<Row>(StableId(1));
    assert!(
        batch
            .projected::<Row>(&store, StableId(1))
            .unwrap()
            .is_none()
    );
    batch.insert(
        StableId(1),
        Row {
            scope: 7,
            value: 11,
        },
    );
    assert_eq!(
        batch.projected::<Row>(&store, StableId(1)),
        Err(Error::Unavailable)
    );
}

#[test]
fn batch_indexes_follow_appends_types_entity_counts_and_duplicate_removals() {
    let store = fixture();
    let mut batch = Batch::new();
    batch.insert(StableId(9), 90_u64);
    batch.remove::<Row>(StableId(1));
    batch.insert(
        StableId(9),
        Row {
            scope: 9,
            value: 91,
        },
    );
    assert_eq!(batch.edit_count(StableId(9)), 2);
    assert_eq!(batch.edit_count(StableId(1)), 1);
    assert!(batch.touches::<Row>(StableId(1)));
    assert!(!batch.touches::<u64>(StableId(1)));
    assert_eq!(
        batch.edited_ids::<Row>().collect::<Vec<_>>(),
        [StableId(1), StableId(9)]
    );
    assert_eq!(batch.insertions::<Row>().count(), 1);
    assert_eq!(
        batch.projected::<u64>(&store, StableId(9)).unwrap(),
        Some(&90)
    );
    batch.remove::<Row>(StableId(1));
    assert_eq!(batch.edit_count(StableId(1)), 2);
    assert_eq!(
        batch.projected::<Row>(&store, StableId(1)),
        Err(Error::Unavailable)
    );
    let mut footprint = ReadFootprint::new();
    footprint.protect::<Row>(StableId(1));
    assert!(!footprint.has_one_edit_per_row(&batch));
    assert!(!footprint.is_untouched(&batch));
}
