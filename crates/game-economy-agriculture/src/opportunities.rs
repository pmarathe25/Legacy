//! Bounded cultivation opportunities; discovery never reserves or advances fields.
use crate::{Agriculture, Phase};
use game_economy_person::Person;
use game_lots::{Movement, Place};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// The exact physical action a person may consider, without any stock promise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opportunity {
    pub person: u64,
    pub field: u64,
    pub region: u64,
    pub crop: u32,
    pub phase: Phase,
    /// Planting's exact seed movement; harvests have no seed input.
    pub planting: Option<Movement>,
}

/// Sample at most four local fields per person from immutable regional indexes.
/// Seed availability is deliberately not read: a chosen intent may fail later.
pub fn discover(
    owner: &Agriculture,
    people: &[Person],
    eligible: &[u64],
    busy: &[u64],
    season: Option<&game_environment::Season>,
    seed: u64,
    round: u64,
) -> Result<Vec<Opportunity>, &'static str> {
    if eligible.par_windows(2).any(|p| p[0] >= p[1])
        || busy.par_windows(2).any(|p| p[0] >= p[1])
        || people.par_windows(2).any(|p| p[0].id >= p[1].id)
    {
        return Err("invalid cultivation opportunity actors");
    }
    let mut fields: Vec<_> = owner
        .fields
        .par_iter()
        .filter(|field| match field.phase {
            Phase::Growing => false,
            Phase::Fallow => season.is_none_or(|s| s.planting_allowed),
            Phase::Ripe => season.is_none_or(|s| s.harvest_allowed),
        })
        .collect();
    if fields
        .par_windows(2)
        .any(|p| (p[0].region, p[0].id) > (p[1].region, p[1].id))
    {
        fields.par_sort_unstable_by_key(|field| (field.region, field.id));
    }
    let mut crops: Vec<_> = owner.crops.par_iter().collect();
    if crops.par_windows(2).any(|p| p[0].id > p[1].id) {
        crops.par_sort_unstable_by_key(|crop| crop.id);
    }
    let eligible_contains = |id: u64| {
        eligible
            .first()
            .and_then(|first| id.checked_sub(*first))
            .and_then(|offset| usize::try_from(offset).ok())
            .is_some_and(|offset| eligible.get(offset) == Some(&id))
            || eligible.binary_search(&id).is_ok()
    };
    let results: Vec<Vec<Opportunity>> = people
        .par_chunks(1024)
        .map(|people| {
            let mut results = Vec::with_capacity(people.len() * 4);
            for person in people {
                if person.health == 0
                    || person.controlled
                    || !eligible_contains(person.id)
                    || busy.binary_search(&person.id).is_ok()
                {
                    continue;
                }
                let Some(region) = person.region else {
                    continue;
                };
                let first = fields.partition_point(|field| field.region < region);
                let end = fields.partition_point(|field| field.region <= region);
                let local = &fields[first..end];
                if local.is_empty() {
                    continue;
                }
                let selected_seed = owner
                    .provenance
                    .as_ref()
                    .and_then(|provenance| provenance.choose(person.id));
                let start =
                    (game_genetics::random(seed, person.id, round) % local.len() as u64) as usize;
                let mut rows: [Option<Opportunity>; 4] = [None; 4];
                for (offset, slot) in rows.iter_mut().enumerate().take(local.len().min(4)) {
                    let field = local[(start + offset) % local.len()];
                    if field.phase == Phase::Fallow && selected_seed == Some(None) {
                        continue;
                    }
                    let Some(crop) = crops
                        .binary_search_by_key(&field.crop, |crop| crop.id)
                        .ok()
                        .map(|index| crops[index])
                    else {
                        continue;
                    };
                    let planting = (field.phase == Phase::Fallow).then_some(Movement {
                        id: 0,
                        from: Place::Stock {
                            region,
                            good: crop.seed,
                        },
                        to: None,
                        good: crop.seed,
                        quantity: crop.seed_quantity,
                        item: None,
                        batch: selected_seed.flatten(),
                    });
                    *slot = Some(Opportunity {
                        person: person.id,
                        field: field.id,
                        region,
                        crop: crop.id,
                        phase: field.phase,
                        planting,
                    });
                }
                rows.sort_unstable_by_key(|row| row.map_or(u64::MAX, |row| row.field));
                results.extend(rows.into_iter().flatten());
            }
            results
        })
        .collect();
    Ok(results.into_par_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Crop, Field};
    use std::sync::Arc;

    fn person(id: u64, region: Option<u64>) -> Person {
        Person {
            id,
            region,
            health: 100,
            hunger: 90,
            hunger_remainder: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        }
    }

    #[test]
    fn discovery_is_bounded_personal_and_does_not_advance_fields() {
        let owner = Agriculture {
            provenance: Some(crate::provenance::Provenance {
                profiles: Arc::new(vec![crate::provenance::SeedProfile {
                    person: 1,
                    idle: 0,
                    act_on_tie: false,
                    preferences: vec![crate::provenance::SeedPreference {
                        batch: 17,
                        enabled: true,
                        value: 1,
                    }],
                }]),
                ..Default::default()
            }),
            inherit_births: false,
            requests: None,
            soil: None,
            crops: vec![Crop {
                id: 1,
                seed: 2,
                seed_quantity: 3,
                output: 4,
                output_quantity: 5,
                growth_ticks: 2,
            }],
            fields: (1..=8)
                .map(|id| Field {
                    id,
                    region: 1,
                    crop: 1,
                    phase: if id == 8 {
                        Phase::Growing
                    } else {
                        Phase::Fallow
                    },
                    remaining: u32::from(id == 8),
                })
                .collect(),
            policies: vec![],
            max_hunger: 1,
        };
        let people = vec![person(1, Some(1)), person(2, Some(1)), person(3, None)];
        let opening = owner.clone();
        let expected = discover(&owner, &people, &[1, 2, 3], &[2], None, 7, 9).unwrap();
        assert_eq!(owner, opening);
        assert_eq!(expected.len(), 4);
        assert!(expected.iter().all(|row| row.person == 1
            && row.field != 8
            && row.planting.unwrap().batch == Some(17)));
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            assert_eq!(
                pool.install(|| discover(&owner, &people, &[1, 2, 3], &[2], None, 7, 9).unwrap()),
                expected
            );
        }
        assert!(discover(&owner, &people, &[2, 1], &[], None, 7, 9).is_err());
        let mut refusing = owner.clone();
        let provenance = refusing.provenance.as_mut().unwrap();
        Arc::make_mut(&mut provenance.profiles)[0].preferences[0].value = -1;
        assert!(
            discover(&refusing, &people, &[1], &[], None, 7, 9)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    #[ignore = "concentrated cultivation opportunity scaling measurement"]
    fn concentrated_opportunity_scaling() {
        let count = std::env::var("LEGACY_OPPORTUNITY_PEOPLE")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(100_000);
        let owner = Agriculture {
            provenance: None,
            inherit_births: false,
            requests: None,
            soil: None,
            crops: vec![Crop {
                id: 1,
                seed: 2,
                seed_quantity: 3,
                output: 4,
                output_quantity: 5,
                growth_ticks: 2,
            }],
            fields: (1..=count)
                .map(|id| Field {
                    id,
                    region: 1,
                    crop: 1,
                    phase: Phase::Fallow,
                    remaining: 0,
                })
                .collect(),
            policies: vec![],
            max_hunger: 1,
        };
        let people: Vec<_> = (1..=count).map(|id| person(id, Some(1))).collect();
        let eligible: Vec<_> = (1..=count).collect();
        let mut expected = None;
        for workers in [1, 2, 4, 6] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let mut durations = Vec::new();
            for _ in 0..3 {
                let start = std::time::Instant::now();
                let found =
                    pool.install(|| discover(&owner, &people, &eligible, &[], None, 7, 9).unwrap());
                durations.push(start.elapsed());
                assert_eq!(found.len(), count as usize * 4);
                if let Some(expected) = &expected {
                    assert_eq!(&found, expected);
                } else {
                    expected = Some(found);
                }
            }
            durations.sort_unstable();
            eprintln!(
                "{workers} workers: {:?} median for {count} people",
                durations[1]
            );
        }
    }
}
