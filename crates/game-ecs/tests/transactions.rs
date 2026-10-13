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
fn exclusive_entity_allows_declared_compound_shape_and_rejects_foreign_components() {
    let mut schema = fixture().schema().clone();
    schema.register::<bool>("test/flag", |_| Ok(())).unwrap();
    let store = Store::with_schema(schema);
    let id = StableId(3);
    let mut reads = ReadSet::new();
    reads.exclusive_entity(&store, id).unwrap();
    let mut admitted = ReadFootprint::new();
    admitted.protect::<Row>(id);
    admitted.protect::<u64>(id);
    let mut batch = Batch::new();
    // Two cooperating owners deliberately declare the complete compound shape.
    batch.insert(
        id,
        Row {
            scope: 7,
            value: 30,
        },
    );
    batch.insert(id, 31_u64);
    let guard = reads.clone().bind(&store, &batch, &admitted).unwrap();
    guard.validate(&store, &batch).unwrap();
    batch.insert(StableId(4), true);
    guard.validate(&store, &batch).unwrap();
    let before = store.snapshot().unwrap();
    batch.insert(id, true);
    assert!(guard.validate(&store, &batch).is_err());
    // An already staged foreign component cannot be silently captured at bind.
    assert!(reads.bind(&store, &batch, &admitted).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn exclusive_entity_rejects_occupied_empty_duplicate_and_removal_shapes() {
    let mut store = fixture();
    let id = StableId(3);
    let mut reads = ReadSet::new();
    assert_eq!(
        reads.exclusive_entity(&store, StableId(1)),
        Err(Error::Unavailable)
    );
    assert!(matches!(
        reads.exclusive_entity(&store, StableId(0)),
        Err(Error::InvalidConfiguration(_))
    ));
    reads.exclusive_entity(&store, id).unwrap();
    assert!(
        reads
            .clone()
            .bind(&store, &Batch::new(), &ReadFootprint::new())
            .is_err()
    );
    let mut admitted = ReadFootprint::new();
    admitted.protect::<u64>(id);
    let mut removal = Batch::new();
    removal.remove::<u64>(id);
    assert!(reads.clone().bind(&store, &removal, &admitted).is_err());
    let mut batch = Batch::new();
    batch.insert(id, 30_u64);
    let guard = reads.clone().bind(&store, &batch, &admitted).unwrap();
    batch.remove::<u64>(id);
    assert!(guard.validate(&store, &batch).is_err());
    assert!(reads.clone().bind(&store, &batch, &admitted).is_err());
    store.spawn(id).unwrap();
    assert_eq!(reads.validate(&store), Err(Error::Unavailable));
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

#[test]
fn bounded_projection_diff_guards_unchanged_absent_and_allocated_rows() {
    let mut schema = Schema::new();
    schema
        .register::<u64>("number", |row| {
            if *row == 0 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    schema.register::<String>("name", |_| Ok(())).unwrap();
    let mut source = Store::with_schema(schema);
    source.insert_or_spawn(StableId(1), 10_u64).unwrap();
    source
        .insert_or_spawn(StableId(2), "unrelated".to_owned())
        .unwrap();
    let opening = source.project_entities([StableId(1), StableId(9)]).unwrap();
    assert_eq!(opening.ids().collect::<Vec<_>>(), vec![StableId(1)]);
    let mut preview = opening.clone();
    assert!(preview.insert(StableId(1), 0_u64).is_err());
    preview
        .insert(StableId(1), "introduced".to_owned())
        .unwrap();
    preview.insert_or_spawn(StableId(9), 9_u64).unwrap();
    let mut batch = Batch::new();
    let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
    assert_eq!(batch.edit_count(StableId(1)), 1);
    let guard = reads.bind(&source, &batch, &writes).unwrap();
    guard.validate(&source, &batch).unwrap();
    source.insert(StableId(1), 11_u64).unwrap();
    assert!(guard.validate(&source, &batch).is_err());
    source.insert(StableId(1), 10_u64).unwrap();
    batch.insert(StableId(9), "foreign allocation shape".to_owned());
    assert!(guard.validate(&source, &batch).is_err());
}

#[test]
fn bounded_projection_diff_protects_selected_absence_and_removed_rows() {
    let mut schema = Schema::new();
    schema.register::<u64>("number", |_| Ok(())).unwrap();
    schema.register::<String>("name", |_| Ok(())).unwrap();
    let mut source = Store::with_schema(schema);
    source.insert_or_spawn(StableId(1), 10_u64).unwrap();
    let opening = source
        .project_entities([StableId(1), StableId(50)])
        .unwrap();
    let mut preview = opening.clone();
    preview.remove::<u64>(StableId(1)).unwrap();
    let mut batch = Batch::new();
    let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.bind(&source, &batch, &writes).unwrap();
    batch.insert(StableId(1), "introduced foreign type".to_owned());
    assert!(guard.validate(&source, &batch).is_err());
    let mut batch = Batch::new();
    let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.bind(&source, &batch, &writes).unwrap();
    source.spawn(StableId(50)).unwrap();
    assert!(guard.validate(&source, &batch).is_err());
}

#[test]
fn projection_diff_rejects_invalid_mutation_without_partial_staging() {
    let mut schema = Schema::new();
    schema
        .register::<u64>("number", |row| {
            if *row == 0 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut source = Store::with_schema(schema);
    source.insert_or_spawn(StableId(1), 1_u64).unwrap();
    source.insert_or_spawn(StableId(2), 2_u64).unwrap();
    let opening = source.project_entities([StableId(1), StableId(2)]).unwrap();
    let mut preview = opening.clone();
    *preview.get_mut::<u64>(StableId(1)).unwrap() = 3;
    *preview.get_mut::<u64>(StableId(2)).unwrap() = 0;
    let mut batch = Batch::new();
    batch.insert(StableId(80), 80_u64);
    assert!(preview.diff_into(&opening, &mut batch).is_err());
    assert_eq!(
        batch.edited_entities().collect::<Vec<_>>(),
        vec![StableId(80)]
    );
}

#[test]
fn typed_projection_import_does_not_copy_or_guard_unimported_remote_columns() {
    let mut schema = Schema::new();
    schema.register::<u64>("number", |_| Ok(())).unwrap();
    schema.register::<String>("name", |_| Ok(())).unwrap();
    let mut source = Store::with_schema(schema);
    source.insert_or_spawn(StableId(1), 1_u64).unwrap();
    source
        .insert_pair_or_spawn(StableId(2), 2_u64, "private".to_owned())
        .unwrap();
    let mut opening = source.project_entities([StableId(1)]).unwrap();
    opening.import_rows::<u64>(&source, [StableId(2)]).unwrap();
    assert_eq!(opening.get::<String>(StableId(2)), None);
    let mut preview = opening.clone();
    preview.insert(StableId(2), 3_u64).unwrap();
    let mut batch = Batch::new();
    let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.bind(&source, &batch, &writes).unwrap();
    source
        .insert(StableId(2), "changed private".to_owned())
        .unwrap();
    guard.validate(&source, &batch).unwrap();
    preview
        .insert(StableId(2), "unauthorized".to_owned())
        .unwrap();
    assert!(preview.diff_into(&opening, &mut Batch::new()).is_err());
    source.insert(StableId(2), 4_u64).unwrap();
    assert!(guard.validate(&source, &batch).is_err());
}

#[test]
fn absent_partial_import_guards_entity_collision_even_without_selected_component() {
    let mut schema = Schema::new();
    schema.register::<u64>("number", |_| Ok(())).unwrap();
    schema.register::<String>("name", |_| Ok(())).unwrap();
    let mut source = Store::with_schema(schema);
    let mut opening = source.project_entities([]).unwrap();
    opening.import_rows::<u64>(&source, [StableId(90)]).unwrap();
    let preview = opening.clone();
    let mut batch = Batch::new();
    let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.bind(&source, &batch, &writes).unwrap();
    source
        .insert_or_spawn(StableId(90), "other type collision".to_owned())
        .unwrap();
    assert!(guard.validate(&source, &batch).is_err());
}

#[test]
fn registration_affinity_is_distinct_from_restored_store_value_equality() {
    let source = fixture();
    let cloned_schema = source.schema().clone();
    assert!(source.schema().same_registrations(&cloned_schema));
    let restored = Store::restore(cloned_schema, source.snapshot().unwrap()).unwrap();
    assert_eq!(source, restored);
    assert!(source.schema().same_registrations(restored.schema()));

    // Independently registering the same keys and types preserves semantic
    // equality, while creating a different immutable registration configuration.
    let independently_registered = fixture();
    assert_eq!(source, independently_registered);
    assert_eq!(source.schema(), independently_registered.schema());
    assert!(
        !source
            .schema()
            .same_registrations(independently_registered.schema())
    );
}

#[test]
fn projection_guards_reject_changed_validators_before_binding_and_publication() {
    fn original(row: &u64) -> Result<(), Error> {
        if *row <= 100 {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
    fn replacement(row: &u64) -> Result<(), Error> {
        if *row <= 20 {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
    let mut schema = Schema::new();
    schema.register::<u64>("number", original).unwrap();
    let mut source = Store::with_schema(schema);
    source.insert_or_spawn(StableId(1), 10_u64).unwrap();
    let opening = source.project_entities([StableId(1)]).unwrap();
    let mut preview = opening.clone();
    preview.insert(StableId(1), 11_u64).unwrap();
    let mut batch = Batch::new();
    let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.clone().bind(&source, &batch, &writes).unwrap();

    let mut new_schema = Schema::new();
    new_schema.register::<u64>("number", replacement).unwrap();
    let replaced = Store::restore(new_schema, source.snapshot().unwrap()).unwrap();
    assert_eq!(source, replaced);
    // Both configurations accept the prospective row. Stale configuration,
    // rather than row validation failure, must reject the prepared effect.
    replaced.validate_batch(&batch).unwrap();
    let before = replaced.snapshot().unwrap();
    assert_eq!(reads.validate(&replaced), Err(Error::Unavailable));
    assert!(reads.bind(&replaced, &batch, &writes).is_err());
    assert_eq!(guard.validate(&replaced, &batch), Err(Error::Unavailable));
    assert_eq!(replaced.snapshot().unwrap(), before);
    guard.validate(&source, &batch).unwrap();

    let different_preview = replaced.project_entities([StableId(1)]).unwrap();
    let mut destination = Batch::new();
    destination.insert(StableId(9), 9_u64);
    assert!(
        different_preview
            .diff_into(&opening, &mut destination)
            .is_err()
    );
    assert_eq!(
        destination.edited_entities().collect::<Vec<_>>(),
        [StableId(9)]
    );
}

#[test]
fn compressed_full_columns_reject_inert_foreign_edits_before_and_after_binding() {
    for id in [StableId(1), StableId(50)] {
        let source = fixture();
        let opening = source.project_entities([id]).unwrap();
        let preview = opening.clone();
        let mut batch = Batch::new();
        let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
        assert!(batch.edited_entity_ids().next().is_none());
        let guard = reads.clone().bind(&source, &batch, &writes).unwrap();
        // u64 is absent on both the existing and the absent selected identity.
        // This edit changes neither final shape nor values nor entity presence.
        batch.remove::<u64>(id);
        assert!(reads.bind(&source, &batch, &writes).is_err());
        assert_eq!(guard.validate(&source, &batch), Err(Error::Unavailable));
    }

    let source = fixture();
    let opening = source.project_entities([StableId(1)]).unwrap();
    let mut batch = Batch::new();
    let (reads, writes) = opening.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.clone().bind(&source, &batch, &writes).unwrap();
    batch.insert(StableId(1), source.get::<Row>(StableId(1)).unwrap().clone());
    assert!(reads.bind(&source, &batch, &writes).is_err());
    assert!(guard.validate(&source, &batch).is_err());
}

#[test]
fn compressed_full_columns_reject_stale_added_source_types_and_late_duplicates() {
    let mut source = fixture();
    let opening = source.project_entities([StableId(1)]).unwrap();
    let mut preview = opening.clone();
    preview.get_mut::<Row>(StableId(1)).unwrap().value += 1;
    let mut batch = Batch::new();
    let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.bind(&source, &batch, &writes).unwrap();
    source.insert(StableId(2), 99_u64).unwrap();
    guard.validate(&source, &batch).unwrap();
    source.insert(StableId(1), 99_u64).unwrap();
    assert_eq!(guard.validate(&source, &batch), Err(Error::Unavailable));
    source.remove::<u64>(StableId(1)).unwrap();
    guard.validate(&source, &batch).unwrap();
    batch.insert(
        StableId(1),
        preview.get::<Row>(StableId(1)).unwrap().clone(),
    );
    assert_eq!(guard.validate(&source, &batch), Err(Error::Unavailable));
}

#[test]
fn sparse_partial_absence_keeps_canonical_foreign_values_independent_but_rejects_foreign_batch_keys()
 {
    let mut source = fixture();
    let mut opening = source.project_entities([]).unwrap();
    opening.import_rows::<u64>(&source, [StableId(1)]).unwrap();
    assert!(opening.contains(StableId(1)));
    assert_eq!(opening.get::<u64>(StableId(1)), None);
    let mut batch = Batch::new();
    let (reads, writes) = opening.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.clone().bind(&source, &batch, &writes).unwrap();
    source.get_mut::<Row>(StableId(1)).unwrap().value += 1;
    guard.validate(&source, &batch).unwrap();
    source.insert(StableId(1), 10_u64).unwrap();
    assert!(guard.validate(&source, &batch).is_err());
    source.remove::<u64>(StableId(1)).unwrap();
    guard.validate(&source, &batch).unwrap();
    batch.insert(StableId(1), source.get::<Row>(StableId(1)).unwrap().clone());
    assert!(reads.bind(&source, &batch, &writes).is_err());
    assert!(guard.validate(&source, &batch).is_err());

    let mut batch = Batch::new();
    let (reads, writes) = opening.diff_into(&opening, &mut batch).unwrap();
    let guard = reads.bind(&source, &batch, &writes).unwrap();
    source.remove::<Row>(StableId(1)).unwrap();
    guard.validate(&source, &batch).unwrap();
    batch.remove::<Row>(StableId(1));
    assert!(guard.validate(&source, &batch).is_err());

    let mut invalid = opening.clone();
    invalid
        .insert(
            StableId(1),
            Row {
                scope: 7,
                value: 11,
            },
        )
        .unwrap();
    let mut untouched = Batch::new();
    untouched.insert(StableId(80), 80_u64);
    assert!(invalid.diff_into(&opening, &mut untouched).is_err());
    assert_eq!(
        untouched.edited_entity_ids().collect::<Vec<_>>(),
        [StableId(80)]
    );
}

#[test]
fn sparse_projection_copy_and_equality_preserve_empty_entities_and_typed_shapes() {
    let mut source = fixture();
    source.spawn(StableId(9)).unwrap();
    source.insert(StableId(2), 22_u64).unwrap();
    let cloned = source.clone();
    assert_eq!(source, cloned);
    assert!(cloned.contains(StableId(9)));
    let restored = Store::restore(source.schema().clone(), source.snapshot().unwrap()).unwrap();
    assert_eq!(source, restored);
    let projection = source.project_entities(source.ids()).unwrap();
    assert_eq!(source, projection);
    assert_eq!(source.snapshot().unwrap(), projection.snapshot().unwrap());

    let mut same_column_count = cloned.clone();
    same_column_count.remove::<Row>(StableId(1)).unwrap();
    same_column_count.insert(StableId(1), 10_u64).unwrap();
    assert_ne!(source, same_column_count);

    let mut closing = projection.clone();
    closing.remove::<Row>(StableId(1)).unwrap();
    closing.insert(StableId(1), 10_u64).unwrap();
    closing.insert(StableId(9), 9_u64).unwrap();
    let mut batch = Batch::new();
    let (reads, writes) = closing.diff_into(&projection, &mut batch).unwrap();
    let guard = reads.bind(&source, &batch, &writes).unwrap();
    guard.validate(&source, &batch).unwrap();
    source.apply_batch(batch).unwrap();
    assert_eq!(source, closing);
}
