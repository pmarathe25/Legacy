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
