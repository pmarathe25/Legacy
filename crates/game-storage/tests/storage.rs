use game_storage::{Records, Table};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[test]
fn splits_merges_snapshots_and_serde_match_ordered_reference() {
    let mut table = Table::<u64, u64>::new();
    let mut reference = BTreeMap::new();
    let mut random = 1u64;
    let mut snapshots = vec![];
    for turn in 0..50_000 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let key = random % 5000;
        match random % 3 {
            0 => assert_eq!(table.remove(&key), reference.remove(&key)),
            1 => assert_eq!(table.insert(key, turn), reference.insert(key, turn)),
            _ => {
                if let Some(v) = table.get_mut(&key) {
                    *v += 1;
                }
                if let Some(v) = reference.get_mut(&key) {
                    *v += 1;
                }
            }
        }
        if turn % 1000 == 0 {
            assert!(table.iter().eq(reference.iter()));
            snapshots.push((table.clone(), reference.clone()));
        }
    }
    for (snapshot, expected) in snapshots {
        assert!(snapshot.iter().eq(expected.iter()));
    }
    let encoded = serde_json::to_string(&table).unwrap();
    let decoded: Table<u64, u64> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, table);
    assert!(serde_json::from_str::<Table<u64, u64>>(r#"{"1":2,"1":3}"#).is_err());
    for key in reference.keys() {
        table.remove(key);
    }
    assert!(table.is_empty());
    table.insert(u64::MAX, 7);
    assert_eq!(table[&u64::MAX], 7);
}
#[derive(Debug)]
struct Counted(Arc<AtomicUsize>);
impl Clone for Counted {
    fn clone(&self) -> Self {
        self.0.fetch_add(1, Ordering::Relaxed);
        Self(self.0.clone())
    }
}
#[test]
fn single_write_copies_bounded_leaf_or_one_sparse_record() {
    for count in [1000, 20_000] {
        let copies = Arc::new(AtomicUsize::new(0));
        let table: Table<_, _> = (0..count).map(|id| (id, Counted(copies.clone()))).collect();
        copies.store(0, Ordering::Relaxed);
        let mut staged = table.clone();
        assert_eq!(copies.load(Ordering::Relaxed), 0);
        staged.get_mut(&500).unwrap();
        assert!(copies.load(Ordering::Relaxed) <= 64);
        let records: Records<_, _> = (0..count).map(|id| (id, Counted(copies.clone()))).collect();
        copies.store(0, Ordering::Relaxed);
        let mut staged = records.clone();
        staged.get_mut(&500).unwrap();
        assert_eq!(copies.load(Ordering::Relaxed), 1);
        assert!(records.contains_key(&500));
    }
}
#[test]
fn parallel_staging_preserves_snapshots_and_panics_leave_committed_state_intact() {
    let table: Table<u64, u64> = (0..20_000).map(|id| (id, id)).collect();
    let mut canonical = None;
    for workers in [1, 2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut staged = table.clone();
        staged.update(&pool, |id, v| *v += id);
        assert!(table.iter().all(|(id, v)| id == v));
        if let Some(expected) = &canonical {
            assert_eq!(&staged, expected);
        } else {
            canonical = Some(staged);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut staged = table.clone();
            staged.update(&pool, |id, v| {
                *v = 0;
                assert_ne!(*id, 789);
            });
        }));
        assert!(result.is_err());
        assert!(table.iter().all(|(id, v)| id == v));
    }
}
#[test]
fn changed_paths_include_every_new_value_across_structural_changes() {
    let mut before: Table<_, _> = (0..20_000).map(|id| (id, id)).collect();
    for turn in 0..100 {
        let mut after = before.clone();
        let key = turn * 197;
        after.remove(&key);
        *after.get_mut(&(key + 1)).unwrap() += 1;
        after.insert(30_000 + turn, turn);
        let changed: BTreeMap<_, _> = after.changed(&before).map(|(k, v)| (*k, *v)).collect();
        for (k, v) in &after {
            if before.get(k) != Some(v) {
                assert_eq!(changed.get(k), Some(v));
            }
        }
        assert!(changed.len() <= 128);
        before = after;
    }
    let records: Records<u64, Vec<u64>> = (0..1000).map(|id| (id, vec![id])).collect();
    let mut after = records.clone();
    after.get_mut(&89).unwrap().push(5);
    assert_eq!(
        after
            .changed(&records)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [89]
    );
}
#[test]
fn paged_scans_keep_stable_order_after_deletion_and_shared_splits() {
    let before: Table<u64, u64> = (0..10_000).map(|id| (id, id)).collect();
    let mut after = before.clone();
    for id in (0..10_000).step_by(3) {
        after.remove(&id);
    }
    for id in 10_000..10_100 {
        after.insert(id, id);
    }
    for cursor in [0, 31, 63, 1023, 9999, 10_099, u64::MAX] {
        assert!(
            after
                .scan_after(Some(&cursor))
                .eq(after.iter().filter(|(id, _)| **id > cursor))
        );
    }
    assert!(after.scan_after(None).eq(after.iter()));
}

