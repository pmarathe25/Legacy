//! Deterministic receiver-owned arbitration for partnership proposals.
use game_parallel_genetics::random;
use rayon::prelude::*;
use std::collections::BTreeSet;

/// Resolve proposals for one region, retaining the first eligible proposal for
/// each receiver. Input must be ordered by `(random(seed, mother, tick), mother)`;
/// the order of returned winners is not guaranteed.
pub fn resolve(mut proposed: Vec<(u64, u64, u64)>, seed: u64, tick: u64) -> Vec<(u64, u64, u64)> {
    if proposed.len() < 1024 {
        let mut paired = BTreeSet::new();
        proposed
            .into_iter()
            .filter(|(_, father, _)| paired.insert(*father))
            .collect()
    } else {
        // The original first proposal for a father has the smallest female priority.
        proposed.par_sort_unstable_by_key(|(mother, father, _)| {
            (*father, random(seed, *mother, tick), *mother)
        });
        proposed
            .par_iter()
            .enumerate()
            .filter_map(|(i, proposal)| {
                (i == 0 || proposed[i - 1].1 != proposal.1).then_some(*proposal)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_matching_selects_same_winners_as_ordered_reference() {
        let seed = 123;
        let tick = 7;
        let mut proposals: Vec<_> = (1..5000).map(|mother| (mother, mother % 531, 42)).collect();
        proposals.sort_unstable_by_key(|p| (random(seed, p.0, tick), p.0));
        let mut seen = BTreeSet::new();
        let mut expected: Vec<_> = proposals
            .iter()
            .copied()
            .filter(|p| seen.insert(p.1))
            .collect();
        expected.sort_unstable();
        for workers in [1, 2, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let mut actual = pool.install(|| resolve(proposals.clone(), seed, tick));
            actual.sort_unstable();
            assert_eq!(actual, expected);
        }
    }
}
