use game_core::{Error, SimTime};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, Schema, StableId};
use game_sim::{Clock, ClockWorld};
use game_world::{GenericHost, Transaction};

type Host = GenericHost<u64, &'static str>;
fn fixture() -> Host {
    let mut schema = Schema::new();
    schema
        .register::<u64>("test/count", |value| {
            if *value > 100 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    schema.register::<u32>("test/reserve", |_| Ok(())).unwrap();
    let mut host = Host::new(schema, 1, 0);
    let id = host.allocate_id().unwrap();
    host.storage.insert_or_spawn(id, 10_u64).unwrap();
    host.storage.insert(id, 20_u32).unwrap();
    host
}

#[test]
fn preview_retains_logical_opening_without_history_or_source_affinity() {
    let mut source = fixture();
    source.publish_clock(Clock {
        time: SimTime(77),
        revision: 9,
    });
    source.append_many(vec!["historical"]).unwrap();
    let opening = source.transaction_opening();
    let before = source.snapshot().unwrap();
    let mut preview = Host::preview(
        game_ecs::Store::with_schema(source.storage.schema().clone()),
        123,
        &opening,
        source.seed(),
    );
    assert_eq!(preview.clock(), source.clock());
    assert_eq!(preview.next_id(), source.next_id());
    assert_eq!(preview.next_event(), source.next_event());
    assert_eq!(preview.seed(), source.seed());
    assert_eq!(preview.state, 123);
    assert!(preview.storage.is_empty());
    assert!(preview.events().is_empty());
    assert_ne!(preview.transaction_opening(), opening);
    assert_eq!(
        preview.commit_transaction(Transaction::new(opening, Batch::new())),
        Err(Error::Unavailable)
    );
    preview.append_many(vec!["projected"]).unwrap();
    assert_eq!(preview.events().len(), 1);
    assert_eq!(preview.events()[0].sequence, source.next_event());
    assert_eq!(preview.events()[0].at, source.clock().time);
    assert_eq!(source.snapshot().unwrap(), before);
}

#[test]
fn preview_tail_preserves_pending_sequences_and_partitions_by_cursor() {
    let mut source = fixture();
    source.publish_clock(Clock {
        time: SimTime(77),
        revision: 9,
    });
    source
        .append_many(vec!["a", "b", "c", "d", "e", "f"])
        .unwrap();
    let opening = source.transaction_opening();
    let before = source.snapshot().unwrap();
    let tail = source.events_since(3);
    assert_eq!(
        tail.iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        [4, 5, 6]
    );
    let mut preview = Host::preview_with_tail(
        game_ecs::Store::with_schema(source.storage.schema().clone()),
        123,
        &opening,
        source.seed(),
        tail,
    )
    .unwrap();
    assert_eq!(preview.events(), tail);
    assert_eq!(preview.next_event(), opening.next_event);
    assert_eq!(preview.next_id(), opening.next_id);
    assert_eq!(preview.clock(), opening.clock);
    assert_ne!(preview.transaction_opening(), opening);
    preview.append_many(vec!["projected"]).unwrap();
    assert_eq!(
        preview
            .events_since(4)
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        [5, 6, 7]
    );
    assert_eq!(preview.events_since(6)[0].event, "projected");
    assert!(preview.events_since(u64::MAX).is_empty());
    assert_eq!(preview.events_since(0).len(), 4);
    assert_eq!(source.snapshot().unwrap(), before);
}

#[test]
fn preview_tail_rejects_bad_sequences_or_timestamps_without_touching_source() {
    let mut source = fixture();
    source.publish_clock(Clock {
        time: SimTime(77),
        revision: 9,
    });
    source.append_many(vec!["a", "b", "c"]).unwrap();
    let opening = source.transaction_opening();
    let before = source.snapshot().unwrap();
    let mutations: [fn(&mut Vec<game_world::Published<&'static str>>); 6] = [
        |records| records[0].sequence = 0,
        |records| records[0].sequence = 4,
        |records| records[1].sequence = records[0].sequence,
        |records| records.swap(0, 1),
        |records| records[0].at = SimTime(78),
        |records| records[1].at = SimTime(76),
    ];
    for mutate in mutations {
        let mut tail = source.events().to_vec();
        mutate(&mut tail);
        assert!(
            Host::preview_with_tail(
                game_ecs::Store::with_schema(source.storage.schema().clone()),
                123,
                &opening,
                source.seed(),
                &tail,
            )
            .is_err()
        );
        assert_eq!(source.snapshot().unwrap(), before);
        assert_eq!(source.transaction_opening(), opening);
    }
}

#[test]
fn transaction_publishes_rows_cursors_and_records_at_one_final_logical_time() {
    let mut host = fixture();
    host.publish_clock(Clock {
        time: SimTime(5),
        revision: 3,
    });
    host.append_many(vec!["historical"]).unwrap();
    let opening = host.transaction_opening();
    let mut batch = Batch::new();
    batch.insert(StableId(1), 11_u64);
    batch.insert(StableId(opening.next_id), 30_u64);
    let mut transaction = Transaction::new(opening, batch);
    assert_eq!(transaction.time, SimTime(5));
    transaction.time = SimTime(9);
    transaction.ids = 1;
    transaction.events = vec!["first", "second"];
    transaction.revisions = 2;
    host.commit_transaction(transaction).unwrap();
    assert_eq!(
        host.clock(),
        Clock {
            time: SimTime(9),
            revision: 5
        }
    );
    assert_eq!(host.next_id(), 3);
    assert_eq!(host.next_event(), 4);
    assert_eq!(host.storage.get::<u64>(StableId(1)), Some(&11));
    assert_eq!(host.storage.get::<u64>(StableId(2)), Some(&30));
    assert_eq!(
        host.events()
            .iter()
            .map(|record| (record.sequence, record.at, record.event))
            .collect::<Vec<_>>(),
        [
            (1, SimTime(5), "historical"),
            (2, SimTime(9), "first"),
            (3, SimTime(9), "second")
        ]
    );
}

#[test]
fn invalid_or_stale_interval_publication_preserves_the_complete_host() {
    for failure in 0..4 {
        let mut host = fixture();
        host.state = 41;
        host.publish_clock(Clock {
            time: SimTime(5),
            revision: 3,
        });
        host.append_many(vec!["historical"]).unwrap();
        let mut reads = ReadSet::new();
        reads.read::<u64>(&host.storage, StableId(1));
        let mut writes = ReadFootprint::new();
        writes.protect::<u64>(StableId(1));
        let opening = host.transaction_opening();
        let mut batch = Batch::new();
        batch.insert(StableId(1), if failure == 0 { 101_u64 } else { 11_u64 });
        let guard = reads.bind(&host.storage, &batch, &writes).unwrap();
        batch.insert(StableId(opening.next_id), 30_u64);
        let mut transaction = Transaction::new(opening, batch);
        transaction.time = SimTime(9);
        transaction.ids = 1;
        transaction.events = vec!["rejected"];
        transaction.revisions = 2;
        transaction.guards.push(guard);
        match failure {
            1 => *host.storage.get_mut::<u64>(StableId(1)).unwrap() = 12,
            2 => host.publish_clock(Clock {
                time: SimTime(6),
                revision: 3,
            }),
            3 => transaction.time = SimTime(4),
            _ => {}
        }
        let before = host.snapshot().unwrap();
        let clock = host.clock();
        let expected = if failure == 3 {
            Error::SequenceConflict
        } else {
            Error::Unavailable
        };
        assert_eq!(host.commit_transaction(transaction), Err(expected));
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock(), clock);
    }
}

// A tiny owner finalization token: domain validation happens before staging,
// and its precomputed runtime cursor is published without fallible work.
struct Finish {
    guard: ReadGuard,
    cursor: u64,
}
fn stage_first(host: &Host, batch: &mut Batch) -> Finish {
    let mut reads = ReadSet::new();
    let before = *reads.read::<u64>(&host.storage, StableId(1)).unwrap();
    let mut admitted = ReadFootprint::new();
    admitted.protect::<u64>(StableId(1));
    batch.insert(StableId(1), before + 1);
    Finish {
        guard: reads.bind(&host.storage, batch, &admitted).unwrap(),
        cursor: host.state + 1,
    }
}

#[test]
fn composed_owner_tokens_publish_one_batch_and_finalize_metadata() {
    let mut host = fixture();
    let opening = host.transaction_opening();
    let mut batch = Batch::new();
    let first = stage_first(&host, &mut batch);
    let mut reads = ReadSet::new();
    reads.read::<u32>(&host.storage, StableId(1));
    let mut admitted = ReadFootprint::new();
    admitted.protect::<u32>(StableId(1));
    batch.insert(StableId(1), 21_u32);
    let second = reads.bind(&host.storage, &batch, &admitted).unwrap();
    let new_id = StableId(opening.next_id);
    batch.insert(new_id, 30_u64);
    // Owners retain their typed tokens while their generic guards join admission.
    let mut transaction = Transaction::new(opening, batch);
    transaction.guards = vec![first.guard.clone(), second];
    transaction.ids = 1;
    transaction.events = vec!["first", "second"];
    transaction.revisions = 2;
    host.commit_transaction(transaction).unwrap();
    host.state = first.cursor;
    assert_eq!(host.storage.get::<u64>(StableId(1)), Some(&11));
    assert_eq!(host.storage.get::<u32>(StableId(1)), Some(&21));
    assert_eq!(host.storage.get::<u64>(new_id), Some(&30));
    assert_eq!(host.state, 1);
    assert_eq!(host.next_id(), 3);
    assert_eq!(host.next_event(), 3);
    assert_eq!(host.clock().revision, 2);
    assert_eq!(
        host.events()
            .iter()
            .map(|record| (record.sequence, record.event))
            .collect::<Vec<_>>(),
        [(1, "first"), (2, "second")]
    );
}

#[test]
fn schema_or_owner_failure_preserves_rows_allocations_events_clock_and_metadata() {
    let mut host = fixture();
    let before = host.snapshot().unwrap();
    let clock = host.clock();
    let mut batch = Batch::new();
    let finish = stage_first(&host, &mut batch);
    batch.insert(StableId(host.next_id()), 101_u64);
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.guards.push(finish.guard);
    transaction.ids = 1;
    transaction.events.push("rejected");
    transaction.revisions = 1;
    assert_eq!(
        host.commit_transaction(transaction),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.clock(), clock);

    let mut batch = Batch::new();
    let finish = stage_first(&host, &mut batch);
    host.storage
        .get_mut::<u64>(StableId(1))
        .unwrap()
        .clone_from(&12);
    let before = host.snapshot().unwrap();
    let mut transaction = Transaction::new(host.transaction_opening(), batch);
    transaction.guards.push(finish.guard);
    transaction.events.push("stale");
    transaction.revisions = 1;
    assert_eq!(
        host.commit_transaction(transaction),
        Err(Error::Unavailable)
    );
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.clock(), clock);
}

#[test]
fn host_opening_checks_each_clock_and_cursor_field() {
    for mutate in [
        |host: &mut Host| {
            host.allocate_id().unwrap();
        },
        |host: &mut Host| {
            host.append_many(vec!["outside"]).unwrap();
        },
        |host: &mut Host| {
            host.bump_revision(1).unwrap();
        },
        |host: &mut Host| {
            host.publish_clock(Clock {
                time: SimTime(1),
                ..host.clock()
            });
        },
    ] {
        let mut host = fixture();
        let opening = host.transaction_opening();
        let mut batch = Batch::new();
        batch.insert(StableId(1), 11_u64);
        let mut transaction = Transaction::new(opening, batch);
        transaction.events.push("stale");
        transaction.revisions = 1;
        mutate(&mut host);
        let before = host.snapshot().unwrap();
        let clock = host.clock();
        assert_eq!(
            host.commit_transaction(transaction),
            Err(Error::Unavailable)
        );
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock(), clock);
    }
}