#[test]
fn borrowed_ranges_match_reference_through_splits_and_deletions() {
    use std::ops::Bound::{Excluded, Included, Unbounded};
    let mut table: Table<Arc<str>, u64> = (0..5000)
        .map(|i| (format!("fact:{i:05}").into(), i))
        .collect();
    for i in (0..5000).step_by(3) {
        table.remove(format!("fact:{i:05}").as_str());
    }
    let snapshot = table.clone();
    *table.get_mut("fact:00001").unwrap() = 9;
    assert_eq!(snapshot["fact:00001"], 1);
    for lower in [
        Unbounded,
        Included("fact:00031"),
        Excluded("fact:00032"),
        Included("z"),
    ] {
        for upper in [
            Unbounded,
            Included("fact:01024"),
            Excluded("fact:01024"),
            Excluded("a"),
        ] {
            let expected = table.iter().filter(|(k, _)| {
                let lo = match lower {
                    Unbounded => true,
                    Included(s) => k.as_ref() >= s,
                    Excluded(s) => k.as_ref() > s,
                };
                let hi = match upper {
                    Unbounded => true,
                    Included(s) => k.as_ref() <= s,
                    Excluded(s) => k.as_ref() < s,
                };
                lo && hi
            });
            assert!(table.range((lower, upper)).eq(expected));
        }
    }
    assert_eq!(table.iter().len(), table.len());
}

#[test]
fn agendas_replace_deadlines_keep_time_key_order_and_restore_indexes() {
    use game_storage::Agenda;
    let mut agenda = Agenda::<u64, u64>::default();
    let mut reference = BTreeMap::new();
    let mut seed = 1u64;
    for i in 0..10_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let key = seed % 1000;
        let at = !seed.is_multiple_of(7);
        agenda.set(key, at.then_some(seed % 100));
        if at {
            reference.insert(key, seed % 100);
        } else {
            reference.remove(&key);
        }
        if i % 500 == 0 {
            let snapshot = agenda.clone();
            assert!(snapshot.valid());
            let mut expected: Vec<_> = reference
                .iter()
                .filter(|(_, at)| **at <= 50)
                .map(|(key, at)| (*at, *key))
                .collect();
            expected.sort();
            assert_eq!(
                snapshot.due(&50).copied().collect::<Vec<_>>(),
                expected.iter().map(|(_, key)| *key).collect::<Vec<_>>()
            );
            let bytes = serde_json::to_string(&snapshot).unwrap();
            let loaded: Agenda<u64, u64> = serde_json::from_str(&bytes).unwrap();
            assert_eq!(loaded, snapshot);
            assert!(loaded.valid());
        }
    }
}

#[test]
fn replacement_and_removal_return_shared_snapshots_without_copying_cold_records() {
    let copies = Arc::new(AtomicUsize::new(0));
    let records: Records<_, _> = (0..1000).map(|id| (id, Counted(copies.clone()))).collect();
    let mut staged = records.clone();
    copies.store(0, Ordering::Relaxed);
    let replaced = staged.insert(500, Counted(copies.clone())).unwrap();
    let removed = staged.remove(&501).unwrap();
    assert_eq!(copies.load(Ordering::Relaxed), 0);
    assert!(records.contains_key(&500) && records.contains_key(&501));
    assert!(!staged.contains_key(&501));
    assert!(Arc::ptr_eq(&records.shared(&500).unwrap(), &replaced));
    assert!(Arc::ptr_eq(&records.shared(&501).unwrap(), &removed));
}

#[test]
fn ordered_stream_merge_retains_tails_and_suppresses_shared_keys() {
    assert_eq!(
        game_storage::merge_unique([1, 3, 9].into_iter(), [2, 3, 7, 10].into_iter())
            .collect::<Vec<_>>(),
        [1, 2, 3, 7, 9, 10]
    );
    assert_eq!(
        game_storage::merge_unique(std::iter::empty(), [2, 3].into_iter()).collect::<Vec<_>>(),
        [2, 3]
    );
}

#[test]
fn paired_changes_match_point_lookups_across_different_tree_shapes() {
    let mut before: Table<u64, u64> = (0..20_000).map(|k| (k * 2, k)).collect();
    for round in 0..30 {
        let mut after = before.clone();
        for k in (round..40_000).step_by(97) {
            if k % 3 == 0 {
                after.remove(&k);
            } else {
                after.modify(k, || 0, |v| *v += 7);
            }
        }
        for (k, v, old) in after.changed_pairs(&before) {
            assert_eq!(after.get(k), Some(v));
            assert_eq!(old, before.get(k));
        }
        let changed: BTreeMap<_, _> = after
            .changed_pairs(&before)
            .map(|(k, v, _)| (*k, *v))
            .collect();
        assert!(
            after
                .iter()
                .all(|(k, v)| before.get(k) == Some(v) || changed.get(k) == Some(v))
        );
        before = after;
    }
    let empty = Table::new();
    assert!(
        before
            .changed_pairs(&empty)
            .all(|(_, _, old)| old.is_none())
    );
    assert_eq!(empty.changed_pairs(&before).count(), 0);
    assert_eq!(before.changed_pairs(&before.clone()).count(), 0);
}

#[test]
fn cache_versions_invalidate_on_writes_and_do_not_retain_rows() {
    let record = Arc::new(());
    let mut table = Table::from([(1, record.clone())]);
    let original = table.version();
    assert!(original.matches(&table));
    let snapshot = table.clone();
    *table.get_mut(&1).unwrap() = record.clone();
    assert!(!original.matches(&table));
    assert!(original.matches(&snapshot));
    let version = table.version();
    drop(table);
    drop(snapshot);
    assert_eq!(Arc::strong_count(&record), 1);
    let empty = Table::<u64, ()>::new();
    assert!(empty.version().matches(&Table::new()));
    for _ in 0..1000 {
        let mut replacement = Table::from([(1, record.clone())]);
        assert!(!version.matches(&replacement));
        replacement.get_mut(&1).unwrap();
        assert!(!version.matches(&replacement));
    }
}
