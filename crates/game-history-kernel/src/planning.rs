use super::*;
use game_history_economy::FoodClaim;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
type Event = (String, Vec<u64>, u64, u64);
pub(super) fn settle(
    pop: &mut Population,
    economy: &mut Economy,
    pool: &ThreadPool,
    seed: u64,
    at: u64,
    events: &mut Vec<Event>,
) -> Result<(), String> {
    // One local public roster per boundary, shared by all evaluators. No global person scan per choice.
    let mut carers = BTreeMap::<u64, Vec<u64>>::new();
    for p in pop.people().iter().filter(|p| {
        p.alive
            && !p.controlled
            && !p.dependent
            && p.hunger < HUNGER
            && !matches!(p.activity, Activity::Travel { .. })
    }) {
        carers.entry(p.area).or_default().push(p.id);
    }
    let claims: Vec<_> = pool.install(|| {
        pop.people()
            .par_iter()
            .filter_map(|p| {
                if !p.alive
                    || p.controlled
                    || p.hunger < HUNGER
                    || matches!(p.activity, Activity::Travel { .. })
                {
                    return None;
                }
                let actor = if p.dependent {
                    let local = carers.get(&p.area)?;
                    *local.get((p.id as usize) % local.len())?
                } else {
                    p.id
                };
                Some(FoodClaim {
                    area: p.area,
                    actor,
                    patient: p.id,
                    hunger: p.hunger,
                })
            })
            .collect()
    });
    let mut occupied = BTreeSet::new();
    for claim in economy.allocate(claims)? {
        pop.feed(claim.patient)?;
        let until = at.checked_add(15).ok_or("meal time overflow")?;
        pop.assign(claim.patient, Activity::Eat { until })?;
        occupied.insert(claim.patient);
        if claim.actor != claim.patient {
            pop.assign(
                claim.actor,
                Activity::Care {
                    patient: claim.patient,
                    until,
                },
            )?;
            occupied.insert(claim.actor);
        }
        events.push((
            if claim.actor == claim.patient {
                "meal"
            } else {
                "care"
            }
            .into(),
            vec![claim.actor, claim.patient],
            claim.area,
            1,
        ));
    }
    let richest = economy
        .settlements()
        .iter()
        .max_by_key(|s| (s.food, std::cmp::Reverse(s.id)))
        .map(|s| (s.id, s.food));
    let plans: Vec<_> = pool.install(|| {
        pop.people()
            .par_iter()
            .filter_map(|p| {
                if !p.alive
                    || p.controlled
                    || occupied.contains(&p.id)
                    || matches!(p.activity, Activity::Travel { .. })
                {
                    return None;
                }
                // Existing commitments continue; only bodily thresholds interrupt them.
                if p.hunger >= HUNGER
                    && economy.get(p.area).is_some_and(|s| s.food == 0)
                    && !p.dependent
                {
                    if let Some((area, food)) =
                        richest.filter(|(area, food)| *area != p.area && *food > 0)
                    {
                        let _ = food;
                        return Some((
                            p.id,
                            Activity::Travel {
                                destination: area,
                                until: at.saturating_add(180),
                            },
                        ));
                    }
                }
                if p.fatigue >= FATIGUE && !matches!(p.activity, Activity::Rest { .. }) {
                    return Some((
                        p.id,
                        Activity::Rest {
                            until: at.saturating_add(p.fatigue.div_ceil(3).min(480)),
                        },
                    ));
                }
                if !matches!(p.activity, Activity::Idle)
                    && !(at % 720 == 0 && matches!(p.activity, Activity::Work { .. }))
                {
                    return None;
                }
                if p.dependent {
                    return None;
                }
                let duration = 360.min(FATIGUE.saturating_sub(p.fatigue)).max(1);
                Some((
                    p.id,
                    Activity::Work {
                        until: at.saturating_add(duration),
                    },
                ))
            })
            .collect()
    });
    // Ordinary social allocation is deterministic matching; both parties are locally available.
    let mut social = BTreeMap::<u64, Vec<u64>>::new();
    if at % 720 == 0 {
        for &(id, ref activity) in &plans {
            if matches!(activity, Activity::Work { .. }) && random(seed, id, at / 720) % 4 == 0 {
                social
                    .entry(pop.get(id).unwrap().area)
                    .or_default()
                    .push(id);
            }
        }
    }
    let mut paired = BTreeSet::new();
    for peers in social.values() {
        for pair in peers.chunks_exact(2) {
            let until = at.checked_add(60).ok_or("social time overflow")?;
            pop.assign(
                pair[0],
                Activity::Social {
                    partner: pair[1],
                    until,
                },
            )?;
            pop.assign(
                pair[1],
                Activity::Social {
                    partner: pair[0],
                    until,
                },
            )?;
            paired.extend(pair.iter().copied());
        }
    }
    for (id, activity) in plans {
        if !paired.contains(&id) {
            pop.assign(id, activity)?;
        }
    }
    Ok(())
}
