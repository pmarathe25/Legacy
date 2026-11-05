//! Bounded personal decisions to initiate tactical conflict.
use super::{Battle, Draft, Encounter, Fighter, Order, Placement, Plan, Point, Response, Runtime};
use game_parallel_economy_person::Person;
use game_parallel_opinions::Opinions;
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Choice {
    actor: u64,
    target: u64,
    region: u64,
}

pub struct Selection {
    choices: Vec<Choice>,
    starts: Vec<(u64, [Point; 2])>,
    pub actors: Vec<u64>,
}

/// Select disjoint encounters from prior personal opinions. No target consents.
pub fn select(
    runtime: Option<&Runtime>,
    opinions: Option<&Opinions>,
    people: &[Person],
    unavailable: &[u64],
    unable_to_initiate: &[u64],
    tick: u64,
) -> Option<Selection> {
    let runtime = runtime?;
    let opinions = opinions.filter(|owner| {
        owner.sensitivities.par_iter().any(|row| {
            row.confrontation
                .as_ref()
                .is_some_and(|policy| policy.enabled && tick.is_multiple_of(policy.interval))
        })
    })?;
    let mut known: Vec<_> = opinions.records.par_iter().collect();
    if known.par_windows(2).any(|pair| {
        (pair[0].observer, pair[0].trust, pair[0].target)
            > (pair[1].observer, pair[1].trust, pair[1].target)
    }) {
        known.par_sort_unstable_by_key(|row| (row.observer, row.trust, row.target));
    }
    let dense_known = known.first().zip(known.last()).and_then(|(first, last)| {
        (last.observer.checked_sub(first.observer) == Some((known.len() - 1) as u64)
            && known
                .par_windows(2)
                .all(|pair| pair[0].observer < pair[1].observer))
        .then_some(first.observer)
    });
    let mut starts: Vec<_> = runtime
        .state
        .tactics
        .maps
        .par_iter()
        .filter_map(|map| {
            let mut points = Vec::with_capacity(2);
            'outer: for x in 0..=map.rules.width.min(63) {
                for y in 0..=map.rules.height.min(63) {
                    let point = Point { x, y };
                    if map.open(point) {
                        points.push(point);
                    }
                    if points.len() == 2 {
                        break 'outer;
                    }
                }
            }
            (points.len() == 2).then(|| (map.region, [points[0], points[1]]))
        })
        .collect();
    starts.par_sort_unstable_by_key(|row| row.0);
    let dense_people = people.first().zip(people.last()).and_then(|(first, last)| {
        (last.id.checked_sub(first.id) == Some((people.len() - 1) as u64)).then_some(first.id)
    });
    let find = |id: u64| {
        let index = if let Some(first) = dense_people {
            id.checked_sub(first)
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < people.len())
        } else {
            people.binary_search_by_key(&id, |p| p.id).ok()
        }?;
        Some(&people[index])
    };
    let mut choices: Vec<_> = opinions
        .sensitivities
        .par_iter()
        .filter_map(|preference| {
            let policy = preference.confrontation.as_ref()?;
            let actor = find(preference.person)?;
            if !policy.enabled
                || !tick.is_multiple_of(policy.interval)
                || actor.controlled
                || actor.health == 0
                || unavailable.binary_search(&actor.id).is_ok()
                || unable_to_initiate.binary_search(&actor.id).is_ok()
            {
                return None;
            }
            let region = actor.region?;
            if starts.binary_search_by_key(&region, |row| row.0).is_err() {
                return None;
            }
            let (start, end) = if let Some(first) = dense_known {
                let i = actor
                    .id
                    .checked_sub(first)
                    .and_then(|offset| usize::try_from(offset).ok())
                    .filter(|&index| index < known.len())?;
                (i, i + 1)
            } else {
                (
                    known.partition_point(|row| row.observer < actor.id),
                    known.partition_point(|row| row.observer <= actor.id),
                )
            };
            known[start..end].iter().take(32).find_map(|opinion| {
                if opinion.trust > policy.trust_at_most
                    || opinion.fear > policy.fear_at_most
                    || unavailable.binary_search(&opinion.target).is_ok()
                {
                    return None;
                }
                let target = find(opinion.target)?;
                (target.health > 0 && target.region == Some(region) && target.id != actor.id)
                    .then_some(Choice {
                        actor: actor.id,
                        target: target.id,
                        region,
                    })
            })
        })
        .collect();
    if choices.is_empty() {
        return None;
    }
    choices.par_sort_unstable_by_key(|row| (row.actor, row.target));
    let mut claims: Vec<_> = choices
        .par_iter()
        .enumerate()
        .flat_map_iter(|(i, row)| [(row.actor, i), (row.target, i)])
        .collect();
    claims.par_sort_unstable();
    let choices: Vec<_> = choices
        .into_par_iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let a = claims.partition_point(|claim| claim.0 < row.actor);
            let b = claims.partition_point(|claim| claim.0 < row.target);
            (claims[a].1 == i && claims[b].1 == i).then_some(row)
        })
        .collect();
    if choices.is_empty() {
        return None;
    }
    let mut actors: Vec<_> = choices
        .par_iter()
        .flat_map_iter(|row| [row.actor, row.target])
        .collect();
    actors.par_sort_unstable();
    Some(Selection {
        choices,
        starts,
        actors,
    })
}

