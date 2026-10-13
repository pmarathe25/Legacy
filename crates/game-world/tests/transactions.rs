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