#[test]
fn host_lineage_rejects_equivalent_other_hosts_clones_and_restores() {
    let original = fixture();
    let opening = original.transaction_opening();
    let restored = Host::restore(
        original.storage.schema().clone(),
        original.snapshot().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    for mut host in [fixture(), original.clone(), restored] {
        assert_eq!(host.snapshot().unwrap(), original.snapshot().unwrap());
        let before = host.snapshot().unwrap();
        let mut batch = Batch::new();
        batch.insert(StableId(1), 11_u64);
        assert_eq!(
            host.commit_transaction(Transaction::new(opening.clone(), batch)),
            Err(Error::Unavailable)
        );
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock(), original.clock());
    }
}

#[test]
fn identity_reservations_reject_collisions_and_unreserved_creation() {
    for occupied in [false, true] {
        let mut host = fixture();
        if occupied {
            host.storage.insert_or_spawn(StableId(2), 20_u64).unwrap();
        }
        let before = host.snapshot().unwrap();
        let mut batch = Batch::new();
        batch.insert(StableId(if occupied { 2 } else { 3 }), 30_u64);
        let mut transaction = Transaction::new(host.transaction_opening(), batch);
        transaction.ids = 1;
        transaction.events.push("rejected");
        transaction.revisions = 1;
        assert_eq!(
            host.commit_transaction(transaction),
            Err(Error::Unavailable)
        );
        assert_eq!(host.snapshot().unwrap(), before);
        assert_eq!(host.clock().revision, 0);
    }
    let mut host = fixture();
    let mut batch = Batch::new();
    batch.remove::<u64>(StableId(900));
    host.commit_transaction(Transaction::new(host.transaction_opening(), batch))
        .unwrap();
    assert_eq!(host.next_id(), 2);
}