impl Draft {
    /// Start surviving selected encounters after the old round, without applying damage yet.
    pub fn add_confrontations(
        &mut self,
        selection: &Selection,
        people: &[Person],
    ) -> Result<(), String> {
        let find = |id| {
            people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .map(|i| &people[i])
        };
        let choices: Vec<_> = selection
            .choices
            .par_iter()
            .copied()
            .filter(|row| {
                find(row.actor).zip(find(row.target)).is_some_and(|(a, b)| {
                    a.health > 0
                        && b.health > 0
                        && a.region == Some(row.region)
                        && b.region == Some(row.region)
                })
            })
            .collect();
        if choices.is_empty() {
            return Ok(());
        }
        let first = self.state.conflict.next_id;
        let next = first
            .checked_add(choices.len() as u64)
            .ok_or("encounter identity overflow")?;
        let mut reactions: Vec<_> = self.state.reactions.par_iter().collect();
        reactions.par_sort_unstable_by_key(|row| row.person);
        let (encounters, battles): (Vec<_>, Vec<_>) = choices
            .par_iter()
            .enumerate()
            .map(|(i, row)| {
                let target = find(row.target).expect("selected target");
                let response = reactions
                    .binary_search_by_key(&row.target, |r| r.person)
                    .ok()
                    .map(|i| &reactions[i].response);
                let order = if target.controlled {
                    Order::Defend
                } else {
                    match response {
                        Some(Response::Fight) => Order::Attack { target: row.actor },
                        Some(Response::Retreat) => Order::Retreat,
                        _ => Order::Defend,
                    }
                };
                let mut fighters = vec![
                    Fighter {
                        person: row.actor,
                        side: 1,
                        order: Order::Attack { target: row.target },
                    },
                    Fighter {
                        person: row.target,
                        side: 2,
                        order,
                    },
                ];
                fighters.sort_unstable_by_key(|f| f.person);
                let points = selection.starts[selection
                    .starts
                    .binary_search_by_key(&row.region, |s| s.0)
                    .expect("selected map")]
                .1;
                let mut placements = vec![
                    Placement {
                        person: row.actor,
                        position: points[0],
                        last_move: None,
                    },
                    Placement {
                        person: row.target,
                        position: points[1],
                        last_move: None,
                    },
                ];
                placements.sort_unstable_by_key(|p| p.person);
                let id = first + i as u64;
                (
                    Encounter {
                        id,
                        region: row.region,
                        fighters,
                    },
                    Battle {
                        encounter: id,
                        region: row.region,
                        people: placements,
                    },
                )
            })
            .unzip();
        self.state.conflict.next_id = next;
        self.state.conflict.encounters.par_extend(encounters);
        self.state.tactics.battles.par_extend(battles);
        self.replacement = Some((
            Some(Plan::compile(&self.state.conflict).map_err(str::to_owned)?),
            self.state.tactics.index().map_err(str::to_owned)?,
            Some(self.state.conflict.actors()),
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_parallel_opinions::{Confrontation, Opinion, Rules, Sensitivity};

    #[test]
    #[ignore = "performance measurement"]
    fn confrontation_selection_benchmark() {
        let count: u64 = std::env::var("LEGACY_COUNT")
            .unwrap_or_else(|_| "100000".into())
            .parse()
            .unwrap();
        let workers: usize = std::env::var("LEGACY_WORKERS")
            .unwrap_or_else(|_| "1".into())
            .parse()
            .unwrap();
        let hot = std::env::var_os("LEGACY_HOT_TARGET").is_some();
        let people: Vec<_> = (1..=count)
            .map(|id| Person {
                id,
                region: Some(1),
                health: 100,
                hunger: 0,
                willing_to_work: false,
                seek_meals: false,
                controlled: false,
            })
            .collect();
        let initiators: Vec<_> = (1..count).filter(|id| hot || id % 2 == 1).collect();
        let opinions = Opinions {
            rules: Rules {
                minimum: -10,
                maximum: 10,
                initial: 0,
                fulfilled_gain: 0,
                default_loss: 0,
                theft_loss: 0,
                encounter_gain: 0,
                fear_per_damage: 0,
                fear_decay_per_tick: 0,
                gratitude_gain: 0,
                obligation_relief: 0,
                minimum_report_trust: None,
                affinity: None,
                newborn_obligation_development: None,
            },
            sensitivities: initiators
                .iter()
                .map(|&person| Sensitivity {
                    person,
                    confrontation: Some(Confrontation {
                        trust_at_most: -2,
                        fear_at_most: 0,
                        interval: 1,
                        enabled: true,
                    }),
                    ..Default::default()
                })
                .collect::<Vec<_>>()
                .into(),
            records: initiators
                .iter()
                .map(|&person| Opinion {
                    observer: person,
                    target: if hot { count } else { person + 1 },
                    trust: -3,
                    fear: 0,
                    obligation: 0,
                    affinity: 0,
                })
                .collect(),
            events: vec![],
        };
        let state = super::super::initial(
            &[1],
            1,
            1,
            super::super::Rules {
                width: 10,
                height: 10,
                obstacle_count: 0,
                max_obstacle_size: 1,
                movement: 1,
                attack_range: 1,
            },
            vec![],
        )
        .unwrap();
        let runtime = Runtime {
            plan: Plan::compile(&state.conflict).unwrap(),
            index: state.tactics.index().unwrap(),
            actors: vec![],
            goods: Default::default(),
            state,
        };
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut samples = Vec::new();
                for _ in 0..5 {
                    let now = std::time::Instant::now();
                    let selected =
                        select(Some(&runtime), Some(&opinions), &people, &[], &[], 1).unwrap();
                    samples.push(now.elapsed().as_secs_f64());
                    assert_eq!(
                        selected.choices.len(),
                        if hot { 1 } else { (count / 2) as usize }
                    );
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} people={count} hot_target={hot} select_seconds={:.6}",
                    samples[2]
                );
            });
    }
}
