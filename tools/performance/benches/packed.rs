//! Compare production ID indexes under dense and extremely sparse stable IDs.
use game_packed::Packed;
use std::{hint::black_box, time::Instant};
fn main() {
    use game_performance::{metrics, setting};
    metrics::metadata();
    let count = setting("LEGACY_PACKED_POPULATION", 100000usize);
    let lookups = setting("LEGACY_PACKED_LOOKUPS", 1000000usize);
    assert!(count > 0 && count < 1000000);
    for stride in [1u64, 1024, 1_000_000_000_000] {
        let sample = metrics::Sample::new();
        let start = Instant::now();
        let mut rows: Packed<u64, u64> = (0..count)
            .rev()
            .map(|i| (1 + i as u64 * stride, i as u64 * 7))
            .collect();
        let initialize = start.elapsed().as_secs_f64() * 1000.;
        sample.report("packed_index_initialize");
        metrics::rss();
        let mut rng = 9u64;
        let mut actual = 0u64;
        let mut expected = 0u64;
        let start = Instant::now();
        for _ in 0..lookups {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let i = (rng >> 16) % count as u64;
            actual = actual.wrapping_add(*rows.get(&black_box(1 + i * stride)).unwrap());
            expected = expected.wrapping_add(i * 7);
        }
        let random = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(actual, expected);
        let start = Instant::now();
        let mut scan = 0u64;
        for _ in 0..10 {
            for (k, v) in rows.iter() {
                scan = scan.wrapping_add(black_box(*k) ^ *v);
            }
        }
        let scan_ms = start.elapsed().as_secs_f64() * 1000.;
        let reader = rows.clone();
        let original = reader
            .iter()
            .fold(0u64, |h, (k, v)| h.wrapping_add(*k ^ *v));
        let sample = metrics::Sample::new();
        let start = Instant::now();
        let mark = rows.begin();
        for i in 0..count / 10 {
            *rows.get_mut(&(1 + i as u64 * stride)).unwrap() += 1;
        }
        rows.commit(mark);
        let edit_ms = start.elapsed().as_secs_f64() * 1000.;
        sample.report("packed_index_retained_reader_edit");
        assert_eq!(
            reader
                .iter()
                .fold(0u64, |h, (k, v)| h.wrapping_add(*k ^ *v)),
            original
        );
        assert_eq!(scan, original.wrapping_mul(10));
        println!(
            "packed_index={} population={count} stride={stride} lookups={lookups} initialize_ms={initialize:.3} random_ms={random:.3} scan_ms={scan_ms:.3} reader_edit_ms={edit_ms:.3} hash={scan:016x}",
            game_packed::INDEX_NAME
        );
    }
}
