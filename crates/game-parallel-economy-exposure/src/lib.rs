//! Bounded physical exposure through actual care and admitted transfers.
use game_parallel_economy_person::{
    Person,
    travel::{Colocation, Travel},
};
use game_parallel_economy_work::care::Plan;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::Place;
use game_parallel_social::Encounter;
use rayon::prelude::*;

pub fn prepare(
    events: &Events,
    care: Option<&Plan>,
    people: &[Person],
    _seed: u64,
    blocked: &[u64],
    medical: &[(u64, u64)],
    travel: Option<&Travel>,
) -> Vec<Encounter> {
    let locations = Colocation::new(travel);
    let local = |a: u64, b: u64| {
        let a = people
            .binary_search_by_key(&a, |p| p.id)
            .ok()
            .map(|i| &people[i])?;
        let b = people
            .binary_search_by_key(&b, |p| p.id)
            .ok()
            .map(|i| &people[i])?;
        let region = locations.region((a.id, a.region), (b.id, b.region))?;
        (a.id != b.id
            && a.health > 0
            && b.health > 0
            && !blocked.contains(&a.id)
            && !blocked.contains(&b.id))
        .then_some(region)
    };
    let pair = |a: u64, b: u64| local(a, b).map(|region| (a.min(b), a.max(b), region));
    let mut pairs: Vec<_> = events
        .physical_movements
        .par_iter()
        .filter_map(|receipt| {
            if !receipt.movement.accepted {
                return None;
            }
            let (Place::Person(a), Some(Place::Person(b))) =
                (receipt.request.from, receipt.request.to)
            else {
                return None;
            };
            pair(a, b)
        })
        .collect();
    if let Some(care) = care {
        pairs.par_extend(
            care.draft
                .events
                .par_iter()
                .filter_map(|event| pair(event.child, event.caregiver?)),
        );
    }
    pairs.par_extend(medical.par_iter().filter_map(|&(a, b)| pair(a, b)));
    if pairs.is_empty() {
        return Vec::new();
    }
    if pairs.par_windows(2).any(|pair| pair[0] > pair[1]) {
        pairs.par_sort_unstable();
    }
    let pairs: Vec<_> = pairs
        .par_chunk_by(|a, b| a == b)
        .map(|group| group[0])
        .collect();
    pairs
        .into_par_iter()
        .map(|(a, b, region)| Encounter {
            tick: events.tick,
            region,
            a,
            b,
            pressured: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_parallel_lot_runtime::ExternalReceipt;
    use game_parallel_lots::{Movement, Outcome};
    fn workload(count: u64) -> (Events, Vec<Person>) {
        let people = (1..=count)
            .map(|id| Person {
                id,
                region: Some(1),
                health: 100,
                hunger: 0,
                willing_to_work: true,
                seek_meals: false,
                controlled: false,
            })
            .collect();
        let physical_movements = (2..=count)
            .map(|id| {
                let request = Movement {
                    id,
                    from: Place::Person(1),
                    to: Some(Place::Person(id)),
                    good: 1,
                    quantity: 1,
                };
                ExternalReceipt {
                    action: id,
                    request,
                    movement: Outcome {
                        id,
                        accepted: true,
                        lots: vec![],
                    },
                }
            })
            .collect();
        let events = Events {
            tick: 1,
            started: vec![],
            start_denied: vec![],
            abandoned: vec![],
            finished: vec![],
            delivered: vec![],
            output_lost: vec![],
            interrupted: vec![],
            returned: vec![],
            empty_returns: vec![],
            spoiled: vec![],
            physical_actions: vec![],
            physical_movements,
        };
        (events, people)
    }
    #[test]
    fn concentrated_contacts_preserve_all_actual_pairs_and_ignore_failed_transfers() {
        let (mut events, people) = workload(4096);
        let failed = events.physical_movements.last_mut().unwrap();
        failed.movement.accepted = false;
        let failed_person = match failed.request.to {
            Some(Place::Person(id)) => id,
            _ => unreachable!(),
        };
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| prepare(&events, None, &people, 7, &[], &[], None))
        };
        let one = run(1);
        assert_eq!(one.len(), people.len() - 2);
        assert!(one.iter().all(|event| event.b != failed_person));
        assert_eq!(one, run(4));
    }
    #[test]
    #[ignore]
    fn concentrated_exposure_benchmark() {
        let workers = std::env::var("LEGACY_WORKERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let count = std::env::var("LEGACY_PEOPLE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000u64);
        let (events, people) = workload(count);
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut samples = Vec::new();
                for round in 0..4 {
                    let started = std::time::Instant::now();
                    assert_eq!(
                        prepare(&events, None, &people, 7, &[], &[], None).len(),
                        count as usize - 1
                    );
                    let elapsed = started.elapsed().as_secs_f64();
                    if round > 0 {
                        samples.push(elapsed);
                    }
                }
                samples.sort_by(f64::total_cmp);
                println!("workers={workers} people={count} elapsed={:.6}", samples[1]);
            });
    }
}
