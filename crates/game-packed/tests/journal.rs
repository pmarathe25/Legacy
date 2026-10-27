use game_packed::Packed;
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
};
#[test]
fn differential_nested_structural_bulk_and_retained_readers() {
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut store = Packed::<u64, u64>::new();
        let mut expected = BTreeMap::new();
        let mut marks = vec![];
        let mut rng = 17u64;
        let mut readers = vec![];
        for step in 0usize..12000 {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let key = (rng >> 16) % 200;
            let key = if key.is_multiple_of(2) {
                key * 1_000_000_000_000
            } else {
                key
            };
            match rng % 10 {
                0 if marks.len() < 5 => marks.push((store.begin(), expected.clone())),
                1 if !marks.is_empty() => {
                    let (mark, _) = marks.pop().unwrap();
                    store.commit(mark);
                }
                2 if !marks.is_empty() => {
                    let (mark, old) = marks.pop().unwrap();
                    store.rollback(mark);
                    expected = old;
                }
                3 => {
                    assert_eq!(store.remove(&key), expected.remove(&key));
                }
                4 => {
                    store.update(&pool, |_, v| *v = v.wrapping_add(1));
                    for v in expected.values_mut() {
                        *v = v.wrapping_add(1);
                    }
                }
                5 => {
                    if let Some(v) = store.get_mut(&key) {
                        *v += 3;
                    }
                    if let Some(v) = expected.get_mut(&key) {
                        *v += 3;
                    }
                }
                _ => {
                    assert_eq!(
                        store.insert(key, rng % 10000),
                        expected.insert(key, rng % 10000)
                    );
                }
            }
            if step.is_multiple_of(137) {
                assert_eq!(
                    store
                        .iter()
                        .map(|(k, v)| (*k, *v))
                        .collect::<BTreeMap<_, _>>(),
                    expected
                );
                assert_eq!(
                    serde_json::to_string(&store).unwrap(),
                    serde_json::to_string(&expected).unwrap()
                );
                readers.push((store.clone(), expected.clone()));
                if readers.len() > 3 {
                    readers.remove(0);
                }
            }
            for (reader, old) in &readers {
                assert_eq!(
                    reader
                        .iter()
                        .map(|(k, v)| (*k, *v))
                        .collect::<BTreeMap<_, _>>(),
                    *old
                );
            }
        }
        while let Some((mark, old)) = marks.pop() {
            store.rollback(mark);
            expected = old;
        }
        assert_eq!(
            store
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect::<BTreeMap<_, _>>(),
            expected
        );
        let mark = store.begin();
        store.insert(u64::MAX, 5);
        assert!(
            catch_unwind(AssertUnwindSafe(|| store.update(&pool, |id, v| {
                *v += 1;
                assert_ne!(*id, u64::MAX);
            })))
            .is_err()
        );
        store.rollback(mark);
        assert_eq!(
            serde_json::to_string(&store).unwrap(),
            serde_json::to_string(&expected).unwrap()
        );
        let loaded: Packed<u64, u64> =
            serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        assert_eq!(loaded, store);
        assert!(serde_json::from_str::<Packed<u64, u64>>("{\"2\":3,\"2\":4}").is_err());
    }
}
#[test]
fn one_row_backup_per_scope_and_successful_child_is_undone_with_parent() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Debug)]
    struct Value {
        text: String,
        copies: Arc<AtomicUsize>,
    }
    impl Clone for Value {
        fn clone(&self) -> Self {
            self.copies.fetch_add(1, Ordering::Relaxed);
            Self {
                text: self.text.clone(),
                copies: self.copies.clone(),
            }
        }
    }
    let copies = Arc::new(AtomicUsize::new(0));
    let mut rows = Packed::from([(
        1u64,
        Value {
            text: "before".into(),
            copies: copies.clone(),
        },
    )]);
    let outer = rows.begin();
    for _ in 0..100 {
        rows.get_mut(&1).unwrap().text.push('a');
    }
    assert_eq!(copies.load(Ordering::Relaxed), 1);
    let inner = rows.begin();
    rows.get_mut(&1).unwrap().text.push('b');
    rows.commit(inner);
    rows.get_mut(&1).unwrap().text.push('c');
    assert_eq!(copies.load(Ordering::Relaxed), 2);
    rows.rollback(outer);
    assert_eq!(rows[&1].text, "before");
}

#[test]
fn selected_parallel_rows_are_atomic_across_nested_rollback_and_reload() {
    let original: Packed<u64, u64> = (0..1000).map(|id| (id, id * 2)).collect();
    let keys: Vec<_> = (0..1000).rev().filter(|id| id % 2 == 0).collect();
    let mut reference = None;
    for workers in [1, 4, 12] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut rows = original.clone();
        let outer = rows.begin();
        assert!(!rows.update_selected(&pool, &[1, 1], 2, |_, value| *value = 0));
        assert!(!rows.update_selected(&pool, &[1, 1000], 2, |_, value| *value = 0));
        assert_eq!(rows, original);
        assert!(rows.update_selected(&pool, &keys, 2, |id, value| *value += *id));
        let encoded = serde_json::to_string(&rows).unwrap();
        assert_eq!(
            rows,
            serde_json::from_str::<Packed<u64, u64>>(&encoded).unwrap()
        );
        if let Some(expected) = &reference {
            assert_eq!(&encoded, expected);
        } else {
            reference = Some(encoded);
        }
        let inner = rows.begin();
        assert!(rows.update_selected(&pool, &keys, 2, |_, value| *value += 1));
        rows.commit(inner);
        rows.rollback(outer);
        assert_eq!(rows, original);
        let mark = rows.begin();
        assert!(
            catch_unwind(AssertUnwindSafe(|| rows.update_selected(
                &pool,
                &keys,
                2,
                |id, value| {
                    *value += 1;
                    assert_ne!(*id, 500);
                }
            )))
            .is_err()
        );
        rows.rollback(mark);
        assert_eq!(rows, original);
    }
}
