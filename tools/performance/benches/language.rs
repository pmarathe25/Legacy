//! Exact sound interpretation, including reverse-index preparation, against the old scan.
use game_communication::*;
use game_core::*;
use std::{hint::black_box, time::Instant};
fn scan(c: &Communication, reader: PersonId, sounds: &[u8]) -> Translation {
    let best = c
        .lexicons
        .get(&reader)
        .into_iter()
        .flat_map(|r| r.iter())
        .flat_map(|(key, variants)| variants.iter().map(move |w| (key, w)))
        .filter(|(_, w)| w.sounds == sounds)
        .max_by_key(|(key, w)| (w.strength, std::cmp::Reverse(*key)));
    Translation {
        surface: render(sounds),
        meaning: best.map(|(key, _)| key.clone()),
        confidence: best.map_or(0, |(_, w)| w.strength),
    }
}
fn sounds(i: usize) -> Vec<u8> {
    (0..4).map(|j| (i / 22usize.pow(j) % 22) as u8).collect()
}
fn queries(c: &Communication, probes: &[Vec<u8>], count: usize, optimized: bool) -> (f64, u64) {
    let start = Instant::now();
    let mut checksum = 0u64;
    for i in 0..count {
        let sound = black_box(&probes[i % probes.len()]);
        let result = if optimized {
            decode(black_box(c), PersonId(1), sound)
        } else {
            scan(black_box(c), PersonId(1), sound)
        };
        checksum = checksum.wrapping_add(u64::from(result.confidence));
        if let Some(key) = result.meaning {
            checksum ^= game_performance::hash(key.as_bytes());
        }
    }
    (start.elapsed().as_secs_f64() * 1000., checksum)
}
fn main() {
    game_performance::metrics::metadata();
    let words = game_performance::setting("LEGACY_WORDS", 2000usize);
    let count = game_performance::setting("LEGACY_DECODES", 20000usize);
    assert!(words > 0 && count > 0);
    let mut c = Communication::default();
    for i in 0..words {
        c.lexicons.entry(PersonId(1)).or_default().insert(
            format!("meaning:{i:06}"),
            vec![Association {
                sounds: sounds(i / 2),
                strength: (i % 7 + 1) as u16,
                source: PersonId(1),
                at: SimTime(0),
            }],
        );
    }
    let save = serde_json::to_string(&c).unwrap();
    let probes: Vec<_> = (0..64).map(|i| sounds(i * 37 % (words + 5))).collect();
    for sound in &probes {
        let a = scan(&c, PersonId(1), sound);
        let b = decode(&c, PersonId(1), sound);
        assert_eq!(
            (a.surface, a.meaning, a.confidence),
            (b.surface, b.meaning, b.confidence)
        );
    }
    let mut expected = None;
    for pair in 0..3 {
        for optimized in if pair % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            // A fresh lexicon version includes preparation in every measured optimized batch.
            let fresh: Communication = serde_json::from_str(&save).unwrap();
            let (ms, checksum) = queries(&fresh, &probes, count, optimized);
            if let Some(previous) = expected {
                assert_eq!(checksum, previous);
            } else {
                expected = Some(checksum);
            }
            assert_eq!(serde_json::to_string(&fresh).unwrap(), save);
            println!(
                "pair={pair} words={words} decodes={count} optimized={optimized} elapsed_ms={ms:.3} checksum={checksum:016x}"
            );
        }
    }
}
