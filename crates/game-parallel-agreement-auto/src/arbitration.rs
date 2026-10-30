//! Identity-sharded, contention-free arbitration of independent pair claims.
use game_parallel_agreements::Contract;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
const CANDIDATES: usize = 1024;
const SHARD: usize = 4096;
// Each sorted local claim has an identity slot and the smallest contract ID.
type Batch = Vec<(usize, Vec<(usize, u64)>)>;

pub fn select<'a>(
    candidates: &[&'a Contract],
    feasible: &[bool],
    people: &[PersonSnapshot],
) -> (Vec<&'a Contract>, Vec<(u64, u64)>) {
    let batches: Vec<Batch> = candidates
        .par_chunks(CANDIDATES)
        .zip(feasible.par_chunks(CANDIDATES))
        .map(|(chunk, feasible)| {
            let mut claims: Vec<_> = chunk
                .iter()
                .zip(feasible)
                .filter(|(_, yes)| **yes)
                .flat_map(|(c, _)| {
                    [
                        (
                            person_index(people, c.debtor).expect("assessed debtor"),
                            c.id,
                        ),
                        (
                            person_index(people, c.creditor).expect("assessed creditor"),
                            c.id,
                        ),
                    ]
                })
                .collect();
            claims.sort_unstable();
            claims.dedup_by_key(|claim| claim.0);
            claims
                .chunk_by(|a, b| a.0 / SHARD == b.0 / SHARD)
                .map(|group| (group[0].0 / SHARD, group.to_vec()))
                .collect()
        })
        .collect();
    // Flatten the sparse local buckets once; each identity shard then reads
    // only its own contiguous range, avoiding a chunk-by-shard cross product.
    let mut groups: Vec<_> = batches
        .into_par_iter()
        .flat_map_iter(|batch| batch)
        .collect();
    if groups.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        groups.par_sort_unstable_by_key(|group| group.0);
    }
    let shards = people.len().div_ceil(SHARD);
    let winners: Vec<u64> = (0..shards)
        .into_par_iter()
        .flat_map_iter(|shard| {
            let base = shard * SHARD;
            let mut minima = vec![u64::MAX; SHARD.min(people.len() - base)];
            let start = groups.partition_point(|group| group.0 < shard);
            let end = groups.partition_point(|group| group.0 <= shard);
            for group in &groups[start..end] {
                for &(slot, id) in &group.1 {
                    minima[slot - base] = minima[slot - base].min(id);
                }
            }
            minima
        })
        .collect();
    let selected: Vec<_> = candidates
        .par_iter()
        .zip(feasible.par_iter())
        .filter_map(|(&contract, &yes)| {
            (yes && winners[person_index(people, contract.debtor).expect("assessed debtor")]
                == contract.id
                && winners[person_index(people, contract.creditor).expect("assessed creditor")]
                    == contract.id)
                .then_some(contract)
        })
        .collect();
    let dense = selected.first().and_then(|first| {
        selected.last().and_then(|last| {
            (last.id.checked_sub(first.id) == Some((selected.len() - 1) as u64)).then_some(first.id)
        })
    });
    let chosen = winners
        .par_iter()
        .enumerate()
        .filter_map(|(slot, &id)| {
            let found = dense
                .and_then(|first| id.checked_sub(first))
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&i| i < selected.len())
                .or_else(|| {
                    if dense.is_none() {
                        selected.binary_search_by_key(&id, |c| c.id).ok()
                    } else {
                        None
                    }
                });
            found.map(|_| (people[slot].0, id))
        })
        .collect();
    (selected, chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_parallel_agreements::Status;
    use std::collections::BTreeMap;
    #[test]
    fn sharded_minima_match_independent_reference_with_hot_sparse_conflicts() {
        let people: Vec<_> = (0..15000u64)
            .map(|i| (10 + i * 3, Some(1), 100, 0))
            .collect();
        let contracts: Vec<_> = (0..24000u64)
            .map(|i| Contract {
                id: i + 1,
                debtor: people[(i as usize * 73) % people.len()].0,
                creditor: if i % 3 == 0 {
                    people[14000].0
                } else {
                    people[((i as usize * 89) + 1) % people.len()].0
                },
                good: 1,
                quantity: 1,
                delivered: 0,
                due: 2,
                status: Status::Active,
            })
            .collect();
        let refs: Vec<_> = contracts.iter().collect();
        let feasible: Vec<_> = (0..contracts.len()).map(|i| i % 7 != 0).collect();
        let mut best = BTreeMap::<u64, u64>::new();
        for (contract, yes) in contracts.iter().zip(&feasible) {
            if *yes {
                for actor in [contract.debtor, contract.creditor] {
                    best.entry(actor)
                        .and_modify(|v| *v = (*v).min(contract.id))
                        .or_insert(contract.id);
                }
            }
        }
        let expected: Vec<_> = contracts
            .iter()
            .zip(&feasible)
            .filter_map(|(c, yes)| {
                (*yes && best[&c.debtor] == c.id && best[&c.creditor] == c.id).then_some(c.id)
            })
            .collect();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let (chosen, actors) = select(&refs, &feasible, &people);
                    let ids: Vec<_> = chosen.iter().map(|c| c.id).collect();
                    let expected_actors: Vec<_> = best
                        .iter()
                        .filter_map(|(&actor, &id)| {
                            ids.binary_search(&id).ok().map(|_| (actor, id))
                        })
                        .collect();
                    assert_eq!(actors, expected_actors);
                    ids
                })
        };
        assert_eq!(run(1), expected);
        assert_eq!(run(4), expected);
    }
}
